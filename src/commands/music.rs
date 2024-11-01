use crate::{
    consts::COLOUR_RED,
    types::{Error, SContext, HTTP_CLIENT},
};

use anyhow::Context;
use core::time::Duration;
use poise::{
    async_trait,
    serenity_prelude::{
        Channel, ChannelId, CreateEmbed, CreateMessage, EmbedMessageBuilding as _, GuildId,
        MessageBuilder,
    },
    CreateReply,
};
use serde::Deserialize;
use songbird::{
    driver::{
        opus::{coder::Encoder, SampleRate},
        Bitrate,
    },
    input::{Input, YoutubeDl},
    tracks::PlayMode,
    Call, CoreEvent, Event as SongBirdEvent, EventContext, EventHandler as VoiceEventHandler,
    Songbird,
};
use sqlx::{query, Pool, Postgres};
use std::sync::Arc;
use tokio::{
    sync::{Mutex, MutexGuard},
    time::{sleep, timeout},
};

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
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel first")
                 .await.ok();
                (None, None)
            }
        }
        None => (None, None),
    }
}

async fn get_configured_handler(handler_lock: &Arc<Mutex<Call>>) -> MutexGuard<'_, Call> {
    let mut handler = handler_lock.lock().await;
    handler.set_bitrate(Bitrate::Max);
    handler
}

pub struct VoiceReceiveHandler {
    guild_id: GuildId,
    music_manager: Arc<Songbird>,
    db: Pool<Postgres>,
    encoder: Arc<Mutex<Encoder>>,
}

impl VoiceReceiveHandler {
    pub const fn new(
        guild_id: GuildId,
        music_manager: Arc<Songbird>,
        db: Pool<Postgres>,
        encoder: Arc<Mutex<Encoder>>,
    ) -> Self {
        Self {
            guild_id,
            music_manager,
            db,
            encoder,
        }
    }
}

