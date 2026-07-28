use std::{
	borrow::Cow,
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
use poise::ReplyHandle;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, to_value};
use serenity::{
	all::{
		ButtonStyle, ChannelId, Colour, ComponentInteraction, ComponentInteractionCollector,
		Context as SerenityContext, CreateActionRow, CreateButton, CreateContainer, CreateMessage,
		EditMessage, Error as SerenityError, GenericChannelId, GuildId, MessageId, UserId,
	},
	async_trait,
	builder::{CreateComponent, CreateContainerComponent, CreateSection},
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
		constants::{FAILED_SONG_FETCH, MESSAGE_LIMIT, QUEUEING_MSG},
		types::{
			ContextType, Data, GuildCache, HTTP_CLIENT, MusicQueue, MusicQueueData, SContext,
			bot_context, utils_config,
		},
	},
	events::interaction::FEEDBACK_BUTTON_CUSTOM_ID,
	log_error,
	stats::counters::METRICS,
	utils::helpers::{
		edit_message_container, get_lyrics, guild_cache, reply_container, separator,
		silent_message, text_display, thumbnail_section, visit_page_button,
	},
};

const EMPTY_VOICE_CHAN_MSG: &str = "No voice channel with at least 1 user found :/";
pub const ALREADY_IN_VOICE_CHAN_MSG: &str =
	"Bruh I'm already in a voice channel!\nUse leave_voice-command if I should leave the channel";

#[derive(Clone)]
struct DriverDisconnectHandler {
	music_manager: Arc<Songbird>,
	guild_cache: Arc<GuildCache>,
}

impl DriverDisconnectHandler {
	const fn new(music_manager: Arc<Songbird>, guild_cache: Arc<GuildCache>) -> Self {
		Self {
			music_manager,
			guild_cache,
		}
	}
}

#[async_trait]
impl VoiceEventHandler for DriverDisconnectHandler {
	async fn act(&self, event: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::DriverDisconnect(disconnect_data) = event
			&& self.guild_cache.music_data.is_songbird_connected()
		{
			self.guild_cache.music_data.disconnected();
			if let Err(err) = self.music_manager.remove(disconnect_data.guild_id).await {
				error!("Failed to remove call (songbird): {err}");
			}
		}
		None
	}
}

#[derive(Clone)]
struct ClientDisconnectHandler {
	channel_id: GenericChannelId,
}

impl ClientDisconnectHandler {
	const fn new(channel_id: GenericChannelId) -> Self {
		Self { channel_id }
	}
}

#[async_trait]
impl VoiceEventHandler for ClientDisconnectHandler {
	async fn act(&self, event: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::ClientDisconnect(client_data) = event
			&& let Err(err) = self
				.channel_id
				.send_message(
					&bot_context().http,
					silent_message(&format!("Bye <@{}>", client_data.user_id)),
				)
				.await
		{
			warn!("Failed to send message: {err}");
		}
		None
	}
}

fn create_components<'a>(
	author_id: UserId,
	metadata: &'a TrackPlayData,
	queue_size: usize,
	payload_type: &PayloadType,
) -> (
	CreateContainerComponent<'a>,
	CreateContainerComponent<'a>,
	Vec<CreateButton<'a>>,
) {
	let optional_data = metadata.optional_data.as_ref();

	let thumbnail_section = {
		let (text, thumbnail) = optional_data.map_or_else(
			|| {
				thumbnail_section(
					"# Unknown song data :/",
					"https://c.tenor.com/gRnPiR82No4AAAAd/tenor.gif",
				)
			},
			|optional_data| {
				thumbnail_section(
					format!(
						"# {}\n**Added by:** <@{author_id}>\n**Artist:** {}\n**Duration:** \
						 {}s\n**Queue size:** {}",
						optional_data.title.as_str(),
						optional_data.artist.as_str(),
						optional_data.duration_sec,
						queue_size.saturating_sub(1)
					),
					optional_data.thumbnail_url.as_str(),
				)
			},
		);
		CreateContainerComponent::Section(CreateSection::new(vec![text], thumbnail))
	};

	let (primary_len, additional_len) = match (payload_type, optional_data.is_some()) {
		(PayloadType::Song, true) => (5, 4),
		(PayloadType::Song, false) => (3, 2),
		(PayloadType::Lavalink, true) => (5, 3),
		(PayloadType::Lavalink, false) => (3, 1),
		(PayloadType::Custom, _) => (1, 2),
		_ => (1, 1),
	};

	let mut primary_buttons = Vec::with_capacity(primary_len);
	let mut additional_buttons = Vec::with_capacity(additional_len);

	primary_buttons.push(
		CreateButton::new("pause")
			.style(ButtonStyle::Primary)
			.label("Pause/Unpause"),
	);

	if *payload_type == PayloadType::Song || *payload_type == PayloadType::Lavalink {
		primary_buttons.push(
			CreateButton::new("clear")
				.style(ButtonStyle::Primary)
				.label("Stop & clear queue"),
		);
		primary_buttons.push(
			CreateButton::new("skip")
				.style(ButtonStyle::Primary)
				.label("Skip"),
		);
		if optional_data.is_some() {
			primary_buttons.push(
				CreateButton::new("forward")
					.style(ButtonStyle::Primary)
					.label("Seek forward 10s"),
			);
			primary_buttons.push(
				CreateButton::new("backwards")
					.style(ButtonStyle::Primary)
					.label("Seek backwards 10s"),
			);
		}
	}

	let primary_row =
		CreateContainerComponent::ActionRow(CreateActionRow::buttons(primary_buttons));

	if *payload_type == PayloadType::Song || *payload_type == PayloadType::Custom {
		additional_buttons.push(
			CreateButton::new("retry")
				.style(ButtonStyle::Secondary)
				.label("Enable/Disable loop"),
		);
	}

	additional_buttons.push(
		CreateButton::new("history")
			.style(ButtonStyle::Secondary)
			.label("Show/Hide song history"),
	);

	if let Some(url) = optional_data.map(|d| d.source_url.as_str()) {
		additional_buttons.push(
			CreateButton::new("lyrics")
				.style(ButtonStyle::Secondary)
				.label("Show/Hide lyrics"),
		);
		additional_buttons.push(visit_page_button(url));
	}

	(thumbnail_section, primary_row, additional_buttons)
}

