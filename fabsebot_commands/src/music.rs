use core::time::Duration;
use std::sync::Arc;

use anyhow::Result as AResult;
use dashmap::DashMap;
use fabsebot_core::{
	config::{
		constants::{COLOUR_BLUE, COLOUR_GREEN, COLOUR_RED, COLOUR_YELLOW},
		types::{Data, Error, HTTP_CLIENT, SContext},
	},
	utils::{
		ai::ai_voice,
		helpers::{get_configured_handler, get_lyrics},
	},
};
use poise::{CreateReply, async_trait};
use serde::Deserialize;
use serenity::all::{
	ButtonStyle, ComponentInteractionCollector, Context as SerenityContext, CreateActionRow,
	CreateButton, CreateComponent, CreateEmbed, CreateMessage, EditMessage,
	EmbedMessageBuilding as _, GenericChannelId, GuildId, MessageBuilder, MessageId, UserId,
};
use songbird::{
	Call, CoreEvent, Event as SongBirdEvent, EventContext, EventHandler as VoiceEventHandler,
	TrackEvent,
	input::{AuxMetadata, Input, YoutubeDl},
	tracks::{PlayMode, Track},
};
use sqlx::query;
use tokio::{process::Command, spawn, sync::Mutex};
use tracing::{error, warn};
use uuid::Uuid;

#[derive(Deserialize)]
struct DeezerResponse {
	tracks: DeezerData,
}

#[derive(Deserialize)]
struct DeezerData {
	data: Vec<DeezerTracks>,
}

#[derive(Deserialize)]
struct DeezerTracks {
	title: String,
	artist: DeezerArtist,
}

#[derive(Deserialize)]
struct DeezerArtist {
	name: String,
}

fn voice_check(ctx: &SContext<'_>) -> Option<(Arc<Mutex<Call>>, GuildId)> {
	let guild_id = ctx.guild_id()?;
	let handler_lock = ctx.data().music_manager.get(guild_id)?;

	Some((handler_lock, guild_id))
}

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
			if let Ok(user) = user_id.to_user(&self.serenity_context.http).await {
				self.channel_id
					.send_message(
						&self.serenity_context.http,
						CreateMessage::default().content(format!("Bye {}", user.display_name())),
					)
					.await
					.ok()?;
			}
		}
		None
	}
}

#[derive(Clone)]
struct PlaybackHandler {
	serenity_context: SerenityContext,
	bot_data: Arc<Data>,
	guild_id: GuildId,
}

impl PlaybackHandler {
	const fn new(
		serenity_context: SerenityContext,
		bot_data: Arc<Data>,
		guild_id: GuildId,
	) -> Self {
		Self {
			serenity_context,
			bot_data,
			guild_id,
		}
	}

