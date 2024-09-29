use crate::types::{Error, SContext};

use poise::{
    serenity_prelude::{Channel, CreateEmbed},
    CreateReply,
};
use sqlx::query;

/// To reset or not to reset, that's the question
#[poise::command(
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn reset_settings(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "REPLACE INTO guild_settings (guild_id) VALUES (?)",
            guild_id.get()
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content("Server settings resetted... probably")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// When you want to escape discord
#[poise::command(slash_command)]
pub async fn set_afk(
    ctx: SContext<'_>,
    #[description = "Reason for afk"] reason: Option<String>,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO user_settings (guild_id, user_id, afk, afk_reason) VALUES (?, ?, TRUE, ?)
            ON DUPLICATE KEY UPDATE afk = TRUE, afk_reason = ?",
            guild_id.get(),
            u64::from(ctx.author().id),
            reason,
            reason,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        let embed_reason = match &reason {
            Some(input) => input,
            None => "Didn't renew life subscription",
        };
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::default()
                    .title(format!("{} killed!", ctx.author().display_name()))
                    .description(format!("Reason: {}", embed_reason))
                    .thumbnail(ctx.author().avatar_url().unwrap())
                    .color(0xFF5733),
            ),
        )
        .await?;
    }
    Ok(())
}

/// When you need ai in your life
#[poise::command(
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_chatbot_channel(ctx: SContext<'_>) -> Result<(), Error> {
    ctx.send(
        CreateReply::default()
            .content("To enable ai-sama, create a channel with the topic set to 'ai-chat'")
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Configure the role for the chatbot individually for each user
#[poise::command(prefix_command, slash_command)]
pub async fn set_chatbot_role(
    ctx: SContext<'_>,
    #[description = "The role the bot should take; if not set, then default role"] role: Option<
        String,
    >,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO user_settings (guild_id, user_id, chatbot_role) VALUES (?, ?, ?)
            ON DUPLICATE KEY UPDATE chatbot_role = ?",
            guild_id.get(),
            u64::from(ctx.author().id),
            role,
            role,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content("Role for chatbot set... probably")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure the occurence of dead chat gifs
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_dead_chat(
    ctx: SContext<'_>,
    #[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: u8,
    #[description = "Channel to send dead chat gifs to"] channel: Channel,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let channel_id = channel.id().to_string();
        query!(
            "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel) VALUES (?, ?, ?)
            ON DUPLICATE KEY UPDATE dead_chat_rate = ?, dead_chat_channel = ?",
            guild_id.get(),
            occurrence,
            channel_id,
            occurrence,
            channel_id,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Dead chat gifs will only be sent every {} minute(s) in {}... probably",
                    occurrence, channel
                ))
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure which prefix to use for commands
#[poise::command(
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_prefix(
    ctx: SContext<'_>,
    #[description = "Character(s) to use as prefix for commands, maximum 5"] characters: String,
) -> Result<(), Error> {
    if characters.len() < 5 && !characters.is_empty() {
        if let Some(guild_id) = ctx.guild_id() {
            query!(
                "INSERT INTO guild_settings (guild_id, prefix) VALUES (?, ?)
                ON DUPLICATE KEY UPDATE prefix = ?",
                guild_id.get(),
                characters,
                characters
            )
            .execute(&mut *ctx.data().db.acquire().await?)
            .await?;
            ctx.send(
                CreateReply::default()
                    .content(format!(
                        "{} set as the prefix for commands... probably",
                        characters
                    ))
                    .ephemeral(true),
            )
            .await?;
        }
    }
    Ok(())
}

/// Configure where to send quotes
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_quote_channel(
    ctx: SContext<'_>,
    #[description = "Channel to send quoted messages to"] channel: Channel,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let channel_id = channel.id().to_string();
        query!(
            "INSERT INTO guild_settings (guild_id, quotes_channel) VALUES (?, ?)
            ON DUPLICATE KEY UPDATE quotes_channel = ?",
            guild_id.get(),
            channel_id,
            channel_id,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Quoted messages will be sent to {}... probably",
                    channel
                ))
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure a channel to always spoiler messages
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_spoiler_channel(
    ctx: SContext<'_>,
    #[description = "Channel to send spoilered messages to"] channel: Channel,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let channel_id = channel.id().to_string();
        query!(
            "INSERT INTO guild_settings (guild_id, spoiler_channel) VALUES (?, ?)
            ON DUPLICATE KEY UPDATE quotes_channel = ?",
            guild_id.get(),
            channel_id,
            channel_id,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Spoilered messages will be sent to {}... probably",
                    channel
                ))
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure custom embed sent on user ping
#[poise::command(slash_command)]
pub async fn set_user_ping(
    ctx: SContext<'_>,
    #[description = "Message to send"] content: String,
    #[description = "Image/gif to send. Write waifu to get a random waifu pic"] media: Option<
        String,
    >,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO user_settings (guild_id, user_id, ping_content, ping_media) VALUES (?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE ping_content = ?, ping_media = ?",
            guild_id.get(),
            u64::from(ctx.author().id),
            content,
            media,
            content,
            media,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content("Custom user ping created... probably")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure words to track count of
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_word_track(
    ctx: SContext<'_>,
    #[description = "Word to track count of, maximum 50 characters in length"] word: String,
) -> Result<(), Error> {
    if word.len() < 50 {
        if let Some(guild_id) = ctx.guild_id() {
            query!(
                "INSERT INTO words_count (guild_id, word) VALUES (?, ?)
                ON DUPLICATE KEY UPDATE word = ?, count = 0",
                guild_id.get(),
                word,
                word,
            )
            .execute(&mut *ctx.data().db.acquire().await?)
            .await?;
            ctx.send(
                CreateReply::default()
                    .content(format!("The count of {} will be tracked... probably", word))
                    .ephemeral(true),
            )
            .await?;
        }
    }
    Ok(())
}