enum PlayerAction {
	Pause,
	Skip,
	Clear,
	SeekForward(i64),
	SeekBackward(i64),
	Loop,
}

enum AudioBackend {
	Songbird(Arc<Mutex<Call>>),
	Lavalink(PlayerContext),
}

impl AudioBackend {
	async fn apply(&self, action: &PlayerAction) -> AResult<()> {
		match action {
			PlayerAction::Pause => self.pause_song().await,
			PlayerAction::Skip => self.skip_song().await,
			PlayerAction::Clear => self.clear_queue().await,
			PlayerAction::SeekForward(duration) => {
				self.seek_song(SeekType::Forward, *duration).await
			}
			PlayerAction::SeekBackward(duration) => {
				self.seek_song(SeekType::Backwards, *duration).await
			}
			PlayerAction::Loop => self.loop_song().await,
		}
	}

	async fn pause_song(&self) -> AResult<()> {
		match self {
			Self::Songbird(lock) => {
				let Some(current_track) = lock.lock().await.queue().current() else {
					return Ok(());
				};
				match current_track.get_info().await.map(|t| t.playing) {
					Ok(PlayMode::Pause) => current_track.play()?,
					Ok(PlayMode::Play) => current_track.pause()?,
					Err(err) => {
						warn!("Failed to get track info. {err}");
					}
					_ => {}
				}
			}
			Self::Lavalink(ctx) => {
				let player_info = ctx.get_player().await?;
				ctx.set_pause(!player_info.paused).await?;
			}
		}
		Ok(())
	}

	async fn clear_queue(&self) -> AResult<()> {
		match self {
			Self::Songbird(lock) => {
				lock.lock().await.queue().stop();
			}
			Self::Lavalink(ctx) => {
				ctx.get_queue().clear()?;
				ctx.stop_now().await?;
			}
		}
		Ok(())
	}

	async fn skip_song(&self) -> AResult<()> {
		match self {
			Self::Songbird(lock) => {
				lock.lock().await.queue().skip()?;
			}
			Self::Lavalink(ctx) => {
				let queue_size = ctx.get_queue().get_count().await?;
				if queue_size > 0 {
					ctx.skip()?;
				}
			}
		}
		Ok(())
	}

