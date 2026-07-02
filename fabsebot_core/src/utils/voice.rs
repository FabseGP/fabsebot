use std::{
	collections::VecDeque,
	fmt::Write as _,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
	time::Duration,
};

use anyhow::{Result as AResult, bail};
use bytes::Bytes;
use lavalink_rs::{
	client::LavalinkClient,
	error::LavalinkError,
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
		EditMessage, GenericChannelId, GuildId, Http, MessageId, UserId,
	},
	async_trait,
	builder::CreateContainerComponent,
	futures::StreamExt as _,
	http::Typing,
};
use songbird::{
	Call, CoreEvent, Event as SongBirdEvent, EventContext, EventHandler as VoiceEventHandler,
	Songbird, TrackEvent,
	driver::Bitrate,
	input::{Input, YoutubeDl, cached::Compressed},
	tracks::{PlayMode, Track},
};
use sqlx::{
	Error, Pool, Postgres, postgres::PgQueryResult, query, query_as, query_scalar,
	types::time::OffsetDateTime,
};
use tokio::{
	select, spawn,
	sync::{
		Mutex,
		watch::{self, Receiver},
	},
};
use tracing::{error, warn};
use url::Url;
use uuid::Uuid;

use crate::{
	config::{
		constants::{
			EMPTY_VOICE_CHAN_MSG, FAILED_SONG_FETCH, MESSAGE_LIMIT, NOT_IN_VOICE_CHAN_MSG,
			QUEUEING_MSG,
		},
		types::{Data, HTTP_CLIENT, SContext, UsersMap},
	},
	errors::commands::MusicError,
	events::interaction::build_feedback_action_row,
	stats::counters::METRICS,
	utils::helpers::{
		edit_message_container, get_lyrics, get_user, reply_container, separator, text_display,
		thumbnail_section, visit_page_button,
	},
};

const INVALID_TRACK_SOURCE: &str = "Only YouTube-links are supported";

#[derive(Clone)]
struct DriverDisconnectHandler {
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
			if let Some((_, (tx, _))) = self
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
struct ClientDisconnectHandler {
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
struct PlaybackHandler {
	serenity_context: SerenityContext,
	guild_id: GuildId,
}

#[derive(PartialEq)]
enum SeekType {
	Forward,
	Backwards,
}

impl PlaybackHandler {
	const fn new(serenity_context: SerenityContext, guild_id: GuildId) -> Self {
		Self {
			serenity_context,
			guild_id,
		}
	}

