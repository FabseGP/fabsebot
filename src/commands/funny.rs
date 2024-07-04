use crate::types::{Context, Error};

use poise::serenity_prelude::{self as serenity, CreateMessage, ExecuteWebhook};
use poise::CreateReply;
use serenity::json::json;

/// Send an anonymous message
#[poise::command(slash_command, prefix_command)]
pub async fn anonymous(
    ctx: Context<'_>,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    ctx.send(
        CreateReply::default()
            .ephemeral(true)
            .content("with big power comes big responsibility"),
    )
    .await?;
    ctx.channel_id().say(&ctx.http(), message).await?;
    Ok(())
}

/// Misuse other users dm
#[poise::command(slash_command, prefix_command)]
pub async fn user_dm(
    ctx: Context<'_>,
    #[description = "Target"] user: serenity::model::user::User,
    #[description = "Message to be sent"] message: String,
) -> Result<(), Error> {
    user.direct_message(ctx, CreateMessage::default().content(message))
        .await?;
    ctx.send(
        CreateReply::default()
            .content("DM sent successfully, RUN!")
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Send message as an another user
#[poise::command(slash_command, prefix_command)]
pub async fn user_misuse(
    ctx: Context<'_>,
    #[description = "Target"] user: poise::serenity_prelude::User,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    let avatar_url = user.avatar_url().unwrap();
    let name = user
        .nick_in(&ctx.http(), ctx.guild_id().unwrap())
        .await
        .unwrap_or(user.name);
    let channel_id = ctx.channel_id();
    let webhook_info = json!({
        "name": name,
        "avatar": avatar_url
    });
    let existing_webhooks = match channel_id.webhooks(&ctx.http()).await {
        Ok(webhooks) => webhooks,
        Err(_) => {
            ctx.send(
                CreateReply::default()
                    .content("no hooks for you")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };
    if existing_webhooks.len() >= 15 {
        for webhook in &existing_webhooks {
            ctx.http().delete_webhook(webhook.id, None).await?;
        }
    }
    if let Some(existing_webhook) = existing_webhooks
        .iter()
        .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
    {
        existing_webhook
            .execute(
                &ctx.http(),
                false,
                ExecuteWebhook::new()
                    .username(name)
                    .avatar_url(avatar_url)
                    .content(message),
            )
            .await?;
    } else {
        let new_webhook = ctx
            .http()
            .create_webhook(channel_id, &webhook_info, None)
            .await;
        new_webhook
            .unwrap()
            .execute(
                &ctx.http(),
                false,
                ExecuteWebhook::new()
                    .username(name)
                    .avatar_url(avatar_url)
                    .content(message),
            )
            .await?;
    }
    if ctx.prefix() != "/" {
        ctx.channel_id()
            .delete_message(ctx.http(), ctx.id())
            .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("you're going to hell")
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}
