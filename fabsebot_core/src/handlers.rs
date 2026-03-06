use std::borrow::Cow;

use anyhow::Result as AResult;
use metrics::counter;
use poise::{ApplicationContext, Context, FrameworkError, PartialContext, PrefixContext};
use serenity::all::{Context as SContext, EventHandler as SEventHandler, FullEvent};
use tracing::error;

use crate::{
	config::types::{Data, Error},
	events::{
		bot_ready::handle_ready, guild_create::handle_guild_create,
		message_delete::handle_message_delete, message_sent::handle_message,
	},
	stats::counters::METRICS,
};

pub async fn on_error(error: FrameworkError<'_, Data, Error>) {
	match error {
		FrameworkError::Command { error, ctx, .. } => {
			error!("Error in command `{}`: {:?}", ctx.command().name, error);
			counter!(
				METRICS.command_errors.clone(),
				"command" => ctx.command().name.clone(),
			)
			.increment(1);
		}
		FrameworkError::DynamicPrefix { error, .. } => {
			error!("Error in dynamic_prefix: {:?}", error);
			counter!(METRICS.prefix_errors.clone()).increment(1);
		}
		_ => {}
	}
}

pub async fn on_command(context: Context<'_, Data, Error>) {
	let (command_name, command_type) = match &context {
		Context::Application(ApplicationContext { command, .. }) => (command.name.clone(), "slash"),
		Context::Prefix(PrefixContext { command, .. }) => (command.name.clone(), "prefix"),
	};
	counter!(
		METRICS.commands.clone(),
		"command" => command_name,
		"type" => command_type
	)
	.increment(1);
}

pub async fn dynamic_prefix(
	ctx: PartialContext<'_, Data, Error>,
) -> AResult<Option<Cow<'static, str>>> {
	let prefix = ctx.guild_id.map_or(Cow::Borrowed("!"), |id| {
		ctx.framework
			.user_data()
			.guilds
			.get(&id)
			.map_or(Cow::Borrowed("!"), |guild_data| {
				guild_data
					.shared
					.settings
					.prefix
					.clone()
					.map_or(Cow::Borrowed("!"), Cow::Owned)
			})
	});

	Ok(Some(prefix))
}

pub struct EventHandler;

#[serenity::async_trait]
impl SEventHandler for EventHandler {
	async fn dispatch(&self, ctx: &SContext, event: &FullEvent) {
		match event {
			FullEvent::Ready { data_about_bot, .. } => {
				if let Err(error) = handle_ready(ctx, data_about_bot).await {
					error!("Error handling connection to Discord: {error}");
					counter!(METRICS.ready_errors.clone()).increment(1);
				}
			}
			FullEvent::Message { new_message, .. } => {
				if !new_message.author.bot()
					&& let Some(guild_id) = new_message.guild_id
					&& let Err(error) = Box::pin(handle_message(ctx, new_message, guild_id)).await
				{
					error!("Error handling sent message: {error}");
					counter!(METRICS.message_errors.clone()).increment(1);
				}
			}
			FullEvent::GuildCreate { guild, is_new, .. } => {
				if let Err(error) = handle_guild_create(ctx.data(), guild, is_new.as_ref()).await {
					error!("Error handling newly created guild: {error}");
					counter!(METRICS.new_guild_errors.clone()).increment(1);
				}
			}
			FullEvent::MessageDelete {
				channel_id,
				deleted_message_id,
				guild_id,
				..
			} => {
				let message_author_id = ctx
					.cache
					.message(*channel_id, *deleted_message_id)
					.map(|msg| msg.author.id);
				if let (Some(author_id), Some(guild_id)) = (message_author_id, *guild_id)
					&& author_id == ctx.cache.current_user().id
					&& let Err(error) =
						handle_message_delete(ctx, *channel_id, guild_id, *deleted_message_id).await
				{
					error!("Error handling deleted message: {error}");
					counter!(METRICS.messages_deleted_errors.clone()).increment(1);
				}
			}
			_ => {}
		}
	}
}