	pub async fn update_info(
		&self,
		guild_tracks: (
			AuxMetadata,
			DashMap<GuildId, (String, MessageId, GenericChannelId)>,
		),
	) -> AResult<()> {
		if let Some(handler_lock) = self.bot_data.music_manager.get(self.guild_id)
			&& let Some(guild_data) = guild_tracks.1.clone().get(&self.guild_id)
		{
			let metadata = guild_tracks.0;
			let mut e =
				CreateEmbed::default()
					.colour(COLOUR_RED)
					.field("Added by:", &guild_data.0, false);
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

			let queue_size = get_configured_handler(&handler_lock).await.queue().len();

			e = e.field(
				"Queue size:",
				format!("{}", queue_size.saturating_sub(1)),
				true,
			);

			let skip_disabled = queue_size == 1;

			let mut buttons_row1 = [
				CreateButton::new(format!("{}_s", guild_data.1))
					.style(ButtonStyle::Primary)
					.disabled(skip_disabled)
					.label("Skip"),
				CreateButton::new(format!("{}_p", guild_data.1))
					.style(ButtonStyle::Primary)
					.label("Pause/Unpause"),
				CreateButton::new(format!("{}_c", guild_data.1))
					.style(ButtonStyle::Primary)
					.label("Stop & clear queue"),
				CreateButton::new(format!("{}_l", guild_data.1))
					.style(ButtonStyle::Primary)
					.label("Show/Hide lyrics"),
				CreateButton::new(format!("{}_h", guild_data.1))
					.style(ButtonStyle::Primary)
					.label("Show/Hide song history"),
			];

			let buttons_row2 = [CreateButton::new(format!("{}_u", guild_data.1))
				.style(ButtonStyle::Primary)
				.label("Update controls")];

			let action_rows = [
				CreateComponent::ActionRow(CreateActionRow::buttons(&buttons_row1)),
				CreateComponent::ActionRow(CreateActionRow::buttons(&buttons_row2)),
			];

			guild_data
				.2
				.edit_message(
					&self.serenity_context.http,
					guild_data.1,
					EditMessage::default()
						.embed(e.clone())
						.components(&action_rows)
						.content(""),
				)
				.await?;
			let message_id_copy = guild_data.1;

			let mut lyrics_shown = false;
			let mut history_shown = false;

			let mut lyrics_embed: Option<CreateEmbed> = None;
			let mut history_embed: Option<CreateEmbed> = None;

			while let Some(interaction) = ComponentInteractionCollector::new(&self.serenity_context)
				.timeout(metadata.duration.unwrap_or(Duration::from_secs(60)))
				.filter(move |interaction| {
					interaction
						.data
						.custom_id
						.starts_with(message_id_copy.to_string().as_str())
				})
				.await
			{
				interaction.defer(&self.serenity_context.http).await?;

				let mut msg = interaction.message;

				let handler = get_configured_handler(&handler_lock).await;
				let queue = handler.queue();
				if interaction.data.custom_id.ends_with('s') {
					for guild in &guild_tracks.1 {
						if guild.key() == &self.guild_id {
							queue.skip()?;
						} else if let Some(handler_lock) =
							self.bot_data.music_manager.get(*guild.key())
						{
							get_configured_handler(&handler_lock).await.queue().skip()?;
						} else {
							continue;
						}
						guild
							.2
							.edit_message(
								&self.serenity_context.http,
								guild.1,
								EditMessage::default()
									.suppress_embeds(true)
									.content("Skipped to next song")
									.components(&[]),
							)
							.await?;
					}
					break;
				} else if interaction.data.custom_id.ends_with('p') {
					for guild in &guild_tracks.1 {
						if guild.key() == &self.guild_id {
							if let Some(current_track) = queue.current()
								&& let Ok(track_info) = current_track.get_info().await
							{
								match track_info.playing {
									PlayMode::Pause => {
										current_track.play()?;
									}
									PlayMode::Play => {
										current_track.pause()?;
									}
									_ => {}
								}
							}
						} else if let Some(handler_lock) =
							self.bot_data.music_manager.get(*guild.key())
							&& let Some(current_track) = get_configured_handler(&handler_lock)
								.await
								.queue()
								.current() && let Ok(track_info) = current_track.get_info().await
						{
							match track_info.playing {
								PlayMode::Pause => {
									current_track.play()?;
								}
								PlayMode::Play => {
									current_track.pause()?;
								}
								_ => {}
							}
						}
					}
				} else if interaction.data.custom_id.ends_with('c') {
					for guild in &guild_tracks.1 {
						if guild.key() == &self.guild_id {
							queue.stop();
						} else if let Some(handler_lock) =
							self.bot_data.music_manager.get(*guild.key())
						{
							get_configured_handler(&handler_lock).await.queue().stop();
						} else {
							continue;
						}
						guild
							.2
							.edit_message(
								&self.serenity_context.http,
								guild.1,
								EditMessage::default()
									.suppress_embeds(true)
									.content("Nothing to play")
									.components(&[]),
							)
							.await?;
					}
					break;
				} else if interaction.data.custom_id.ends_with('u') && queue.len() > 1 {
					buttons_row1[0] = CreateButton::new(format!("{}_s", guild_data.1))
						.style(ButtonStyle::Primary)
						.label("Skip");
					let action_rows = [
						CreateComponent::ActionRow(CreateActionRow::buttons(&buttons_row1)),
						CreateComponent::ActionRow(CreateActionRow::buttons(&buttons_row2)),
					];
					msg.edit(
						self.serenity_context.http.clone(),
						EditMessage::default().components(&action_rows),
					)
					.await?;
				} else if interaction.data.custom_id.ends_with('l') {
					if lyrics_shown {
						lyrics_shown = false;
						msg.edit(
							self.serenity_context.http.clone(),
							EditMessage::default().embed(e.clone()),
						)
						.await?;
					} else {
						lyrics_shown = true;
						history_shown = false;
						let new_embed = if let Some(embed) = &lyrics_embed {
							embed.clone()
						} else if let Some(artist_name) = &metadata.artist
							&& let Some(track_name) = &metadata.title
							&& let Some(lyrics) = get_lyrics(artist_name, track_name).await
						{
							let embed = CreateEmbed::default()
								.title("Lyrics")
								.description(lyrics)
								.colour(COLOUR_BLUE);
							lyrics_embed = Some(embed.clone());
							embed
						} else {
							let embed = CreateEmbed::default()
								.title("Lyrics")
								.description("Not found :(")
								.colour(COLOUR_BLUE);
							lyrics_embed = Some(embed.clone());
							embed
						};
						msg.edit(
							self.serenity_context.http.clone(),
							EditMessage::default().add_embed(new_embed),
						)
						.await?;
					}
				} else if interaction.data.custom_id.ends_with('h') {
					if history_shown {
						history_shown = false;
						msg.edit(
							self.serenity_context.http.clone(),
							EditMessage::default().embed(e.clone()),
						)
						.await?;
					} else {
						history_shown = true;
						lyrics_shown = false;
						let new_embed = if let Some(embed) = &history_embed {
							embed.clone()
						} else {
							let mut embed = CreateEmbed::default()
								.title("Song history")
								.description("Current session")
								.colour(COLOUR_GREEN);
							for track in &self.bot_data.track_metadata {
								if let Some(guild_track) = track.1.get(&self.guild_id) {
									embed = embed.field(
										track.0.title.clone().unwrap_or_default(),
										format!("Added by {}", guild_track.0),
										false,
									);
								}
							}
							history_embed = Some(embed.clone());
							embed
						};
						msg.edit(
							self.serenity_context.http.clone(),
							EditMessage::default().add_embed(new_embed),
						)
						.await?;
					}
				}
			}
			guild_data
				.2
				.edit_message(
					&self.serenity_context.http,
					guild_data.1,
					EditMessage::default()
						.suppress_embeds(true)
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
		if let EventContext::Track(track_list) = ctx {
			for (state, handle) in *track_list {
				if state.playing == PlayMode::Play
					&& let Some(guild_tracks) = self.bot_data.track_metadata.get(&handle.uuid())
				{
					let self_clone = self.clone();
					let guild_tracks_clone = guild_tracks.clone();
					spawn(async move {
						if let Err(err) = self_clone.update_info(guild_tracks_clone).await {
							error!("Failed to update song info: {:?}", &err);
						}
					});
				}
			}
			return None;
		}
		None
	}
}

/// Text to voice, duh
#[poise::command(prefix_command, slash_command)]
pub async fn text_to_voice(ctx: SContext<'_>, input_opt: Option<String>) -> Result<(), Error> {
	if let Some((handler_lock, _)) = voice_check(&ctx) {
		ctx.defer().await?;

		let payload = if let Some(input) = input_opt {
			input
		} else if let Ok(msg) = ctx
			.channel_id()
			.message(&ctx.http(), MessageId::new(ctx.id()))
			.await && let Some(reply) = msg.referenced_message.map(|r| r.content)
		{
			reply.into_string()
		} else {
			ctx.reply("Bruh, reply to a message").await?;
			return Ok(());
		};

		if let Some(bytes) = ai_voice(&payload).await {
			get_configured_handler(&handler_lock)
				.await
				.enqueue_input(Input::from(bytes))
				.await;
			ctx.reply("Here we go").await?;
		} else {
			ctx.reply("I don't wanna speak now").await?;
		}
	} else {
		ctx.reply(
			"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel \
			 first",
		)
		.await?;
	}
	Ok(())
}

async fn queue_song(
	track: Track,
	metadata: AuxMetadata,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: GuildId,
	ctx: SContext<'_>,
) -> AResult<()> {
	let reply = ctx.reply("Song added to queue").await?;
	if let Ok(msg) = reply.message().await {
		let uuid = get_configured_handler(&handler_lock)
			.await
			.enqueue(track)
			.await
			.uuid();

		ctx.data()
			.track_metadata
			.entry(uuid)
			.or_insert_with(|| (metadata.clone(), DashMap::default()))
			.1
			.insert(
				guild_id,
				(
					ctx.author().display_name().to_owned(),
					msg.id,
					msg.channel_id,
				),
			);
	}

	Ok(())
}

async fn queue_song_global(
	track: Track,
	metadata: AuxMetadata,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: GuildId,
	ctx: SContext<'_>,
) -> AResult<()> {
	let mut handler = get_configured_handler(&handler_lock).await;
	if let Some(id) = handler.current_channel()
		&& let Ok(channel) = ctx
			.http()
			.get_channel(GenericChannelId::from(id.get()))
			.await
		&& let Some(guild_channel) = channel.guild()
	{
		let msg = guild_channel
			.send_message(
				ctx.http(),
				CreateMessage::default().content("Song added to queue"),
			)
			.await?;
		let uuid = handler.enqueue(track).await.uuid();

		ctx.data()
			.track_metadata
			.entry(uuid)
			.or_insert_with(|| (metadata.clone(), DashMap::default()))
			.1
			.insert(
				guild_id,
				(
					ctx.author().display_name().to_owned(),
					msg.id,
					msg.channel_id,
				),
			);
	}

	Ok(())
}

/// Play all songs in a playlist from Deezer
#[poise::command(prefix_command, slash_command)]
pub async fn add_deezer_playlist(
	ctx: SContext<'_>,
	#[description = "ID of the playlist in mind"]
	#[rest]
	playlist_id: String,
) -> Result<(), Error> {
	if let Some((handler_lock, guild_id)) = voice_check(&ctx) {
		if let Ok(request) = HTTP_CLIENT
			.get(format!("https://api.deezer.com/playlist/{playlist_id}"))
			.send()
			.await
		{
			ctx.defer().await?;
			if let Some(payload) = request
				.json::<DeezerResponse>()
				.await
				.ok()
				.filter(|output| !output.tracks.data.is_empty())
			{
				for track in payload.tracks.data {
					let search = format!("{} {}", track.title, track.artist.name);
					let src = YoutubeDl::new_search(HTTP_CLIENT.clone(), search);
					let mut input = Input::from(src.clone());
					if let Ok(metadata) = input.aux_metadata().await {
						let uuid = Uuid::new_v4();
						queue_song(
							Track::new_with_uuid(input, uuid),
							metadata.clone(),
							handler_lock.clone(),
							guild_id,
							ctx,
						)
						.await?;
					} else {
						ctx.reply("Nothing is known about this song").await?;
					}
				}
			} else {
				ctx.reply("Deezer refused to serve your request").await?;
			}
		} else {
			ctx.reply("Invalid playlist-id for Deezer playlist").await?;
		}
	} else {
		ctx.reply(
			"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel \
			 first",
		)
		.await?;
	}

	Ok(())
}

/// Play all songs in a playlist from ``YouTube``
#[poise::command(prefix_command, slash_command)]
pub async fn add_youtube_playlist(
	ctx: SContext<'_>,
	#[description = "Url playlist in mind"]
	#[rest]
	playlist_url: String,
) -> Result<(), Error> {
	if let Some((handler_lock, guild_id)) = voice_check(&ctx) {
		ctx.defer().await?;
		let yt_dlp_output = Command::new("yt-dlp")
			.args([
				"--flat-playlist",
				"--print",
				"url",
				"--no-warnings",
				&playlist_url,
			])
			.output()
			.await?;

		let urls_joined = String::from_utf8(yt_dlp_output.stdout)?;

		let urls: Vec<String> = urls_joined
			.lines()
			.filter(|line| line.starts_with("https://"))
			.map(ToString::to_string)
			.collect();

		for url in urls {
			let src = YoutubeDl::new(HTTP_CLIENT.clone(), url);
			let mut input = Input::from(src.clone());
			if let Ok(metadata) = input.aux_metadata().await {
				let uuid = Uuid::new_v4();
				queue_song(
					Track::new_with_uuid(input, uuid),
					metadata.clone(),
					handler_lock.clone(),
					guild_id,
					ctx,
				)
				.await?;
			} else {
				ctx.reply("Nothing is known about this song").await?;
			}
		}
	} else {
		ctx.reply(
			"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel \
			 first",
		)
		.await?;
	}

	Ok(())
}

/// Join the current voice channel with global music playback
#[poise::command(prefix_command, slash_command)]
pub async fn join_voice_global(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		ctx.defer().await?;
		let channel_id = ctx.guild().and_then(|guild| {
			guild
				.voice_states
				.get(&ctx.author().id)
				.and_then(|voice_state| voice_state.channel_id)
		});
		if ctx.data().music_manager.get(guild_id).is_some() {
			ctx.reply(
				"Bruh, I'm already in a voice channel! Use /leave_voice_global to drop the \
				 connection",
			)
			.await?;
		} else if let Some(channel_id) = channel_id
			&& let Ok(handler_lock) = ctx.data().music_manager.join(guild_id, channel_id).await
		{
			query!(
				"INSERT INTO guild_settings (guild_id, global_call)
                        VALUES ($1, TRUE)
                        ON CONFLICT(guild_id)
                        DO UPDATE SET
                            global_call = TRUE,
                            global_music = TRUE",
				i64::from(guild_id),
			)
			.execute(&mut *ctx.data().db.acquire().await?)
			.await?;
			let mut modified_settings = ctx
				.data()
				.guild_data
				.get(&guild_id)
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.settings.global_call = true;
			modified_settings.settings.global_music = true;
			ctx.data()
				.guild_data
				.insert(guild_id, Arc::new(modified_settings));
			ctx.send(
				CreateReply::default().embed(
					CreateEmbed::default()
						.title("I've joined the party!")
						.description("Commands to use (supports prefix):")
						.field(
							"/play_song_global",
							"Play a new song from a YouTube url or from a search",
							false,
						)
						.field(
							"/seek_song",
							"Seek song forward (e.g. +20) or backwards (e.g. -20)",
							false,
						)
						.field(
							"/text_to_voice",
							"Make the bot say smth either by providing an input or replying to a \
							 message",
							false,
						)
						.field("/leave_voice_global", "Make the bot leave the party", false)
						.colour(COLOUR_YELLOW),
				),
			)
			.await?;
			let mut handler = handler_lock.lock().await;
			handler.add_global_event(
				SongBirdEvent::Track(TrackEvent::Playable),
				PlaybackHandler::new(ctx.serenity_context().clone(), ctx.data(), guild_id),
			);
			handler.add_global_event(
				SongBirdEvent::Core(CoreEvent::DriverDisconnect),
				DriverDisconnectHandler::new(ctx.data()),
			);
			handler.add_global_event(
				SongBirdEvent::Core(CoreEvent::ClientDisconnect),
				ClientDisconnectHandler::new(ctx.serenity_context().clone(), ctx.channel_id()),
			);
		} else {
			ctx.reply("I don't wanna join").await?;
		}
	}
	Ok(())
}

/// Join the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn join_voice(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		ctx.defer().await?;
		let channel_id = ctx.guild().and_then(|guild| {
			guild
				.voice_states
				.get(&ctx.author().id)
				.and_then(|voice_state| voice_state.channel_id)
		});
		if ctx.data().music_manager.get(guild_id).is_some() {
			ctx.reply(
				"Bruh, I'm already in a voice channel! Use /leave_voice to drop the connection",
			)
			.await?;
		} else if let Some(channel_id) = channel_id
			&& let Ok(handler_lock) = ctx.data().music_manager.join(guild_id, channel_id).await
		{
			ctx.send(
				CreateReply::default().embed(
					CreateEmbed::default()
						.title("I've joined the party!")
						.description("Commands to use (supports prefix):")
						.field(
							"/play_song",
							"Play a new song from a YouTube url or from a search",
							false,
						)
						.field(
							"/add_youtube_playlist",
							"Add songs in a YouTube-playlist",
							false,
						)
						.field(
							"/add_deezer_playlist",
							"Add songs in a Deezer-playlist",
							false,
						)
						.field(
							"/seek_song",
							"Seek song forward (e.g. +20) or backwards (e.g. -20)",
							false,
						)
						.field(
							"/text_to_voice",
							"Make the bot say smth either by providing an input or replying to a \
							 message",
							false,
						)
						.field("/leave_voice", "Make the bot leave the party", false)
						.colour(COLOUR_YELLOW),
				),
			)
			.await?;
			let mut handler = handler_lock.lock().await;
			handler.add_global_event(
				SongBirdEvent::Track(TrackEvent::Playable),
				PlaybackHandler::new(ctx.serenity_context().clone(), ctx.data(), guild_id),
			);
			handler.add_global_event(
				SongBirdEvent::Core(CoreEvent::DriverDisconnect),
				DriverDisconnectHandler::new(ctx.data()),
			);
			handler.add_global_event(
				SongBirdEvent::Core(CoreEvent::ClientDisconnect),
				ClientDisconnectHandler::new(ctx.serenity_context().clone(), ctx.channel_id()),
			);
		} else {
			ctx.reply("I don't wanna join").await?;
		}
	}
	Ok(())
}

