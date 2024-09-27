use crate::types::{Context, Error};

use poise::{
    serenity_prelude::{CreateEmbed, EmbedMessageBuilding, MessageBuilder},
    CreateReply,
};
use serde::Deserialize;
use songbird::{
    driver::Bitrate,
    input::{Compose, YoutubeDl},
    tracks::PlayMode,
    Call, Config,
};
use std::{num::NonZeroUsize, time::Duration};

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

fn configure_call(handler: &mut Call) {
    let new_config = Config::default()
        .use_softclip(false)
        .playout_buffer_length(NonZeroUsize::new(750).unwrap())
        .playout_spike_length(375);
    handler.set_config(new_config);
    handler.set_bitrate(Bitrate::Max);
}

/// Play all songs in a playlist from Deezer
#[poise::command(prefix_command, slash_command)]
pub async fn add_playlist(
    ctx: Context<'_>,
    #[description = "ID of the playlist in mind"]
    #[rest]
    playlist_id: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let client = &ctx.data().req_client;
                let request = client
                    .get(format!("https://api.deezer.com/playlist/{}", playlist_id))
                    .send()
                    .await?;
                let data: Option<DeezerResponse> = request.json().await?;
                if let Some(payload) = data {
                    if !payload.tracks.data.is_empty() {
                        let mut handler = handler_lock.lock().await;
                        for track in payload.tracks.data {
                            let search = format! {"{} {}", track.title, track.artist.name};
                            let src = YoutubeDl::new_search(client.clone(), search);
                            handler.enqueue_input(src.into()).await;
                        }
                        ctx.reply("Added playlist to queue").await?;
                    }
                }
            }
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}

/// Join your current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn join_voice(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let channel_id = ctx
            .guild()
            .unwrap()
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id);
        let manager = &ctx.data().music_manager;
        manager.join(guild_id, channel_id.unwrap()).await?;
        ctx.reply("I've joined the party").await?;
    }
    Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn leave_voice(ctx: Context<'_>) -> Result<(), Error> {
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
pub async fn pause_continue_song(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;
                configure_call(&mut handler);
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
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}

/// Play song / add song to queue in the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn play_song(
    ctx: Context<'_>,
    #[description = "Link to the song or query to search"]
    #[rest]
    url: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;
                configure_call(&mut handler);
                let mut src = if url.starts_with("http") {
                    YoutubeDl::new(ctx.data().req_client.clone(), url.clone())
                } else {
                    YoutubeDl::new_search(ctx.data().req_client.clone(), url.clone())
                };
                let metadata = src.aux_metadata().await;
                handler.enqueue_input(src.into()).await;
                match metadata {
                    Ok(m) => {
                        let artist = &m.artist;
                        let thumbnail = &m.thumbnail;
                        let title = &m.title;
                        let source_url = &m.source_url;
                        let duration = &m.duration;
                        ctx.send(CreateReply::default().embed({
                            let mut e = CreateEmbed::default();
                            e = e
                                .colour(0xED333B)
                                .field("Added by:", ctx.author().display_name(), false)
                                .url(url);
                            if let Some(artist) = artist {
                                e = e.field("Artist:", artist, true);
                            }
                            if let Some(duration) = duration {
                                e = e.field("Duration:", format!("{:?}", duration), true);
                            }
                            e = e.field("Position:", format!("{:?}", handler.queue().len()), true);
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
                                            MessageBuilder::default()
                                                .push_safe(title.as_str())
                                                .build(),
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
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}

/// Seek current playing song backward
#[poise::command(prefix_command, slash_command)]
pub async fn seek_song_backward(
    ctx: Context<'_>,
    #[description = "Seconds to seek"] seconds: u64,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;
                configure_call(&mut handler);
                let queue = handler.queue();
                if let Some(current_playback) = queue.current() {
                    if let Ok(current_playback_info) = current_playback.get_info().await {
                        let current_position = current_playback_info.position;
                        let seek = current_position - Duration::from_secs(seconds);
                        current_playback.seek_async(seek).await?;
                        ctx.reply(format!("Seeked {} seconds backward", seconds))
                            .await?;
                    }
                }
            }
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}

/// Seek current playing song forward
#[poise::command(prefix_command, slash_command)]
pub async fn seek_song_forward(
    ctx: Context<'_>,
    #[description = "Seconds to seek"] seconds: u64,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;
                configure_call(&mut handler);
                let queue = handler.queue();
                if let Some(current_playback) = queue.current() {
                    if let Ok(current_playback_info) = current_playback.get_info().await {
                        let current_position = current_playback_info.position;
                        let seek = current_position + Duration::from_secs(seconds);
                        current_playback.seek_async(seek).await?;
                        ctx.reply(format!("Seeked {} seconds forward", seconds))
                            .await?;
                    }
                }
            }
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}

/// Skip to the next song in queue
#[poise::command(prefix_command, slash_command)]
pub async fn skip_song(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;
                configure_call(&mut handler);
                let queue = handler.queue();
                if queue.len() - 1 != 0 {
                    queue.skip()?;
                    ctx.reply(format!("Song skipped. {} left in queue", queue.len() - 2))
                        .await?;
                } else {
                    ctx.reply("No songs to skip!").await?;
                }
            }
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}

/// Stop current playing song
#[poise::command(prefix_command, slash_command)]
pub async fn stop_song(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let handler = handler_lock.lock().await;
                let queue = handler.queue();
                queue.stop();
                ctx.reply("Queue cleared").await?;
            }
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse /join_voice in a voice channel first")
                    .await?;
            }
        }
    }
    Ok(())
}
