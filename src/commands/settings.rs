use crate::types::{Error, SContext, HTTP_CLIENT};

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
            "UPDATE guild_settings
            SET dead_chat_rate = NULL,
                dead_chat_channel = NULL,
                quotes_channel = NULL,
                spoiler_channel = NULL,
                prefix = NULL,
                ai_chat_channel = NULL,
                global_chat_channel = NULL
            WHERE guild_id = $1",
            i64::from(guild_id)
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
            "INSERT INTO user_settings (guild_id, user_id, afk, afk_reason)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT(guild_id, user_id)
            DO UPDATE SET
                afk = TRUE,
                afk_reason = $3",
            i64::from(guild_id),
            i64::from(ctx.author().id),
            reason,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        let embed_reason = reason
            .as_deref()
            .unwrap_or("Didn't renew life subscription");
        let user_name = ctx.author().display_name();
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::default()
                    .title(format!("{user_name} killed!"))
                    .description(format!("Reason: {embed_reason}"))
                    .thumbnail(ctx.author().avatar_url().unwrap_or_default())
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
pub async fn set_chatbot_channel(
    ctx: SContext<'_>,
    #[description = "Channel to act as chatbot in"] channel: Channel,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let channel_id = channel.id();
        query!(
            "INSERT INTO guild_settings (guild_id, ai_chat_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                ai_chat_channel = $2",
            i64::from(guild_id),
            i64::from(channel_id),
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!("AI-sama is alive in {channel}... probably"))
                .ephemeral(true),
        )
        .await?;
    }
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
            "INSERT INTO user_settings (guild_id, user_id, chatbot_role)
            VALUES ($1, $2, $3)
            ON CONFLICT(guild_id, user_id)
            DO UPDATE SET
                chatbot_role = $3",
            i64::from(guild_id),
            i64::from(ctx.author().id),
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
    #[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: i64,
    #[description = "Channel to send dead chat gifs to"] channel: Channel,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let channel_id = channel.id();
        query!(
            "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel)
            VALUES ($1, $2, $3)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                dead_chat_rate = $2, 
                dead_chat_channel = $3",
            i64::from(guild_id),
            occurrence,
            i64::from(channel_id),
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Dead chat gifs will only be sent every {occurrence} minute(s) in {channel}... probably",
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
    #[description = "Character(s) to use as prefix for commands"] characters: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO guild_settings (guild_id, prefix)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                prefix = $2",
            i64::from(guild_id),
            characters,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "{characters} set as the prefix for commands... probably"
                ))
                .ephemeral(true),
        )
        .await?;
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
        let channel_id = channel.id();
        query!(
            "INSERT INTO guild_settings (guild_id, quotes_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                quotes_channel = $2",
            i64::from(guild_id),
            i64::from(channel_id),
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Quoted messages will be sent to {channel}... probably"
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
        let channel_id = channel.id();
        query!(
            "INSERT INTO guild_settings (guild_id, spoiler_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                spoiler_channel = $2",
            i64::from(guild_id),
            i64::from(channel_id),
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!(
                    "Spoilered messages will be sent to {channel}... probably"
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
    #[description = "Image/gif to send; write waifu for a random waifu or !gif query for a gif of query"]
    media: Option<String>,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        if let Some(user_media) = &media {
            const INVALID_MEDIA_MSG: &str = "Invalid media given... really bro?";
            if user_media.contains("https") {
                ctx.defer().await?;
                let is_valid =
                    (HTTP_CLIENT.head(user_media).send().await).map_or(false, |response| {
                        response
                            .headers()
                            .get("content-type")
                            .and_then(|ct| ct.to_str().ok())
                            .is_some_and(|ct| ct.starts_with("image/") || ct == "application/gif")
                    });
                if !is_valid {
                    ctx.send(
                        CreateReply::default()
                            .content(INVALID_MEDIA_MSG)
                            .ephemeral(true),
                    )
                    .await?;
                    return Ok(());
                }
            } else if !user_media.contains("!gif") && user_media != "waifu" {
                ctx.send(
                    CreateReply::default()
                        .content(INVALID_MEDIA_MSG)
                        .ephemeral(true),
                )
                .await?;
                return Ok(());
            }
        }
        query!(
            "INSERT INTO user_settings (guild_id, user_id, ping_content, ping_media)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(guild_id, user_id)
            DO UPDATE SET 
                ping_content = $3, 
                ping_media = $4",
            i64::from(guild_id),
            i64::from(ctx.author().id),
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
    #[description = "Word to track count of"] word: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO words_count (guild_id, word)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                word = $2, 
                count = 0",
            i64::from(guild_id),
            word,
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content(format!("The count of {word} will be tracked... probably"))
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}
