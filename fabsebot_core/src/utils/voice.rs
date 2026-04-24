use std::{sync::Arc, time::Duration};

use anyhow::Result as AResult;
use metrics::counter;
use serenity::{
	all::{
		ButtonStyle, Colour, ComponentInteraction, ComponentInteractionCollector,
		Context as SerenityContext, CreateActionRow, CreateButton, CreateComponent,
		CreateContainer, CreateEmbed, CreateMessage, EditMessage, EmbedMessageBuilding as _,
		GenericChannelId, GuildId, MessageBuilder, MessageId, UserId,
	},
	async_trait,
	futures::StreamExt as _,
};
use songbird::{
	Call, CoreEvent, Event as SongBirdEvent, EventContext, EventHandler as VoiceEventHandler,
	TrackEvent,
	driver::Bitrate,
	input::{AudioStream, AuxMetadata, Input, LiveInput, YoutubeDl},
	tracks::{PlayMode, Track},
};
use symphonia::core::io::MediaSource;
use tokio::{
	select, spawn,
	sync::{Mutex, MutexGuard, Notify},
};
use tracing::{error, warn};
use url::Url;
use uuid::Uuid;

use crate::{
	config::{
		constants::{COLOUR_BLUE, COLOUR_GREEN, COLOUR_RED},
		types::{Data, HTTP_CLIENT, Metadata, SContext},
	},
	events::interaction::build_feedback_action_row,
	stats::counters::METRICS,
	utils::helpers::{get_lyrics, send_container, separator, text_display},
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
	notifier: Arc<Notify>,
}

impl PlaybackHandler {
	const fn new(
		serenity_context: SerenityContext,
		bot_data: Arc<Data>,
		guild_id: GuildId,
		notifier: Arc<Notify>,
	) -> Self {
		Self {
			serenity_context,
			bot_data,
			guild_id,
			notifier,
		}
	}

