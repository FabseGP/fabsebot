use crate::types::{Context, Error};

use poise::CreateReply;
use serenity::model::channel::Channel;
use sqlx::query;

/// Configure the occurence of dead chat gifs
#[poise::command(prefix_command, slash_command)]
pub async fn dead_chat(
    ctx: Context<'_>,
    #[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: u8,
    #[description = "Channel to send dead chat gifs to"] channel: Channel,
) -> Result<(), Error> {
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id {
        query!(
        "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel, prefix) VALUES (?, ?, ?, 0, 0, ?)
        ON DUPLICATE KEY UPDATE dead_chat_rate = ?, dead_chat_channel = ?",
        ctx.guild_id().unwrap().get(),
        occurrence,
        channel.id().to_string(),
        "!",
        occurrence,
        channel.id().to_string()
    )
    .execute(&mut *ctx.data().db.acquire().await?)
    .await
    .unwrap();
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Dead chat gifs will only be sent every {} minute(s) in {}... probably",
                    occurrence, channel
                ))
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're not the owner")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure which prefix to use for commands
#[poise::command(prefix_command, slash_command)]
pub async fn prefix(
    ctx: Context<'_>,
    #[description = "Character(s) to use as prefix for commands"] characters: String,
) -> Result<(), Error> {
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id {
        sqlx::query!(
        "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel, prefix) VALUES (?, 0, 0, 0, 0, ?)
        ON DUPLICATE KEY UPDATE prefix = ?", ctx.guild_id().unwrap().get(), characters, characters
    )
    .execute(&mut *ctx.data().db.acquire().await?)
    .await
    .unwrap();
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "{} set as the prefix for commands... probably",
                    characters
                ))
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're not the owner")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure where to send quotes
#[poise::command(prefix_command, slash_command)]
pub async fn quote_channel(
    ctx: Context<'_>,
    #[description = "Channel to send quoted messages to"] channel: Channel,
) -> Result<(), Error> {
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id {
        sqlx::query!(
        "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel, prefix) VALUES (?, 0, 0, ?, 0, ?)
        ON DUPLICATE KEY UPDATE quotes_channel = ?", ctx.guild_id().unwrap().get(), channel.id().to_string(), channel.id().to_string(), "!"
    )
    .execute(&mut *ctx.data().db.acquire().await?)
    .await
    .unwrap();
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Quoted messages will be sent to {}... probably",
                    channel
                ))
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're not the owner")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// To reset or not to reset, that's the question
#[poise::command(prefix_command, slash_command)]
pub async fn reset_settings(ctx: Context<'_>) -> Result<(), Error> {
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id {
        sqlx::query!(
        "REPLACE INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel, prefix) VALUES (?, 0, 0, 0, 0, ?)", 
        ctx.guild_id().unwrap().get(), "!"
    )
    .execute(&mut *ctx.data().db.acquire().await?)
    .await
    .unwrap();
        ctx.send(
            CreateReply::default()
                .content("Server settings resetted... probably")
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're not the owner")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure a channel to always spoiler messages
#[poise::command(prefix_command, slash_command)]
pub async fn spoiler_channel(
    ctx: Context<'_>,
    #[description = "Channel to send spoilered messages to"] channel: Channel,
) -> Result<(), Error> {
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id {
        sqlx::query!(
        "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel, prefix) VALUES (?, 0, 0, 0, ?, ?)
        ON DUPLICATE KEY UPDATE quotes_channel = ?", ctx.guild_id().unwrap().get(), channel.id().to_string(), channel.id().to_string(), "!"
    )
    .execute(&mut *ctx.data().db.acquire().await?)
    .await
    .unwrap();
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Spoilered messages will be sent to {}... probably",
                    channel
                ))
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're not the owner")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}
