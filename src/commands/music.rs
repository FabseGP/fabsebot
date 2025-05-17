use crate::{
    config::{
        constants::COLOUR_RED,
        types::{Error, GuildDataMap, HTTP_CLIENT, SContext},
    },
    utils::ai::ai_voice,
};

use anyhow::Context as _;
use bytes::{BufMut as _, BytesMut};
use core::time::Duration;
use poise::{
    CreateReply, async_trait,
    serenity_prelude::{
        Channel, CreateEmbed, CreateMessage, EmbedMessageBuilding as _, GenericChannelId, GuildId,
        MessageBuilder, MessageId,
    },
};
use serde::Deserialize;
use songbird::{
    Call, CoreEvent, Event as SongBirdEvent, EventContext, EventHandler as VoiceEventHandler,
    Songbird,
    driver::Bitrate,
    input::{Input, YoutubeDl},
    tracks::PlayMode,
};
use sqlx::query;
use std::sync::Arc;
use tokio::{
    sync::{Mutex, MutexGuard},
    time::{sleep, timeout},
};
use tracing::warn;

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

async fn voice_check(ctx: &SContext<'_>) -> (Option<Arc<Mutex<Call>>>, Option<GuildId>) {
    match ctx.guild_id() {
        Some(guild_id) => {
            if let Some(handler_lock) = ctx.data().music_manager.get(guild_id) {
                (Some(handler_lock), Some(guild_id))
            } else {
                if let Err(err) = ctx.reply("Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel first").await {
                    warn!("Failed to notify user that the bot is not in a voice channel: {:?}", err);
                }
                (None, None)
            }
        }
        None => (None, None),
    }
}

pub async fn get_configured_handler(handler_lock: &Arc<Mutex<Call>>) -> MutexGuard<'_, Call> {
    let mut handler = handler_lock.lock().await;
    handler.set_bitrate(Bitrate::Max);
    handler
}

struct VoiceReceiveHandler {
    guild_id: GuildId,
    music_manager: Arc<Songbird>,
    guild_data: Arc<Mutex<GuildDataMap>>,
}

impl VoiceReceiveHandler {
    const fn new(
        guild_id: GuildId,
        music_manager: Arc<Songbird>,
        guild_data: Arc<Mutex<GuildDataMap>>,
    ) -> Self {
        Self {
            guild_id,
            music_manager,
            guild_data,
        }
    }
}

#[async_trait]
impl VoiceEventHandler for VoiceReceiveHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<SongBirdEvent> {
        if let EventContext::VoiceTick(tick) = ctx {
            for audio in tick
                .speaking
                .values()
                .filter_map(|data| data.decoded_voice.as_ref())
            {
                let mut buffer = BytesMut::with_capacity(audio.len().saturating_mul(2));
                for sample in audio {
                    buffer.put(sample.to_le_bytes().as_ref());
                }
                let buffer = buffer.freeze();
                let guild_global_music: Vec<_> = self
                    .guild_data
                    .lock()
                    .await
                    .iter()
                    .filter(|entry| {
                        let settings = &entry.value().settings;
                        entry.key() != &self.guild_id && settings.global_music
                    })
                    .map(|entry| entry.value().settings.guild_id)
                    .collect();
                for guild_id in guild_global_music {
                    if let Ok(guild_id_u64) = u64::try_from(guild_id) {
                        let current_guild_id = GuildId::new(guild_id_u64);
                        if let Some(global_handler_lock) = self.music_manager.get(current_guild_id)
                        {
                            get_configured_handler(&global_handler_lock)
                                .await
                                .enqueue_input(Input::from(buffer.clone()))
                                .await;
                        }
                    } else {
                        warn!("Failed to convert guild id to u64");
                    }
                }
            }
        }
        None
    }
}

/// Text to voice, duh
#[poise::command(prefix_command, slash_command)]
pub async fn text_to_voice(ctx: SContext<'_>) -> Result<(), Error> {
    if let (Some(handler_lock), Some(_)) = voice_check(&ctx).await {
        ctx.defer().await?;
        let msg = ctx
            .channel_id()
            .message(&ctx.http(), MessageId::new(ctx.id()))
            .await?;

        let Some(ref reply) = msg.referenced_message.map(|r| r.content) else {
            ctx.reply("Bruh, reply to a message").await?;
            return Ok(());
        };

        if let Some(bytes) = ai_voice(reply).await {
            get_configured_handler(&handler_lock)
                .await
                .enqueue_input(Input::from(bytes))
                .await;
            ctx.reply("here we go").await?;
        } else {
            ctx.reply("I don't wanna speak now").await?;
        }
    }
    Ok(())
}