	async fn loop_song(&self) -> AResult<()> {
		match self {
			Self::Songbird(lock) => {
				let Some(current_track) = lock.lock().await.queue().current() else {
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
			Self::Lavalink(_ctx) => return Ok(()),
		}
		Ok(())
	}

	async fn seek_song(&self, seek_type: SeekType, song_duration: i64) -> AResult<()> {
		let seek_amount = Duration::from_secs(10);
		match self {
			Self::Songbird(lock) => {
				let Some(current_track) = lock.lock().await.queue().current() else {
					return Ok(());
				};
				let song_info = current_track.get_info().await?;
				let target = match seek_type {
					SeekType::Forward => song_info
						.position
						.saturating_add(seek_amount)
						.min(Duration::from_secs(song_duration.cast_unsigned())),
					SeekType::Backwards => song_info.position.saturating_sub(seek_amount),
				};
				current_track.seek_async(target).await?;
			}
			Self::Lavalink(ctx) => {
				let player_info = ctx.get_player().await?;
				if let Some(track) = player_info.track {
					let current_position = Duration::from_millis(player_info.state.position);
					let track_duration = Duration::from_millis(track.info.length);
					let new_duration = match seek_type {
						SeekType::Forward => current_position
							.saturating_add(seek_amount)
							.min(track_duration),
						SeekType::Backwards => current_position.saturating_sub(seek_amount),
					};
					ctx.set_position(new_duration).await?;
				}
			}
		}
		Ok(())
	}
}

fn fetch_context(bot_data: &Data, guild_id: GuildId) -> AudioBackend {
	bot_data
		.lavalink_client
		.get_player_context(guild_id)
		.map(AudioBackend::Lavalink)
		.or_else(|| {
			bot_data
				.music_manager
				.get(guild_id)
				.map(AudioBackend::Songbird)
		})
		.unwrap()
}

async fn apply_to_all_guilds(
	bot_data: &Data,
	track_guilds: &[i64],
	action: PlayerAction,
) -> AResult<()> {
	for &gid in track_guilds {
		let backend = fetch_context(bot_data, GuildId::from(gid.cast_unsigned()));
		backend.apply(&action).await?;
	}
	Ok(())
}

async fn handle_interaction<'a>(
	interaction: ComponentInteraction,
	lyrics_shown: &mut bool,
	lyrics_container: &mut Option<CreateContainer<'a>>,
	history_shown: &mut bool,
	history_container: &mut Option<CreateContainer<'a>>,
	track: &TrackPlayData,
	track_guilds: &[i64],
	container: &CreateContainer<'a>,
	primary_row: &CreateContainerComponent<'a>,
	secondary_row: &CreateContainerComponent<'a>,
	guild_id: GuildId,
) -> AResult<()> {
	let ctx = bot_context();
	interaction.defer(&ctx.http).await?;

	let mut msg = interaction.message;

	if interaction.data.custom_id == "skip" {
		apply_to_all_guilds(&ctx.data, track_guilds, PlayerAction::Skip).await?;
	} else if interaction.data.custom_id == "pause" {
		apply_to_all_guilds(&ctx.data, track_guilds, PlayerAction::Pause).await?;
	} else if interaction.data.custom_id == "clear" {
		apply_to_all_guilds(&ctx.data, track_guilds, PlayerAction::Clear).await?;
	} else if interaction.data.custom_id == "backwards" {
		apply_to_all_guilds(
			&ctx.data,
			track_guilds,
			PlayerAction::SeekBackward(track.optional_data.as_ref().unwrap().duration_sec),
		)
		.await?;
	} else if interaction.data.custom_id == "forward" {
		apply_to_all_guilds(
			&ctx.data,
			track_guilds,
			PlayerAction::SeekForward(track.optional_data.as_ref().unwrap().duration_sec),
		)
		.await?;
	} else if interaction.data.custom_id == "retry" {
		apply_to_all_guilds(&ctx.data, track_guilds, PlayerAction::Loop).await?;
	} else {
		if interaction.data.custom_id == "lyrics" {
			if *lyrics_shown {
				*lyrics_shown = false;
			} else if let Some(optional_data) = &track.optional_data {
				*lyrics_shown = true;
				*history_shown = false;
				if lyrics_container.is_none() {
					let lyrics = get_lyrics(&optional_data.title, &optional_data.artist).await;
					let mut text = String::with_capacity(lyrics.len().saturating_add(16));
					write!(text, "# Lyrics\n{lyrics}")?;
					text.truncate(MESSAGE_LIMIT);
					let text_display = vec![text_display(text)];
					let container = CreateContainer::new(text_display)
						.add_component(separator())
						.add_component(primary_row.clone())
						.add_component(separator())
						.add_component(secondary_row.clone())
						.accent_colour(Colour::BLUE);
					*lyrics_container = Some(container);
				}
			}
		} else {
			if *history_shown {
				*history_shown = false;
			} else {
				*history_shown = true;
				*lyrics_shown = false;
				if history_container.is_none() {
					let queue_history =
						get_queue_history(i64::from(guild_id), &ctx.data.db).await?;
					let mut history_string = String::with_capacity(MESSAGE_LIMIT);
					writeln!(
						history_string,
						"# History of {} last played songs",
						queue_history.len()
					)?;
					for track in queue_history {
						writeln!(
							history_string,
							"**{}:** *<@{}> - <t:{}:F>*",
							track.title,
							track.requested_by,
							track.played_at.unix_timestamp()
						)?;
					}
					history_string.truncate(MESSAGE_LIMIT);
					let text_display = vec![text_display(history_string)];
					let container = CreateContainer::new(text_display)
						.add_component(separator())
						.add_component(primary_row.clone())
						.add_component(separator())
						.add_component(secondary_row.clone())
						.accent_colour(Colour::BLUE);
					*history_container = Some(container);
				}
			}
		}
		let new_container = if *history_shown {
			history_container.as_ref().unwrap().clone()
		} else if *lyrics_shown {
			lyrics_container.as_ref().unwrap().clone()
		} else {
			container.clone()
		};
		msg.edit(
			ctx.http.clone(),
			edit_message_container(vec![CreateComponent::Container(new_container)]),
		)
		.await?;
	}
	Ok(())
}

async fn update_info(
	queue_data: &QueueData,
	mut track_receiver: Receiver<TrackSignal>,
	mut status_receiver: Receiver<ConnectionStatus>,
	guild_id: GuildId,
	serenity_context: &SerenityContext,
) -> AResult<()> {
	let bot_data: Arc<Data> = serenity_context.data();

	let queue_size = match &fetch_context(&bot_data, guild_id) {
		AudioBackend::Lavalink(ctx) => ctx.get_queue().get_count().await?,
		AudioBackend::Songbird(lock) => lock.lock().await.queue().len(),
	};

	let track_data = &queue_data.track_data;

	let (thumbnail_section, primary_row, additional_buttons) = create_components(
		track_data.requested_by,
		track_data,
		queue_size,
		&queue_data.payload_type,
	);

	let thumbnail_slice = [thumbnail_section];

	let mut base_container = CreateContainer::new(&thumbnail_slice)
		.add_component(separator())
		.accent_colour(Colour::RED);

	let secondary_row =
		CreateContainerComponent::ActionRow(CreateActionRow::buttons(additional_buttons.clone()));

	let full_container = base_container
		.clone()
		.add_component(primary_row.clone())
		.add_component(separator())
		.add_component(secondary_row.clone());

	let full_component = [CreateComponent::Container(full_container.clone())];

	track_data
		.requested_channel
		.edit_message(
			&serenity_context.http,
			track_data.request_message_id,
			edit_message_container(&full_component),
		)
		.await?;

	let mut lyrics_shown = false;
	let mut history_shown = false;

	let mut lyrics_container: Option<CreateContainer> = None;
	let mut history_embed: Option<CreateContainer> = None;

	let mut collector_stream = ComponentInteractionCollector::new(serenity_context)
		.timeout(Duration::from_hours(2))
		.message_id(track_data.request_message_id)
		.stream();

	let track_guilds = {
		let guild_id_i64 = i64::from(guild_id);
		let guild_cache = bot_data.guilds.get(&guild_id).unwrap();
		if guild_cache.music_data.global.load(Ordering::Relaxed) {
			Cow::Owned(
				get_matching_guild_plays(queue_data.track_data.uuid, guild_id_i64, &bot_data.db)
					.await?,
			)
		} else {
			Cow::Borrowed(&[guild_id_i64][..])
		}
	};

	let mut track_exception = false;

	loop {
		select! {
			interaction = collector_stream.next() => {
				match interaction {
					Some(interaction) => {
						handle_interaction(
							interaction,
							&mut lyrics_shown,
							&mut lyrics_container,
							&mut history_shown,
							&mut history_embed,
							track_data,
							&track_guilds,
							&full_container,
							&primary_row,
							&secondary_row,
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
				if result.is_ok() {
					match &*track_receiver.borrow() {
						TrackSignal::Exception =>
						{
							track_exception = true;
							break;
						}
						TrackSignal::Ended =>
						{
							break;
						}
						TrackSignal::Idle => {}
					}
				} else {
					break
				}
			},
			result = status_receiver.changed() => {
				if result.is_ok() {
					if *status_receiver.borrow() == ConnectionStatus::Disconnected {
						break
					}
				} else {
					break
				}
			},
		}
	}

	if track_exception {
		let text_display = [text_display("# Track errored on playback :/")];
		let container = CreateContainer::new(&text_display).accent_colour(Colour::ORANGE);
		let component = [CreateComponent::Container(container)];
		track_data
			.requested_channel
			.edit_message(
				&serenity_context.http,
				track_data.request_message_id,
				edit_message_container(&component),
			)
			.await?;
	} else if track_data.optional_data.is_some() {
		let visit_button = [additional_buttons.last().unwrap().clone()];
		base_container = base_container.add_component(CreateContainerComponent::ActionRow(
			CreateActionRow::buttons(&visit_button),
		));
		let component = [CreateComponent::Container(base_container)];
		track_data
			.requested_channel
			.edit_message(
				&serenity_context.http,
				track_data.request_message_id,
				edit_message_container(&component),
			)
			.await?;
	}

	Ok(())
}

async fn track_error(error: &str, guild_id: GuildId) {
	let guild_cache = bot_context().data.guilds.get(&guild_id).unwrap();
	if let Err(err) = guild_cache
		.music_data
		.track_signals
		.send(TrackSignal::Exception)
	{
		error!("Failed to broadcast track exception: {err}");
	}
	counter!(METRICS.music_queue_errors.as_str()).increment(1);
	log_error(format!("# Failed to play track\n{error}")).await;
}

#[derive(Clone)]
struct PlaybackHandler {
	music_queue: MusicQueue,
	guild_id: GuildId,
}

#[derive(PartialEq)]
enum SeekType {
	Forward,
	Backwards,
}

impl PlaybackHandler {
	const fn new(guild_id: GuildId, music_queue: MusicQueue) -> Self {
		Self {
			music_queue,
			guild_id,
		}
	}
}

#[async_trait]
impl VoiceEventHandler for PlaybackHandler {
	async fn act(&self, event: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::Track(tracks) = event {
			for (state, handle) in *tracks {
				let queue_data: Arc<QueueData> = handle.data();
				if let PlayMode::Errored(error) = &state.playing
					&& queue_data.first_error.swap(false, Ordering::Relaxed)
				{
					track_error(&error.to_string(), self.guild_id).await;
				} else if queue_data.payload_type != PayloadType::TextToVoice {
					if state.playing == PlayMode::Play {
						if queue_data.first_play.swap(false, Ordering::Relaxed)
							&& let Err(err) = self.music_queue.send(queue_data).await
						{
							error!("Failed to queue track: {err}");
						}
					} else if state.playing == PlayMode::End {
						notify_end(self.guild_id);
					}
				}
			}
		}
		return None;
	}
}

#[derive(PartialEq, Eq)]
pub enum TrackSignal {
	Ended,
	Exception,
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
	handler_lock: &Mutex<Call>,
	global: bool,
	guild_cache: Arc<GuildCache>,
) {
	let bot_data: Arc<Data> = ctx.data();

	guild_cache
		.music_data
		.global
		.store(global, Ordering::Relaxed);

	let mut handler = handler_lock.lock().await;

	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Play),
		PlaybackHandler::new(guild_id, guild_cache.music_data.queue.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::End),
		PlaybackHandler::new(guild_id, guild_cache.music_data.queue.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Error),
		PlaybackHandler::new(guild_id, guild_cache.music_data.queue.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::DriverDisconnect),
		DriverDisconnectHandler::new(bot_data.music_manager.clone(), guild_cache.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Core(CoreEvent::ClientDisconnect),
		ClientDisconnectHandler::new(channel_id),
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
	handler_lock: &Mutex<Call>,
	payload: Bytes,
	payload_type: PayloadType,
	guild_id: GuildId,
) -> AResult<()> {
	let reply = ctx.reply("Payload queued").await?;
	let msg = reply.message().await?;

	let queue_data = QueueData {
		track_data: TrackPlayData {
			requested_channel: msg.channel_id,
			request_message_id: msg.id,
			requested_by: ctx.author().id,
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

	global_queue(guild_id, compressed, queue_data).await?;

	Ok(())
}

fn track_uuid(url: Option<&String>) -> Uuid {
	url.as_ref().map_or_else(Uuid::new_v4, |url| {
		Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())
	})
}

async fn enqueue(
	queue_data: QueueData,
	input: Input,
	handler_lock: &Mutex<Call>,
	guild_id: i64,
	global_play_opt: Option<(&Pool<Postgres>, i64)>,
) -> AResult<()> {
	if let Some((pool, author_id)) = global_play_opt {
		insert_guild_play(&queue_data, guild_id, pool, author_id).await?;
	}

	handler_lock
		.lock()
		.await
		.enqueue(Track::new_with_uuid_and_data(
			input,
			queue_data.track_data.uuid,
			Arc::new(queue_data),
		))
		.await;

	Ok(())
}

async fn join_container<'a>(ctx: &SContext<'a>) -> Result<ReplyHandle<'a>, SerenityError> {
	let playback_info = "# I've joined the party!\n## Commands:\n
	- **/play_song**: *Queue a song or playlist from YouTube with an url OR search for a song*
	- **/play_song_old**: *Old implementation, prone to blocking from YouTube*
	- **/play_file**: *Queue a custom audio file*
	- **/text_to_voice**: *Make the bot say smth either by providing an input or replying to a \
	                     message*
	- **/leave_voice**: *Make the bot leave the party*\n### NEW: *Set a music channel with \
	                     /configure_server_settings and I'll listen to your song requests there*";

	let text = [text_display(playback_info)];

	let feedback_action_row =
		CreateContainerComponent::ActionRow(CreateActionRow::Buttons(Cow::Borrowed(&[
			CreateButton::new(FEEDBACK_BUTTON_CUSTOM_ID)
				.label(format!("Give feedback on {}", utils_config().bot_name))
				.style(ButtonStyle::Secondary),
		])));

	let container = CreateContainer::new(&text)
		.add_component(separator())
		.add_component(feedback_action_row)
		.accent_colour(Colour::GOLD);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await
}

async fn configure_handler(handler_lock: &Mutex<Call>) {
	handler_lock.lock().await.set_bitrate(Bitrate::Max);
}

async fn join_handler(
	music_manager: &Songbird,
	guild_id: GuildId,
	channel_id: ChannelId,
) -> Option<Arc<Mutex<Call>>> {
	let handler_lock = match music_manager.join(guild_id, channel_id).await {
		Ok(lock) => lock,
		Err(err) => {
			warn!("Failed to join voice channel: {err}");
			return None;
		}
	};
	configure_handler(&handler_lock).await;
	Some(handler_lock)
}

fn voice_channel_id(ctx: SContext<'_>) -> Option<ChannelId> {
	let guild = ctx.guild()?;
	guild
		.voice_states
		.get(&ctx.author().id)
		.and_then(|voice_state| voice_state.channel_id)
}

async fn voice_channel(ctx: SContext<'_>, guild_id: GuildId) -> Option<Arc<Mutex<Call>>> {
	let channel_id = voice_channel_id(ctx)?;
	let handler_lock = join_handler(&ctx.data().music_manager, guild_id, channel_id).await?;
	Some(handler_lock)
}

#[must_use]
pub fn check_in_channel(ctx: SContext<'_>, lavalink: bool) -> Option<GuildId> {
	let guild_id = ctx.guild_id()?;
	if ctx.data().music_manager.get(guild_id).is_some()
		&& (!lavalink
			|| ctx
				.data()
				.lavalink_client
				.get_player_context(guild_id)
				.is_some())
	{
		return None;
	}
	Some(guild_id)
}

pub async fn try_voice(
	ctx: SContext<'_>,
	global: bool,
) -> AResult<Option<(Option<Typing>, GuildId, Arc<Mutex<Call>>)>> {
	let typing = ctx.defer_or_broadcast().await?;
	let guild_id = ctx.guild_id().unwrap();
	let guild_cache = guild_cache(
		&ctx.data(),
		guild_id,
		Some(ctx.author().id.get().cast_signed()),
		ctx.serenity_context(),
	)
	.await?;
	let handler_lock = if let Some(lock) = ctx.data().music_manager.get(guild_id)
		&& guild_cache.music_data.is_songbird_connected()
	{
		lock
	} else {
		if guild_cache.music_data.is_disconnected() {
			join_container(&ctx).await?;
		} else if guild_cache.music_data.is_lavalink_connected() {
			ctx.data().lavalink_client.delete_player(guild_id).await?;
		}
		guild_cache
			.music_data
			.connected(ConnectionStatus::SongbirdConnected);
		let Some(handler_lock) = voice_channel(ctx, guild_id).await else {
			ctx.reply(EMPTY_VOICE_CHAN_MSG).await?;
			return Ok(None);
		};
		add_voice_events(
			ctx.serenity_context(),
			guild_id,
			ctx.channel_id(),
			&handler_lock,
			global,
			guild_cache,
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

	Ok(Some((typing, guild_id, handler_lock)))
}

pub async fn remove_handler(guild_id: GuildId) -> AResult<()> {
	let bot_data = bot_context().data.clone();

	let guild_cache = bot_data.guilds.get(&guild_id).unwrap();

	guild_cache.music_data.disconnected();

	bot_data.music_manager.remove(guild_id).await?;

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
struct OptionalTrackData {
	title: String,
	artist: String,
	source_url: String,
	duration_sec: i64,
	thumbnail_url: String,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct TrackPlayData {
	optional_data: Option<OptionalTrackData>,
	requested_by: UserId,
	requested_channel: GenericChannelId,
	request_message_id: MessageId,
	uuid: Uuid,
}

async fn insert_guild_play(
	queue_data: &QueueData,
	guild_id: i64,
	conn: &Pool<Postgres>,
	author_id: i64,
) -> Result<PgQueryResult, Error> {
	let optional_data = queue_data.track_data.optional_data.as_ref().unwrap();
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
		queue_data.track_data.uuid,
		guild_id,
		author_id,
		optional_data.title,
		optional_data.artist,
		optional_data.source_url,
		optional_data.duration_sec,
		optional_data.thumbnail_url
	)
	.execute(conn)
	.await
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
	requested_by: i64,
	title: String,
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
		track_exception: Some(track_exception),
		websocket_closed: Some(websocket_closed),
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
	ctx: ContextType<'_>,
	guild_id: GuildId,
	author_id: UserId,
) -> AResult<Option<(Option<Typing>, PlayerContext)>> {
	let (typing, bot_data, guild_cache) = {
		let author_id_i64 = author_id.get().cast_signed();
		match ctx {
			ContextType::Poise(ctx) => {
				let bot_data = ctx.data();
				(
					ctx.defer_or_broadcast().await?,
					bot_data.clone(),
					guild_cache(
						&bot_data,
						guild_id,
						Some(author_id_i64),
						ctx.serenity_context(),
					)
					.await?,
				)
			}
			ContextType::Serenity(ctx) => {
				let bot_data: Arc<Data> = ctx.data();
				(
					None,
					bot_data.clone(),
					guild_cache(&bot_data, guild_id, Some(author_id_i64), ctx).await?,
				)
			}
		}
	};

	let player_context = if let Some(context) =
		bot_data.lavalink_client.get_player_context(guild_id)
		&& guild_cache.music_data.is_lavalink_connected()
	{
		context
	} else {
		let channel_id = match ctx {
			ContextType::Poise(ctx) => {
				let Some(channel_id) = voice_channel_id(ctx) else {
					ctx.reply(EMPTY_VOICE_CHAN_MSG).await?;
					return Ok(None);
				};
				channel_id
			}
			ContextType::Serenity(ctx) => {
				let voice_state = guild_id.get_user_voice_state(&ctx.http, author_id).await?;
				voice_state.channel_id.unwrap()
			}
		};
		if bot_data.music_manager.get(guild_id).is_some() {
			remove_handler(guild_id).await?;
			sleep(Duration::from_secs(5)).await;
		} else if let ContextType::Poise(poise_ctx) = ctx {
			join_container(&poise_ctx).await?;
		}
		let (connection_info, handler_lock) = bot_data
			.music_manager
			.join_gateway(guild_id, channel_id)
			.await?;
		configure_handler(&handler_lock).await;
		guild_cache
			.music_data
			.connected(ConnectionStatus::LavalinkConnected);
		bot_data
			.lavalink_client
			.create_player_context(guild_id, connection_info)
			.await?
	};

	Ok(Some((typing, player_context)))
}

pub async fn lavalink_play(
	ctx: &SerenityContext,
	guild_id: GuildId,
	msg_id: MessageId,
	channel_id: GenericChannelId,
	author_id: UserId,
	input: &str,
	player: PlayerContext,
	pool: &Pool<Postgres>,
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
			vec![TrackInQueue::from(search.into_iter().next().unwrap())]
		}
		Some(TrackLoadData::Playlist(playlist)) => playlist
			.tracks
			.into_iter()
			.map(TrackInQueue::from)
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
		let uuid = track_uuid(track_info.uri.as_ref());
		let optional_data = if let Some(source_url) = track_info.uri
			&& let Some(thumbnail_url) = track_info.artwork_url
		{
			Some(OptionalTrackData {
				title: track_info.title,
				artist: track_info.author,
				source_url,
				duration_sec: duration.as_secs().cast_signed(),
				thumbnail_url,
			})
		} else {
			None
		};
		let queue_data = QueueData {
			track_data: TrackPlayData {
				optional_data,
				requested_by: author_id,
				requested_channel: channel_id,
				request_message_id: msg_id,
				uuid,
			},
			first_play: AtomicBool::new(true),
			first_error: AtomicBool::new(true),
			payload_type: PayloadType::Lavalink,
		};
		insert_guild_play(
			&queue_data,
			guild_id.get().cast_signed(),
			pool,
			i64::from(author_id),
		)
		.await?;
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
async fn track_start(_client: LavalinkClient, _session_id: String, event: &events::TrackStart) {
	let guild_cache = bot_context()
		.data
		.guilds
		.get(&GuildId::from(event.guild_id.0))
		.unwrap();
	if let Some(track_data) = event.track.user_data.as_ref()
		&& let Ok(queue_data) = from_value(track_data.clone())
		&& let Err(err) = guild_cache
			.music_data
			.queue
			.send(Arc::new(queue_data))
			.await
	{
		error!("Failed to send track data: {err}");
	}
}

fn notify_end(guild_id: GuildId) {
	let guild_cache = bot_context().data.guilds.get(&guild_id).unwrap();
	if !guild_cache.music_data.has_track_exception()
		&& let Err(err) = guild_cache
			.music_data
			.track_signals
			.send(TrackSignal::Ended)
	{
		error!("Failed to broadcast track ending: {err}");
	}
}

#[hook]
async fn track_end(_client: LavalinkClient, _session_id: String, event: &events::TrackEnd) {
	notify_end(GuildId::from(event.guild_id.0));
}

#[hook]
async fn track_exception(
	_client: LavalinkClient,
	_session_id: String,
	event: &events::TrackException,
) {
	if let Some(track_data) = event.track.user_data.as_ref()
		&& let Ok(queue_data) = from_value::<QueueData>(track_data.clone())
		&& queue_data.first_error.swap(false, Ordering::Relaxed)
	{
		let error = format!(
			"{}:{}:{}",
			event.exception.severity, event.exception.message, event.exception.cause
		);
		track_error(&error, GuildId::from(event.guild_id.0)).await;
	}
}

#[hook]
async fn websocket_closed(
	client: LavalinkClient,
	_session_id: String,
	event: &events::WebSocketClosed,
) {
	let bot_data = &bot_context().data;
	let guild_id = GuildId::from(event.guild_id.0);
	let guild_cache = bot_data.guilds.get(&guild_id).unwrap();
	if let Err(err) = client.delete_player(event.guild_id).await {
		error!("Failed to delete player: {err}");
	}
	if guild_cache.music_data.is_lavalink_connected() {
		guild_cache.music_data.disconnected();
		if let Err(err) = bot_data.music_manager.remove(guild_id).await {
			error!("Failed to remove call (lavalink): {err}");
		}
	}
}

async fn global_queue(
	guild_id: GuildId,
	compressed: Compressed,
	mut queue_data: QueueData,
) -> AResult<()> {
	let ctx = bot_context();
	if ctx
		.data
		.guilds
		.get(&guild_id)
		.unwrap()
		.music_data
		.global
		.load(Ordering::Relaxed)
	{
		for global_guild in ctx
			.data
			.guilds
			.iter()
			.filter(|t| t.music_data.global.load(Ordering::Relaxed) && *t.key() != guild_id)
			.map(|t| *t.key())
		{
			let Some(global_handler_lock) = ctx.data.music_manager.get(global_guild) else {
				continue;
			};
			let Some(channel_id) = global_handler_lock.lock().await.current_channel() else {
				continue;
			};
			if let Ok(channel) = ctx
				.http
				.get_channel(GenericChannelId::new(channel_id.get()))
				.await && let Some(guild_channel) = channel.guild()
			{
				let mut msg = guild_channel
					.send_message(&ctx.http, CreateMessage::new().content(QUEUEING_MSG))
					.await?;
				let input = Input::from(compressed.new_handle());
				queue_data.track_data.requested_channel = msg.channel_id;
				queue_data.track_data.request_message_id = msg.id;
				if let Err(err) = enqueue(
					queue_data.clone(),
					input,
					&global_handler_lock,
					global_guild.get().cast_signed(),
					None,
				)
				.await
				{
					warn!("Failed to queue global song: {err}");
					msg.edit(&ctx.http, EditMessage::new().content(FAILED_SONG_FETCH))
						.await?;
				}
			}
		}
	}

	Ok(())
}

pub async fn add_youtube_song(
	url: String,
	handler_lock: &Mutex<Call>,
	guild_id: GuildId,
	msg_id: MessageId,
	channel_id: GenericChannelId,
	author_id: UserId,
	conn: &Pool<Postgres>,
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

	let uuid = track_uuid(metadata.source_url.as_ref());

	let optional_data = if let Some(title) = metadata.title
		&& let Some(artist) = metadata.artist
		&& let Some(source_url) = metadata.source_url
		&& let Some(duration) = metadata.duration
		&& let Some(thumbnail_url) = metadata.thumbnail
	{
		Some(OptionalTrackData {
			title,
			artist,
			source_url,
			duration_sec: duration.as_secs().cast_signed(),
			thumbnail_url,
		})
	} else {
		None
	};

	let queue_data = QueueData {
		track_data: TrackPlayData {
			optional_data,
			requested_by: author_id,
			requested_channel: channel_id,
			request_message_id: msg_id,
			uuid,
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
		queue_data
			.track_data
			.optional_data
			.as_ref()
			.map(|_| (conn, i64::from(author_id))),
	)
	.await?;

	global_queue(guild_id, compressed, queue_data).await?;

	Ok(())
}

pub async fn music_task(
	mut rx: mpsc::Receiver<MusicQueueData>,
	guild_id: GuildId,
	ctx: SerenityContext,
) {
	let bot_data: Arc<Data> = ctx.data();
	let guild_cache = bot_data.guilds.get(&guild_id).unwrap();
	let mut track_watch = guild_cache.music_data.track_signals.subscribe();
	let mut connection_watch = guild_cache.music_data.connection_signals.subscribe();
	while let Some(data) = rx.recv().await {
		track_watch.mark_unchanged();
		connection_watch.mark_unchanged();
		if let Err(err) = update_info(
			&data,
			track_watch.clone(),
			connection_watch.clone(),
			guild_id,
			&ctx,
		)
		.await
		{
			error!("Failed to update song info: {err}");
		}
	}
}
