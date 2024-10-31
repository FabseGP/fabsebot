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
use std::sync::Arc;
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

/// Join your current voice channel
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
    if let Some(handler_lock) = voice_check(ctx).await {
        let handler = get_configured_handler(&handler_lock).await;
        if let Some(current_track) = handler.queue().current() {
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
                    ctx.reply(response).await?;
                }
                Err(_) => {
                    ctx.reply("Failed to get current track information").await?;
                }
            }
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
    if let Some(handler_lock) = voice_check(ctx).await {
        ctx.defer().await?;
        let mut src = if url.starts_with("https") {
            if url.contains("youtu") {
                Input::from(YoutubeDl::new(HTTP_CLIENT.clone(), url))
            } else {
                ctx.reply("Only YouTube-links are supported").await?;
                return Ok(());
            }
        } else {
            Input::from(YoutubeDl::new_search(HTTP_CLIENT.clone(), url))
        };
        match src.aux_metadata().await {
            Ok(m) => {
                let queue_len = {
                    let mut handler = get_configured_handler(&handler_lock).await;
                    handler.enqueue_input(src).await;
                    handler.queue().len()
                };
                let artist = &m.artist;
                let thumbnail = &m.thumbnail;
                let title = &m.title;
                let source_url = &m.source_url;
                let duration = &m.duration;
                ctx.send(CreateReply::default().embed({
                    let mut e = CreateEmbed::default().colour(0xED333B).field(
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
                    e = e.field("Position:", format!("{queue_len}"), true);
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
    if let Some(handler_lock) = voice_check(ctx).await {
        let handler = get_configured_handler(&handler_lock).await;
        let queue = handler.queue();
        if queue.len() - 1 != 0 {
            if queue.skip().is_ok() {
                let queue_len = queue.len() - 2;
                ctx.reply(format!("Song skipped. {queue_len} left in queue"))
                    .await?;
            } else {
                ctx.reply("Song couldn't be skipped, try again!").await?;
            }
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
        if !handler_lock.lock().await.queue().is_empty() {
            handler_lock.lock().await.queue().stop();
            ctx.reply("Queue cleared").await?;
        } else {
            ctx.reply("Bruh, empty queue!").await?;
        }
    }
    Ok(())
}
