use crate::types::{Context, Error};

use poise::serenity_prelude::CreateEmbed;
use poise::CreateReply;
use serenity::{async_trait, http::Http, model::prelude::ChannelId};
use songbird::{
    events::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent},
    input::{Compose, YoutubeDl},
    SerenityInit,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

struct TrackEndNotifier {
    chan_id: ChannelId,
    http: Arc<Http>,
}

#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            let _ = self
                .chan_id
                .say(&self.http, &format!("Tracks ended: {}.", track_list.len()))
                .await;
        }
        None
    }
}

struct ChannelDurationNotifier {
    chan_id: ChannelId,
    count: Arc<AtomicUsize>,
    http: Arc<Http>,
}

#[async_trait]
impl VoiceEventHandler for ChannelDurationNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let count_before = self.count.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .chan_id
            .say(
                &self.http,
                &format!(
                    "I've been in this channel for {} minutes!",
                    count_before + 1
                ),
            )
            .await;
        None
    }
}

/// Add songs to the queue
#[poise::command(slash_command, prefix_command)]
pub async fn add_queue(
    ctx: Context<'_>,
    #[description = "YouTube link"] url: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        let src = YoutubeDl::new(reqwest::Client::new(), url);
        handler.enqueue_input(src.into()).await;
        ctx.say(format!(
            "Added song to queue: position {}",
            handler.queue().len()
        ))
        .await?;
    } else {
        ctx.say("bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}

/// Join your current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn join_voice(ctx: Context<'_>) -> Result<(), Error> {
    let (guild_id, channel_id) = {
        let guild = ctx.guild().unwrap();
        let channel_id = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id);
        (guild.id, channel_id)
    };
    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            ctx.say("Not in a voice channel").await?;
            return Ok(());
        }
    };
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    if let Ok(handle_lock) = manager.join(guild_id, connect_to).await {
        ctx.say("hurry already and play some songs!").await?;

        let chan_id = ctx.channel_id();
        let send_http = ctx.http();
        let handle = handle_lock.lock().await;
    /*
    handle.add_global_event(
        Event::Track(TrackEvent::End),
        TrackEndNotifier {
            chan_id,
            http: Arc::new(send_http),
        },
    );

    handle.add_global_event(
        Event::Periodic(Duration::from_secs(60), None),
        ChannelDurationNotifier {
            chan_id,
            count: Default::default(),
            http: Arc::new(send_http),
        },
    );*/
    } else {
        ctx.say("blame india for this error, no voice channels joined")
            .await?;
    }
    Ok(())
}

/// Leave the current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn leave_voice(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild().unwrap().id;
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    let has_handler = manager.get(guild_id).is_some();
    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            ctx.say(format!("Failed: {:?}", e)).await?;
        }

        ctx.say("Left voice channel, don't forget me").await?;
    } else {
        ctx.reply("bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}
/// Play song in the current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn play_song(
    ctx: Context<'_>,
    #[description = "YouTube link"] url: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        let mut src = YoutubeDl::new(reqwest::Client::new(), url.clone());
        let metadata = src.aux_metadata().await;
        let _ = handler.enqueue_input(src.into()).await;
        ctx.send(CreateReply::default().embed(CreateEmbed::new().title("Playing song")))
            .await?;
    } else {
        ctx.say("bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}

/// Skip to the next song in queue
#[poise::command(slash_command, prefix_command)]
pub async fn skip_song(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        let _ = queue.skip();
        ctx.say(format!("Song skipped: {} in queue", queue.len()))
            .await?;
    } else {
        ctx.say("bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}

/// Stop current playing song
#[poise::command(slash_command, prefix_command)]
pub async fn stop_song(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        queue.stop();
        ctx.say("Queue cleared").await?;
    } else {
        ctx.say("bruh, I'm not even in a voice channel").await?;
    }
    Ok(())
}
