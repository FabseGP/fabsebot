use std::{collections::VecDeque, fmt::Write as _, sync::Arc, time::Duration};

use anyhow::{Result as AResult, bail};
use fabsebot_db::guild::{insert_channel, set_current_voice_channel};
use lavalink_rs::{
	client::LavalinkClient,
	model::{
		UserId as LavaUserId, client::NodeDistributionStrategy, events::Events,
		search::SearchEngines, track::TrackLoadData,
	},
	node::NodeBuilder,
	player_context::TrackInQueue,
};
use metrics::counter;
use serenity::{
	all::{
		ButtonStyle, ChannelId, Colour, ComponentInteraction, ComponentInteractionCollector,
		Context as SerenityContext, CreateActionRow, CreateButton, CreateContainer, CreateMessage,
		EditMessage, GenericChannelId, GuildId, MessageId, UserId,
	},
	async_trait,
	builder::CreateContainerComponent,
	futures::StreamExt as _,
};
use songbird::{
	Call, CoreEvent, Event as SongBirdEvent, EventContext, EventHandler as VoiceEventHandler,
	Songbird, TrackEvent,
	driver::Bitrate,
	input::{AudioStream, AuxMetadata, Input, LiveInput, YoutubeDl},
	tracks::{PlayMode, Track},
};
use sqlx::{Pool, Postgres, query, query_as, query_scalar, types::time::OffsetDateTime};
use symphonia::core::io::MediaSource;
use tokio::{
	select, spawn,
	sync::{
		Mutex, MutexGuard,
		watch::{self, Receiver, Sender},
	},
	time::sleep,
};
use tracing::{error, warn};
use url::Url;
use uuid::Uuid;

use crate::{
	config::{
		constants::{
			EMPTY_VOICE_CHAN_MSG, FAILED_SONG_FETCH, INVALID_TRACK_SOURCE, MESSAGE_LIMIT,
			NOT_IN_VOICE_CHAN_MSG,
		},
		types::{Data, HTTP_CLIENT, SContext},
	},
	errors::commands::MusicError,
	events::interaction::build_feedback_action_row,
	log_error,
	stats::counters::METRICS,
	utils::helpers::{
		edit_message_container, get_lyrics, reply_container, separator, text_display,
		thumbnail_section, visit_page_button,
	},
};

#[derive(Clone)]
pub struct DriverDisconnectHandler {
	bot_data: Arc<Data>,
}

impl DriverDisconnectHandler {
	const fn new(bot_data: Arc<Data>) -> Self {
		Self { bot_data }
	}
}

#[async_trait]
impl VoiceEventHandler for DriverDisconnectHandler {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::DriverDisconnect(disconnect_data) = ctx {
			self.bot_data
				.music_manager
				.remove(disconnect_data.guild_id)
				.await
				.ok()?;
			if let Some((_, tx)) = self
				.bot_data
				.track_signals
				.remove(&disconnect_data.guild_id.get())
			{
				tx.send(TrackSignal::Disconnected).ok()?;
			}
		}
		None
	}
}

#[derive(Clone)]
pub struct ClientDisconnectHandler {
	serenity_context: SerenityContext,
	channel_id: GenericChannelId,
}

impl ClientDisconnectHandler {
	const fn new(serenity_context: SerenityContext, channel_id: GenericChannelId) -> Self {
		Self {
			serenity_context,
			channel_id,
		}
	}
}

#[async_trait]
impl VoiceEventHandler for ClientDisconnectHandler {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::ClientDisconnect(client_data) = ctx {
			let user_id = UserId::new(client_data.user_id.0);
			match user_id.to_user(&self.serenity_context.http).await {
				Ok(user) => {
					self.channel_id
						.send_message(
							&self.serenity_context.http,
							CreateMessage::default()
								.content(format!("Bye {}", user.display_name())),
						)
						.await
						.ok()?;
				}
				Err(err) => {
					warn!("Failed to fetch user: {err}");
				}
			}
		}
		None
	}
}

