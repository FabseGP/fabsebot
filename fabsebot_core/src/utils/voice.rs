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
	hook,
	model::{
		UserId as LavaUserId, client::NodeDistributionStrategy, events, search::SearchEngines,
		track::TrackLoadData,
	},
	node::NodeBuilder,
	player_context::{PlayerContext, TrackInQueue},
};
use metrics::counter;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, to_value};
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
	input::{Compose as _, Input, LiveInput, YoutubeDl, cached::Compressed},
	tracks::{LoopState, PlayMode, Track},
};
use sqlx::{
	Error, Pool, Postgres, postgres::PgQueryResult, query, query_as, query_scalar,
	types::time::OffsetDateTime,
};
use tokio::{
	select,
	sync::{Mutex, mpsc, watch::Receiver},
	time::sleep,
};
use tracing::{error, warn};
use url::Url;
use uuid::Uuid;

use crate::{
	config::{
		constants::{EMPTY_VOICE_CHAN_MSG, FAILED_SONG_FETCH, MESSAGE_LIMIT, QUEUEING_MSG},
		types::{
			Data, GuildCache, HTTP_CLIENT, MusicQueue, MusicQueueData, SContext, UsersMap,
			bot_context,
		},
	},
	events::interaction::build_feedback_action_row,
	log_error,
	stats::counters::METRICS,
	utils::helpers::{
		edit_message_container, get_lyrics, get_user, guild_cache, reply_container, separator,
		text_display, thumbnail_section, visit_page_button,
	},
};

const ALREADY_IN_VOICE_CHAN_MSG: &str =
	"Bruh I'm already in a voice channel!\nUse leave_voice-command if I should leave the channel";

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
			if let Err(err) = self
				.bot_data
				.music_manager
				.remove(disconnect_data.guild_id)
				.await
			{
				error!("Failed to remove call: {err}");
			}
			let guild_cache = self
				.bot_data
				.guilds
				.get(&GuildId::from(disconnect_data.guild_id.get()))
				.unwrap();
			if let Err(err) = guild_cache
				.music_data
				.connection_signals
				.0
				.send(ConnectionStatus::Disconnected)
			{
				error!("Failed to notify about disconnect: {err}");
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
					if let Err(err) = self
						.channel_id
						.send_message(
							&self.serenity_context.http,
							CreateMessage::default()
								.content(format!("Bye {}", user.display_name())),
						)
						.await
					{
						error!("Failed to send message: {err}");
					}
				}
				Err(err) => {
					warn!("Failed to fetch user: {err}");
				}
			}
		}
		None
	}
}

fn create_components<'a>(
	author_name: &'a str,
	msg_id: MessageId,
	metadata: &'a TrackPlayData,
	queue_size: usize,
	payload_type: &PayloadType,
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

	let (primary_len, additional_len) = if *payload_type == PayloadType::Song {
		(5, 4)
	} else if *payload_type == PayloadType::Lavalink {
		(5, 3)
	} else if *payload_type == PayloadType::Custom {
		(1, 2)
	} else {
		(1, 1)
	};

	let mut primary_buttons = Vec::with_capacity(primary_len);

	primary_buttons.push(
		CreateButton::new(format!("{msg_id}_p"))
			.style(ButtonStyle::Primary)
			.label("Pause/Unpause"),
	);

	if *payload_type == PayloadType::Song || *payload_type == PayloadType::Lavalink {
		primary_buttons.push(
			CreateButton::new(format!("{msg_id}_c"))
				.style(ButtonStyle::Primary)
				.label("Stop & clear queue"),
		);
		primary_buttons.push(
			CreateButton::new(format!("{msg_id}_s"))
				.style(ButtonStyle::Primary)
				.label("Skip"),
		);
		primary_buttons.push(
			CreateButton::new(format!("{msg_id}_f"))
				.style(ButtonStyle::Primary)
				.label("Seek forward 10s"),
		);
		primary_buttons.push(
			CreateButton::new(format!("{msg_id}_b"))
				.style(ButtonStyle::Primary)
				.label("Seek backwards 10s"),
		);
	}

	let primary_row =
		CreateContainerComponent::ActionRow(CreateActionRow::buttons(primary_buttons));

	let mut additional_buttons = Vec::with_capacity(additional_len);

	if *payload_type == PayloadType::Song || *payload_type == PayloadType::Custom {
		additional_buttons.push(
			CreateButton::new(format!("{msg_id}_r"))
				.style(ButtonStyle::Secondary)
				.label("Enable/Disable loop"),
		);
	}
	additional_buttons.push(
		CreateButton::new(format!("{msg_id}_h"))
			.style(ButtonStyle::Secondary)
			.label("Show/Hide song history"),
	);

	if metadata.title.is_some() {
		additional_buttons.push(
			CreateButton::new(format!("{msg_id}_l"))
				.style(ButtonStyle::Secondary)
				.label("Show/Hide lyrics"),
		);
		additional_buttons.push(visit_page_button(url));
	}

	(thumbnail_section, primary_row, additional_buttons)
}

