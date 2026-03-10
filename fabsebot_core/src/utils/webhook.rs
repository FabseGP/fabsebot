use anyhow::{Result as AResult, bail};
use serde::Serialize;
use serenity::all::{
	Channel, Context as SContext, CreateAttachment, ExecuteWebhook, GenericChannelId, GuildId,
	Message, Webhook,
};
use tracing::warn;

use crate::{
	config::types::{HTTP_CLIENT, WebhookMap},
	utils::helpers::channel_counter,
};

const FABSEBOT_WEBHOOK_NAME: &str = "fabsebot";
const FABSEBOT_WEBHOOK_PFP: &str =
	"http://img2.wikia.nocookie.net/__cb20150611192544/pokemon/images/e/ef/Psyduck_Confusion.png";

pub async fn spoiler_message(
	ctx: &SContext,
	message: &Message,
	channel_id: Option<i64>,
	data: WebhookMap,
) -> AResult<()> {
	if let Some(spoiler_channel) = channel_id
		&& message.channel_id.get() == spoiler_channel.cast_unsigned()
	{
		channel_counter("spoiler".to_owned());
		let Some(avatar_url) = message.author.avatar_url() else {
			bail!("Avatar not found");
		};
		let webhook = match webhook_find(ctx, message.guild_id, message.channel_id, data).await {
			Ok(webhook) => webhook,
			Err(err) => {
				bail!(err);
			}
		};
		let username = message.author.display_name();
		for (i, payload) in message.attachments.iter().enumerate() {
			let download_bytes = match HTTP_CLIENT
				.get(payload.url.as_str())
				.send()
				.await?
				.bytes()
				.await
			{
				Ok(bytes) => bytes,
				Err(err) => {
					warn!("Couldn't download attachment: {err}");
					continue;
				}
			};

			let attachment =
				CreateAttachment::bytes(download_bytes, format!("SPOILER_{}", &payload.filename));

			let mut webhook_execute = ExecuteWebhook::default()
				.username(username)
				.avatar_url(avatar_url.as_str())
				.add_file(attachment);

			if i == 0 {
				webhook_execute = webhook_execute.content(message.content.as_str());
			}

			webhook.execute(&ctx.http, false, webhook_execute).await?;
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
	cached_webhooks: WebhookMap,
) -> AResult<Webhook> {
	if let Some(webhook) = cached_webhooks.get(&channel_id) {
		return Ok(webhook);
	}
	let guild_channel = match channel_id
		.to_channel(&ctx.http, guild_id)
		.await
		.map(Channel::guild)
	{
		Ok(channel_opt) => {
			if let Some(channel) = channel_opt {
				channel
			} else {
				bail!("Not in a guild channel");
			}
		}
		Err(err) => {
			bail!("Failed to fetch guild channel: {err}");
		}
	};
	let existing_webhooks = match guild_channel.id.webhooks(&ctx.http).await {
		Ok(webhooks) => webhooks,
		Err(err) => {
			bail!("Failed to fetch existing webhooks: {err}");
		}
	};
	if existing_webhooks.len() >= 15
		&& let Some(first_webhook_id) = existing_webhooks.first().map(|w| w.id)
		&& let Err(err) = ctx.http.delete_webhook(first_webhook_id, None).await
	{
		warn!("Failed to delete webhook: {err}");
	}
	let webhook_info = WebhookInfo {
		name: FABSEBOT_WEBHOOK_NAME,
		avatar: FABSEBOT_WEBHOOK_PFP,
	};
	ctx.http
		.create_webhook(guild_channel.id, &webhook_info, None)
		.await
		.map_or_else(
			|err| bail!("Failed to create webhook: {err}"),
			|webhook| {
				cached_webhooks.insert(channel_id, webhook.clone());
				Ok(webhook)
			},
		)
}
