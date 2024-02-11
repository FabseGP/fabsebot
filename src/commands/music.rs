use crate::types::{Context, Error};

use songbird::{
    events::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent},
    input::YoutubeDl,
    SerenityInit,
};

/// Add songs to the queue
#[poise::command(slash_command, prefix_command)]
pub async fn add_queue(
    ctx: Context<'_>,
    #[description = "YouTube link"] track: String,
) -> Result<(), Error> {
    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("rip singing bird")
        .clone();
    if let Some(handler_lock) = manager.get(ctx.guild_id().unwrap()) {
        let mut handler = handler_lock.lock().await;
        let source = match ytdl(track, true).await {
            Ok(source) => source,
            Err(_why) => {
                ctx.say("Couldn't fetch the song").await?;
                return Ok(());
            }
        };
        handler.enqueue_input(source.into());
        ctx.say(format!(
            "Added song to queue: position {}",
            handler.queue().len()
        ))
        .await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }
    Ok(())
}

/// Join your current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn join_voice(ctx: Context<'_>) -> Result<(), Error> {
    let guild = ctx.guild().expect("Could not get guild");
    let guild_id = guild.id;
    let channel_id = guild
        .voice_states
        .get(&ctx.author().id)
        .and_then(|voice_state| voice_state.channel_id);
    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            ctx.say("Not in a voice channel");
            return Ok(());
        }
    };
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    let _ = manager.join(guild_id, connect_to).await;
    ctx.say("Never gonna give you up!").await?;
    Ok(())
}

/// Leave the current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn leave_voice(ctx: Context<'_>) -> Result<(), Error> {
    let guild = ctx.guild().expect("Could not get guild");
    let guild_id = guild.id;
    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    let has_handler = manager.get(guild_id).is_some();
    if has_handler {
        manager.remove(guild_id).await?;
        ctx.say("Left voice channel").await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }
    Ok(())
}

/// Play song in the current voice channel
#[poise::command(slash_command, prefix_command)]
pub async fn play_song(
    ctx: Context<'_>,
    #[description = "YouTube link"] track: String,
) -> Result<(), Error> {
    let do_search = !track.starts_with("http");
    let http_client = {
        let data = ctx.data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("Guaranteed to exist in the typemap.")
    };
    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("rip singing bird")
        .clone();
    if let Some(handler_lock) = manager.get(ctx.guild_id().unwrap()) {
        let mut handler = handler_lock.lock().await;
        let source = match songbird::ytdl(&track).await {
            Ok(source) => source,
            Err(_why) => {
                ctx.say("Couldn't fetch the song").await?;
                return Ok(());
            }
        };
        let title = source
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| "Unknown Title".to_string());
        handler.enqueue_input(source);
        ctx.say(format!("Playing \"{}\"", title)).await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }
    Ok(())
}

/// Skip to the next song in queue
#[poise::command(slash_command, prefix_command)]
pub async fn skip_song(ctx: Context<'_>) -> Result<(), Error> {
    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("rip singing bird")
        .clone();
    if let Some(handler_lock) = manager.get(ctx.guild_id().unwrap()) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        let _ = queue.skip();
        ctx.say(format!("Song skipped: {} in queue.", queue.len()))
            .await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }
    Ok(())
}

/// Stop current playing song
#[poise::command(slash_command, prefix_command)]
pub async fn stop_song(ctx: Context<'_>) -> Result<(), Error> {
    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("rip singing bird")
        .clone();
    if let Some(handler_lock) = manager.get(ctx.guild_id().unwrap()) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        queue.stop();
        ctx.say("Queue cleared").await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }
    Ok(())
}