#[derive(Clone)]
pub struct PlaybackHandler {
	serenity_context: SerenityContext,
	bot_data: Arc<Data>,
	guild_id: GuildId,
	channel_id: GenericChannelId,
	track_watch: Sender<TrackSignal>,
}

impl PlaybackHandler {
	const fn new(
		serenity_context: SerenityContext,
		bot_data: Arc<Data>,
		guild_id: GuildId,
		channel_id: GenericChannelId,
		track_watch: Sender<TrackSignal>,
	) -> Self {
		Self {
			serenity_context,
			bot_data,
			guild_id,
			channel_id,
			track_watch,
		}
	}

	fn create_components<'a>(
		author_name: &'a str,
		msg_id: MessageId,
		metadata: &'a TrackData,
		queue_size: usize,
	) -> (
		CreateContainerComponent<'a>,
		CreateContainerComponent<'a>,
		CreateContainerComponent<'a>,
	) {
		let title = metadata.title.as_ref().map_or("Unknown title", |t| t);
		let artist = metadata.artist.as_ref().map_or("Unknown artist", |a| a);
		let url = metadata.source_url.as_ref().map_or("Unknown source", |s| s);
		let duration = metadata.duration_sec.unwrap_or(0);

		let text = format!(
			"# {title}\n**Added by:** {author_name}\n**Artist:** {artist}\n**Duration:** \
			 {duration}s\n**Queue size:** {}",
			queue_size.saturating_sub(1)
		);

		let thumbnail_section = thumbnail_section(text, metadata.thumbnail_url.clone().unwrap());

		let primary_row = CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
			CreateButton::new(format!("{msg_id}_s"))
				.style(ButtonStyle::Primary)
				.label("Skip"),
			CreateButton::new(format!("{msg_id}_p"))
				.style(ButtonStyle::Primary)
				.label("Pause/Unpause"),
			CreateButton::new(format!("{msg_id}_c"))
				.style(ButtonStyle::Primary)
				.label("Stop & clear queue"),
			CreateButton::new(format!("{msg_id}_l"))
				.style(ButtonStyle::Primary)
				.label("Show/Hide lyrics"),
			CreateButton::new(format!("{msg_id}_h"))
				.style(ButtonStyle::Primary)
				.label("Show/Hide song history"),
		]));

		let visit_button = visit_page_button(url);

		(thumbnail_section, primary_row, visit_button)
	}

	async fn handle_interaction<'a>(
		&self,
		interaction: ComponentInteraction,
		handler_lock: Arc<Mutex<Call>>,
		lyrics_shown: &mut bool,
		lyrics_container: &mut Option<CreateContainer<'a>>,
		history_shown: &mut bool,
		history_container: &mut Option<CreateContainer<'a>>,
		track: &TrackData,
		track_guilds: &Vec<i64>,
		container: &CreateContainer<'a>,
		action_row: &CreateContainerComponent<'a>,
		requested_channel: i64,
	) -> AResult<()> {
		interaction.defer(&self.serenity_context.http).await?;

		let mut msg = interaction.message;

		if interaction.data.custom_id.ends_with('s') {
			let handler = get_configured_songbird_handler(&handler_lock).await;
			let queue = handler.queue();
			if queue.len() > 1 {
				queue.skip()?;
				drop(handler);
				for guild_id in track_guilds {
					if *guild_id == i64::from(self.guild_id) {
						continue;
					}
					if let Some(handler_lock) = self
						.bot_data
						.music_manager
						.get(GuildId::from(guild_id.cast_unsigned()))
					{
						get_configured_songbird_handler(&handler_lock)
							.await
							.queue()
							.skip()?;
					}
				}
			}
		} else if interaction.data.custom_id.ends_with('p') {
			for guild_id in track_guilds {
				let handler_lock = if *guild_id == i64::from(self.guild_id) {
					handler_lock.clone()
				} else if let Some(handler_lock) = self
					.bot_data
					.music_manager
					.get(GuildId::new(guild_id.cast_unsigned()))
				{
					handler_lock
				} else {
					continue;
				};
				let Some(current_track) = get_configured_songbird_handler(&handler_lock)
					.await
					.queue()
					.current()
				else {
					continue;
				};
				match current_track.get_info().await.map(|t| t.playing) {
					Ok(state) => match state {
						PlayMode::Pause => {
							current_track.play()?;
						}
						PlayMode::Play => {
							current_track.pause()?;
						}
						_ => {}
					},
					Err(err) => {
						warn!("Failed to get track state. {err}");
					}
				}
			}
		} else if interaction.data.custom_id.ends_with('c') {
			for guild_id in track_guilds {
				let handler_lock = if *guild_id == i64::from(self.guild_id) {
					handler_lock.clone()
				} else if let Some(handler_lock) = self
					.bot_data
					.music_manager
					.get(GuildId::new(guild_id.cast_unsigned()))
				{
					handler_lock
				} else {
					continue;
				};
				get_configured_songbird_handler(&handler_lock)
					.await
					.queue()
					.stop();
			}
		} else if interaction.data.custom_id.ends_with('l') {
			let container = if *lyrics_shown {
				*lyrics_shown = false;
				container.clone()
			} else {
				*lyrics_shown = true;
				*history_shown = false;
				if let Some(container) = &lyrics_container {
					container.clone()
				} else {
					let lyrics = if let Some(title) = &track.title
						&& let Some(artist) = &track.artist
						&& let Some(lyrics) =
							get_lyrics(&self.serenity_context, title, artist).await
					{
						lyrics
					} else {
						"Not found :(".to_owned()
					};
					let mut text = format!("# Lyrics\n{lyrics}");
					text.truncate(MESSAGE_LIMIT);
					let text_display = vec![text_display(text)];
					let container = CreateContainer::new(text_display)
						.add_component(separator())
						.add_component(action_row.clone())
						.accent_colour(Colour::BLUE);
					*lyrics_container = Some(container.clone());
					container
				}
			};
			msg.edit(
				self.serenity_context.http.clone(),
				edit_message_container(container),
			)
			.await?;
		} else if interaction.data.custom_id.ends_with('h') {
			let container = if *history_shown {
				*history_shown = false;
				container.clone()
			} else {
				*history_shown = true;
				*lyrics_shown = false;
				if let Some(container) = &history_container {
					container.clone()
				} else {
					let queue_history =
						get_queue_history(requested_channel, &self.bot_data.db).await?;
					let mut history_string = String::with_capacity(512);
					writeln!(
						history_string,
						"# History of {} last played songs",
						queue_history.len()
					)?;
					for track in queue_history {
						if let Some(title) = track.title {
							let author_name = track
								.requested_by
								.get_author_name(&self.serenity_context)
								.await?;
							writeln!(
								history_string,
								"**{title}:** *{author_name} - {}*",
								track.played_at.to_utc().truncate_to_second()
							)?;
						}
					}
					history_string.truncate(MESSAGE_LIMIT);
					let text_display = vec![text_display(history_string)];
					let container = CreateContainer::new(text_display)
						.add_component(separator())
						.add_component(action_row.clone())
						.accent_colour(Colour::BLUE);
					*history_container = Some(container.clone());
					container
				}
			};
			msg.edit(
				self.serenity_context.http.clone(),
				edit_message_container(container),
			)
			.await?;
		}

		Ok(())
	}

	pub async fn update_info(
		&self,
		track: TrackData,
		song_play: GuildPlay,
		mut receiver: Receiver<TrackSignal>,
	) -> AResult<()> {
		let Some(handler_lock) = self.bot_data.music_manager.get(self.guild_id) else {
			return Ok(());
		};
		let queue_size = get_configured_songbird_handler(&handler_lock)
			.await
			.queue()
			.len();

		let channel_id = GenericChannelId::new(song_play.requested_channel.cast_unsigned());
		let message_id = MessageId::new(song_play.request_message_id.cast_unsigned());

		let author_name = song_play
			.requested_by
			.get_author_name(&self.serenity_context)
			.await?;

		let (thumbnail_section, action_row, visit_button) =
			Self::create_components(&author_name, message_id, &track, queue_size);

		let base_container = CreateContainer::new(vec![thumbnail_section])
			.add_component(separator())
			.accent_colour(Colour::RED);

		let full_container = base_container
			.clone()
			.add_component(action_row.clone())
			.add_component(separator())
			.add_component(visit_button.clone());

		channel_id
			.edit_message(
				&self.serenity_context.http,
				message_id,
				edit_message_container(full_container.clone()),
			)
			.await?;

		let message_id_copy = song_play.request_message_id.to_string();

		let mut lyrics_shown = false;
		let mut history_shown = false;

		let mut lyrics_container: Option<CreateContainer> = None;
		let mut history_embed: Option<CreateContainer> = None;

		let mut collector_stream = ComponentInteractionCollector::new(&self.serenity_context)
			.timeout(Duration::from_hours(1))
			.filter(move |interaction| {
				interaction
					.data
					.custom_id
					.starts_with(message_id_copy.as_str())
			})
			.stream();

		let track_guilds = get_matching_guild_plays(track.track_uuid, &self.bot_data.db).await?;

		loop {
			select! {
				Some(interaction) = collector_stream.next() => {
					self.handle_interaction(
						interaction,
						handler_lock.clone(),
						&mut lyrics_shown,
						&mut lyrics_container,
						&mut history_shown,
						&mut history_embed,
						&track,
						&track_guilds,
										&full_container,
						&action_row,
						song_play.requested_channel
					)
					.await?;
				},
				result = receiver.changed() => {
					match result {
						Err(err) => {
							error!("Sender dropped: {err}");
							break;
						}
						Ok(()) => {
							match *receiver.borrow() {
								TrackSignal::Ended(uuid) if uuid == track.track_uuid => break,
								TrackSignal::Disconnected => break,
									_ => {}
							}
						}
					}
				},
			}
		}

		let final_container = base_container.add_component(visit_button);

		channel_id
			.edit_message(
				&self.serenity_context.http,
				message_id,
				edit_message_container(final_container),
			)
			.await?;

		Ok(())
	}
}