	fn create_components<'a>(
		author_name: &'a str,
		msg_id: MessageId,
		metadata: &'a AuxMetadata,
		queue_size: usize,
	) -> (CreateEmbed<'a>, [CreateComponent<'a>; 1]) {
		let mut e =
			CreateEmbed::default()
				.colour(COLOUR_RED)
				.field("Added by:", author_name, false);
		if let Some(artist) = &metadata.artist {
			e = e.field("Artist:", artist, true);
		}
		if let Some(url) = &metadata.source_url {
			e = e.url(url);
		}
		if let Some(duration) = &metadata.duration {
			e = e.field("Duration:", format!("{}s", duration.as_secs()), true);
		}
		if let Some(title) = &metadata.title {
			if let Some(u) = &metadata.source_url {
				e = e.description(
					MessageBuilder::default()
						.push_named_link_safe(title.as_str(), u.as_str())
						.build(),
				);
			} else {
				e = e.description(MessageBuilder::default().push_safe(title.as_str()).build());
			}
		}
		if let Some(url) = &metadata.thumbnail {
			e = e.image(url);
		}

		e = e.field(
			"Queue size:",
			format!("{}", queue_size.saturating_sub(1)),
			true,
		);

		let action_rows = [CreateComponent::ActionRow(CreateActionRow::buttons(vec![
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
		]))];

		(e, action_rows)
	}

	async fn handle_interaction(
		&self,
		interaction: ComponentInteraction,
		handler_lock: Arc<Mutex<Call>>,
		lyrics_shown: &mut bool,
		lyrics_embed: &mut Option<CreateEmbed<'_>>,
		history_shown: &mut bool,
		history_embed: &mut Option<CreateEmbed<'_>>,
		guild_tracks: Metadata,
		embed: &CreateEmbed<'_>,
	) -> AResult<()> {
		interaction.defer(&self.serenity_context.http).await?;

		let mut msg = interaction.message;

		if interaction.data.custom_id.ends_with('s') {
			let handler = get_configured_songbird_handler(&handler_lock).await;
			let queue = handler.queue();
			if queue.len() > 1 {
				queue.skip()?;
				drop(handler);
				for guild in &guild_tracks.1 {
					guild
						.1
						.2
						.edit_message(
							&self.serenity_context.http,
							guild.1.1,
							EditMessage::default()
								.content("Skipped to next song")
								.components(&[]),
						)
						.await?;
					if guild.0 == &self.guild_id {
						continue;
					}
					if let Some(handler_lock) = self.bot_data.music_manager.get(*guild.0) {
						get_configured_songbird_handler(&handler_lock)
							.await
							.queue()
							.skip()?;
					}
				}
			}
		} else if interaction.data.custom_id.ends_with('p') {
			let handler = get_configured_songbird_handler(&handler_lock).await;
			let queue = handler.queue();
			if let Some(current_track) = queue.current() {
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
			drop(handler);
			for guild in &guild_tracks.1 {
				let current_track = if guild.0 == &self.guild_id {
					continue;
				} else if let Some(handler_lock) = self.bot_data.music_manager.get(*guild.0)
					&& let Some(current_track) = get_configured_songbird_handler(&handler_lock)
						.await
						.queue()
						.current()
				{
					current_track
				} else {
					continue;
				};

				let track_state = match current_track.get_info().await.map(|t| t.playing) {
					Ok(state) => state,
					Err(err) => {
						error!("Failed to get track info: {err}");
						continue;
					}
				};

				match track_state {
					PlayMode::Pause => {
						current_track.play()?;
					}
					PlayMode::Play => {
						current_track.pause()?;
					}
					_ => {}
				}
			}
		} else if interaction.data.custom_id.ends_with('c') {
			get_configured_songbird_handler(&handler_lock)
				.await
				.queue()
				.stop();
			for guild in &guild_tracks.1 {
				guild
					.1
					.2
					.edit_message(
						&self.serenity_context.http,
						guild.1.1,
						EditMessage::default()
							.suppress_embeds(true)
							.content("Nothing to play")
							.components(&[]),
					)
					.await?;
				if guild.0 == &self.guild_id {
					continue;
				}
				if let Some(handler_lock) = self.bot_data.music_manager.get(*guild.0) {
					get_configured_songbird_handler(&handler_lock)
						.await
						.queue()
						.stop();
				}
			}
		} else if interaction.data.custom_id.ends_with('l') {
			let embed = if *lyrics_shown {
				*lyrics_shown = false;
				embed.clone()
			} else {
				*lyrics_shown = true;
				*history_shown = false;
				if let Some(embed) = &lyrics_embed {
					embed.clone()
				} else {
					let lyrics = if let Some(artist_name) = &guild_tracks.0.artist
						&& let Some(track_name) = &guild_tracks.0.title
						&& let Some(lyrics) =
							get_lyrics(&self.serenity_context, artist_name, track_name).await
					{
						lyrics
					} else {
						"Not found :(".to_owned()
					};
					let embed = CreateEmbed::default()
						.title("Lyrics")
						.description(lyrics)
						.colour(COLOUR_BLUE);
					*lyrics_embed = Some(embed.clone());
					embed
				}
			};
			msg.edit(
				self.serenity_context.http.clone(),
				EditMessage::default().embed(embed),
			)
			.await?;
		} else if interaction.data.custom_id.ends_with('h') {
			let embed = if *history_shown {
				*history_shown = false;
				embed.clone()
			} else {
				*history_shown = true;
				*lyrics_shown = false;
				if let Some(embed) = &history_embed {
					embed.clone()
				} else {
					let mut embed = CreateEmbed::default()
						.title("Song history")
						.description("Current session")
						.colour(COLOUR_GREEN);
					for track in &self.bot_data.track_metadata {
						if let Some(guild_track) = track.1.get(&self.guild_id)
							&& let Some(title) = track.0.title.clone()
						{
							embed = embed.field(title, guild_track.0.clone(), false);
						}
					}
					*history_embed = Some(embed.clone());
					embed
				}
			};
			msg.edit(
				self.serenity_context.http.clone(),
				EditMessage::default().embed(embed),
			)
			.await?;
		}

		Ok(())
	}

	pub async fn update_info(&self, guild_tracks: Metadata, notifier: Arc<Notify>) -> AResult<()> {
		if let Some(handler_lock) = self.bot_data.music_manager.get(self.guild_id)
			&& let Some(guild_data) = guild_tracks.1.get(&self.guild_id)
		{
			let queue_size = get_configured_songbird_handler(&handler_lock)
				.await
				.queue()
				.len();

			let (embed, action_rows) =
				Self::create_components(&guild_data.0, guild_data.1, &guild_tracks.0, queue_size);

			guild_data
				.2
				.edit_message(
					&self.serenity_context.http,
					guild_data.1,
					EditMessage::default()
						.embed(embed.clone())
						.components(&action_rows)
						.content(""),
				)
				.await?;
			let message_id_copy = guild_data.1.to_string();

			let mut lyrics_shown = false;
			let mut history_shown = false;

			let mut lyrics_embed: Option<CreateEmbed> = None;
			let mut history_embed: Option<CreateEmbed> = None;

			let mut collector_stream = ComponentInteractionCollector::new(&self.serenity_context)
				.timeout(Duration::from_hours(1))
				.filter(move |interaction| {
					interaction
						.data
						.custom_id
						.starts_with(message_id_copy.as_str())
				})
				.stream();

			loop {
				select! {
					Some(interaction) = collector_stream.next() => {
						self.handle_interaction(
							interaction,
							handler_lock.clone(),
							&mut lyrics_shown,
							&mut lyrics_embed,
							&mut history_shown,
							&mut history_embed,
							guild_tracks.clone(),
											&embed
						)
						.await?;
					},
					() = notifier.notified() => {
						break;
					},
				}
			}
			guild_data
				.2
				.edit_message(
					&self.serenity_context.http,
					guild_data.1,
					EditMessage::default()
						.components(&[])
						.content("Song finished"),
				)
				.await?;
		}

		Ok(())
	}
}