/// Play all songs in a playlist from Deezer
#[poise::command(prefix_command, slash_command)]
pub async fn add_playlist(
    ctx: SContext<'_>,
    #[description = "ID of the playlist in mind"]
    #[rest]
    playlist_id: String,
) -> Result<(), Error> {
    if let (Some(handler_lock), Some(_)) = voice_check(&ctx).await {
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
                    let src = Input::from(YoutubeDl::new_search(HTTP_CLIENT.clone(), search));
                    get_configured_handler(&handler_lock)
                        .await
                        .enqueue_input(src)
                        .await;
                }
                ctx.reply("Added playlist to queue").await?;
            } else {
                ctx.reply("Deezer refused to serve your request").await?;
            }
        } else {
            ctx.reply("Invalid playlist-id for Deezer playlist").await?;
        }
    }
    Ok(())
}

/// End global music playback across guilds
#[poise::command(prefix_command, slash_command)]
pub async fn global_music_end(ctx: SContext<'_>) -> Result<(), Error> {
    if let (Some(_), Some(guild_id)) = voice_check(&ctx).await {
        query!(
            "INSERT INTO guild_settings (guild_id, global_music)
            VALUES ($1, FALSE)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                global_music = FALSE",
            i64::from(guild_id),
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.data().global_chats.invalidate(&guild_id);
        {
            let ctx_data = ctx.data();
            let guild_settings_lock = ctx_data.guild_data.lock().await;
            let mut current_settings_opt = guild_settings_lock.get(&guild_id);
            let mut modified_settings = current_settings_opt
                .get_or_insert_default()
                .as_ref()
                .clone();
            modified_settings.settings.global_music = false;
            guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
        }
        ctx.reply("Global music playback ended...").await?;
    }
    Ok(())
}

/// Start global music playback across guilds
#[poise::command(prefix_command, slash_command)]
pub async fn global_music_start(ctx: SContext<'_>) -> Result<(), Error> {
    if let (Some(_), Some(guild_id)) = voice_check(&ctx).await {
        let guild_id_i64 = i64::from(guild_id);
        let mut tx = ctx.data().db.begin().await?;
        query!(
            "INSERT INTO guild_settings (guild_id, global_music)
            VALUES ($1, TRUE)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                global_music = TRUE",
            guild_id_i64,
        )
        .execute(&mut *tx)
        .await?;
        let message = ctx.reply("Starting the party...").await?;
        let ctx_data = ctx.data();
        {
            let guild_settings_lock = ctx_data.guild_data.lock().await;
            let mut current_settings_opt = guild_settings_lock.get(&guild_id);
            let mut modified_settings = current_settings_opt
                .get_or_insert_default()
                .as_ref()
                .clone();
            modified_settings.settings.global_music = true;
            guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
        }
        let result = timeout(Duration::from_secs(60), async {
            loop {
                let has_other_calls =
                    ctx_data.guild_data.lock().await.iter().any(|entry| {
                        entry.key() != &guild_id && entry.value().settings.global_music
                    });
                if has_other_calls {
                    return Ok::<_, Error>(true);
                }
                sleep(Duration::from_secs(5)).await;
            }
        })
        .await;
        if result.is_ok() {
            message
                .edit(
                    ctx,
                    CreateReply::default()
                        .reply(true)
                        .content("Connected to global music playback!"),
                )
                .await?;
        } else {
            query!(
                "UPDATE guild_settings SET global_music = FALSE WHERE guild_id = $1",
                guild_id_i64
            )
            .execute(&mut *tx)
            .await?;
            message
                .edit(
                    ctx,
                    CreateReply::default()
                        .reply(true)
                        .content("No one joined the party within 1 minute 😢"),
                )
                .await?;
            let guild_settings_lock = ctx_data.guild_data.lock().await;
            let mut current_settings_opt = guild_settings_lock.get(&guild_id);
            let mut modified_settings = current_settings_opt
                .get_or_insert_default()
                .as_ref()
                .clone();
            modified_settings.settings.global_music = false;
            guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
        }
        tx.commit()
            .await
            .context("Failed to commit sql-transaction")?;
    }

    Ok(())
}

/// Join the current voice channel with global voice call
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
        let reply = if let Some(channel_id) = channel_id
            && let Ok(handler_lock) = ctx.data().music_manager.join(guild_id, channel_id).await
        {
            query!(
                "INSERT INTO guild_settings (guild_id, global_call)
                        VALUES ($1, TRUE)
                        ON CONFLICT(guild_id)
                        DO UPDATE SET
                            global_call = TRUE",
                i64::from(guild_id),
            )
            .execute(&mut *ctx.data().db.acquire().await?)
            .await?;
            {
                let ctx_data = ctx.data();
                let guild_settings_lock = ctx_data.guild_data.lock().await;
                let mut current_settings_opt = guild_settings_lock.get(&guild_id);
                let mut modified_settings = current_settings_opt
                    .get_or_insert_default()
                    .as_ref()
                    .clone();
                modified_settings.settings.global_call = true;
                guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
            }
            handler_lock.lock().await.add_global_event(
                CoreEvent::VoiceTick.into(),
                VoiceReceiveHandler::new(
                    guild_id,
                    ctx.data().music_manager.clone(),
                    ctx.data().guild_data.clone(),
                ),
            );

            "I've joined the party"
        } else {
            "I don't wanna join"
        };
        ctx.reply(reply).await?;
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
        let reply = if let Some(channel_id) = channel_id
            && ctx
                .data()
                .music_manager
                .join(guild_id, channel_id)
                .await
                .is_ok()
        {
            "I've joined the party"
        } else {
            "I don't wanna join"
        };
        ctx.reply(reply).await?;
    }
    Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn leave_voice(ctx: SContext<'_>) -> Result<(), Error> {
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
            let ctx_data = ctx.data();
            let guild_settings_lock = ctx_data.guild_data.lock().await;
            let mut current_settings_opt = guild_settings_lock.get(&guild_id);
            let mut modified_settings = current_settings_opt
                .get_or_insert_default()
                .as_ref()
                .clone();
            modified_settings.settings.global_music = false;
            modified_settings.settings.global_call = false;
            guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
        } else {
            ctx.reply(
                "Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first",
            )
            .await?;
        }
    }
    Ok(())
}