#[async_trait]
impl VoiceEventHandler for PlaybackHandler {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::Track([(state, handle)]) = ctx {
			if state.playing == PlayMode::Play {
				if let Ok(guild_track) = get_track(handle.uuid(), &self.bot_data.db).await
					&& let Ok(song_play) = get_guild_play(
						handle.uuid(),
						i64::from(self.guild_id),
						i64::from(self.channel_id),
						&self.bot_data.db,
					)
					.await
				{
					let self_clone = self.clone();
					let track_end_rx = self_clone.track_watch.subscribe();
					spawn(async move {
						if let Err(err) = self_clone
							.update_info(guild_track, song_play, track_end_rx)
							.await
						{
							error!("Failed to update song info: {err}");
						}
					});
				}
			} else if state.playing == PlayMode::End || state.playing == PlayMode::Stop {
				if let Err(err) = self.track_watch.send(TrackSignal::Ended(handle.uuid())) {
					error!("Failed to broadcast track ending: {err}");
				}
			} else if let PlayMode::Errored(error) = &state.playing {
				error!("Failed to play track: {error}");
				counter!(METRICS.prefix_errors.clone()).increment(1);
				if let Ok(song_play) = get_guild_play(
					handle.uuid(),
					i64::from(self.guild_id),
					i64::from(self.channel_id),
					&self.bot_data.db,
				)
				.await && let Err(err) =
					GenericChannelId::new(song_play.requested_channel.cast_unsigned())
						.edit_message(
							&self.serenity_context.http,
							MessageId::new(song_play.request_message_id.cast_unsigned()),
							EditMessage::default()
								.content("Track errored on playback, try a different source :/"),
						)
						.await
				{
					error!("Failed to notify user about track error: {err}");
				}
			}
		}
		return None;
	}
}

