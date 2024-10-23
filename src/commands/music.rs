use crate::types::{Error, SContext, HTTP_CLIENT};

use poise::{
    async_trait,
    serenity_prelude::{ChannelId, CreateEmbed, EmbedMessageBuilding, Http, MessageBuilder},
    CreateReply,
};
use serde::Deserialize;
use songbird::{
    driver::Bitrate,
    input::{Compose, Input, YoutubeDl},
    tracks::PlayMode,
    Call, Config, Event as SongbirdEvent, EventContext, EventHandler as VoiceEventHandler,
    TrackEvent,
};
use std::{borrow::Cow, sync::Arc, time::Duration};

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

async fn voice_check(ctx: SContext<'_>) -> Result<bool, Error> {
    match ctx.guild_id() {
        Some(guild_id) => match ctx.data().music_manager.get(guild_id) {
            Some(_) => Ok(true),
            None => {
                ctx.reply("Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel first")
                    .await?;
                Ok(false)
            }
        },
        None => Ok(false),
    }
}

fn configure_call(handler: &mut Call) {
    let new_config = Config::default().use_softclip(false);
    handler.set_config(new_config);
    handler.set_bitrate(Bitrate::Max);
}

/// Play all songs in a playlist from Deezer
#[poise::command(prefix_command, slash_command, check = "voice_check")]
pub async fn add_playlist(
    ctx: SContext<'_>,
    #[description = "ID of the playlist in mind"]
    #[rest]
    playlist_id: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        ctx.defer().await?;
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let request = HTTP_CLIENT
                    .get(format!("https://api.deezer.com/playlist/{playlist_id}"))
                    .send()
                    .await?;
                let data: Option<DeezerResponse> = request.json().await?;
                if let Some(payload) = data {
                    if !payload.tracks.data.is_empty() {
                        let mut handler = handler_lock.lock().await;
                        for track in payload.tracks.data {
                            let title = track.title;
                            let artist = track.artist.name;
                            let search = format!("{title} {artist}");
                            let src = YoutubeDl::new_search(HTTP_CLIENT.clone(), search);
                            handler.enqueue_input(Input::from(src)).await;
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

struct TrackEndNotifier {
    channel_id: ChannelId,
    http: Arc<Http>,
}

#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<SongbirdEvent> {
        if let EventContext::Track(track_list) = ctx {
            let track_len = track_list.len();
            let _ = self
                .channel_id
                .say(&self.http, &format!("Tracks ended: {track_len}."))
                .await;
        }

        None
    }
}

/// Join your current voice channel
#[poise::command(prefix_command, slash_command)]
pub async fn join_voice(ctx: SContext<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(g) => g.clone(),
        None => {
            return Ok(());
        }
    };
    match guild
        .voice_states
        .get(&ctx.author().id)
        .and_then(|voice_state| voice_state.channel_id)
    {
        Some(channel_id) => {
            let manager = &ctx.data().music_manager;
            match manager.join(guild.id, channel_id).await {
                Ok(handler_lock) => {
                    let mut handle = handler_lock.lock().await;
                    handle.add_global_event(
                        SongbirdEvent::Track(TrackEvent::End),
                        TrackEndNotifier {
                            channel_id,
                            http: ctx.serenity_context().http.clone(),
                        },
                    );
                    ctx.reply("I've joined the party").await?;
                }
                Err(_) => {
                    ctx.reply("I don't wanna join").await?;
                }
            }
        }
        None => {
            return Ok(());
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
    ctx: SContext<'_>,
    #[description = "Link to the song or query to search"]
    #[rest]
    url: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        ctx.defer().await?;
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;
                configure_call(&mut handler);
                let url_cow: Cow<'static, str> = Cow::Owned(url);
                let mut src = if url_cow.starts_with("http") {
                    YoutubeDl::new(HTTP_CLIENT.clone(), url_cow.clone())
                } else {
                    YoutubeDl::new_search(HTTP_CLIENT.clone(), url_cow.clone())
                };
                let metadata = src.aux_metadata().await;
                handler.enqueue_input(Input::from(src)).await;
                match metadata {
                    Ok(m) => {
                        let artist = &m.artist;
                        let thumbnail = &m.thumbnail;
                        let title = &m.title;
                        let source_url = &m.source_url;
                        let duration = &m.duration;
                        let queue_len = handler.queue().len();
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
    ctx: SContext<'_>,
    #[description = "Seconds to seek"] seconds: u64,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        ctx.defer().await?;
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
                        if !seek.is_zero() {
                            current_playback.seek_async(seek).await?;
                            ctx.reply(format!("Seeked {seconds} seconds backward"))
                                .await?;
                        } else {
                            ctx.reply("Can't seek back for more seconds than what already have been played")
                            .await?;
                        }
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
    ctx: SContext<'_>,
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
                        ctx.reply(format!("Seeked {seconds} seconds forward"))
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
pub async fn skip_song(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let manager = &ctx.data().music_manager;
        match manager.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;
                configure_call(&mut handler);
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
pub async fn stop_song(ctx: SContext<'_>) -> Result<(), Error> {
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