/// Continue/pause the current playing song
#[poise::command(prefix_command, slash_command)]
pub async fn pause_continue_song(ctx: SContext<'_>) -> Result<(), Error> {
    if let (Some(handler_lock), Some(guild_id)) = voice_check(&ctx).await {
        if get_configured_handler(&handler_lock)
            .await
            .queue()
            .current()
            .is_some()
        {
            let guild_id_i64 = i64::from(guild_id);
            {
                let ctx_data = ctx.data();
                let guild_data_lock = ctx_data.guild_data.lock().await;
                for guild in guild_data_lock.iter() {
                    let (current_track_opt, global_channel) = if guild.settings.guild_id
                        == guild_id_i64
                    {
                        (
                            get_configured_handler(&handler_lock)
                                .await
                                .queue()
                                .current(),
                            None,
                        )
                    } else if guild.settings.global_music {
                        if let Ok(guild_id_u64) = u64::try_from(guild.settings.guild_id) {
                            let current_guild_id = GuildId::new(guild_id_u64);
                            if let Some(global_handler_lock) =
                                ctx.data().music_manager.get(current_guild_id)
                            {
                                let current_channel = get_configured_handler(&global_handler_lock)
                                    .await
                                    .current_channel();
                                let channel_opt = match current_channel {
                                    Some(id) => (ctx
                                        .http()
                                        .get_channel(GenericChannelId::from(id.get()))
                                        .await)
                                        .map_or_else(|_| None, Channel::guild),
                                    _ => None,
                                };
                                (
                                    get_configured_handler(&global_handler_lock)
                                        .await
                                        .queue()
                                        .current(),
                                    channel_opt,
                                )
                            } else {
                                (None, None)
                            }
                        } else {
                            warn!("Failed to convert guild id to u64");
                            (None, None)
                        }
                    } else {
                        (None, None)
                    };
                    if let Some(current_track) = current_track_opt {
                        if let Ok(track_info) = current_track.get_info().await {
                            let response = match track_info.playing {
                                PlayMode::Pause => match current_track.play() {
                                    Ok(()) => "Resumed playback",
                                    Err(_) => "Failed to continue playback",
                                },
                                PlayMode::Play => match current_track.pause() {
                                    Ok(()) => "Paused playback",
                                    Err(_) => "Failed to pause playback",
                                },
                                _ => {
                                    ctx.reply("No playable track found").await?;
                                    return Ok(());
                                }
                            };
                            if let Some(channel) = global_channel {
                                channel
                                    .send_message(
                                        ctx.http(),
                                        CreateMessage::default().content(response),
                                    )
                                    .await?;
                            } else {
                                ctx.reply(response).await?;
                            }
                        } else {
                            ctx.reply("Failed to get current track information").await?;
                        }
                    } else {
                        ctx.reply("Bruh, nothing is playing!").await?;
                    }
                }
            }
        } else {
            ctx.reply("Bruh, nothing is playing!").await?;
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
    if let (Some(handler_lock), Some(guild_id)) = voice_check(&ctx).await {
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
        if let Ok(m) = Input::from(src.clone()).aux_metadata().await {
            let queue_len = {
                let mut handler = get_configured_handler(&handler_lock).await;
                handler.enqueue_input(Input::from(src.clone())).await;
                handler.queue().len().saturating_sub(1)
            };
            let artist = &m.artist;
            let thumbnail = &m.thumbnail;
            let title = &m.title;
            let source_url = &m.source_url;
            let duration = &m.duration;
            let mut e = CreateEmbed::default().colour(COLOUR_RED).field(
                "Added by:",
                ctx.author().display_name(),
                false,
            );
            if let Some(artist) = artist {
                e = e.field("Artist:", artist, true);
            }
            if let Some(url) = source_url {
                e = e.url(url);
            }
            if let Some(duration) = duration {
                e = e.field("Duration:", format!("{}s", duration.as_secs()), true);
            }
            if let Some(title) = title {
                if let Some(u) = source_url {
                    e = e.description(
                        MessageBuilder::default()
                            .push_named_link_safe(title.as_str(), u.as_str())
                            .build(),
                    );
                } else {
                    e = e.description(MessageBuilder::default().push_safe(title.as_str()).build());
                }
            }
            if let Some(url) = thumbnail {
                e = e.image(url);
            }
            e = e.field("Position:", format!("{queue_len}"), true);
            ctx.send(CreateReply::default().reply(true).embed(e.clone()))
                .await?;
            let guild_global_music: Vec<_> = ctx
                .data()
                .guild_data
                .lock()
                .await
                .iter()
                .filter(|entry| {
                    let settings = &entry.value().settings;
                    entry.key() != &guild_id && settings.global_music
                })
                .map(|entry| entry.value().settings.guild_id)
                .collect();
            for guild_id in guild_global_music {
                if let Ok(guild_id_u64) = u64::try_from(guild_id) {
                    let current_guild_id = GuildId::new(guild_id_u64);
                    if let Some(global_handler_lock) =
                        ctx.data().music_manager.get(current_guild_id)
                    {
                        let current_channel_opt = {
                            let mut handler = get_configured_handler(&global_handler_lock).await;
                            handler.enqueue_input(Input::from(src.clone())).await;
                            handler.current_channel()
                        };
                        if let Some(id) = current_channel_opt
                            && let Ok(channel) = ctx
                                .http()
                                .get_channel(GenericChannelId::from(id.get()))
                                .await
                            && let Some(guild_channel) = channel.guild()
                        {
                            guild_channel
                                .send_message(ctx.http(), CreateMessage::default().embed(e.clone()))
                                .await?;
                        }
                    }
                } else {
                    warn!("Failed to convert guild id to u64");
                }
            }
        } else {
            ctx.reply("Like you, nothing is known about this song")
                .await?;
        }
    }
    Ok(())
}

/// Seek current playing song backward
#[poise::command(prefix_command, slash_command)]
pub async fn seek_song(
    ctx: SContext<'_>,
    #[description = "Seconds to seek, i.e. '-20' or '+20'"] seconds: String,
) -> Result<(), Error> {
    if let (Some(handler_lock), Some(_)) = voice_check(&ctx).await {
        ctx.defer().await?;
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
                } else if current_playback.seek_async(seek).await.is_ok() {
                    ctx.reply(format!("Seeked {}s backward", seconds_value.abs()))
                        .await?;
                } else {
                    ctx.reply("Failed to seek song backwards").await?;
                }
            } else {
                let seconds_to_add = u64::try_from(seconds_value).unwrap_or(0);
                let seek = current_position.saturating_add(Duration::from_secs(seconds_to_add));
                if current_playback.seek_async(seek).await.is_ok() {
                    ctx.reply(format!("Seeked {seconds_value}s forward"))
                        .await?;
                } else {
                    ctx.reply("Bruh, you seeked more forward than the length of the song! I'm bailing out")
                                .await?;
                }
            }
        }
    }
    Ok(())
}

