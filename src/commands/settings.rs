use crate::types::{Context, Error};

use poise::CreateReply;

/// Configure the occurence of dead chat gifs
#[poise::command(slash_command, prefix_command, owners_only)]
pub async fn dead_chat(
    ctx: Context<'_>,
    #[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: u8,
    #[description = "Channel to send dead chat gifs to"] channel: serenity::model::channel::Channel,
) -> Result<(), Error> {
    let mut conn = ctx.data().db.acquire().await?;
    sqlx::query!(
        "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel) VALUES (?, ?, ?, 0, 0)
        ON DUPLICATE KEY UPDATE dead_chat_rate = ?, dead_chat_channel = ?",
        ctx.guild_id().unwrap().get(),
        occurrence,
        channel.id().to_string(),
        occurrence,
        channel.id().to_string()
    )
    .bind(ctx.guild_id().unwrap().get())
    .bind(occurrence)
    .bind(channel.id().to_string())
    .execute(&mut *conn)
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
    Ok(())
}

/// Configure the occurence of dead chat gifs
#[poise::command(slash_command, prefix_command, owners_only)]
pub async fn quote_channel(
    ctx: Context<'_>,
    #[description = "Channel to send quoted messages to"]
    channel: serenity::model::channel::Channel,
) -> Result<(), Error> {
    let mut conn = ctx.data().db.acquire().await?;
    sqlx::query!(
        "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel) VALUES (?, 0, 0, ?, 0)
        ON DUPLICATE KEY UPDATE quotes_channel = ?", ctx.guild_id().unwrap().get(), channel.id().to_string(), channel.id().to_string()
    )
    .execute(&mut *conn)
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
    Ok(())
}

/// Configure the occurence of dead chat gifs
#[poise::command(slash_command, prefix_command, owners_only)]
pub async fn spoiler_channel(
    ctx: Context<'_>,
    #[description = "Channel to send spoilered messages to"]
    channel: serenity::model::channel::Channel,
) -> Result<(), Error> {
    let mut conn = ctx.data().db.acquire().await?;
    sqlx::query!(
        "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, quotes_channel, spoiler_channel) VALUES (?, 0, 0, 0, ?)
        ON DUPLICATE KEY UPDATE quotes_channel = ?", ctx.guild_id().unwrap().get(), channel.id().to_string(), channel.id().to_string()
    )
    .execute(&mut *conn)
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
    Ok(())
}