async fn pause_song(
	handler_lock: Option<Arc<Mutex<Call>>>,
	lavalink_context: Option<&PlayerContext>,
) -> AResult<()> {
	if let Some(handler_lock) = handler_lock {
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
				warn!("Failed to get track info. {err}");
			}
		}
	} else {
		let context = lavalink_context.unwrap();
		let player_info = context.get_player().await?;
		context.set_pause(!player_info.paused).await?;
	}

	Ok(())
}

async fn clear_queue(
	handler_lock: Option<Arc<Mutex<Call>>>,
	lavalink_context: Option<&PlayerContext>,
) -> AResult<()> {
	if let Some(handler_lock) = handler_lock {
		handler_lock.lock().await.queue().stop();
	} else {
		let context = lavalink_context.unwrap();
		context.get_queue().clear()?;
		context.stop_now().await?;
	}

	Ok(())
}

async fn skip_song(
	handler_lock: Option<Arc<Mutex<Call>>>,
	lavalink_context: Option<&PlayerContext>,
) -> AResult<()> {
	if let Some(handler_lock) = handler_lock {
		let handler = handler_lock.lock().await;
		let queue = handler.queue();
		if queue.len() > 1 {
			queue.skip()?;
		}
	} else {
		let context = lavalink_context.unwrap();
		let queue_size = context.get_queue().get_count().await?;
		if queue_size > 0 {
			context.skip()?;
		}
	}

	Ok(())
}

async fn loop_song(handler_lock: Option<Arc<Mutex<Call>>>) -> AResult<()> {
	if let Some(handler_lock) = handler_lock {
		let Some(current_track) = handler_lock.lock().await.queue().current() else {
			return Ok(());
		};
		match current_track.get_info().await.map(|t| t.loops) {
			Ok(loops) => {
				if loops == LoopState::Infinite {
					current_track.disable_loop()?;
				} else {
					current_track.enable_loop()?;
				}
			}
			Err(err) => {
				warn!("Failed to get track info. {err}");
			}
		}
	}
	Ok(())
}

fn fetch_context(
	bot_data: Arc<Data>,
	guild_id: GuildId,
) -> (Option<Arc<Mutex<Call>>>, Option<PlayerContext>) {
	bot_data
		.lavalink_client
		.get_player_context(guild_id)
		.map_or_else(
			|| {
				bot_data
					.music_manager
					.get(guild_id)
					.map_or_else(|| (None, None), |handler_lock| (Some(handler_lock), None))
			},
			|context| (None, Some(context)),
		)
}

async fn seek_song(
	handler_lock: Option<Arc<Mutex<Call>>>,
	lavalink_context: Option<&PlayerContext>,
	seek_type: SeekType,
	song_duration: i64,
) -> AResult<()> {
	let seek_amount = Duration::from_secs(10);
	if let Some(handler_lock) = handler_lock {
		let Some(current_track) = handler_lock.lock().await.queue().current() else {
			return Ok(());
		};
		let song_info = current_track.get_info().await?;
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
	} else {
		let context = lavalink_context.unwrap();
		let player_info = context.get_player().await?;
		if let Some(track) = player_info.track {
			let current_position = Duration::from_millis(player_info.state.position);
			let track_duration = Duration::from_millis(track.info.length);
			let new_duration = if seek_type == SeekType::Forward {
				current_position
					.saturating_add(seek_amount)
					.min(track_duration)
			} else if seek_type == SeekType::Backwards {
				current_position.saturating_sub(seek_amount)
			} else {
				warn!("Unknown seek type");
				return Ok(());
			};
			context.set_position(new_duration).await?;
		}
	}

	Ok(())
}