#[async_trait]
impl VoiceEventHandler for VoiceReceiveHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<SongBirdEvent> {
        if let EventContext::VoiceTick(tick) = ctx {
            for data in tick.speaking.values() {
                if let Some(voice) = &data.decoded_voice {
                    let mut encode_output = vec![0u8; 1280];
                    let value = self.encoder.lock().await.encode(voice, &mut encode_output);
                    if let Ok(encoded_len) = value {
                        encode_output.truncate(encoded_len);
                        if let Ok(mut conn) = self.db.acquire().await {
                            if let Ok(guild_global_call) = query!(
                                "SELECT guild_id FROM guild_settings
                                WHERE guild_id != $1 AND global_call = TRUE",
                                i64::from(self.guild_id)
                            )
                            .fetch_all(&mut *conn)
                            .await
                            {
                                for guild in &guild_global_call {
                                    let current_guild_id = GuildId::new(
                                        u64::try_from(guild.guild_id)
                                            .expect("guild id out of bounds for u64"),
                                    );
                                    if let Some(global_handler_lock) =
                                        self.music_manager.get(current_guild_id)
                                    {
                                        get_configured_handler(&global_handler_lock)
                                            .await
                                            .play_input(Input::from(encode_output.clone()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
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
        ctx.defer().await?;
        if let Ok(request) = HTTP_CLIENT
            .get(format!("https://api.deezer.com/playlist/{playlist_id}"))
            .send()
            .await
        {
            match request
                .json::<DeezerResponse>()
                .await
                .ok()
                .filter(|output| !output.tracks.data.is_empty())
            {
                Some(payload) => {
                    for track in payload.tracks.data {
                        let title = track.title;
                        let artist = track.artist.name;
                        let search = format!("{title} {artist}");
                        let src = Input::from(YoutubeDl::new_search(HTTP_CLIENT.clone(), search));
                        get_configured_handler(&handler_lock)
                            .await
                            .enqueue_input(src)
                            .await;
                    }
                    ctx.reply("Added playlist to queue").await?;
                }
                None => {
                    ctx.reply("Deezer refused to serve your request").await?;
                }
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
        ctx.data()
            .global_chat_last
            .remove(&guild_id)
            .unwrap_or_default();
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
        let result = timeout(Duration::from_secs(60), async {
            loop {
                let other_calls = query!(
                    "SELECT EXISTS(
                        SELECT 1 FROM guild_settings
                        WHERE guild_id != $1 AND global_music = TRUE
                    ) AS HAS_CALL",
                    guild_id_i64
                )
                .fetch_optional(&mut *tx)
                .await?;
                if other_calls.is_some() {
                    return Ok::<_, Error>(true);
                }
                sleep(Duration::from_secs(5)).await;
            }
        })
        .await;
        let found_call = result.unwrap_or(Ok(false))?;
        if found_call {
            message
                .edit(
                    ctx,
                    CreateReply::default().content("Connected to global music playback!"),
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
                    CreateReply::default().content("No one joined the party within 1 minute 😢"),
                )
                .await?;
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
        let reply = match channel_id {
            Some(channel_id) => {
                if let Ok(handler_lock) = ctx.data().music_manager.join(guild_id, channel_id).await
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
                    let mut handler = handler_lock.lock().await;
                    let mut encoder = Encoder::new(
                        SampleRate::Hz48000,
                        songbird::driver::opus::Channels::Stereo,
                        songbird::driver::opus::Application::Voip,
                    )
                    .unwrap();
                    encoder.set_bitrate(Bitrate::Max).unwrap();
                    handler.add_global_event(
                        CoreEvent::VoiceTick.into(),
                        VoiceReceiveHandler::new(
                            guild_id,
                            ctx.data().music_manager.clone(),
                            ctx.data().db.clone(),
                            Arc::new(Mutex::new(encoder)),
                        ),
                    );

                    "I've joined the party"
                } else {
                    "I don't wanna join"
                }
            }
            None => "I don't wanna join",
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
        let reply = match channel_id {
            Some(channel_id) => {
                if ctx
                    .data()
                    .music_manager
                    .join(guild_id, channel_id)
                    .await
                    .is_ok()
                {
                    "I've joined the party"
                } else {
                    "I don't wanna join"
                }
            }
            None => "I don't wanna join",
        };
        ctx.reply(reply).await?;
    }
    Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn leave_voice(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        match &ctx.data().music_manager.remove(guild_id).await {
            Ok(()) => {
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
            }
            Err(_) => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}

/// Continue/pause the current playing song
#[poise::command(prefix_command, slash_command)]
pub async fn pause_continue_song(ctx: SContext<'_>) -> Result<(), Error> {
    if let (Some(handler_lock), Some(guild_id)) = voice_check(&ctx).await {
        let handler = get_configured_handler(&handler_lock).await;
        if handler.queue().current().is_some() {
            let guild_id_i64 = i64::from(guild_id);
            let guild_global_music = query!("SELECT guild_id, global_music FROM guild_settings",)
                .fetch_all(&mut *ctx.data().db.acquire().await?)
                .await?;
            for guild in &guild_global_music {
                let (current_track_opt, global_channel) = if guild.guild_id == guild_id_i64 {
                    (handler.queue().current(), None)
                } else if guild.global_music == Some(true) {
                    let current_guild_id = GuildId::new(
                        u64::try_from(guild.guild_id).expect("guild id out of bounds for u64"),
                    );
                    match ctx.data().music_manager.get(current_guild_id) {
                        Some(global_handler_lock) => {
                            let handler = get_configured_handler(&global_handler_lock).await;
                            let channel_opt = if let Some(id) = handler.current_channel() {
                                (ctx.http().get_channel(ChannelId::from(id.get())).await)
                                    .map_or_else(|_| None, Channel::guild)
                            } else {
                                None
                            };
                            (handler.queue().current(), channel_opt)
                        }
                        None => (None, None),
                    }
                } else {
                    (None, None)
                };
                if let Some(current_track) = current_track_opt {
                    match current_track.get_info().await {
                        Ok(track_info) => {
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
                        }
                        Err(_) => {
                            ctx.reply("Failed to get current track information").await?;
                        }
                    }
                } else {
                    ctx.reply("Bruh, nothing is playing!").await?;
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
    if let (Some(handler_lock), Some(guild_id)) = voice_check(&ctx).await {
        ctx.defer().await?;
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
        match Input::from(src.clone()).aux_metadata().await {
            Ok(m) => {
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
                    e = e.field("Duration:", format!("{duration:?}"), true);
                }
                if let Some(title) = title {
                    match source_url {
                        Some(u) => {
                            e = e.description(
                                MessageBuilder::default()
                                    .push_named_link_safe(title.as_str(), u.as_str())
                                    .build(),
                            );
                        }
                        None => {
                            e = e.description(
                                MessageBuilder::default().push_safe(title.as_str()).build(),
                            );
                        }
                    }
                }
                if let Some(url) = thumbnail {
                    e = e.image(url);
                };
                let queue_len = {
                    let mut handler = get_configured_handler(&handler_lock).await;
                    handler.enqueue_input(Input::from(src.clone())).await;
                    handler.queue().len() - 1
                };
                e = e.field("Position:", format!("{queue_len}"), true);
                ctx.send(CreateReply::default().embed(e.clone())).await?;
                let guild_id_i64 = i64::from(guild_id);
                let guild_global_music = query!(
                    "SELECT guild_id FROM guild_settings 
                    WHERE guild_id != $1 AND global_music = true",
                    guild_id_i64
                )
                .fetch_all(&mut *ctx.data().db.acquire().await?)
                .await?;
                for guild in &guild_global_music {
                    let current_guild_id = GuildId::new(
                        u64::try_from(guild.guild_id).expect("guild id out of bounds for u64"),
                    );
                    match ctx.data().music_manager.get(current_guild_id) {
                        Some(global_handler_lock) => {
                            let mut handler = get_configured_handler(&global_handler_lock).await;
                            handler.enqueue_input(Input::from(src.clone())).await;
                            if let Some(id) = handler.current_channel() {
                                if let Ok(channel) =
                                    ctx.http().get_channel(ChannelId::from(id.get())).await
                                {
                                    if let Some(guild_channel) = channel.guild() {
                                        guild_channel
                                            .send_message(
                                                ctx.http(),
                                                CreateMessage::default().embed(e.clone()),
                                            )
                                            .await?;
                                    }
                                }
                            }
                        }
                        None => continue,
                    }
                }
            }
            Err(_) => {
                ctx.reply("Like you, nothing is known about this song")
                    .await?;
            }
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
        let handler = get_configured_handler(&handler_lock).await;
        if let Some(current_playback) = handler.queue().current() {
            if let Ok(current_playback_info) = current_playback.get_info().await {
                let current_position = current_playback_info.position;
                let Ok(seconds_value) = seconds.parse::<i64>() else {
                    ctx.reply("Bruh, provide a valid number with a sign (e.g. '+20' or '-20')!")
                        .await?;
                    return Ok(());
                };
                let current_secs = i64::try_from(current_position.as_secs()).unwrap_or(0);
                if seconds_value.is_negative() {
                    let new_position = u64::try_from(current_secs + seconds_value).unwrap_or(0);
                    let seek = Duration::from_secs(new_position);
                    if !seek.is_zero() {
                        match current_playback.seek_async(seek).await {
                            Ok(_) => {
                                ctx.reply(format!(
                                    "Seeked {} seconds backward",
                                    seconds_value.abs()
                                ))
                                .await?;
                            }
                            Err(_) => {
                                ctx.reply("Failed to seek song backwards").await?;
                            }
                        }
                    } else {
                        ctx.reply(
                            "Bruh, wanting to seek more seconds back than what have been played",
                        )
                        .await?;
                    }
                } else {
                    let seconds_to_add = u64::try_from(seconds_value).unwrap_or(0);
                    let seek = current_position + Duration::from_secs(seconds_to_add);
                    match current_playback.seek_async(seek).await {
                        Ok(_) => {
                            ctx.reply(format!("Seeked {seconds_value} seconds forward"))
                                .await?;
                        }
                        Err(_) => {
                            ctx.reply("Bruh, you seeked more forward than the length of the song! I'm bailing out")
                                .await?;
                        }
                    }
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
            queue_len - 2
        );
        ctx.reply(&content).await?;
        let guild_global_music = query!(
            "SELECT guild_id FROM guild_settings
                WHERE guild_id != $1 AND global_music = TRUE",
            i64::from(guild_id)
        )
        .fetch_all(&mut *ctx.data().db.acquire().await?)
        .await?;
        for guild in &guild_global_music {
            let current_guild_id = GuildId::new(
                u64::try_from(guild.guild_id).expect("guild id out of bounds for u64"),
            );
            if let Some(global_handler_lock) = ctx.data().music_manager.get(current_guild_id) {
                let channel_id = {
                    let handler = get_configured_handler(&global_handler_lock).await;
                    let queue = handler.queue();
                    if queue.skip().is_err() {
                        continue;
                    }
                    handler.current_channel()
                };

                if let Some(id) = channel_id {
                    if let Ok(channel) = ctx.http().get_channel(ChannelId::from(id.get())).await {
                        if let Some(guild_channel) = channel.guild() {
                            guild_channel.say(ctx.http(), &content).await?;
                        }
                    }
                }
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
        let guild_global_music = query!(
            "SELECT guild_id FROM guild_settings
                WHERE guild_id != $1 AND global_music = TRUE",
            i64::from(guild_id)
        )
        .fetch_all(&mut *ctx.data().db.acquire().await?)
        .await?;
        for guild in &guild_global_music {
            let current_guild_id = GuildId::new(
                u64::try_from(guild.guild_id).expect("guild id out of bounds for u64"),
            );
            if let Some(global_handler_lock) = ctx.data().music_manager.get(current_guild_id) {
                let channel_id = {
                    let handler = get_configured_handler(&global_handler_lock).await;
                    let queue = handler.queue();
                    queue.stop();
                    handler.current_channel()
                };
                if let Some(id) = channel_id {
                    if let Ok(channel) = ctx.http().get_channel(ChannelId::from(id.get())).await {
                        if let Some(guild_channel) = channel.guild() {
                            guild_channel.say(ctx.http(), content).await?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
