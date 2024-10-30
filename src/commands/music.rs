use crate::types::{Error, SContext, HTTP_CLIENT};

use core::time::Duration;
use poise::{
    serenity_prelude::{CreateEmbed, EmbedMessageBuilding as _, MessageBuilder},
    CreateReply,
};
use serde::Deserialize;
use songbird::{
    driver::Bitrate,
    input::{Input, YoutubeDl},
    tracks::PlayMode,
    Call, Config,
};
use std::{borrow::Cow, sync::Arc};
use tokio::sync::{Mutex, MutexGuard};

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

async fn voice_check(ctx: SContext<'_>) -> Option<Arc<Mutex<Call>>> {
    match ctx.guild_id() {
        Some(guild_id) => {
            if let Some(handler_lock) = ctx.data().music_manager.get(guild_id) {
                Some(handler_lock)
            } else {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel first")
                 .await.ok();
                None
            }
        }
        None => None,
    }
}

async fn get_configured_handler(handler_lock: &Arc<Mutex<Call>>) -> MutexGuard<'_, Call> {
    let mut handler = handler_lock.lock().await;
    let new_config = Config::default().use_softclip(false);
    handler.set_config(new_config);
    handler.set_bitrate(Bitrate::Max);
    handler
}

/// Play all songs in a playlist from Deezer
#[poise::command(prefix_command, slash_command)]
pub async fn add_playlist(
    ctx: SContext<'_>,
    #[description = "ID of the playlist in mind"]
    #[rest]
    playlist_id: String,
) -> Result<(), Error> {
    if let Some(handler_lock) = voice_check(ctx).await {
        ctx.defer().await?;
        let request = HTTP_CLIENT
            .get(format!("https://api.deezer.com/playlist/{playlist_id}"))
            .send()
            .await?;
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
    }
    Ok(())
}

/// Join your current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn join_voice(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        ctx.defer().await?;
        let channel_id = match ctx.guild() {
            Some(guild) => guild
                .voice_states
                .get(&ctx.author().id)
                .and_then(|voice_state| voice_state.channel_id),
            None => None,
        };
        if let Some(channel_id) = channel_id {
            let manager = &ctx.data().music_manager;
            ctx.reply(match manager.join(guild_id, channel_id).await {
                Ok(_) => "I've joined the party",
                Err(_) => "I don't wanna join",
            })
            .await?;
        } else {
            ctx.reply("I don't wanna join").await?;
        }
    }
    Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn leave_voice(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        let handler = manager.get(guild_id);
        match handler {
            Some(_) => {
                manager.remove(guild_id).await?;
                ctx.reply("Left voice channel, don't forget me").await?;
            }
            None => {
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
    if let Some(handler_lock) = voice_check(ctx).await {
        let handler = get_configured_handler(&handler_lock).await;
        let queue = handler.queue();
        if let Some(current_playback) = queue.current() {
            if let Ok(current_playback_info) = current_playback.get_info().await {
                let status = current_playback_info.playing;
                match status {
                    PlayMode::Pause => current_playback.play().unwrap(),
                    PlayMode::Play => current_playback.pause().unwrap(),
                    _ => {
                        ctx.reply("Bruh, no song is playing").await?;
                        return Ok(());
                    }
                }
                ctx.reply("Song is either continued or paused").await?;
            }
        }
    }
    Ok(())
}

/// Play song / add song to queue in the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn play_song(
    ctx: SContext<'_>,
    #[description = "Link to the song or query to search"]
    #[rest]
    url: String,
) -> Result<(), Error> {
    if let Some(handler_lock) = voice_check(ctx).await {
        ctx.defer().await?;
        let url_cow: Cow<'static, str> = Cow::Owned(url);
        let mut src = if url_cow.starts_with("http") {
            Input::from(YoutubeDl::new(HTTP_CLIENT.clone(), url_cow.clone()))
        } else {
            Input::from(YoutubeDl::new_search(HTTP_CLIENT.clone(), url_cow.clone()))
        };
        let metadata = src.aux_metadata().await;
        let queue_len = {
            let mut handler = get_configured_handler(&handler_lock).await;
            handler.enqueue_input(src).await;
            handler.queue().len()
        };
        match metadata {
            Ok(m) => {
                let artist = &m.artist;
                let thumbnail = &m.thumbnail;
                let title = &m.title;
                let source_url = &m.source_url;
                let duration = &m.duration;
                ctx.send(CreateReply::default().embed({
                    let mut e = CreateEmbed::default()
                        .colour(0xED333B)
                        .field("Added by:", ctx.author().display_name(), false)
                        .url(url_cow);
                    if let Some(artist) = artist {
                        e = e.field("Artist:", artist, true);
                    }
                    if let Some(duration) = duration {
                        e = e.field("Duration:", format!("{duration:?}"), true);
                    }
                    e = e.field("Position:", format!("{queue_len}"), true);
                    if let Some(url) = source_url {
                        e = e.url(url);
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
                    e
                }))
                .await?;
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
    if let Some(handler_lock) = voice_check(ctx).await {
        ctx.defer().await?;
        let handler = get_configured_handler(&handler_lock).await;
        let queue = handler.queue();
        if let Some(current_playback) = queue.current() {
            if let Ok(current_playback_info) = current_playback.get_info().await {
                let current_position = current_playback_info.position;
                let Ok(seconds_value) = seconds.parse::<i64>() else {
                    ctx.reply("Bruh, provide a valid number with a sign (e.g. '+20' or '-20')!")
                        .await?;
                    return Ok(());
                };
                let current_secs = i64::try_from(current_position.as_secs()).unwrap_or(0);

                if seconds_value.is_negative() {
                    if current_secs + seconds_value < 0 {
                        ctx.reply(
                            "Can't seek back for more seconds than what already have been played",
                        )
                        .await?;
                        return Ok(());
                    }
                    let new_position = u64::try_from(current_secs + seconds_value).unwrap_or(0);
                    let seek = Duration::from_secs(new_position);

                    if !seek.is_zero() {
                        current_playback.seek_async(seek).await?;
                        ctx.reply(format!("Seeked {} seconds backward", seconds_value.abs()))
                            .await?;
                    }
                } else {
                    let seconds_to_add = u64::try_from(seconds_value).unwrap_or(0);
                    let seek = current_position + Duration::from_secs(seconds_to_add);
                    if !seek.is_zero() {
                        current_playback.seek_async(seek).await?;
                        ctx.reply(format!("Seeked {seconds_value} seconds forward"))
                            .await?;
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
    if let Some(handler_lock) = voice_check(ctx).await {
        let handler = get_configured_handler(&handler_lock).await;
        let queue = handler.queue();
        if queue.len() - 1 != 0 {
            queue.skip()?;
            let queue_len = queue.len() - 2;
            ctx.reply(format!("Song skipped. {queue_len} left in queue"))
                .await?;
        } else {
            ctx.reply("No songs to skip!").await?;
        }
    }
    Ok(())
}

/// Stop current playing song
#[poise::command(prefix_command, slash_command)]
pub async fn stop_song(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(handler_lock) = voice_check(ctx).await {
        handler_lock.lock().await.queue().stop();
        ctx.reply("Queue cleared").await?;
    }
    Ok(())
}
