use std::borrow::Cow;

use anyhow::Result as AResult;
use metrics::{counter, describe_counter};
use poise::{ApplicationContext, Context, FrameworkError, PartialContext, PrefixContext};
use serenity::all::{Context as SContext, EventHandler as SEventHandler, FullEvent};
use tracing::{error, warn};

use crate::{
	config::types::{Data, Error},
	events::{
		bot_ready::handle_ready, guild_create::handle_guild_create,
		message_delete::handle_message_delete, message_sent::handle_message,
	},
};

pub fn initialize_counters() {
	describe_counter!("commands_counter", "Counter for commands");
	describe_counter!("errors_counter", "Error counter for commands");
}

pub async fn on_error(error: FrameworkError<'_, Data, Error>) {
	match error {
		FrameworkError::Command { error, ctx, .. } => {
			error!("Error in command `{}`: {:?}", ctx.command().name, error);
		}
		FrameworkError::DynamicPrefix { error, .. } => {
			error!("Error in dynamic_prefix: {:?}", error);
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
		"commands_counter",
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
			.guild_data
			.get(&id)
			.map_or(Cow::Borrowed("!"), |guild_data| {
				guild_data
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
					warn!("Error handling connection to Discord: {error}");
				}
			}
			FullEvent::Message { new_message, .. } => {
				if let Err(error) = handle_message(ctx, new_message).await {
					warn!("Error handling sent message: {error}");
				}
			}
			FullEvent::GuildCreate { guild, is_new, .. } => {
				if let Err(error) = handle_guild_create(ctx.data(), guild, is_new.as_ref()).await {
					warn!("Error handling newly created guild: {error}");
				}
			}
			FullEvent::MessageDelete {
				channel_id,
				deleted_message_id,
				guild_id,
				..
			} => {
				if let Err(error) =
					handle_message_delete(ctx, *channel_id, *guild_id, *deleted_message_id).await
				{
					warn!("Error handling deleted message: {error}");
				}
			}
			_ => {}
		}
	}
}