async fn handle_interaction<'a>(
	interaction: ComponentInteraction,
	handler_lock: Option<Arc<Mutex<Call>>>,
	lavalink_context: Option<&PlayerContext>,
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
	serenity_context: &SerenityContext,
	guild_id: GuildId,
) -> AResult<()> {
	interaction.defer(&serenity_context.http).await?;

	let mut msg = interaction.message;

	let bot_data: Arc<Data> = serenity_context.data();

	if interaction.data.custom_id.ends_with('s') {
		skip_song(handler_lock, lavalink_context).await?;
		if let Some(track_guilds) = track_guilds {
			for guild_id in track_guilds {
				let (handler_lock, lavalink_context) =
					fetch_context(bot_data.clone(), GuildId::from(guild_id.cast_unsigned()));
				skip_song(handler_lock, lavalink_context.as_ref()).await?;
			}
		}
	} else if interaction.data.custom_id.ends_with('p') {
		pause_song(handler_lock, lavalink_context).await?;
		if let Some(track_guilds) = track_guilds {
			for guild_id in track_guilds {
				let (handler_lock, lavalink_context) =
					fetch_context(bot_data.clone(), GuildId::from(guild_id.cast_unsigned()));
				pause_song(handler_lock, lavalink_context.as_ref()).await?;
			}
		}
	} else if interaction.data.custom_id.ends_with('c') {
		clear_queue(handler_lock, lavalink_context).await?;
		if let Some(track_guilds) = track_guilds {
			for guild_id in track_guilds {
				let (handler_lock, lavalink_context) =
					fetch_context(bot_data.clone(), GuildId::from(guild_id.cast_unsigned()));
				clear_queue(handler_lock, lavalink_context.as_ref()).await?;
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
					&& let Some(lyrics) = get_lyrics(serenity_context, title, artist).await
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
			serenity_context.http.clone(),
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
				let queue_history = get_queue_history(i64::from(guild_id), &bot_data.db).await?;
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
							.get_author_name(&serenity_context.http, users)
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
			serenity_context.http.clone(),
			edit_message_container(container),
		)
		.await?;
	} else if interaction.data.custom_id.ends_with('b')
		&& let Some(duration) = track.duration_sec
	{
		seek_song(
			handler_lock,
			lavalink_context,
			SeekType::Backwards,
			duration,
		)
		.await?;
		if let Some(track_guilds) = track_guilds {
			for guild_id in track_guilds {
				let (handler_lock, lavalink_context) =
					fetch_context(bot_data.clone(), GuildId::from(guild_id.cast_unsigned()));
				seek_song(
					handler_lock,
					lavalink_context.as_ref(),
					SeekType::Backwards,
					duration,
				)
				.await?;
			}
		}
	} else if interaction.data.custom_id.ends_with('f')
		&& let Some(duration) = track.duration_sec
	{
		seek_song(handler_lock, lavalink_context, SeekType::Forward, duration).await?;
		if let Some(track_guilds) = track_guilds {
			for guild_id in track_guilds {
				let (handler_lock, lavalink_context) =
					fetch_context(bot_data.clone(), GuildId::from(guild_id.cast_unsigned()));
				seek_song(
					handler_lock,
					lavalink_context.as_ref(),
					SeekType::Forward,
					duration,
				)
				.await?;
			}
		}
	} else if interaction.data.custom_id.ends_with('r') {
		loop_song(handler_lock).await?;
		if let Some(track_guilds) = track_guilds {
			for guild_id in track_guilds {
				let (handler_lock, _lavalink_context) =
					fetch_context(bot_data.clone(), GuildId::from(guild_id.cast_unsigned()));
				loop_song(handler_lock).await?;
			}
		}
	}

	Ok(())
}

async fn update_info(
	queue_data: Arc<QueueData>,
	mut track_receiver: Receiver<TrackSignal>,
	mut status_receiver: Receiver<ConnectionStatus>,
	track_uuid: Option<Uuid>,
	track_identifier: Option<String>,
	serenity_context: SerenityContext,
	guild_id: GuildId,
) -> AResult<()> {
	let bot_data: Arc<Data> = serenity_context.data();
	let (handler_lock, lavalink_context) = fetch_context(bot_data.clone(), guild_id);

	let queue_size = if let Some(context) = &lavalink_context {
		context.get_queue().get_count().await?
	} else {
		handler_lock.as_ref().unwrap().lock().await.queue().len()
	};

	let track_data = &queue_data.track_data;

	let channel_id = GenericChannelId::new(track_data.requested_channel.cast_unsigned());
	let message_id = MessageId::new(track_data.request_message_id.cast_unsigned());

	let author_name = track_data
		.requested_by
		.get_author_name(&serenity_context.http, &bot_data.users)
		.await;

	let (thumbnail_section, primary_row, additional_buttons) = create_components(
		&author_name,
		message_id,
		track_data,
		queue_size,
		&queue_data.payload_type,
	);

	let mut base_container = CreateContainer::new(vec![thumbnail_section])
		.add_component(separator())
		.accent_colour(Colour::RED);

	let secondary_row =
		CreateContainerComponent::ActionRow(CreateActionRow::buttons(additional_buttons.clone()));

	let full_container = base_container
		.clone()
		.add_component(primary_row.clone())
		.add_component(separator())
		.add_component(secondary_row.clone());

	channel_id
		.edit_message(
			&serenity_context.http,
			message_id,
			edit_message_container(full_container.clone()),
		)
		.await?;

	let message_id_copy = track_data.request_message_id.to_string();

	let mut lyrics_shown = false;
	let mut history_shown = false;

	let mut lyrics_container: Option<CreateContainer> = None;
	let mut history_embed: Option<CreateContainer> = None;

	let mut collector_stream = ComponentInteractionCollector::new(&serenity_context)
		.timeout(Duration::from_hours(1))
		.filter(move |interaction| {
			interaction
				.data
				.custom_id
				.starts_with(message_id_copy.as_str())
		})
		.stream();

	let track_guilds = if let Some(track_uuid) = track_uuid
		&& let Some(guild_cache) = bot_data.guilds.get(&guild_id)
		&& guild_cache.music_data.global.load(Ordering::Relaxed)
	{
		Some(get_matching_guild_plays(track_uuid, i64::from(guild_id), &bot_data.db).await?)
	} else {
		None
	};

	loop {
		select! {
			interaction = collector_stream.next() => {
				match interaction {
					Some(interaction) => {
						handle_interaction(
							interaction,
							handler_lock.clone(),
							lavalink_context.as_ref(),
							&mut lyrics_shown,
							&mut lyrics_container,
							&mut history_shown,
							&mut history_embed,
							track_data,
							track_guilds.as_ref(),
							&full_container,
							&primary_row,
							&secondary_row,
							&bot_data.users,
							&serenity_context,
							guild_id
						)
						.await?;
					}
					None => {
						break;
					}
				}
			},
			result = track_receiver.changed() => {
				match result {
					Err(err) => {
						error!("Sender dropped: {err}");
						break;
					}
					Ok(()) => {
						match &*track_receiver.borrow_and_update() {
							TrackSignal::Ended(uuid)
								if let Some(track_uuid) = track_uuid
									&& uuid == &track_uuid =>
							{
								break;
							}
							TrackSignal::Finished(identifier)
								if let Some(ref track_identifier) = track_identifier
									&& identifier == track_identifier =>
							{
								break;
							}
							_ => {}
						}
					}
				}
			},
			result = status_receiver.changed() => {
				match result {
					Err(err) => {
						error!("Sender dropped: {err}");
						break;
					}
					Ok(()) => {
						if *status_receiver.borrow_and_update() == ConnectionStatus::Disconnected {
							break;
						}
					}
				}
			},
		}
	}

	if queue_data.payload_type == PayloadType::Song
		|| queue_data.payload_type == PayloadType::Lavalink
	{
		let visit_button = vec![additional_buttons.last().unwrap().clone()];
		base_container = base_container.add_component(CreateContainerComponent::ActionRow(
			CreateActionRow::buttons(visit_button),
		));
	}

	channel_id
		.edit_message(
			&serenity_context.http,
			message_id,
			edit_message_container(base_container),
		)
		.await?;

	Ok(())
}

#[derive(Clone)]
struct PlaybackHandler {
	serenity_context: SerenityContext,
	guild_id: GuildId,
	music_queue: MusicQueue,
}

#[derive(PartialEq)]
enum SeekType {
	Forward,
	Backwards,
}

impl PlaybackHandler {
	const fn new(
		serenity_context: SerenityContext,
		guild_id: GuildId,
		music_queue: MusicQueue,
	) -> Self {
		Self {
			serenity_context,
			guild_id,
			music_queue,
		}
	}
}

#[async_trait]
impl VoiceEventHandler for PlaybackHandler {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::Track(tracks) = ctx {
			let bot_data: Arc<Data> = self.serenity_context.data();
			for (state, handle) in *tracks {
				let queue_data: Arc<QueueData> = handle.data();
				if let PlayMode::Errored(error) = &state.playing
					&& queue_data.first_error.swap(false, Ordering::Relaxed)
				{
					counter!(METRICS.music_queue_errors.clone()).increment(1);
					log_error(
						&format!("# Failed to play track\n{error}"),
						&self.serenity_context,
					)
					.await;
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
				} else if queue_data.payload_type != PayloadType::TextToVoice {
					if state.playing == PlayMode::Play {
						if queue_data.first_play.swap(false, Ordering::Relaxed) {
							let track_uuid = handle.uuid();
							if let Err(err) = self
								.music_queue
								.send((queue_data, Some(track_uuid), None))
								.await
							{
								error!("Failed to queue track: {err}");
							}
						}
					} else if state.playing == PlayMode::End || state.playing == PlayMode::Stop {
						let guild_cache = bot_data.guilds.get(&self.guild_id).unwrap();
						if let Err(err) = guild_cache
							.music_data
							.track_signals
							.0
							.send(TrackSignal::Ended(handle.uuid()))
						{
							error!("Failed to broadcast track ending: {err}");
						}
					}
				}
			}
		}
		return None;
	}
}