/// Leave the current voice channel with global voice call
#[poise::command(prefix_command, slash_command)]
pub async fn leave_voice_global(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		if ctx.data().music_manager.remove(guild_id).await.is_ok() {
			ctx.reply("Left voice channel, don't forget me").await?;
			query!(
				"INSERT INTO guild_settings (guild_id, global_music, global_call)
                    VALUES ($1, FALSE, FALSE)
                    ON CONFLICT(guild_id)
                    DO UPDATE SET
                        global_music = FALSE,
                        global_call = FALSE",
				i64::from(guild_id),
			)
			.execute(&mut *ctx.data().db.acquire().await?)
			.await?;
			let mut modified_settings = ctx
				.data()
				.guild_data
				.get(&guild_id)
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.settings.global_music = false;
			modified_settings.settings.global_call = false;
			ctx.data()
				.guild_data
				.insert(guild_id, Arc::new(modified_settings));
			ctx.data()
				.track_metadata
				.retain(|_key, value| !value.1.contains_key(&guild_id));
		} else {
			ctx.reply(
				"Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first",
			)
			.await?;
		}
	}
	Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn leave_voice(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		if ctx.data().music_manager.remove(guild_id).await.is_ok() {
			ctx.reply("Left voice channel, don't forget me").await?;
			ctx.data()
				.track_metadata
				.retain(|_key, value| !value.1.contains_key(&guild_id));
		} else {
			ctx.reply(
				"Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first",
			)
			.await?;
		}
	}
	Ok(())
}

