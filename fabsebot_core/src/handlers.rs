use std::{borrow::Cow, sync::Arc};

use anyhow::Result as AResult;
use metrics::counter;
use poise::{ApplicationContext, Context, FrameworkError, PartialContext, PrefixContext};
use serenity::all::{Context as SContext, EventHandler as SEventHandler, FullEvent};
use sqlx::query_scalar;
use tracing::error;

use crate::{
	config::types::{Data, Error},
	events::{
		bot_ready::handle_ready,
		guild_create::handle_guild_create,
		interaction::{
			FEEDBACK_BUTTON_CUSTOM_ID, FEEDBACK_MODAL_CUSTOM_ID, handle_feedback_modal_button,
			handle_feedback_modal_reply,
		},
		member_addition::handle_member_addition,
		message_delete::handle_message_delete,
		message_sent::handle_message,
	},
	log_error,
	stats::counters::METRICS,
	utils::webhook::error_hook,
};

pub async fn on_error(error: FrameworkError<'_, Data, Error>) {
	match error {
		FrameworkError::Command { error, ctx, .. } => {
			let error_title = format!("# Error in command '{}'", ctx.command().name);
			error!("{error_title}: {error}");
			counter!(
				METRICS.command_errors.clone(),
				"command" => ctx.command().name.clone(),
			)
			.increment(1);
			if let Err(err) =
				error_hook(ctx.serenity_context(), &error_title, error.to_string()).await
			{
				error!("Failed to send command error to webhook: {err}");
			}
		}
		FrameworkError::DynamicPrefix { error, ctx, .. } => {
			log_error(
				"# Error in dynamic prefix",
				error.to_string(),
				ctx.framework.serenity_context,
				METRICS.prefix_errors.clone(),
			)
			.await;
		}
		FrameworkError::MissingBotPermissions {
			missing_permissions,
			ctx,
			..
		} => {
			let error_title = format!(
				"# Missing bot permissions in command {}",
				ctx.command().name
			);
			log_error(
				&error_title,
				missing_permissions.to_string(),
				ctx.serenity_context(),
				METRICS.bot_permissions_errors.clone(),
			)
			.await;
		}
		FrameworkError::MissingUserPermissions {
			missing_permissions: Some(missing_permissions),
			ctx,
			..
		} => {
			let error_title = format!(
				"# Missing user permissions in command {}",
				ctx.command().name
			);
			log_error(
				&error_title,
				missing_permissions.to_string(),
				ctx.serenity_context(),
				METRICS.user_permissions_errors.clone(),
			)
			.await;
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
	let data: Arc<Data> = ctx.framework.serenity_context.data();
	if let Some(guild_id) = ctx.guild_id
		&& let Some(Some(prefix)) = query_scalar!(
			r#"
			SELECT prefix FROM guild_settings
			WHERE guild_id = $1
			"#,
			i64::from(guild_id)
		)
		.fetch_optional(&mut *data.db.acquire().await?)
		.await?
	{
		return Ok(Some(Cow::Owned(prefix)));
	}
	Ok(Some(Cow::Borrowed("!")))
}

pub struct EventHandler;

#[serenity::async_trait]
impl SEventHandler for EventHandler {
	async fn dispatch(&self, ctx: &SContext, event: &FullEvent) {
		match event {
			FullEvent::Ready { data_about_bot, .. } => {
				if let Err(error) = handle_ready(ctx, data_about_bot).await {
					log_error(
						"# Error handling connection to Discord",
						error.to_string(),
						ctx,
						METRICS.ready_errors.clone(),
					)
					.await;
				}
			}
			FullEvent::Message { new_message, .. } => {
				if !new_message.author.bot()
					&& let Some(guild_id) = new_message.guild_id
					&& let Err(error) = Box::pin(handle_message(ctx, new_message, guild_id)).await
				{
					log_error(
						"# Error handling sent message",
						error.to_string(),
						ctx,
						METRICS.message_errors.clone(),
					)
					.await;
				}
			}
			FullEvent::GuildCreate {
				guild,
				is_new: Some(is_new),
				..
			} => {
				if let Err(error) = handle_guild_create(ctx.data(), guild, *is_new).await {
					log_error(
						"# Error handling newly created guild",
						error.to_string(),
						ctx,
						METRICS.new_guild_errors.clone(),
					)
					.await;
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
					log_error(
						"# Error handling deleted message",
						error.to_string(),
						ctx,
						METRICS.messages_deleted_errors.clone(),
					)
					.await;
				}
			}
			FullEvent::GuildMemberAddition { new_member, .. } => {
				if let Err(error) = handle_member_addition(ctx.data(), new_member).await {
					log_error(
						"# Error handling new guild member",
						error.to_string(),
						ctx,
						METRICS.member_addition_errors.clone(),
					)
					.await;
				}
			}
			FullEvent::InteractionCreate { interaction, .. } => {
				if let Some(component_interaction) = interaction.as_message_component()
					&& component_interaction.data.custom_id == FEEDBACK_BUTTON_CUSTOM_ID
					&& let Err(error) =
						handle_feedback_modal_button(ctx, component_interaction).await
				{
					log_error(
						"# Error handling feedback modal",
						error.to_string(),
						ctx,
						METRICS.feedback_modal_errors.clone(),
					)
					.await;
				}
				if let Some(modal_interaction) = interaction.as_modal_submit()
					&& modal_interaction.data.custom_id == FEEDBACK_MODAL_CUSTOM_ID
					&& let Some(guild_id) = interaction.guild_id()
					&& let Err(error) =
						handle_feedback_modal_reply(ctx, modal_interaction, guild_id).await
				{
					log_error(
						"# Error handling feedback reply",
						error.to_string(),
						ctx,
						METRICS.feedback_reply_errors.clone(),
					)
					.await;
				}
			}
			_ => {}
		}
	}
}