/// Skip to the next song in queue
#[poise::command(prefix_command, slash_command)]
pub async fn skip_song(ctx: SContext<'_>) -> Result<(), Error> {
    if let (Some(handler_lock), Some(guild_id)) = voice_check(&ctx).await {
        let queue_len = {
            let handler = get_configured_handler(&handler_lock).await;
            let queue = handler.queue();
            if queue.skip().is_err() {
                ctx.reply("No songs to skip!").await?;
                return Ok(());
            }
            queue.len()
        };
        let content = format!(
            "Song skipped by {}. {} left in queue",
            ctx.author().display_name(),
            queue_len.saturating_sub(2)
        );
        ctx.reply(&content).await?;
        let guild_global_music: Vec<_> = ctx
            .data()
            .guild_data
            .lock()
            .await
            .iter()
            .filter(|entry| {
                let settings = &entry.value().settings;
                entry.key() != &guild_id && settings.global_music
            })
            .map(|entry| entry.value().settings.guild_id)
            .collect();
        for guild_id in guild_global_music {
            if let Ok(guild_id_u64) = u64::try_from(guild_id) {
                let current_guild_id = GuildId::new(guild_id_u64);
                if let Some(global_handler_lock) = ctx.data().music_manager.get(current_guild_id) {
                    let channel_id = {
                        let handler = get_configured_handler(&global_handler_lock).await;
                        let queue = handler.queue();
                        if queue.skip().is_err() {
                            continue;
                        }
                        handler.current_channel()
                    };

                    if let Some(id) = channel_id
                        && let Ok(channel) = ctx
                            .http()
                            .get_channel(GenericChannelId::from(id.get()))
                            .await
                    {
                        channel.id().say(ctx.http(), &content).await?;
                    }
                }
            } else {
                warn!("Failed to convert guild id to u64");
            }
        }
    }
    Ok(())
}

