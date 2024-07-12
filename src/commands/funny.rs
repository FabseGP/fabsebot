use crate::types::{Context, Error};

use poise::serenity_prelude::ExecuteWebhook;
use poise::CreateReply;
use serde_json::json;

/// Send an anonymous message
#[poise::command(slash_command, prefix_command)]
pub async fn anonymous(
    ctx: Context<'_>,
    #[description = "Channel to send message"] channel: poise::serenity_prelude::ChannelId,
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
    channel.say(ctx.http(), message).await?;
    Ok(())
}

/*
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
*/

/// Send message as an another user
#[poise::command(slash_command, prefix_command)]
pub async fn user_misuse(
    ctx: Context<'_>,
    #[description = "Target"] user: poise::serenity_prelude::User,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    if (ctx.guild_id().unwrap() == 1103723321683611698
        && (ctx.author().id == 1014524859532980255 || ctx.author().id == 999604056072929321))
        || ctx.guild_id().unwrap() != 1103723321683611698
    {
        let member = ctx
            .http()
            .get_member(ctx.guild_id().unwrap(), user.id)
            .await?;
        let avatar_url = member.avatar_url().unwrap_or(user.avatar_url().unwrap());
        let name = member.nick.unwrap_or(user.name);
        let channel_id = ctx.channel_id();
        let webhook_info = json!({
            "name": name,
            "avatar": avatar_url
        });
        let existing_webhooks = match channel_id.webhooks(ctx.http()).await {
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

        let webhook = {
            if let Some(existing_webhook) = existing_webhooks
                .iter()
                .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
            {
                existing_webhook
            } else {
                &ctx.http()
                    .create_webhook(channel_id, &webhook_info, None)
                    .await
                    .unwrap()
            }
        };

        webhook
            .execute(
                ctx.http(),
                false,
                ExecuteWebhook::new()
                    .username(name)
                    .avatar_url(avatar_url)
                    .content(message),
            )
            .await?;

        if ctx.prefix() != "/" {
            let reason: Option<&str> = Some("anonymous");
            ctx.channel_id()
                .delete_message(ctx.http(), ctx.id().into(), reason)
                .await?;
        } else {
            ctx.send(
                CreateReply::default()
                    .content("you're going to hell")
                    .ephemeral(true),
            )
            .await?;
        }
    } else {
        ctx.send(
            CreateReply::default()
                .content("you're not fabseman, hush!")
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}