/// Play song / add song to queue in the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn play_song(
	ctx: SContext<'_>,
	#[description = "YouTube link or query to search"]
	#[rest]
	url: String,
) -> Result<(), Error> {
	ctx.defer().await?;
	if let Some((handler_lock, guild_id)) = voice_check(&ctx) {
		let src = if url.starts_with("https") {
			if url.contains("youtu") {
				YoutubeDl::new(HTTP_CLIENT.clone(), url)
			} else {
				ctx.reply("Only YouTube-links are supported").await?;
				return Ok(());
			}
		} else {
			YoutubeDl::new_search(HTTP_CLIENT.clone(), url)
		};
		let mut input = Input::from(src.clone());
		if let Ok(metadata) = input.aux_metadata().await {
			let uuid = Uuid::new_v4();
			queue_song(
				Track::new_with_uuid(input, uuid),
				metadata.clone(),
				handler_lock,
				guild_id,
				ctx,
			)
			.await?;
		} else {
			ctx.reply("Nothing is known about this song").await?;
		}
	} else {
		ctx.reply(
			"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel \
			 first",
		)
		.await?;
	}

	Ok(())
}

/// Play song / add song to queue in the current voice channel (global)
#[poise::command(prefix_command, slash_command)]
pub async fn play_song_global(
	ctx: SContext<'_>,
	#[description = "YouTube link or query to search"]
	#[rest]
	url: String,
) -> Result<(), Error> {
	ctx.defer().await?;
	if let Some((handler_lock, guild_id)) = voice_check(&ctx) {
		let src = if url.starts_with("https") {
			if url.contains("youtu") {
				YoutubeDl::new(HTTP_CLIENT.clone(), url)
			} else {
				ctx.reply("Only YouTube-links are supported").await?;
				return Ok(());
			}
		} else {
			YoutubeDl::new_search(HTTP_CLIENT.clone(), url)
		};
		let mut input = Input::from(src.clone());
		if let Ok(metadata) = input.aux_metadata().await {
			let uuid = Uuid::new_v4();
			queue_song(
				Track::new_with_uuid(input, uuid),
				metadata.clone(),
				handler_lock,
				guild_id,
				ctx,
			)
			.await?;
			let guild_global_music: Vec<_> = ctx
				.data()
				.guild_data
				.iter()
				.filter(|entry| {
					let settings = &entry.value().settings;
					entry.key() != &guild_id && settings.global_music
				})
				.map(|entry| entry.value().settings.guild_id)
				.collect();

			let uuid = Uuid::new_v4();
			for guild_id_global in guild_global_music {
				if let Ok(guild_id_u64) = u64::try_from(guild_id_global) {
					let current_guild_id = GuildId::new(guild_id_u64);
					if let Some(global_handler_lock) =
						ctx.data().music_manager.get(current_guild_id)
					{
						queue_song_global(
							Track::new_with_uuid(Input::from(src.clone()), uuid),
							metadata.clone(),
							global_handler_lock,
							current_guild_id,
							ctx,
						)
						.await?;
					}
				} else {
					warn!("Failed to convert guild id to u64");
				}
			}
		} else {
			ctx.reply("Nothing is known about this song").await?;
		}
	} else {
		ctx.reply(
			"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel \
			 first",
		)
		.await?;
	}

	Ok(())
}

