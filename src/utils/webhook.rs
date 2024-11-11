use crate::config::types::{Data, Error, HTTP_CLIENT};

use anyhow::anyhow;
use poise::serenity_prelude::{
    self as serenity, builder::CreateAttachment, ChannelId, ExecuteWebhook, Message, Webhook,
};
use serde::Serialize;
use std::{path::Path, sync::Arc};
use tokio::{
    fs::{remove_file, File},
    io::AsyncWriteExt as _,
};

pub async fn spoiler_message(
    ctx: &serenity::Context,
    message: &Message,
    text: &str,
    data: &Arc<Data>,
) -> Result<(), Error> {
    if let Some(avatar_url) = message.author.avatar_url() {
        let username = message.author.display_name();
        let mut is_first = true;
        for attachment in &message.attachments {
            let target = attachment.url.as_str();
            let download = HTTP_CLIENT.get(target).send().await?.bytes().await;
            let attachment_name = &attachment.filename;
            let filename = format!("SPOILER_{attachment_name}");
            let mut file = File::create(&filename).await?;
            let Ok(download_bytes) = download else {
                continue;
            };
            file.write_all(&download_bytes).await?;
            let webhook_try = webhook_find(ctx, message.channel_id, data).await;
            if let Ok(webhook) = webhook_try {
                let attachment = CreateAttachment::path(Path::new(&filename)).await?;
                if is_first {
                    webhook
                        .execute(
                            &ctx.http,
                            false,
                            ExecuteWebhook::default()
                                .username(username)
                                .avatar_url(avatar_url.as_str())
                                .content(text)
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
            remove_file(&filename).await?;
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
    channel_id: ChannelId,
    data: &Arc<Data>,
) -> Result<Webhook, Error> {
    if let Some(webhook) = data.channel_webhooks.get(&channel_id) {
        return Ok(webhook.clone());
    }
    let existing_webhooks_get = channel_id.webhooks(&ctx.http).await;
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
                .create_webhook(channel_id, &webhook_info, None)
                .await)
                .map_or_else(
                    |_| Err(anyhow!("")),
                    |webhook| {
                        data.channel_webhooks.insert(channel_id, webhook.clone());
                        Ok(webhook)
                    },
                )
        }
        Err(_) => Err(anyhow!("")),
    }
}
