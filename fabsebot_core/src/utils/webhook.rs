use anyhow::{Result as AResult, bail};
use fabsebot_db::guild::GuildSettings;
use serde::Serialize;
use serenity::{
	all::{
		Channel, Context as SContext, CreateComponent, CreateContainer, Error, ExecuteWebhook,
		GenericChannelId, GuildId, Message, MessageFlags, Webhook,
	},
	builder::CreateAttachment,
};
use tracing::warn;

use crate::{
	config::types::{WebhookMap, utils_config},
	utils::helpers::{channel_counter, text_display, url_bytes, user_pfp},
};

const FABSEBOT_WEBHOOK_NAME: &str = "fabsebot";
const FABSEBOT_WEBHOOK_PFP: &str =
	"http://img2.wikia.nocookie.net/__cb20150611192544/pokemon/images/e/ef/Psyduck_Confusion.png";

pub async fn webhook_components<'a>(
	webhook: Webhook,
	ctx: &SContext,
	component: &'a [CreateComponent<'a>],
) -> Result<Option<Message>, Error> {
	webhook
		.execute(
			&ctx.http,
			false,
			ExecuteWebhook::default()
				.with_components(true)
				.flags(MessageFlags::IS_COMPONENTS_V2)
				.components(component),
		)
		.await
}

pub async fn error_hook(ctx: &SContext, output: &str) -> AResult<()> {
	let webhook = Webhook::from_url(&ctx.http, &utils_config().error_webhook).await?;
	let component = CreateComponent::Container(CreateContainer::new(vec![text_display(output)]));

	webhook_components(webhook, ctx, &[component]).await?;

	Ok(())
}

pub async fn spoiler_message(
	ctx: &SContext,
	message: &Message,
	settings: Option<&GuildSettings>,
	data: WebhookMap,
) -> AResult<()> {
	if let Some(settings) = settings
		&& let Some(spoiler_channel) = settings.spoiler_channel
		&& i64::from(message.channel_id) == spoiler_channel
	{
		channel_counter("spoiler".to_owned());
		let webhook = webhook_find(ctx, message.guild_id, message.channel_id, data).await?;
		let avatar_url = user_pfp(&message.author);
		let username = message.author.display_name();
		let mut webhook_execute = ExecuteWebhook::default()
			.username(username)
			.avatar_url(avatar_url.as_str());
		if !message.content.is_empty() {
			webhook_execute = webhook_execute.content(message.content.as_str());
		}
		for attachment in &message.attachments {
			let Ok(bytes) = url_bytes(&attachment.url).await else {
				continue;
			};
			webhook_execute = webhook_execute.add_file(
				CreateAttachment::bytes(bytes, attachment.filename.clone()).spoiler(true),
			);
		}

		webhook.execute(&ctx.http, false, webhook_execute).await?;
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