pub enum TrackSignal {
	Ended(Uuid),
	Disconnected,
	Connected,
}

pub async fn add_voice_events(
	ctx: &SerenityContext,
	guild_id: GuildId,
	channel_id: GenericChannelId,
	handler_lock: Arc<Mutex<Call>>,
) {
	let mut handler = handler_lock.lock().await;

	let (tx, _rx) = watch::channel::<TrackSignal>(TrackSignal::Connected);

	let data: Arc<Data> = ctx.data();
	data.track_signals.insert(guild_id.get(), tx.clone());

	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Playable),
		PlaybackHandler::new(ctx.clone(), ctx.data(), guild_id, channel_id, tx.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::End),
		PlaybackHandler::new(ctx.clone(), ctx.data(), guild_id, channel_id, tx.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Error),
		PlaybackHandler::new(ctx.clone(), ctx.data(), guild_id, channel_id, tx),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::DriverDisconnect),
		DriverDisconnectHandler::new(ctx.data()),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::ClientDisconnect),
		ClientDisconnectHandler::new(ctx.clone(), channel_id),
	);
}

pub async fn get_configured_songbird_handler(
	handler_lock: &Arc<Mutex<Call>>,
) -> MutexGuard<'_, Call> {
	let mut handler = handler_lock.lock().await;
	handler.set_bitrate(Bitrate::Max);
	handler
}

