use std::{borrow::Cow, sync::Arc};

use anyhow::Result as AResult;
use fabsebot_db::guild::delete_guild;
use metrics::counter;
use poise::{ApplicationContext, Context, FrameworkError, PartialContext, PrefixContext};
use serenity::all::{Context as SContext, EventHandler as SEventHandler, FullEvent};
use sqlx::query_scalar;
use tracing::warn;

use crate::{
	config::types::{Data, Error},
	events::{
		bot_ready::handle_ready,
		interaction::{
			FEEDBACK_BUTTON_CUSTOM_ID, FEEDBACK_MODAL_CUSTOM_ID, handle_feedback_modal_button,
			handle_feedback_modal_reply,
		},
		message_delete::handle_message_delete,
		message_sent::handle_message,
	},
	log_error,
	stats::counters::METRICS,
};

pub async fn on_error(error: FrameworkError<'_, Data, Error>) {
	match error {
		FrameworkError::Command { error, ctx, .. } => {
			let output = format!("# Error in command '{}'\n{error}", ctx.command().name);
			counter!(
				METRICS.command_errors.clone(),
				"command" => ctx.command().name.clone(),
			)
			.increment(1);
			log_error(output, ctx.serenity_context()).await;
		}
		FrameworkError::DynamicPrefix { error, ctx, .. } => {
			let output = format!("# Error in dynamic prefix\n{error}");
			counter!(METRICS.prefix_errors.clone()).increment(1);
			log_error(output, ctx.framework.serenity_context).await;
		}
		FrameworkError::MissingBotPermissions {
			missing_permissions,
			ctx,
			..
		} => {
			let output = format!(
				"# Missing bot permissions in command '{}'\n{missing_permissions}",
				ctx.command().name
			);
			if let Err(err) = ctx.say(&output).await {
				warn!("Failed to notify user about missing bot permissions: {err}");
			}
			counter!(METRICS.bot_permissions_errors.clone()).increment(1);
			log_error(output, ctx.serenity_context()).await;
		}
		FrameworkError::MissingUserPermissions {
			missing_permissions: Some(missing_permissions),
			ctx,
			..
		} => {
			let output = format!(
				"# Missing user permissions in command '{}'\n{missing_permissions}",
				ctx.command().name
			);
			if let Err(err) = ctx.say(&output).await {
				warn!("Failed to notify user about missing user permissions: {err}");
			}
			counter!(METRICS.user_permissions_errors.clone()).increment(1);
			log_error(output, ctx.serenity_context()).await;
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
	let bot_data: Arc<Data> = ctx.framework.serenity_context.data();
	let prefix = if let Some(guild_id) = ctx.guild_id
		&& let Some(Some(prefix)) = query_scalar!(
			r#"
			SELECT prefix FROM guild_settings
			WHERE guild_id = $1
			"#,
			i64::from(guild_id)
		)
		.fetch_optional(&bot_data.db)
		.await?
	{
		Cow::Owned(prefix)
	} else {
		Cow::Borrowed("!")
	};
	Ok(Some(prefix))
}

pub struct EventHandler;

#[serenity::async_trait]
impl SEventHandler for EventHandler {
	async fn dispatch(&self, ctx: &SContext, event: &FullEvent) {
		let bot_data: Arc<Data> = ctx.data();
		match event {
			FullEvent::Ready { data_about_bot, .. } => {
				if let Err(error) = handle_ready(ctx, data_about_bot).await {
					let output = format!("# Error handling connection to Discord\n{error}");
					counter!(METRICS.ready_errors.clone()).increment(1);
					log_error(output, ctx).await;
				}
			}
			FullEvent::Message { new_message, .. } => {
				if !new_message.author.bot()
					&& let Some(guild_id) = new_message.guild_id
					&& let Err(error) = Box::pin(handle_message(ctx, new_message, guild_id)).await
				{
					let output = format!("# Error handling sent message\n{error}");
					counter!(METRICS.message_errors.clone()).increment(1);
					log_error(output, ctx).await;
				}
			}
			FullEvent::GuildDelete { incomplete, .. } => {
				if !incomplete.unavailable
					&& let Err(error) = delete_guild(i64::from(incomplete.id), &bot_data.db).await
				{
					let output = format!("# Error handling deleted guild\n{error}");
					counter!(METRICS.deleted_guild_errors.clone()).increment(1);
					log_error(output, ctx).await;
				}
			}

			FullEvent::MessageDelete {
				channel_id,
				deleted_message_id,
				guild_id: Some(guild_id),
				..
			} => {
				let message_author_id = ctx
					.cache
					.message(*channel_id, *deleted_message_id)
					.map(|msg| msg.author.id);
				if let Some(author_id) = message_author_id
					&& author_id == ctx.cache.current_user().id
					&& let Err(error) =
						handle_message_delete(ctx, *channel_id, *guild_id, *deleted_message_id)
							.await
				{
					let output = format!("# Error handling deleted message\n{error}");
					counter!(METRICS.messages_deleted_errors.clone()).increment(1);
					log_error(output, ctx).await;
				}
			}
			FullEvent::InteractionCreate { interaction, .. } => {
				if let Some(component_interaction) = interaction.as_message_component()
					&& component_interaction.data.custom_id == FEEDBACK_BUTTON_CUSTOM_ID
					&& let Err(error) =
						handle_feedback_modal_button(ctx, component_interaction).await
				{
					let output = format!("# Error handling feedback modal\n{error}");
					counter!(METRICS.feedback_modal_errors.clone()).increment(1);
					log_error(output, ctx).await;
				}
				if let Some(modal_interaction) = interaction.as_modal_submit()
					&& modal_interaction.data.custom_id == FEEDBACK_MODAL_CUSTOM_ID
					&& let Some(guild_id) = interaction.guild_id()
					&& let Err(error) =
						handle_feedback_modal_reply(ctx, modal_interaction, guild_id).await
				{
					let output = format!("# Error handling feedback reply\n{error}");
					counter!(METRICS.feedback_reply_errors.clone()).increment(1);
					log_error(output, ctx).await;
				}
			}
			_ => {}
		}
	}
}
