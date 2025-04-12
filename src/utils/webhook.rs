use crate::config::types::{Error, HTTP_CLIENT, WebhookMap};

use anyhow::anyhow;
use poise::serenity_prelude::{
    self as serenity, ExecuteWebhook, GenericChannelId, GuildId, Message, Webhook,
    builder::CreateAttachment,
};
use serde::Serialize;
use std::sync::Arc;

pub async fn spoiler_message(
    ctx: &serenity::Context,
    message: &Message,
    data: Arc<WebhookMap>,
) -> Result<(), Error> {
    if let Some(avatar_url) = message.author.avatar_url() {
        let webhook_try = webhook_find(ctx, message.guild_id, message.channel_id, data).await;
        if let Ok(webhook) = webhook_try {
            let username = message.author.display_name();
            let mut is_first = true;
            for attachment in &message.attachments {
                let download = HTTP_CLIENT
                    .get(attachment.url.as_str())
                    .send()
                    .await?
                    .bytes()
                    .await;

                let Ok(download_bytes) = download else {
                    continue;
                };
                let attachment_name = &attachment.filename;
                let attachment =
                    CreateAttachment::bytes(download_bytes, format!("SPOILER_{attachment_name}"));
                if is_first {
                    webhook
                        .execute(
                            &ctx.http,
                            false,
                            ExecuteWebhook::default()
                                .username(username)
                                .avatar_url(avatar_url.as_str())
                                .content(message.content.as_str())
                                .add_file(attachment),
                        )
                        .await?;
                    is_first = false;
                } else {
                    webhook
                        .execute(
                            &ctx.http,
                            false,
                            ExecuteWebhook::default()
                                .username(username)
                                .avatar_url(avatar_url.as_str())
                                .add_file(attachment),
                        )
                        .await?;
                }
            }
        }
        message.delete(&ctx.http, None).await?;
    }
    Ok(())
}

#[derive(Serialize)]
struct WebhookInfo {
    name: &'static str,
    avatar: &'static str,
}

pub async fn webhook_find(
    ctx: &serenity::Context,
    guild_id: Option<GuildId>,
    channel_id: GenericChannelId,
    cached_webhooks: Arc<WebhookMap>,
) -> Result<Webhook, Error> {
    if let Some(webhook) = cached_webhooks.get(&channel_id) {
        return Ok(webhook);
    }
    if let Ok(channel) = channel_id.to_channel(&ctx.http, guild_id).await
        && let Some(guild_channel) = channel.guild()
    {
        let existing_webhooks_get = guild_channel.id.webhooks(&ctx.http).await;
        match existing_webhooks_get {
            Ok(existing_webhooks) => {
                if existing_webhooks.len() >= 15 {
                    ctx.http
                        .delete_webhook(existing_webhooks.first().unwrap().id, None)
                        .await?;
                }
                let webhook_info = WebhookInfo {
                    name: "fabsebot",
                    avatar: "http://img2.wikia.nocookie.net/__cb20150611192544/pokemon/images/e/ef/Psyduck_Confusion.png",
                };
                (ctx.http
                    .create_webhook(guild_channel.id, &webhook_info, None)
                    .await)
                    .map_or_else(
                        |_| Err(anyhow!("")),
                        |webhook| {
                            cached_webhooks.insert(channel_id, webhook.clone());
                            Ok(webhook)
                        },
                    )
            }
            Err(_) => Err(anyhow!("")),
        }
    } else {
        Err(anyhow!(""))
    }
}
