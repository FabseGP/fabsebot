use std::sync::Arc;

use anyhow::{Result as AResult, bail};
use serde::Serialize;
use serenity::{
	all::{
		Channel, Context as SContext, CreateComponent, CreateContainer, Error, ExecuteWebhook,
		GenericChannelId, GuildId, Message, MessageFlags, Webhook,
	},
	builder::CreateAttachment,
	http::Http,
};

use crate::{
	config::types::{WebhookMap, bot_context, utils_config},
	utils::helpers::{channel_counter, text_display, url_bytes, user_pfp},
};

const FABSEBOT_WEBHOOK_NAME: &str = "fabsebot";
const FABSEBOT_WEBHOOK_PFP: &str =
	"http://img2.wikia.nocookie.net/__cb20150611192544/pokemon/images/e/ef/Psyduck_Confusion.png";

pub async fn webhook_components<'a>(
	webhook: Webhook,
	http: &Http,
	component: &'a [CreateComponent<'a>],
) -> Result<Option<Message>, Error> {
	webhook
		.execute(
			http,
			false,
			ExecuteWebhook::new()
				.with_components(true)
				.flags(MessageFlags::IS_COMPONENTS_V2)
				.components(component),
		)
		.await
}

pub async fn error_hook(output: &str) -> AResult<()> {
	let http = bot_context().http.clone();
	let webhook = Webhook::from_url(&http, &utils_config().error_webhook).await?;
	let display = [text_display(output)];
	let component = CreateComponent::Container(CreateContainer::new(&display));

	webhook_components(webhook, &http, &[component]).await?;

	Ok(())
}

pub async fn spoiler_message(ctx: &SContext, message: &Message, data: &WebhookMap) -> AResult<()> {
	channel_counter("spoiler");
	let Some(webhook) = webhook_find(ctx, message.guild_id, message.channel_id, data).await? else {
		return Ok(());
	};
	let avatar_url = user_pfp(&message.author);
	let username = &message.author.name;
	let mut webhook_execute = ExecuteWebhook::new()
		.username(username)
		.avatar_url(avatar_url.as_str());
	if !message.content.is_empty() {
		webhook_execute = webhook_execute.content(message.content.as_str());
	}
	for attachment in &message.attachments {
		let Ok(bytes) = url_bytes(&attachment.url).await else {
			continue;
		};
		webhook_execute = webhook_execute
			.add_file(CreateAttachment::bytes(bytes, attachment.filename.clone()).spoiler(true));
	}

	webhook.execute(&ctx.http, false, webhook_execute).await?;
	message.delete(&ctx.http, None).await?;

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
	cached_webhooks: &WebhookMap,
) -> AResult<Option<Arc<Webhook>>> {
	if let Some(webhook) = cached_webhooks.get(&channel_id) {
		return Ok(Some(webhook));
	}
	let guild_channel = match channel_id
		.to_channel(&ctx.http, guild_id)
		.await
		.map(Channel::guild)
	{
		Ok(Some(channel)) => channel.id,
		Ok(None) => return Ok(None),
		Err(err) => {
			bail!("Failed to fetch guild channel: {err}");
		}
	};
	let existing_webhooks = match guild_channel.webhooks(&ctx.http).await {
		Ok(webhooks) => webhooks,
		Err(err) => {
			bail!("Failed to fetch existing webhooks: {err}");
		}
	};
	if existing_webhooks.len() >= 15
		&& let Some(first_webhook_id) = existing_webhooks.first().map(|w| w.id)
		&& let Err(err) = ctx.http.delete_webhook(first_webhook_id, None).await
	{
		bail!("Failed to delete webhook: {err}");
	}
	let webhook_info = WebhookInfo {
		name: FABSEBOT_WEBHOOK_NAME,
		avatar: FABSEBOT_WEBHOOK_PFP,
	};
	ctx.http
		.create_webhook(guild_channel, &webhook_info, None)
		.await
		.map_or_else(
			|err| bail!("Failed to create webhook: {err}"),
			|webhook| {
				let webhook_arc = Arc::new(webhook);
				cached_webhooks.insert(channel_id, webhook_arc.clone());
				Ok(Some(webhook_arc))
			},
		)
}
