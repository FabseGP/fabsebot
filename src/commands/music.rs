use crate::types::{Context, Error};

use poise::{
    futures_util::{Stream, StreamExt},
    serenity_prelude::{CreateEmbed, EmbedMessageBuilding, MessageBuilder},
    CreateReply,
};
use serde::{Deserialize, Serialize};
use serenity::futures;
use songbird::{
    driver::Bitrate,
    input::{Compose, YoutubeDl},
    tracks::PlayMode,
    Call, Config,
};
use std::{num::NonZeroUsize, time::Duration};

#[derive(Deserialize, Serialize)]
struct DeezerResponse {
    tracks: DeezerData,
}

#[derive(Deserialize, Serialize)]
struct DeezerData {
    data: Vec<DeezerTracks>,
}

#[derive(Deserialize, Serialize)]
struct DeezerTracks {
    title: String,
    artist: DeezerArtist,
}

#[derive(Deserialize, Serialize)]
struct DeezerArtist {
    name: String,
}

fn configure_call(handler: &mut Call) {
    let mut new_config = handler.config().clone();
    new_config = Config::use_softclip(new_config, false);
    new_config = Config::playout_buffer_length(new_config, NonZeroUsize::new(100).unwrap());
    new_config = Config::playout_spike_length(new_config, 25);
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
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
        let client = &ctx.data().req_client;
        let request = client
            .get(format!("https://api.deezer.com/playlist/{}", playlist_id))
            .send()
            .await?;
        let data: DeezerResponse = request.json().await.unwrap();
        if !data.tracks.data.is_empty() {
            let mut handler = handler_lock.lock().await;
            for track in data.tracks.data {
                let search = format! {"{} {}", track.title, track.artist.name};
                let src = YoutubeDl::new_search(ctx.data().req_client.clone(), search);
                handler.enqueue_input(src.into()).await;
            }
            ctx.reply("Added playlist to queue").await?;
        }
    } else {
        ctx.reply(
            "bruh, I'm not even in a voice channel!\nuse /join_voice in a voice channel first",
        )
        .await?;
    }
    Ok(())
}

/// Join your current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn join_voice(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let channel_id = ctx
        .guild()
        .unwrap()
        .voice_states
        .get(&ctx.author().id)
        .and_then(|voice_state| voice_state.channel_id);
    let manager = &ctx.data().music_manager;
    manager.join(guild_id, channel_id.unwrap()).await?;
    ctx.reply("I've joined the party").await?;
    Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn leave_voice(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    let has_handler = manager.get(guild_id).is_some();
    if has_handler {
        manager.remove(guild_id).await?;
        ctx.reply("Left voice channel, don't forget me").await?;
    } else {
        ctx.reply(
            "bruh, I'm not even in a voice channel!\nuse /join_voice in a voice channel first",
        )
        .await?;
    }
    Ok(())
}

/// Continue/pause the current playing song
#[poise::command(prefix_command, slash_command)]
pub async fn pause_continue_song(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        configure_call(&mut handler);
        let queue = handler.queue();
        let status = queue.current().unwrap().get_info().await.unwrap().playing;
        if status == PlayMode::Pause {
            queue.current().unwrap().play().unwrap();
        } else if status == PlayMode::Play {
            queue.current().unwrap().pause().unwrap();
        } else {
            ctx.reply("bruh, no song is playing").await?;
            return Ok(());
        }
        ctx.reply("Song is either continued or paused").await?;
    } else {
        ctx.reply(
            "bruh, I'm not even in a voice channel!\nuse /join_voice in a voice channel first",
        )
        .await?;
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
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
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
                    let mut e = CreateEmbed::new();
                    e = e
                        .colour(0xED333B)
                        .field("Added by: ", ctx.author().to_string(), false)
                        .url(url);
                    if let Some(artist) = artist {
                        e = e.field("Artist:", format!("{:?}", artist), true);
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
                                    MessageBuilder::new()
                                        .push_named_link_safe(title.as_str(), u.as_str())
                                        .build(),
                                );
                            }
                            None => {
                                e = e.description(
                                    MessageBuilder::new().push_safe(title.as_str()).build(),
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
    } else {
        ctx.reply(
            "bruh, I'm not even in a voice channel!\nuse /join_voice in a voice channel first",
        )
        .await?;
    }
    Ok(())
}

async fn autocomplete_choice<'a>(
    _ctx: Context<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    futures::stream::iter(&["forward", "backward"])
        .filter(move |name| futures::future::ready(name.starts_with(partial)))
        .map(|name| name.to_string())
}

/// Seek current playing song
#[poise::command(prefix_command, slash_command)]
pub async fn seek_song(
    ctx: Context<'_>,
    #[description = "Seconds to seek"] seconds: u64,
    #[description = "Forward or backward"]
    #[autocomplete = "autocomplete_choice"]
    #[rest]
    direction: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        configure_call(&mut handler);
        let queue = handler.queue();
        let current_position = queue.current().unwrap().get_info().await.unwrap().position;
        let seek = if direction == "forward" {
            current_position + Duration::from_secs(seconds)
        } else if direction == "backward" {
            current_position - Duration::from_secs(seconds)
        } else {
            ctx.reply("you managed to destroy the matrix smh").await?;
            return Ok(());
        };
        queue.current().unwrap().seek_async(seek).await?;
        ctx.reply(format!("Seeked {} seconds {}", seconds, direction))
            .await?;
    } else {
        ctx.reply(
            "bruh, I'm not even in a voice channel!\nuse /join_voice in a voice channel first",
        )
        .await?;
    }
    Ok(())
}

/// Skip to the next song in queue
#[poise::command(prefix_command, slash_command)]
pub async fn skip_song(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
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
    } else {
        ctx.reply(
            "bruh, I'm not even in a voice channel!\nuse /join_voice in a voice channel first",
        )
        .await?;
    }
    Ok(())
}

/// Stop current playing song
#[poise::command(prefix_command, slash_command)]
pub async fn stop_song(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        queue.stop();
        ctx.reply("Queue cleared").await?;
    } else {
        ctx.reply(
            "bruh, I'm not even in a voice channel!\nuse /join_voice in a voice channel first",
        )
        .await?;
    }
    Ok(())
}