pub async fn youtube_source(url: String) -> Option<YoutubeDl<'static>> {
	match Url::parse(&url) {
		Ok(parsed_url) => parsed_url
			.domain()
			.filter(|d| {
				*d == "youtube.com"
					|| *d == "www.youtube.com"
					|| *d == "youtu.be"
					|| *d == "m.youtube.com"
			})
			.map(|_| YoutubeDl::new(HTTP_CLIENT.clone(), url)),
		Err(_) => Some(YoutubeDl::new_search(HTTP_CLIENT.clone(), url)),
	}
}

pub async fn queue_song(
	metadata: AuxMetadata,
	audio: AudioStream<Box<dyn MediaSource>>,
	source: YoutubeDl<'static>,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: i64,
	data: Arc<Data>,
	message_id: i64,
	channel_id: i64,
	author_id: i64,
) -> AResult<()> {
	let uuid = metadata
		.source_url
		.as_ref()
		.map_or_else(Uuid::new_v4, |url| {
			Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())
		});

	insert_channel(guild_id, channel_id, &data.db).await?;
	insert_track(metadata, uuid, &data.db).await?;
	insert_guild_play(uuid, guild_id, channel_id, author_id, message_id, &data.db).await?;

	get_configured_songbird_handler(&handler_lock)
		.await
		.enqueue(Track::new_with_uuid(
			Input::Live(LiveInput::Raw(audio), Some(Box::new(source))),
			uuid,
		))
		.await;

	Ok(())
}

pub async fn join_container(ctx: &SContext<'_>) -> AResult<()> {
	let playback_info = "# I've joined the party!\n## Commands:\n
	- **/play_song**: *Queue a new song from a YouTube url or from a search*
	- **/seek_song**: *Seek song forward (e.g. +20) or backwards (e.g. -20)*
	- **/text_to_voice**: *Make the bot say smth either by providing an input or replying to a \
	                     message*
	- **/leave_voice**: *Make the bot leave the party*
	- **/add_youtube_playlist**: *Add songs in a YouTube-playlist*
	- **/add_deezer_playlist**: *Add songs in a Deezer-playlist*\n### NEW: *Set a music channel with \
	                     /configure_server_settings and I'll listen to your song requests there*";

	let text = [text_display(playback_info)];

	let container = CreateContainer::new(&text)
		.add_component(separator())
		.add_component(build_feedback_action_row())
		.accent_colour(Colour::GOLD);

	ctx.send(reply_container(container)).await?;

	Ok(())
}