	fn create_components<'a>(
		author_name: &'a str,
		msg_id: MessageId,
		metadata: &'a TrackPlayData,
		queue_size: usize,
	) -> (
		CreateContainerComponent<'a>,
		CreateContainerComponent<'a>,
		Vec<CreateButton<'a>>,
	) {
		let title = metadata.title.as_ref().map_or("Unknown title", |t| t);
		let artist = metadata.artist.as_ref().map_or("Unknown artist", |a| a);
		let url = metadata.source_url.as_ref().map_or("Unknown source", |s| s);
		let thumbnail = metadata
			.thumbnail_url
			.as_ref()
			.map_or("https://c.tenor.com/gRnPiR82No4AAAAd/tenor.gif", |t| t);
		let duration = metadata.duration_sec.unwrap_or(0);

		let text = format!(
			"# {title}\n**Added by:** {author_name}\n**Artist:** {artist}\n**Duration:** \
			 {duration}s\n**Queue size:** {}",
			queue_size.saturating_sub(1)
		);

		let thumbnail_section = thumbnail_section(text, thumbnail);

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
			CreateButton::new(format!("{msg_id}_f"))
				.style(ButtonStyle::Primary)
				.label("Seek forward 10s"),
			CreateButton::new(format!("{msg_id}_b"))
				.style(ButtonStyle::Primary)
				.label("Seek backwards 10s"),
		]));

		let additional_buttons = vec![
			CreateButton::new(format!("{msg_id}_l"))
				.style(ButtonStyle::Secondary)
				.label("Show/Hide lyrics"),
			CreateButton::new(format!("{msg_id}_h"))
				.style(ButtonStyle::Secondary)
				.label("Show/Hide song history"),
			visit_page_button(url),
		];

		(thumbnail_section, primary_row, additional_buttons)
	}

	async fn pause_song(handler_lock: Arc<Mutex<Call>>) -> AResult<()> {
		let Some(current_track) = handler_lock.lock().await.queue().current() else {
			return Ok(());
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

		Ok(())
	}

	async fn seek_song(
		handler_lock: Arc<Mutex<Call>>,
		seek_type: SeekType,
		song_duration: i64,
	) -> AResult<()> {
		let Some(current_track) = handler_lock.lock().await.queue().current() else {
			return Ok(());
		};
		let song_info = current_track.get_info().await?;
		let seek_amount = Duration::from_secs(10);
		let target = if seek_type == SeekType::Forward {
			song_info
				.position
				.saturating_add(seek_amount)
				.min(Duration::from_secs(song_duration.cast_unsigned()))
		} else if seek_type == SeekType::Backwards {
			song_info.position.saturating_sub(seek_amount)
		} else {
			warn!("Unknown seek type");
			return Ok(());
		};
		current_track.seek_async(target).await?;
		Ok(())
	}

	async fn handle_interaction<'a>(
		&self,
		interaction: ComponentInteraction,
		handler_lock: Arc<Mutex<Call>>,
		lyrics_shown: &mut bool,
		lyrics_container: &mut Option<CreateContainer<'a>>,
		history_shown: &mut bool,
		history_container: &mut Option<CreateContainer<'a>>,
		track: &TrackPlayData,
		track_guilds: Option<&Vec<i64>>,
		container: &CreateContainer<'a>,
		primary_row: &CreateContainerComponent<'a>,
		secondary_row: &CreateContainerComponent<'a>,
		users: &UsersMap,
	) -> AResult<()> {
		interaction.defer(&self.serenity_context.http).await?;

		let mut msg = interaction.message;

		let bot_data: Arc<Data> = self.serenity_context.data();

		if interaction.data.custom_id.ends_with('s') {
			let handler = handler_lock.lock().await;
			let queue = handler.queue();
			if !queue.is_empty() {
				queue.skip()?;
				drop(handler);
				if let Some(track_guilds) = track_guilds {
					for guild_id in track_guilds {
						if let Some(handler_lock) = bot_data
							.music_manager
							.get(GuildId::from(guild_id.cast_unsigned()))
						{
							handler_lock.lock().await.queue().skip()?;
						}
					}
				}
			}
		} else if interaction.data.custom_id.ends_with('p') {
			Self::pause_song(handler_lock).await?;
			if let Some(track_guilds) = track_guilds {
				for guild_id in track_guilds {
					if let Some(handler_lock) = bot_data
						.music_manager
						.get(GuildId::from(guild_id.cast_unsigned()))
					{
						Self::pause_song(handler_lock).await?;
					}
				}
			}
		} else if interaction.data.custom_id.ends_with('c') {
			handler_lock.lock().await.queue().stop();
			if let Some(track_guilds) = track_guilds {
				for guild_id in track_guilds {
					if let Some(handler_lock) = bot_data
						.music_manager
						.get(GuildId::from(guild_id.cast_unsigned()))
					{
						handler_lock.lock().await.queue().stop();
					}
				}
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
						.add_component(primary_row.clone())
						.add_component(separator())
						.add_component(secondary_row.clone())
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
						get_queue_history(i64::from(self.guild_id), &bot_data.db).await?;
					let mut history_string = String::with_capacity(2048);
					writeln!(
						history_string,
						"# History of {} last played songs",
						queue_history.len()
					)?;
					for track in queue_history {
						if let Some(title) = track.title {
							let author_name = track
								.requested_by
								.get_author_name(&self.serenity_context.http, users)
								.await;
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
						.add_component(primary_row.clone())
						.add_component(separator())
						.add_component(secondary_row.clone())
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
		} else if interaction.data.custom_id.ends_with('b')
			&& let Some(duration) = track.duration_sec
		{
			Self::seek_song(handler_lock, SeekType::Backwards, duration).await?;
		} else if interaction.data.custom_id.ends_with('f')
			&& let Some(duration) = track.duration_sec
		{
			Self::seek_song(handler_lock, SeekType::Forward, duration).await?;
		}

		Ok(())
	}

	async fn update_info(
		&self,
		queue_data: Arc<QueueData>,
		mut receiver: Receiver<TrackSignal>,
		track_uuid: Uuid,
	) -> AResult<()> {
		let bot_data: Arc<Data> = self.serenity_context.data();
		let Some(handler_lock) = bot_data.music_manager.get(self.guild_id) else {
			bail!("Not in a voice channel?");
		};
		let queue_size = handler_lock.lock().await.queue().len();

		let track_data = &queue_data.track_data;

		let channel_id = GenericChannelId::new(track_data.requested_channel.cast_unsigned());
		let message_id = MessageId::new(track_data.request_message_id.cast_unsigned());

		let author_name = track_data
			.requested_by
			.get_author_name(&self.serenity_context.http, &bot_data.users)
			.await;

		let (thumbnail_section, primary_row, additional_buttons) =
			Self::create_components(&author_name, message_id, track_data, queue_size);

		let base_container = CreateContainer::new(vec![thumbnail_section])
			.add_component(separator())
			.accent_colour(Colour::RED);

		let secondary_row = CreateContainerComponent::ActionRow(CreateActionRow::buttons(
			additional_buttons.clone(),
		));

		let full_container = base_container
			.clone()
			.add_component(primary_row.clone())
			.add_component(separator())
			.add_component(secondary_row.clone());

		channel_id
			.edit_message(
				&self.serenity_context.http,
				message_id,
				edit_message_container(full_container.clone()),
			)
			.await?;

		let message_id_copy = track_data.request_message_id.to_string();

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

		let track_guilds = if let Some(is_global) = bot_data
			.track_signals
			.get(&self.guild_id.get())
			.map(|t| t.1)
			&& is_global
		{
			Some(
				get_matching_guild_plays(track_uuid, i64::from(self.guild_id), &bot_data.db)
					.await?,
			)
		} else {
			None
		};

		loop {
			select! {
				interaction = collector_stream.next() => {
					match interaction {
						Some(interaction) => {
							self.handle_interaction(
								interaction,
								handler_lock.clone(),
								&mut lyrics_shown,
								&mut lyrics_container,
								&mut history_shown,
								&mut history_embed,
								track_data,
								track_guilds.as_ref(),
								&full_container,
								&primary_row,
								&secondary_row,
								&bot_data.users
							)
							.await?;
						}
						None => {
							break;
						}
					}
				},
				result = receiver.changed() => {
					match result {
						Err(err) => {
							error!("Sender dropped: {err}");
							break;
						}
						Ok(()) => {
							match *receiver.borrow() {
								TrackSignal::Ended(uuid) if uuid == track_uuid => break,
								TrackSignal::Disconnected => break,
									_ => {}
							}
						}
					}
				},
			}
		}

		let visit_button = vec![additional_buttons.get(2).unwrap().clone()];
		let final_container = base_container.add_component(CreateContainerComponent::ActionRow(
			CreateActionRow::buttons(visit_button),
		));

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
		if let EventContext::Track(tracks) = ctx {
			let bot_data: Arc<Data> = self.serenity_context.data();
			for (state, handle) in *tracks {
				let queue_data: Arc<QueueData> = handle.data();
				if queue_data.payload_type != PayloadType::Song {
					continue;
				}
				if state.playing == PlayMode::Play {
					if queue_data.first_play.swap(false, Ordering::Relaxed) {
						let self_clone = self.clone();
						let track_watch = bot_data
							.track_signals
							.get(&self.guild_id.get())
							.unwrap()
							.0
							.subscribe();
						let track_uuid = handle.uuid();
						let queue_data_clone = queue_data.clone();
						spawn(async move {
							if let Err(err) = self_clone
								.update_info(queue_data_clone, track_watch, track_uuid)
								.await
							{
								error!("Failed to update song info: {err}");
							}
						});
					}
				} else if state.playing == PlayMode::End || state.playing == PlayMode::Stop {
					let track_watch = bot_data.track_signals.get(&self.guild_id.get()).unwrap();
					if let Err(err) = track_watch.0.send(TrackSignal::Ended(handle.uuid())) {
						error!("Failed to broadcast track ending: {err}");
					}
				} else if let PlayMode::Errored(error) = &state.playing {
					error!("Failed to play track: {error}");
					counter!(METRICS.prefix_errors.clone()).increment(1);
					let text_display = [text_display("# Track errored on playback :/")];
					let container =
						CreateContainer::new(&text_display).accent_colour(Colour::ORANGE);
					if let Err(err) = GenericChannelId::new(
						queue_data.track_data.requested_channel.cast_unsigned(),
					)
					.edit_message(
						&self.serenity_context.http,
						MessageId::new(queue_data.track_data.request_message_id.cast_unsigned()),
						edit_message_container(container),
					)
					.await
					{
						error!("Failed to notify user about track error: {err}");
					}
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
	global: bool,
) {
	let mut handler = handler_lock.lock().await;

	let (tx, _rx) = watch::channel::<TrackSignal>(TrackSignal::Connected);

	let bot_data: Arc<Data> = ctx.data();
	bot_data.track_signals.insert(guild_id.get(), (tx, global));

	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Play),
		PlaybackHandler::new(ctx.clone(), guild_id),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::End),
		PlaybackHandler::new(ctx.clone(), guild_id),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Error),
		PlaybackHandler::new(ctx.clone(), guild_id),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::DriverDisconnect),
		DriverDisconnectHandler::new(bot_data),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::ClientDisconnect),
		ClientDisconnectHandler::new(ctx.clone(), channel_id),
	);
}