/// Stop current playing song and clear the queue
#[poise::command(prefix_command, slash_command)]
pub async fn stop_song(ctx: SContext<'_>) -> Result<(), Error> {
    if let (Some(handler_lock), Some(guild_id)) = voice_check(&ctx).await {
        let has_songs = {
            let handler = get_configured_handler(&handler_lock).await;
            let queue = handler.queue();
            if queue.is_empty() {
                false
            } else {
                queue.stop();
                true
            }
        };
        if !has_songs {
            ctx.reply("Bruh, empty queue!").await?;
            return Ok(());
        }
        let content = "Queue cleared";
        ctx.reply(content).await?;
        let guild_global_music: Vec<_> = ctx
            .data()
            .guild_data
            .lock()
            .await
            .iter()
            .filter(|entry| {
                let settings = &entry.value().settings;
                entry.key() != &guild_id && settings.global_music
            })
            .map(|entry| entry.value().settings.guild_id)
            .collect();
        for guild_id in guild_global_music {
            if let Ok(guild_id_u64) = u64::try_from(guild_id) {
                let current_guild_id = GuildId::new(guild_id_u64);
                if let Some(global_handler_lock) = ctx.data().music_manager.get(current_guild_id) {
                    let channel_id = {
                        let handler = get_configured_handler(&global_handler_lock).await;
                        let queue = handler.queue();
                        queue.stop();
                        handler.current_channel()
                    };
                    if let Some(id) = channel_id
                        && let Ok(channel) = ctx
                            .http()
                            .get_channel(GenericChannelId::from(id.get()))
                            .await
                    {
                        channel.id().say(ctx.http(), content).await?;
                    }
                }
            } else {
                warn!("Failed to convert guild id to u64");
            }
        }
    }
    Ok(())
}