pub enum TrackSignal {
	Ended(Uuid),
	Finished(String),
	Idle,
}

#[derive(PartialEq, Eq)]
pub enum ConnectionStatus {
	Disconnected,
	SongbirdConnected,
	LavalinkConnected,
}

async fn add_voice_events(
	ctx: &SerenityContext,
	guild_id: GuildId,
	channel_id: GenericChannelId,
	handler_lock: Arc<Mutex<Call>>,
	global: bool,
	guild_cache: Arc<GuildCache>,
) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();

	guild_cache
		.music_data
		.global
		.store(global, Ordering::Relaxed);

	guild_cache
		.music_data
		.connection_signals
		.0
		.send(ConnectionStatus::SongbirdConnected)?;

	let mut handler = handler_lock.lock().await;

	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Play),
		PlaybackHandler::new(ctx.clone(), guild_id, guild_cache.music_data.queue.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::End),
		PlaybackHandler::new(ctx.clone(), guild_id, guild_cache.music_data.queue.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Error),
		PlaybackHandler::new(ctx.clone(), guild_id, guild_cache.music_data.queue.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::DriverDisconnect),
		DriverDisconnectHandler::new(bot_data),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::ClientDisconnect),
		ClientDisconnectHandler::new(ctx.clone(), channel_id),
	);

	Ok(())
}

