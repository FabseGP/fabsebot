use crate::types::{Context, Error};

use poise::{
    serenity_prelude::{ChannelId, ExecuteWebhook, User},
    CreateReply,
};
use serde_json::json;

/// Send an anonymous message
#[poise::command(slash_command)]
pub async fn anonymous(
    ctx: Context<'_>,
    #[description = "Channel to send message"] channel: ChannelId,
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
#[poise::command(prefix_command, slash_command)]
pub async fn user_dm(
    ctx: Context<'_>,
    #[description = "Target"] user: User,
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
#[poise::command(prefix_command, slash_command)]
pub async fn user_misuse(
    ctx: Context<'_>,
    #[description = "Target"] user: User,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        if guild_id != 1103723321683611698
            || ctx.author().id == 1014524859532980255
            || ctx.author().id == 999604056072929321
        {
            let member = ctx.http().get_member(guild_id, user.id).await?;
            let avatar_url = member.avatar_url().unwrap_or(user.avatar_url().unwrap());
            let name = member.display_name();
            let channel_id = ctx.channel_id();
            let webhook_info = json!({
                "name": name,
                "avatar": avatar_url
            });
            let existing_webhooks = match channel_id.webhooks(ctx.http()).await {
                Ok(webhooks) => webhooks,
                Err(err) => {
                    ctx.send(
                        CreateReply::default()
                            .content(
                                "no hooks for you, aká lacks permissions to manage/create webhooks",
                            )
                            .ephemeral(true),
                    )
                    .await?;
                    tracing::warn!("Error retrieving webhooks: {:?}", err);
                    return Ok(());
                }
            };
            if existing_webhooks.len() >= 15 {
                let webhooks_to_delete = existing_webhooks.len() - 14;
                for webhook in existing_webhooks.iter().take(webhooks_to_delete) {
                    let _ = (ctx.http()).delete_webhook(webhook.id, None).await;
                }
            }

            let webhook = {
                if let Some(existing_webhook) = existing_webhooks
                    .iter()
                    .find(|webhook| webhook.name.as_deref() == Some("fabsebot"))
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
    }

    Ok(())
}