pub async fn join_handler(
	music_manager: &Arc<Songbird>,
	guild_id: GuildId,
	channel_id: ChannelId,
) -> AResult<Arc<Mutex<Call>>> {
	let handler_lock = match music_manager.join(guild_id, channel_id).await {
		Ok(lock) => lock,
		Err(err) => {
			return Err(err.into());
		}
	};

	Ok(handler_lock)
}

pub async fn voice_channel_id(ctx: SContext<'_>) -> AResult<ChannelId> {
	let Some(channel_id) = ctx.guild().and_then(|guild| {
		guild
			.voice_states
			.get(&ctx.author().id)
			.and_then(|voice_state| voice_state.channel_id)
	}) else {
		ctx.reply(EMPTY_VOICE_CHAN_MSG).await?;
		bail!("User tried to join in empty voice channel");
	};

	Ok(channel_id)
}

pub async fn voice_channel(ctx: SContext<'_>, guild_id: GuildId) -> AResult<Arc<Mutex<Call>>> {
	let channel_id = voice_channel_id(ctx).await?;
	let handler_lock = match join_handler(&ctx.data().music_manager, guild_id, channel_id).await {
		Ok(lock) => lock,
		Err(err) => {
			ctx.reply("I don't wanna join").await?;
			return Err(err);
		}
	};
	Ok(handler_lock)
}

pub async fn try_voice(ctx: SContext<'_>, guild_id: GuildId) -> AResult<Arc<Mutex<Call>>> {
	let handler_lock = if let Some(lock) = ctx.data().music_manager.get(guild_id) {
		lock
	} else {
		match voice_channel(ctx, guild_id).await {
			Ok(lock) => {
				join_container(&ctx).await?;
				add_voice_events(
					ctx.serenity_context(),
					guild_id,
					ctx.channel_id(),
					lock.clone(),
				)
				.await;
				lock
			}
			Err(voice_err) => {
				bail!("{voice_err}");
			}
		}
	};

	set_current_voice_channel(
		i64::from(guild_id),
		i64::from(ctx.channel_id().expect_channel()),
		&ctx.data().db,
	)
	.await?;

	Ok(handler_lock)
}

pub async fn remove_handler(ctx: SContext<'_>, guild_id: GuildId) -> AResult<()> {
	if ctx.data().music_manager.remove(guild_id).await.is_err() {
		ctx.reply(NOT_IN_VOICE_CHAN_MSG).await?;
		return Err(MusicError::NotInVoiceChan.into());
	}

	query!(
		r#"
		UPDATE guild_settings
		SET current_voice_channel = NULL
		WHERE guild_id = $1
		"#,
		i64::from(guild_id),
	)
	.execute(&ctx.data().db)
	.await?;

	Ok(())
}

pub async fn insert_track(metadata: AuxMetadata, uuid: Uuid, conn: &Pool<Postgres>) -> AResult<()> {
	query!(
		r#"
    	INSERT INTO tracks (track_uuid, title, artist, source_url, duration_sec, thumbnail_url)
   		VALUES ($1, $2, $3, $4, $5, $6)
    	ON CONFLICT (track_uuid) 
    	DO NOTHING
    	"#,
		uuid,
		metadata.title,
		metadata.artist,
		metadata.source_url,
		metadata.duration.map(|d| d.as_secs().cast_signed()),
		metadata.thumbnail
	)
	.execute(conn)
	.await?;

	Ok(())
}

pub struct TrackData {
	pub track_uuid: Uuid,
	pub title: Option<String>,
	pub artist: Option<String>,
	pub source_url: Option<String>,
	pub duration_sec: Option<i64>,
	pub thumbnail_url: Option<String>,
	pub last_seen: OffsetDateTime,
	pub first_seen: OffsetDateTime,
}

pub async fn get_track(uuid: Uuid, conn: &Pool<Postgres>) -> AResult<TrackData> {
	let track = query_as!(
		TrackData,
		r#"
    	SELECT * FROM tracks
    	WHERE track_uuid = $1
    	"#,
		uuid,
	)
	.fetch_one(conn)
	.await?;

	Ok(track)
}