#[must_use]
fn youtube_source(url: &str) -> bool {
	Url::parse(url).is_ok_and(|parsed_url| {
		parsed_url.domain().is_some_and(|d| {
			d == "youtube.com" || d == "www.youtube.com" || d == "youtu.be" || d == "m.youtube.com"
		})
	})
}

#[derive(PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum PayloadType {
	Song,
	Lavalink,
	Custom,
	TextToVoice,
}

#[derive(Serialize, Deserialize)]
pub struct QueueData {
	track_data: TrackPlayData,
	first_play: AtomicBool,
	first_error: AtomicBool,
	payload_type: PayloadType,
}

impl Clone for QueueData {
	fn clone(&self) -> Self {
		Self {
			track_data: self.track_data.clone(),
			first_play: AtomicBool::new(self.first_play.load(Ordering::Relaxed)),
			first_error: AtomicBool::new(self.first_error.load(Ordering::Relaxed)),
			payload_type: self.payload_type.clone(),
		}
	}
}

pub async fn add_payload(
	ctx: &SContext<'_>,
	handler_lock: Arc<Mutex<Call>>,
	payload: Bytes,
	payload_type: PayloadType,
	guild_id: GuildId,
) -> AResult<()> {
	let reply = ctx.reply("Payload queued").await?;
	let msg = reply.message().await?;

	let queue_data = QueueData {
		track_data: TrackPlayData {
			requested_channel: i64::from(msg.channel_id),
			request_message_id: i64::from(msg.id),
			requested_by: i64::from(ctx.author().id),
			..Default::default()
		},
		first_error: AtomicBool::new(true),
		first_play: AtomicBool::new(true),
		payload_type,
	};

	let input = Input::from(payload);
	let compressed = Compressed::new(input, Bitrate::Max).await?;
	let new_input = Input::from(compressed.new_handle());

	enqueue(
		queue_data.clone(),
		new_input,
		handler_lock,
		i64::from(guild_id),
		None,
	)
	.await?;

	global_queue(guild_id, ctx, compressed, queue_data).await?;

	Ok(())
}