#[must_use]
fn youtube_source(url: &str) -> bool {
	Url::parse(url).is_ok_and(|parsed_url| {
		parsed_url.domain().is_some_and(|d| {
			d == "youtube.com" || d == "www.youtube.com" || d == "youtu.be" || d == "m.youtube.com"
		})
	})
}

#[derive(PartialEq, Default, Clone)]
enum PayloadType {
	Song,
	#[default]
	Custom,
}

#[derive(Default)]
struct QueueData {
	track_data: TrackPlayData,
	first_play: AtomicBool,
	payload_type: PayloadType,
}

impl Clone for QueueData {
	fn clone(&self) -> Self {
		Self {
			track_data: self.track_data.clone(),
			first_play: AtomicBool::new(self.first_play.load(Ordering::Relaxed)),
			payload_type: self.payload_type.clone(),
		}
	}
}

pub async fn queue_payload(
	ctx: &SContext<'_>,
	handler_lock: Arc<Mutex<Call>>,
	payload: Bytes,
) -> AResult<()> {
	ctx.reply("Payload queued").await?;
	let queue_data = Arc::new(QueueData::default());
	handler_lock
		.lock()
		.await
		.enqueue(Track::new_with_data(Input::from(payload), queue_data))
		.await;

	Ok(())
}

