use crate::types::{Context, Error};

use poise::{serenity_prelude::CreateEmbed, CreateReply};
use serenity::model::channel::Channel;
use sqlx::query;

/// To reset or not to reset, that's the question
#[poise::command(prefix_command, slash_command)]
pub async fn reset_settings(ctx: Context<'_>) -> Result<(), Error> {
    let admin_perms = ctx
        .author_member()
        .await
        .unwrap()
        .permissions(ctx.cache())
        .unwrap()
        .administrator();
    if ctx.author().id == 1014524859532980255
        || ctx.author().id == ctx.partial_guild().await.unwrap().owner_id
        || admin_perms
    {
        query!(
            "REPLACE INTO guild_settings (guild_id) VALUES (?)",
            ctx.guild_id().unwrap().get()
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.send(
            CreateReply::default()
                .content("Server settings resetted... probably")
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're either not the owner or don't have admin perms")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// When you want to escape discord
#[poise::command(slash_command)]
pub async fn set_afk(
    ctx: Context<'_>,
    #[description = "Reason for afk"] reason: Option<String>,
) -> Result<(), Error> {
    query!(
        "INSERT INTO user_settings (guild_id, user_id, afk, afk_reason) VALUES (?, ?, TRUE, ?)
        ON DUPLICATE KEY UPDATE afk = TRUE, afk_reason = ?",
        ctx.guild_id().unwrap().get(),
        u64::from(ctx.author().id),
        reason,
        reason,
    )
    .execute(&mut *ctx.data().db.acquire().await?)
    .await?;
    let embed_reason = if let Some(input) = reason {
        input
    } else {
        "didn't renew life subscription".to_string()
    };
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .title(format!("{} killed!", ctx.author().display_name()))
                .description(format!("Reason: {}", embed_reason))
                .thumbnail(ctx.author().avatar_url().unwrap())
                .color(0xFF5733),
        ),
    )
    .await?;
    Ok(())
}

/// Configure the role for the chatbot individually for each user
#[poise::command(prefix_command, slash_command)]
pub async fn set_chatbot_role(
    ctx: Context<'_>,
    #[description = "The role the bot should take; if not set, then default role"] role: Option<
        String,
    >,
) -> Result<(), Error> {
    query!(
        "INSERT INTO user_settings (guild_id, user_id, chatbot_role) VALUES (?, ?, ?)
        ON DUPLICATE KEY UPDATE chatbot_role = ?",
        ctx.guild_id().unwrap().get(),
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
    Ok(())
}

/// Configure the occurence of dead chat gifs
#[poise::command(slash_command)]
pub async fn set_dead_chat(
    ctx: Context<'_>,
    #[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: u8,
    #[description = "Channel to send dead chat gifs to"] channel: Channel,
) -> Result<(), Error> {
    let admin_perms = ctx
        .author_member()
        .await
        .unwrap()
        .permissions(ctx.cache())
        .unwrap()
        .administrator();
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id || admin_perms {
        query!(
            "INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel) VALUES (?, ?, ?)
            ON DUPLICATE KEY UPDATE dead_chat_rate = ?, dead_chat_channel = ?",
            ctx.guild_id().unwrap().get(),
            occurrence,
            channel.id().to_string(),
            occurrence,
            channel.id().to_string()
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
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're either not the owner or don't have admin perms")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure which prefix to use for commands
#[poise::command(prefix_command, slash_command)]
pub async fn set_prefix(
    ctx: Context<'_>,
    #[description = "Character(s) to use as prefix for commands"] characters: String,
) -> Result<(), Error> {
    if characters.len() < 5 {
        let admin_perms = ctx
            .author_member()
            .await
            .unwrap()
            .permissions(ctx.cache())
            .unwrap()
            .administrator();
        if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id || admin_perms {
            query!(
                "INSERT INTO guild_settings (guild_id, prefix) VALUES (?, ?)
                ON DUPLICATE KEY UPDATE prefix = ?",
                ctx.guild_id().unwrap().get(),
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
        } else {
            ctx.send(
                CreateReply::default()
                    .content("hush, you're either not the owner or don't have admin perms")
                    .ephemeral(true),
            )
            .await?;
        }
    } else {
        ctx.send(
            CreateReply::default()
                .content("maximum 5 characters are allowed as prefix")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure where to send quotes
#[poise::command(slash_command)]
pub async fn set_quote_channel(
    ctx: Context<'_>,
    #[description = "Channel to send quoted messages to"] channel: Channel,
) -> Result<(), Error> {
    let admin_perms = ctx
        .author_member()
        .await
        .unwrap()
        .permissions
        .unwrap()
        .administrator();
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id || admin_perms {
        query!(
            "INSERT INTO guild_settings (guild_id, quotes_channel) VALUES (?, ?)
            ON DUPLICATE KEY UPDATE quotes_channel = ?",
            ctx.guild_id().unwrap().get(),
            channel.id().to_string(),
            channel.id().to_string(),
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
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're either not the owner or don't have admin perms")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure a channel to always spoiler messages
#[poise::command(slash_command)]
pub async fn set_spoiler_channel(
    ctx: Context<'_>,
    #[description = "Channel to send spoilered messages to"] channel: Channel,
) -> Result<(), Error> {
    let admin_perms = ctx
        .author_member()
        .await
        .unwrap()
        .permissions
        .unwrap()
        .administrator();
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id || admin_perms {
        query!(
            "INSERT INTO guild_settings (guild_id, spoiler_channel) VALUES (?, ?)
            ON DUPLICATE KEY UPDATE quotes_channel = ?",
            ctx.guild_id().unwrap().get(),
            channel.id().to_string(),
            channel.id().to_string()
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
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're either not the owner or don't have admin perms")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Configure where to send quotes
#[poise::command(slash_command)]
pub async fn set_word_track(
    ctx: Context<'_>,
    #[description = "Word to track count of"] word: String,
) -> Result<(), Error> {
    if word.len() < 50 {
        let admin_perms = ctx
            .author_member()
            .await
            .unwrap()
            .permissions
            .unwrap()
            .administrator();
        if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id || admin_perms {
            query!(
                "INSERT INTO words_count (guild_id, word) VALUES (?, ?)
                ON DUPLICATE KEY UPDATE word = ?",
                ctx.guild_id().unwrap().get(),
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
        } else {
            ctx.send(
                CreateReply::default()
                    .content("hush, you're either not the owner or don't have admin perms")
                    .ephemeral(true),
            )
            .await?;
        }
    } else {
        ctx.send(
            CreateReply::default()
                .content("maximum 50 characters are allowed as a word")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}