/// Seek current playing song backward
#[poise::command(prefix_command, slash_command)]
pub async fn seek_song(
	ctx: SContext<'_>,
	#[description = "Seconds to seek, i.e. '-20' or '+20'"] seconds: String,
) -> Result<(), Error> {
	if let Some((handler_lock, _)) = voice_check(&ctx) {
		ctx.defer_ephemeral().await?;
		let current_playback_opt = get_configured_handler(&handler_lock)
			.await
			.queue()
			.current();
		if let Some(current_playback) = current_playback_opt
			&& let Ok(current_playback_info) = current_playback.get_info().await
		{
			let current_position = current_playback_info.position;
			let Ok(seconds_value) = seconds.parse::<i64>() else {
				ctx.reply("Bruh, provide a valid number with a sign (e.g. '+20' or '-20')!")
					.await?;
				return Ok(());
			};
			let current_secs = i64::try_from(current_position.as_secs()).unwrap_or(0);
			if seconds_value.is_negative() {
				let new_position =
					u64::try_from(current_secs.saturating_add(seconds_value)).unwrap_or(0);
				let seek = Duration::from_secs(new_position);
				if seek.is_zero() {
					ctx.reply("Bruh, wanting to seek more seconds back than what have been played")
						.await?;
				} else if let Err(err) = current_playback.seek_async(seek).await {
					ctx.reply("Failed to seek song backwards").await?;
					warn!("Error seeking song backwards: {:?}", err);
				} else {
					ctx.reply(format!("Seeked {}s backward", seconds_value.abs()))
						.await?;
				}
			} else {
				let seconds_to_add = u64::try_from(seconds_value).unwrap_or(0);
				let seek = current_position.saturating_add(Duration::from_secs(seconds_to_add));
				if let Err(err) = current_playback.seek_async(seek).await {
					ctx.reply(
						"Bruh, you seeked more forward than the length of the song! I'm bailing \
						 out",
					)
					.await?;
					warn!("Error seeking song forward: {:?}", err);
				} else {
					ctx.reply(format!("Seeked {seconds_value}s forward"))
						.await?;
				}
			}
		}
	} else {
		ctx.reply(
			"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel \
			 first",
		)
		.await?;
	}

	Ok(())
}