#[async_trait]
impl VoiceEventHandler for PlaybackHandler {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<SongBirdEvent> {
		if let EventContext::Track([(state, handle)]) = ctx {
			if state.playing == PlayMode::Play
				&& let Some(guild_tracks) = self.bot_data.track_metadata.get(&handle.uuid())
			{
				let self_clone = self.clone();
				let notifier_clone = self.notifier.clone();
				spawn(async move {
					if let Err(err) = self_clone.update_info(guild_tracks, notifier_clone).await {
						error!("Failed to update song info: {err}");
					}
				});
			} else if state.playing == PlayMode::End {
				self.notifier.notify_one();
			} else if let PlayMode::Errored(error) = &state.playing {
				error!("Failed to play track: {error}");
				counter!(METRICS.prefix_errors.clone()).increment(1);
				if let Some(guild_tracks) = self.bot_data.track_metadata.get(&handle.uuid())
					&& let Some(guild_data) = guild_tracks.1.get(&self.guild_id)
					&& let Err(err) = guild_data
						.2
						.edit_message(
							&self.serenity_context.http,
							guild_data.1,
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

pub async fn add_voice_events(
	ctx: &SerenityContext,
	guild_id: GuildId,
	channel_id: GenericChannelId,
	handler_lock: Arc<Mutex<Call>>,
) {
	let mut handler = handler_lock.lock().await;
	let track_notify = Arc::new(Notify::new());

	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Playable),
		PlaybackHandler::new(ctx.clone(), ctx.data(), guild_id, track_notify.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::End),
		PlaybackHandler::new(ctx.clone(), ctx.data(), guild_id, track_notify.clone()),
	);
	handler.add_global_event(
		SongBirdEvent::Track(TrackEvent::Error),
		PlaybackHandler::new(ctx.clone(), ctx.data(), guild_id, track_notify),
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
	guild_id: GuildId,
	data: Arc<Data>,
	msg_id: MessageId,
	channel_id: GenericChannelId,
	author_name: &str,
) {
	let uuid = metadata
		.source_url
		.as_ref()
		.map_or_else(Uuid::new_v4, |url| {
			Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())
		});

	let mut track_metadata = data
		.track_metadata
		.get(&uuid)
		.unwrap_or_default()
		.as_ref()
		.clone();

	track_metadata.0 = metadata;
	track_metadata.1.insert(
		guild_id,
		(format!("Added by {author_name}"), msg_id, channel_id),
	);

	data.track_metadata.insert(uuid, Arc::new(track_metadata));

	get_configured_songbird_handler(&handler_lock)
		.await
		.enqueue(Track::new_with_uuid(
			Input::Live(LiveInput::Raw(audio), Some(Box::new(source))),
			uuid,
		))
		.await;
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

	send_container(ctx, container).await?;

	Ok(())
}
