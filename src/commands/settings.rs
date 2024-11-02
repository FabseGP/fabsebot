use crate::{
    consts::COLOUR_RED,
    types::{Error, SContext, HTTP_CLIENT},
};

use anyhow::Context;
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
        let guild_id_i64 = i64::from(guild_id);
        let mut tx = ctx
            .data()
            .db
            .begin()
            .await
            .context("Failed to acquire savepoint")?;
        ctx.send(
            CreateReply::default()
                .content("Server settings resetted... probably")
                .ephemeral(true),
        )
        .await?;
        query!(
            "UPDATE guild_settings
            SET dead_chat_rate = NULL,
                dead_chat_channel = NULL,
                quotes_channel = NULL,
                spoiler_channel = NULL,
                prefix = NULL,
                ai_chat_channel = NULL,
                global_chat_channel = NULL,
                global_chat = FALSE,
                global_music = FALSE,
                global_call = FALSE
            WHERE guild_id = $1",
            guild_id_i64
        )
        .execute(&mut *tx)
        .await?;
        query!(
            "DELETE FROM guild_word_tracking
            WHERE guild_id = $1",
            guild_id_i64
        )
        .execute(&mut *tx)
        .await?;
        query!(
            "DELETE FROM guild_word_reaction
            WHERE guild_id = $1",
            guild_id_i64
        )
        .execute(&mut *tx)
        .await?;
        tx.commit()
            .await
            .context("Failed to commit sql-transaction")?;
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
                    .thumbnail(ctx.author().avatar_url().unwrap_or_else(|| {
                        ctx.author()
                            .static_avatar_url()
                            .unwrap_or_else(|| ctx.author().default_avatar_url())
                    }))
                    .color(COLOUR_RED),
            ),
        )
        .await?;
    }
    Ok(())
}

/// When you need ai in your life
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_chatbot_channel(
    ctx: SContext<'_>,
    #[description = "Channel to act as chatbot in"] channel: Channel,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO guild_settings (guild_id, ai_chat_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                ai_chat_channel = $2",
            i64::from(guild_id),
            i64::from(ctx.channel_id()),
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
#[poise::command(slash_command)]
pub async fn set_chatbot_role(
    ctx: SContext<'_>,
    #[description = "The role the bot should take; if not set, then default role"]
    #[rest]
    role: Option<String>,
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
        query!(
            "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel)
            VALUES ($1, $2, $3)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                dead_chat_rate = $2, 
                dead_chat_channel = $3",
            i64::from(guild_id),
            occurrence,
            i64::from(ctx.channel_id()),
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
    #[description = "Character(s) to use as prefix for commands"]
    #[rest]
    characters: String,
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
        query!(
            "INSERT INTO guild_settings (guild_id, quotes_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                quotes_channel = $2",
            i64::from(guild_id),
            i64::from(ctx.channel_id()),
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
        query!(
            "INSERT INTO guild_settings (guild_id, spoiler_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                spoiler_channel = $2",
            i64::from(guild_id),
            i64::from(ctx.channel_id()),
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
        let valid = if let Some(user_media) = &media {
            if user_media.starts_with("https") {
                ctx.defer().await?;
                HTTP_CLIENT
                    .head(user_media)
                    .send()
                    .await
                    .map_or(false, |response| {
                        response
                            .headers()
                            .get("content-type")
                            .and_then(|ct| ct.to_str().ok())
                            .is_some_and(|ct| ct.starts_with("image/") || ct == "application/gif")
                    })
            } else if let Some(media_stripped) = user_media.strip_prefix("!gif") {
                !media_stripped.is_empty()
            } else {
                user_media == "waifu"
            }
        } else {
            true
        };
        if valid {
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
        } else {
            ctx.send(
                CreateReply::default()
                    .content("Invalid media given... really bro?")
                    .ephemeral(true),
            )
            .await?;
        }
    }
    Ok(())
}

/// Configure words to react to with custom content
#[poise::command(slash_command)]
pub async fn set_word_react(
    ctx: SContext<'_>,
    #[description = "Word to react to"] word: String,
    #[description = "Text to send on react"] content: String,
    #[description = "Media to send on react; use !gif query for a random gif of query"]
    media: Option<String>,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let valid = if let Some(user_media) = &media {
            if user_media.starts_with("https") {
                ctx.defer().await?;
                HTTP_CLIENT
                    .head(user_media)
                    .send()
                    .await
                    .map_or(false, |response| {
                        response
                            .headers()
                            .get("content-type")
                            .and_then(|ct| ct.to_str().ok())
                            .is_some_and(|ct| ct.starts_with("image/") || ct == "application/gif")
                    })
            } else if let Some(media_stripped) = user_media.strip_prefix("!gif") {
                !media_stripped.is_empty()
            } else {
                false
            }
        } else {
            true
        };
        if valid {
            query!(
                "INSERT INTO guild_word_reaction (guild_id, word, content, media)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT(guild_id, word)
                DO UPDATE SET
                    word = $2,
                    content = $3,
                    media = $4",
                i64::from(guild_id),
                word,
                content,
                media
            )
            .execute(&mut *ctx.data().db.acquire().await?)
            .await?;
            ctx.send(
                CreateReply::default()
                    .content(format!("{word} will be reacted to from now on... probably"))
                    .ephemeral(true),
            )
            .await?;
        } else {
            ctx.send(
                CreateReply::default()
                    .content("Invalid media given... really bro?")
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

/// Configure words to track count of
#[poise::command(slash_command)]
pub async fn set_word_track(
    ctx: SContext<'_>,
    #[description = "Word to track count of"] word: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO guild_word_tracking (guild_id, word)
            VALUES ($1, $2)
            ON CONFLICT(guild_id, word)
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