async fn enqueue(
	queue_data: QueueData,
	input: Input,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: i64,
	pool: Option<&Pool<Postgres>>,
) -> AResult<()> {
	let uuid = queue_data
		.track_data
		.source_url
		.as_ref()
		.map_or_else(Uuid::new_v4, |url| {
			Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())
		});

	if let Some(pool) = pool {
		insert_guild_play(uuid, &queue_data, guild_id, pool).await?;
	}

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
	- **/play_song**: *Queue a song or playlist from YouTube with an url OR search for a song*
	- **/play_song_old**: *Old implementation, prone to blocking from YouTube*
	- **/play_file**: *Queue a custom audio file*
	- **/text_to_voice**: *Make the bot say smth either by providing an input or replying to a \
	                     message*
	- **/leave_voice**: *Make the bot leave the party*\n### NEW: *Set a music channel with \
	                     /configure_server_settings and I'll listen to your song requests there*";

	let text = [text_display(playback_info)];

	let container = CreateContainer::new(&text)
		.add_component(separator())
		.add_component(build_feedback_action_row())
		.accent_colour(Colour::GOLD);

	ctx.send(reply_container(container)).await?;

	Ok(())
}

async fn configure_handler(handler_lock: Arc<Mutex<Call>>) {
	handler_lock.lock().await.set_bitrate(Bitrate::Max);
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
	configure_handler(handler_lock.clone()).await;

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

pub async fn check_in_channel(ctx: SContext<'_>) -> AResult<GuildId> {
	let guild_id = ctx.guild_id().unwrap();
	if ctx.data().music_manager.get(guild_id).is_some() {
		ctx.reply(ALREADY_IN_VOICE_CHAN_MSG).await?;
		bail!("");
	}
	Ok(guild_id)
}

pub async fn try_voice(
	ctx: SContext<'_>,
	global: bool,
) -> AResult<(Option<Typing>, GuildId, Arc<Mutex<Call>>)> {
	let typing = ctx.defer_or_broadcast().await?;
	let guild_id = ctx.guild_id().unwrap();
	let player_context_opt = ctx
		.data()
		.lavalink_client
		.get_player_context(guild_id)
		.is_none();
	let handler_lock = if let Some(lock) = ctx.data().music_manager.get(guild_id)
		&& player_context_opt
	{
		lock
	} else {
		if !player_context_opt {
			ctx.data().lavalink_client.delete_player(guild_id).await?;
		}
		let handler_lock = voice_channel(ctx, guild_id).await?;
		let guild_cache = guild_cache(
			ctx.data(),
			guild_id,
			ctx.author().id.get().cast_signed(),
			ctx.serenity_context(),
		)
		.await?;
		if *guild_cache.music_data.connection_signals.1.borrow() == ConnectionStatus::Disconnected {
			join_container(&ctx).await?;
		}
		add_voice_events(
			ctx.serenity_context(),
			guild_id,
			ctx.channel_id(),
			handler_lock.clone(),
			global,
			guild_cache.clone(),
		)
		.await?;
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

pub async fn remove_handler(ctx: &SerenityContext, guild_id: GuildId) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();

	let guild_cache = bot_data.guilds.get(&guild_id).unwrap();

	let tx = guild_cache.music_data.connection_signals.0.clone();

	if !tx.is_closed() {
		tx.send(ConnectionStatus::Disconnected)?;
	}

	bot_data.music_manager.remove(guild_id).await?;

	if bot_data
		.lavalink_client
		.get_player_context(guild_id)
		.is_some()
	{
		bot_data.lavalink_client.delete_player(guild_id).await?;
	}

	if guild_cache.music_data.global.load(Ordering::Relaxed) {
		query!(
			r#"
				UPDATE guild_settings
				SET GLOBAL_CALL = FALSE
				WHERE guild_id = $1
				"#,
			i64::from(guild_id),
		)
		.execute(&bot_data.db)
		.await?;
	}

	Ok(())
}

#[derive(Default, Clone, Serialize, Deserialize)]
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
	let events = events::Events {
		track_start: Some(track_start),
		track_end: Some(track_end),
		player_update: Some(player_update),
		..Default::default()
	};

	let node_local = NodeBuilder {
		hostname: host,
		is_ssl: false,
		events: events::Events::default(),
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

pub async fn lavalink_try_join(
	ctx: &SerenityContext,
	guild_id: GuildId,
	author_id: UserId,
	poise_ctx: Option<SContext<'_>>,
) -> AResult<(Option<Typing>, PlayerContext)> {
	let bot_data: Arc<Data> = ctx.data();
	let typing = if let Some(poise_ctx) = poise_ctx {
		poise_ctx.defer_or_broadcast().await?
	} else {
		None
	};
	let player_context = if let Some(player) = bot_data.lavalink_client.get_player_context(guild_id)
		&& let Some(guild_cache) = bot_data.guilds.get(&guild_id)
		&& *guild_cache.music_data.connection_signals.1.borrow()
			== ConnectionStatus::LavalinkConnected
		&& bot_data.music_manager.get(guild_id).is_some()
	{
		player
	} else {
		if bot_data.music_manager.get(guild_id).is_some() {
			remove_handler(ctx, guild_id).await?;
			sleep(Duration::from_secs(5)).await;
		}
		let channel_id = if let Some(poise_ctx) = poise_ctx {
			voice_channel_id(poise_ctx).await?
		} else {
			let voice_state = guild_id.get_user_voice_state(&ctx.http, author_id).await?;
			if let Some(channel_id) = voice_state.channel_id {
				channel_id
			} else {
				bail!("Unknown voice channel");
			}
		};
		let (connection_info, handler_lock) = bot_data
			.music_manager
			.join_gateway(guild_id, channel_id)
			.await?;
		configure_handler(handler_lock).await;
		let guild_cache = guild_cache(
			bot_data.clone(),
			guild_id,
			author_id.get().cast_signed(),
			ctx,
		)
		.await?;
		if *guild_cache.music_data.connection_signals.1.borrow() == ConnectionStatus::Disconnected
			&& let Some(poise_ctx) = poise_ctx
		{
			join_container(&poise_ctx).await?;
		}
		guild_cache
			.music_data
			.connection_signals
			.0
			.send(ConnectionStatus::LavalinkConnected)?;
		bot_data
			.lavalink_client
			.create_player_context(guild_id, connection_info)
			.await?
	};

	Ok((typing, player_context))
}

pub async fn lavalink_play(
	ctx: &SerenityContext,
	guild_id: GuildId,
	msg_id: i64,
	channel_id: i64,
	author_id: i64,
	input: &str,
	player: PlayerContext,
) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();
	let lava_client = bot_data.lavalink_client.clone();
	let query = if youtube_source(input) {
		if input.contains("playlist?list=") {
			input
		} else {
			let clean_url = input.split_once("&pp=").map_or(input, |(b, _)| b);
			&SearchEngines::YouTube.to_query(clean_url)?
		}
	} else {
		&SearchEngines::YouTube.to_query(input)?
	};
	let loaded_tracks = lava_client.load_tracks(guild_id, query).await?;

	let mut tracks: Vec<TrackInQueue> = match loaded_tracks.data {
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
			bail!("{}:{}:{}", err.severity, err.message, err.cause);
		}
		_ => {
			bail!("Failed to load track: {input}");
		}
	};

	for track in &mut tracks {
		let track_info = track.track.info.clone();
		let duration = Duration::from_millis(track_info.length);
		let queue_data = QueueData {
			track_data: TrackPlayData {
				title: Some(track_info.title),
				artist: Some(track_info.author),
				source_url: track_info.uri,
				duration_sec: Some(duration.as_secs().cast_signed()),
				thumbnail_url: track_info.artwork_url,
				requested_by: author_id,
				requested_channel: channel_id,
				request_message_id: msg_id,
			},
			first_play: AtomicBool::new(true),
			first_error: AtomicBool::new(true),
			payload_type: PayloadType::Lavalink,
		};
		let json = to_value(queue_data)?;
		track.track.user_data = Some(json);
	}

	let queue = player.get_queue();
	queue.append(VecDeque::from(tracks))?;

	if let Ok(player_data) = player.get_player().await
		&& player_data.track.is_none()
		&& queue.get_track(0).await.is_ok_and(|x| x.is_some())
	{
		player.skip()?;
	}

	Ok(())
}

#[hook]
pub async fn track_start(_client: LavalinkClient, _session_id: String, event: &events::TrackStart) {
	let bot_data: Arc<Data> = bot_context().data();
	let guild_cache = bot_data
		.guilds
		.get(&GuildId::from(event.guild_id.0))
		.unwrap();
	if let Some(track_data) = event.track.user_data.as_ref()
		&& let Ok(queue_data) = from_value(track_data.clone())
		&& let Err(err) = guild_cache
			.music_data
			.queue
			.send((
				Arc::new(queue_data),
				None,
				Some(event.track.info.identifier.clone()),
			))
			.await
	{
		error!("Failed to send track data: {err}");
	}
}

#[hook]
pub async fn track_end(_client: LavalinkClient, _session_id: String, event: &events::TrackEnd) {
	let bot_data: Arc<Data> = bot_context().data();
	let guild_cache = bot_data
		.guilds
		.get(&GuildId::from(event.guild_id.0))
		.unwrap();
	if let Err(err) = guild_cache
		.music_data
		.track_signals
		.0
		.send(TrackSignal::Finished(event.track.info.identifier.clone()))
	{
		error!("Failed to broadcast track ending: {err}");
	}
}

#[hook]
pub async fn player_update(
	client: LavalinkClient,
	_session_id: String,
	event: &events::PlayerUpdate,
) {
	if !event.state.connected {
		let bot_data: Arc<Data> = bot_context().data();
		let guild_id = GuildId::from(event.guild_id.0);
		let guild_cache = bot_data.guilds.get(&guild_id).unwrap();
		if *guild_cache.music_data.connection_signals.1.borrow()
			== ConnectionStatus::LavalinkConnected
			&& bot_data.music_manager.get(guild_id).is_some()
			&& client.get_player_context(event.guild_id).is_some()
		{
			if let Err(err) = bot_data.music_manager.remove(guild_id).await {
				error!("Failed to remove call: {err}");
			}
			if let Err(err) = client.delete_player(event.guild_id).await {
				error!("Failed to delete player: {err}");
			}
			if let Err(err) = guild_cache
				.music_data
				.connection_signals
				.0
				.send(ConnectionStatus::Disconnected)
			{
				error!("Failed to notify about disconnection: {err}");
			}
		}
	}
}

async fn global_queue(
	guild_id: GuildId,
	ctx: &SContext<'_>,
	compressed: Compressed,
	mut queue_data: QueueData,
) -> AResult<()> {
	let guild_cache = ctx.data().guilds.get(&guild_id).unwrap();
	if guild_cache.music_data.global.load(Ordering::Relaxed) {
		let guild_global_playback: Vec<u64> = ctx
			.data()
			.guilds
			.iter()
			.filter(|t| t.music_data.global.load(Ordering::Relaxed) && *t.key() != guild_id.get())
			.map(|t| t.key().get())
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
				if let Err(err) = enqueue(
					queue_data.clone(),
					input,
					global_handler_lock,
					global_guild.cast_signed(),
					Some(&ctx.data().db),
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
	let mut src = if youtube_source(&url) {
		YoutubeDl::new(HTTP_CLIENT.clone(), url)
	} else {
		YoutubeDl::new_search(HTTP_CLIENT.clone(), url)
	};
	let audio = src.create_async().await?;
	let metadata = src.aux_metadata().await?;
	let input = Input::Live(LiveInput::Raw(audio), Some(Box::new(src)));
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
		first_error: AtomicBool::new(true),
		payload_type: PayloadType::Song,
	};

	enqueue(
		queue_data.clone(),
		new_input,
		handler_lock,
		i64::from(guild_id),
		Some(conn),
	)
	.await?;

	if let Some(ctx) = ctx {
		global_queue(guild_id, ctx, compressed, queue_data).await?;
	}

	Ok(())
}

pub async fn music_task(
	mut rx: mpsc::Receiver<MusicQueueData>,
	ctx: SerenityContext,
	guild_id: GuildId,
) {
	let bot_data: Arc<Data> = ctx.data();
	let guild_cache = bot_data.guilds.get(&guild_id).unwrap();
	let track_watch = guild_cache.music_data.track_signals.0.subscribe();
	let connection_watch = guild_cache.music_data.connection_signals.0.subscribe();
	while let Some(data) = rx.recv().await {
		if let Err(err) = update_info(
			data.0,
			track_watch.clone(),
			connection_watch.clone(),
			data.1,
			data.2,
			ctx.clone(),
			guild_id,
		)
		.await
		{
			error!("Failed to update song info: {err}");
		}
	}
}