pub async fn insert_guild_play(
	uuid: Uuid,
	guild_id: i64,
	channel_id: i64,
	author_id: i64,
	message_id: i64,
	conn: &Pool<Postgres>,
) -> AResult<()> {
	query!(
		r#"
    	INSERT INTO song_plays (track_uuid, guild_id, requested_by, requested_channel, request_message_id)
   		VALUES ($1, $2, $3, $4, $5)
    	"#,
		uuid,
		guild_id,
		author_id,
		channel_id,
		message_id
	)
	.execute(conn)
	.await?;

	Ok(())
}

type DBUserID = Option<i64>;

#[async_trait]
pub trait DBUserIDExt {
	async fn get_author_name(&self, serenity_context: &SerenityContext) -> AResult<String>;
}

#[async_trait]
impl DBUserIDExt for DBUserID {
	async fn get_author_name(&self, serenity_context: &SerenityContext) -> AResult<String> {
		let author_name = if let Some(user_id) = self.map(|u| UserId::new(u.cast_unsigned()))
			&& let Ok(user) = serenity_context.http.get_user(user_id).await
		{
			user.display_name().to_owned()
		} else {
			"Unknown".to_owned()
		};

		Ok(author_name)
	}
}

pub struct GuildPlay {
	pub play_id: i64,
	pub track_uuid: Uuid,
	pub guild_id: i64,
	pub requested_by: DBUserID,
	pub requested_channel: i64,
	pub request_message_id: i64,
	pub played_at: OffsetDateTime,
}

pub async fn get_guild_play(
	uuid: Uuid,
	guild_id: i64,
	channel_id: i64,
	conn: &Pool<Postgres>,
) -> AResult<GuildPlay> {
	let track = query_as!(
		GuildPlay,
		r#"
        SELECT * FROM song_plays
        WHERE track_uuid = $1
          AND guild_id = $2
          AND requested_channel = $3
        ORDER BY played_at DESC
        LIMIT 1
        "#,
		uuid,
		guild_id,
		channel_id,
	)
	.fetch_one(conn)
	.await?;

	Ok(track)
}

pub async fn get_matching_guild_plays(uuid: Uuid, conn: &Pool<Postgres>) -> AResult<Vec<i64>> {
	let track_guilds = query_scalar!(
		r#"
    	SELECT DISTINCT guild_id FROM song_plays
    	WHERE track_uuid = $1
        LIMIT 10
    	"#,
		uuid,
	)
	.fetch_all(conn)
	.await?;

	Ok(track_guilds)
}

pub struct ChannelPlayHistory {
	pub play_id: i64,
	pub played_at: OffsetDateTime,
	pub requested_by: DBUserID,
	pub track_uuid: Uuid,
	pub title: Option<String>,
	pub artist: Option<String>,
	pub source_url: Option<String>,
	pub duration_sec: Option<i64>,
	pub thumbnail_url: Option<String>,
}

pub async fn get_queue_history(
	channel_id: i64,
	conn: &Pool<Postgres>,
) -> AResult<Vec<ChannelPlayHistory>> {
	let queue_history = query_as!(
		ChannelPlayHistory,
		r#"
        SELECT 
            sp.play_id,
            sp.played_at,
            sp.requested_by,
            t.track_uuid,
            t.title,
            t.artist,
            t.source_url,
            t.duration_sec,
            t.thumbnail_url
        FROM song_plays sp
        JOIN tracks t ON sp.track_uuid = t.track_uuid
        WHERE sp.requested_channel = $1
        ORDER BY sp.played_at DESC
        LIMIT 25
        "#,
		channel_id
	)
	.fetch_all(conn)
	.await?;

	Ok(queue_history)
}