async fn queue_song(
	queue_data: QueueData,
	input: Input,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: i64,
	pool: &Pool<Postgres>,
) -> AResult<()> {
	let uuid = queue_data
		.track_data
		.source_url
		.as_ref()
		.map_or_else(Uuid::new_v4, |url| {
			Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())
		});

	insert_guild_play(uuid, &queue_data, guild_id, pool).await?;

	handler_lock
		.lock()
		.await
		.enqueue(Track::new_with_uuid_and_data(
			input,
			uuid,
			Arc::new(queue_data),
		))
		.await;

	Ok(())
}

async fn join_container(ctx: &SContext<'_>) -> AResult<()> {
	let playback_info = "# I've joined the party!\n## Commands:\n
	- **/play_song**: *Queue a new song from a YouTube url or from a search*
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

async fn join_handler(
	music_manager: Arc<Songbird>,
	guild_id: GuildId,
	channel_id: ChannelId,
) -> AResult<Arc<Mutex<Call>>> {
	let handler_lock = match music_manager.join(guild_id, channel_id).await {
		Ok(lock) => lock,
		Err(err) => {
			return Err(err.into());
		}
	};
	handler_lock.lock().await.set_bitrate(Bitrate::Max);

	Ok(handler_lock)
}

async fn voice_channel_id(ctx: SContext<'_>) -> AResult<ChannelId> {
	let Some(channel_id) = ctx.guild().and_then(|guild| {
		guild
			.voice_states
			.get(&ctx.author().id)
			.and_then(|voice_state| voice_state.channel_id)
	}) else {
		ctx.reply(EMPTY_VOICE_CHAN_MSG).await?;
		bail!(EMPTY_VOICE_CHAN_MSG);
	};

	Ok(channel_id)
}

async fn voice_channel(ctx: SContext<'_>, guild_id: GuildId) -> AResult<Arc<Mutex<Call>>> {
	let channel_id = voice_channel_id(ctx).await?;
	let handler_lock =
		match join_handler(ctx.data().music_manager.clone(), guild_id, channel_id).await {
			Ok(lock) => lock,
			Err(err) => {
				ctx.reply("I don't wanna join").await?;
				return Err(err);
			}
		};
	Ok(handler_lock)
}

pub async fn try_voice(
	ctx: SContext<'_>,
	global: bool,
) -> AResult<(Option<Typing>, GuildId, Arc<Mutex<Call>>)> {
	let typing = ctx.defer_or_broadcast().await?;
	let guild_id = ctx.guild_id().unwrap();
	let handler_lock = if let Some(lock) = ctx.data().music_manager.get(guild_id) {
		lock
	} else {
		let handler_lock = voice_channel(ctx, guild_id).await?;
		join_container(&ctx).await?;
		add_voice_events(
			ctx.serenity_context(),
			guild_id,
			ctx.channel_id(),
			handler_lock.clone(),
			global,
		)
		.await;
		if global {
			query!(
				r#"
				UPDATE guild_settings
				SET GLOBAL_CALL = TRUE
				WHERE guild_id = $1
				"#,
				i64::from(guild_id),
			)
			.execute(&ctx.data().db)
			.await?;
		}
		handler_lock
	};

	Ok((typing, guild_id, handler_lock))
}

pub async fn remove_handler(ctx: SContext<'_>, guild_id: GuildId) -> AResult<()> {
	if ctx.data().music_manager.remove(guild_id).await.is_err() {
		ctx.reply(NOT_IN_VOICE_CHAN_MSG).await?;
		return Err(MusicError::NotInVoiceChan.into());
	}

	if let Some((_, (tx, is_global))) = ctx.data().track_signals.remove(&guild_id.get()) {
		if is_global {
			query!(
				r#"
				UPDATE guild_settings
				SET GLOBAL_CALL = FALSE
				WHERE guild_id = $1
				"#,
				i64::from(guild_id),
			)
			.execute(&ctx.data().db)
			.await?;
		}
		if !tx.is_closed() {
			tx.send(TrackSignal::Disconnected)?;
		}
	}

	Ok(())
}

#[derive(Default, Clone)]
struct TrackPlayData {
	title: Option<String>,
	artist: Option<String>,
	source_url: Option<String>,
	duration_sec: Option<i64>,
	thumbnail_url: Option<String>,
	requested_by: DBUserID,
	requested_channel: i64,
	request_message_id: i64,
}

async fn insert_guild_play(
	uuid: Uuid,
	queue_data: &QueueData,
	guild_id: i64,
	conn: &Pool<Postgres>,
) -> Result<PgQueryResult, Error> {
	query!(
		r#"
    	WITH ensured_track AS (
        	INSERT INTO tracks (track_uuid, title, artist, source_url, duration_sec, thumbnail_url)
        	VALUES ($1, $4, $5, $6, $7, $8)
			ON CONFLICT (track_uuid)
			DO UPDATE SET last_seen = NOW()
    	)
    	INSERT INTO song_plays (track_uuid, guild_id, requested_by)
    	VALUES ($1, $2, $3)
    	"#,
		uuid,
		guild_id,
		queue_data.track_data.requested_by,
		queue_data.track_data.title,
		queue_data.track_data.artist,
		queue_data.track_data.source_url,
		queue_data.track_data.duration_sec,
		queue_data.track_data.thumbnail_url
	)
	.execute(conn)
	.await
}

type DBUserID = i64;

#[async_trait]
trait DBUserIDExt {
	async fn get_author_name(&self, http: &Http, users: &UsersMap) -> String;
}

#[async_trait]
impl DBUserIDExt for DBUserID {
	async fn get_author_name(&self, http: &Http, users: &UsersMap) -> String {
		get_user(http, users, UserId::new(self.cast_unsigned()))
			.await
			.map_or_else(
				|_| "Unknown".to_owned(),
				|user| user.display_name().to_owned(),
			)
	}
}

async fn get_matching_guild_plays(
	uuid: Uuid,
	guild_id: i64,
	conn: &Pool<Postgres>,
) -> Result<Vec<i64>, Error> {
	query_scalar!(
		r#"
    	SELECT DISTINCT sp.guild_id
    	FROM song_plays sp
    	JOIN guild_settings gs ON gs.guild_id = sp.guild_id
    	WHERE sp.track_uuid = $1
    		AND sp.guild_id != $2
    		AND GLOBAL_CALL = TRUE
        LIMIT 10
    	"#,
		uuid,
		guild_id
	)
	.fetch_all(conn)
	.await
}

struct ChannelPlayHistory {
	played_at: OffsetDateTime,
	requested_by: DBUserID,
	title: Option<String>,
}

async fn get_queue_history(
	guild_id: i64,
	conn: &Pool<Postgres>,
) -> Result<Vec<ChannelPlayHistory>, Error> {
	query_as!(
		ChannelPlayHistory,
		r#"
        SELECT 
            sp.played_at,
            sp.requested_by,
            t.title
        FROM song_plays sp
        JOIN tracks t ON sp.track_uuid = t.track_uuid
        WHERE sp.guild_id = $1
        ORDER BY sp.played_at DESC
        LIMIT 25
        "#,
		guild_id
	)
	.fetch_all(conn)
	.await
}

pub async fn setup_lavalink(host: String, password: String, bot_id: LavaUserId) -> LavalinkClient {
	let events = Events::default();

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

pub async fn lavalink_delete(ctx: SContext<'_>, guild_id: GuildId) -> Result<(), LavalinkError> {
	ctx.data().lavalink_client.delete_player(guild_id).await
}

pub async fn lavalink_play(ctx: SContext<'_>, guild_id: GuildId, input: String) -> AResult<()> {
	let lava_client = ctx.data().lavalink_client.clone();
	let Some(player) = lava_client.get_player_context(guild_id) else {
		ctx.reply(NOT_IN_VOICE_CHAN_MSG).await?;
		return Err(MusicError::NotInVoiceChan.into());
	};
	let query = if Url::parse(&input).is_ok() {
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

async fn global_songs(
	guild_id: GuildId,
	ctx: &SContext<'_>,
	compressed: Compressed,
	mut queue_data: QueueData,
) -> AResult<()> {
	let Some(is_global) = ctx.data().track_signals.get(&guild_id.get()).map(|t| t.1) else {
		return Ok(());
	};

	if is_global {
		let guild_global_playback: Vec<u64> = ctx
			.data()
			.track_signals
			.iter()
			.filter(|t| t.1 && *t.key() != guild_id.get())
			.map(|t| *t.key())
			.collect();

		for global_guild in guild_global_playback {
			let Some(global_handler_lock) =
				ctx.data().music_manager.get(GuildId::new(global_guild))
			else {
				continue;
			};
			let Some(channel_id) = global_handler_lock.lock().await.current_channel() else {
				continue;
			};
			if let Ok(channel) = ctx
				.http()
				.get_channel(GenericChannelId::new(channel_id.get()))
				.await && let Some(guild_channel) = channel.guild()
			{
				let mut msg = guild_channel
					.send_message(ctx.http(), CreateMessage::default().content(QUEUEING_MSG))
					.await?;
				let input = Input::from(compressed.new_handle());
				queue_data.track_data.requested_channel = i64::from(msg.channel_id);
				queue_data.track_data.request_message_id = i64::from(msg.id);
				if let Err(err) = queue_song(
					queue_data.clone(),
					input,
					global_handler_lock,
					global_guild.cast_signed(),
					&ctx.data().db,
				)
				.await
				{
					warn!("Failed to queue global song: {err}");
					msg.edit(ctx.http(), EditMessage::new().content(FAILED_SONG_FETCH))
						.await?;
				}
			}
		}
	}

	Ok(())
}

pub async fn add_youtube_song(
	url: String,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: GuildId,
	msg_id: i64,
	channel_id: i64,
	author_id: i64,
	conn: &Pool<Postgres>,
	ctx: Option<&SContext<'_>>,
) -> AResult<()> {
	let src = if youtube_source(&url) {
		YoutubeDl::new(HTTP_CLIENT.clone(), url)
	} else {
		YoutubeDl::new_search(HTTP_CLIENT.clone(), url)
	};
	let mut input = Input::from(src);
	let metadata = input.aux_metadata().await?;
	let compressed = Compressed::new(input, Bitrate::Max).await?;
	let new_input = Input::from(compressed.new_handle());

	let queue_data = QueueData {
		track_data: TrackPlayData {
			title: metadata.title,
			artist: metadata.artist,
			source_url: metadata.source_url,
			duration_sec: metadata.duration.map(|d| d.as_secs().cast_signed()),
			thumbnail_url: metadata.thumbnail,
			requested_by: author_id,
			requested_channel: channel_id,
			request_message_id: msg_id,
		},
		first_play: AtomicBool::new(true),
		payload_type: PayloadType::Song,
	};

	queue_song(
		queue_data.clone(),
		new_input,
		handler_lock,
		i64::from(guild_id),
		conn,
	)
	.await?;

	if let Some(ctx) = ctx {
		global_songs(guild_id, ctx, compressed, queue_data).await?;
	}

	Ok(())
}

pub async fn add_playlist(
	ctx: SContext<'_>,
	guild_id: GuildId,
	urls: Vec<String>,
	handler_lock: Arc<Mutex<Call>>,
) -> AResult<()> {
	let reply = ctx.reply(QUEUEING_MSG).await?;
	let msg = reply.message().await?;
	let mut failed_songs: u32 = 0;
	let msg_id_i64 = i64::from(msg.id);
	let channel_id_i64 = i64::from(msg.channel_id);
	let author_id_i64 = i64::from(ctx.author().id);
	for url in urls {
		if let Err(err) = add_youtube_song(
			url,
			handler_lock.clone(),
			guild_id,
			msg_id_i64,
			channel_id_i64,
			author_id_i64,
			&ctx.data().db,
			Some(&ctx),
		)
		.await
		{
			warn!("{err}");
			failed_songs = failed_songs.saturating_add(1);
		}
	}
	if failed_songs != 0 {
		ctx.say(format!(
			"Couldn't queue {failed_songs} songs because of YouTube :/"
		))
		.await?;
	}

	Ok(())
}
