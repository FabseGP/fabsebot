use crate::types::{Context, Error};

use poise::serenity_prelude::{CreateEmbed, EmbedMessageBuilding, MessageBuilder};
use poise::CreateReply;
use serde::{Deserialize, Serialize};
use songbird::input::{Compose, YoutubeDl};

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

/// Play all songs in a playlist from Deezer
#[poise::command(slash_command, prefix_command)]
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
            ctx.say("Added playlist to queue").await?;
        }
    } else {
        ctx.say("Bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}

/// Join your current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn join_voice(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild().unwrap().id;
    let channel_id = ctx
        .guild()
        .unwrap()
        .voice_states
        .get(&ctx.author().id)
        .and_then(|voice_state| voice_state.channel_id);
    let manager = &ctx.data().music_manager;
    manager.join(guild_id, channel_id.unwrap()).await?;
    ctx.say("I've joined the party").await?;
    Ok(())
}

/// Leave the current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn leave_voice(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild().unwrap().id;
    let manager = &ctx.data().music_manager;
    let has_handler = manager.get(guild_id).is_some();
    if has_handler {
        manager.remove(guild_id).await?;
        ctx.say("Left voice channel, don't forget me").await?;
    } else {
        ctx.reply("Bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}

/// Play song / add song to queue in the current voice channel
#[poise::command(slash_command, prefix_command)]
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
                ctx.say("Like you, nothing is known about this song")
                    .await?;
            }
        }
    } else {
        ctx.say("Bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}

/// Skip to the next song in queue
#[poise::command(slash_command, prefix_command)]
pub async fn skip_song(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        queue.skip()?;
        ctx.say(format!("Song skipped. {} left in queue", queue.len() - 2))
            .await?;
    } else {
        ctx.say("Bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}

/// Stop current playing song
#[poise::command(slash_command, prefix_command)]
pub async fn stop_song(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = &ctx.data().music_manager;
    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        queue.stop();
        ctx.say("Queue cleared").await?;
    } else {
        ctx.say("Bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}