pub async fn rejoin_voice(
	ctx: &SerenityContext,
	conn: &Pool<Postgres>,
	music_manager: &Arc<Songbird>,
) -> AResult<()> {
	let persistent_voice_channels = query!(
		r#"
		SELECT guild_id, current_voice_channel FROM guild_settings
		WHERE current_voice_channel IS NOT NULL
		"#
	)
	.fetch_all(conn)
	.await?;

	sleep(Duration::from_secs(5)).await;

	for record in persistent_voice_channels {
		let guild_id = GuildId::new(record.guild_id.cast_unsigned());
		let channel_id =
			GenericChannelId::new(record.current_voice_channel.unwrap().cast_unsigned());
		let handler_lock =
			match join_handler(music_manager, guild_id, channel_id.expect_channel()).await {
				Ok(lock) => lock,
				Err(err) => {
					log_error(
						"# Failed to rejoin voice channel",
						err.to_string(),
						ctx,
						METRICS.voice_join_errors.clone(),
					)
					.await;
					continue;
				}
			};
		add_voice_events(ctx, guild_id, channel_id, handler_lock).await;
	}

	Ok(())
}

pub async fn setup_lavalink(host: String, password: String, bot_id: LavaUserId) -> LavalinkClient {
	let events = Events {
		..Default::default()
	};

	let node_local = NodeBuilder {
		hostname: host,
		is_ssl: false,
		events: Events::default(),
		password,
		user_id: bot_id,
		session_id: None,
	};

	LavalinkClient::new(
		events,
		vec![node_local],
		NodeDistributionStrategy::round_robin(),
	)
	.await
}

pub async fn lavalink_join(ctx: SContext<'_>, guild_id: GuildId) -> AResult<()> {
	let channel_id = voice_channel_id(ctx).await?;
	let connection_info = ctx
		.data()
		.music_manager
		.join_gateway(guild_id, channel_id)
		.await?
		.0;
	ctx.data()
		.lavalink_client
		.create_player_context(guild_id, connection_info)
		.await?;
	join_container(&ctx).await?;
	Ok(())
}

pub async fn lavalink_delete(ctx: SContext<'_>, guild_id: GuildId) -> AResult<()> {
	ctx.data().lavalink_client.delete_player(guild_id).await?;
	Ok(())
}

pub async fn lavalink_play(ctx: SContext<'_>, guild_id: GuildId, input: String) -> AResult<()> {
	let lava_client = ctx.data().lavalink_client.clone();
	let Some(player) = lava_client.get_player_context(guild_id) else {
		ctx.reply(NOT_IN_VOICE_CHAN_MSG).await?;
		return Err(MusicError::NotInVoiceChan.into());
	};
	let query = if input.starts_with("https") {
		input
	} else {
		match SearchEngines::YouTube.to_query(&input) {
			Ok(resp) => resp,
			Err(err) => {
				ctx.reply(INVALID_TRACK_SOURCE).await?;
				bail!("{err}");
			}
		}
	};
	let loaded_tracks = lava_client.load_tracks(guild_id, &query).await?;

	let tracks: Vec<TrackInQueue> = match loaded_tracks.data {
		Some(TrackLoadData::Track(track)) => vec![TrackInQueue::from(track)],
		Some(TrackLoadData::Search(search)) => {
			vec![TrackInQueue::from(search.first().unwrap().clone())]
		}
		Some(TrackLoadData::Playlist(playlist)) => playlist
			.tracks
			.iter()
			.map(|track| TrackInQueue::from(track.clone()))
			.collect(),
		Some(TrackLoadData::Error(err)) => {
			bail!("{}:{}:{}", err.severity, err.message, err.cause)
		}
		_ => {
			bail!("Failed to load track");
		}
	};

	let queue = player.get_queue();
	if let Err(err) = queue.append(VecDeque::from(tracks)) {
		ctx.reply(FAILED_SONG_FETCH).await?;
		bail!("{err}");
	}

	if let Ok(player_data) = player.get_player().await
		&& player_data.track.is_none()
		&& queue.get_track(0).await.is_ok_and(|x| x.is_some())
	{
		player.skip()?;
	}

	ctx.reply("Song playing").await?;

	Ok(())
}
