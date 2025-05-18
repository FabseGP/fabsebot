use std::sync::Arc;

use anyhow::Result as AResult;
use poise::serenity_prelude::{
	Channel, Context as SContext, ExecuteWebhook, GenericChannelId, GuildId, Message, Webhook,
	builder::CreateAttachment,
};
use serde::Serialize;
use tracing::warn;

use crate::config::types::{HTTP_CLIENT, WebhookMap};

pub async fn spoiler_message(
	ctx: &SContext,
	message: &Message,
	data: Arc<WebhookMap>,
) -> AResult<()> {
	if let Some(avatar_url) = message.author.avatar_url() {
		if let Some(webhook) = webhook_find(ctx, message.guild_id, message.channel_id, data).await {
			let username = message.author.display_name();
			let mut is_first = true;
			for payload in &message.attachments {
				let download = HTTP_CLIENT
					.get(payload.url.as_str())
					.send()
					.await?
					.bytes()
					.await;

				let Ok(download_bytes) = download else {
					continue;
				};
				let attachment_name = &payload.filename;
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
	ctx: &SContext,
	guild_id: Option<GuildId>,
	channel_id: GenericChannelId,
	cached_webhooks: Arc<WebhookMap>,
) -> Option<Webhook> {
	if let Some(webhook) = cached_webhooks.get(&channel_id) {
		Some(webhook)
	} else if let Ok(Some(guild_channel)) = channel_id
		.to_channel(&ctx.http, guild_id)
		.await
		.map(Channel::guild)
	{
		let existing_webhooks_get = guild_channel.id.webhooks(&ctx.http).await;
		if let Ok(existing_webhooks) = existing_webhooks_get {
			if existing_webhooks.len() >= 15
				&& let Some(first_webhook) = existing_webhooks.first()
				&& let Err(e) = ctx.http.delete_webhook(first_webhook.id, None).await
			{
				warn!("Failed to delete webhook: {e}");
			}
			let webhook_info = WebhookInfo {
                name: "fabsebot",
                avatar: "http://img2.wikia.nocookie.net/__cb20150611192544/pokemon/images/e/ef/Psyduck_Confusion.png",
            };
			ctx.http
				.create_webhook(guild_channel.id, &webhook_info, None)
				.await
				.ok()
				.map_or_else(
					|| None,
					|webhook| {
						cached_webhooks.insert(channel_id, webhook.clone());
						Some(webhook)
					},
				)
		} else {
			None
		}
	} else {
		None
	}
}
