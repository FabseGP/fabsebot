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
			let error_title = "# Error in dynamic prefix";
			error!("{error_title}: {error}");
			counter!(METRICS.prefix_errors.clone()).increment(1);
			if let Err(err) = error_hook(
				ctx.framework.serenity_context,
				error_title,
				error.to_string(),
			)
			.await
			{
				error!("Failed to send dynamic prefix error to webhook: {err}");
			}
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
			error!("{error_title}: {missing_permissions}");
			counter!(METRICS.bot_permissions_error.clone()).increment(1);
			if let Err(err) = error_hook(
				ctx.serenity_context(),
				&error_title,
				missing_permissions.to_string(),
			)
			.await
			{
				error!("Failed to send bot permissions error to webhook: {err}");
			}
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
			error!("{error_title}: {missing_permissions}");
			counter!(METRICS.user_permissions_error.clone()).increment(1);
			if let Err(err) = error_hook(
				ctx.serenity_context(),
				&error_title,
				missing_permissions.to_string(),
			)
			.await
			{
				error!("Failed to send user permissions error to webhook: {err}");
			}
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
					let error_title = "# Error handling connection to Discord";
					error!("{error_title}: {error}");
					counter!(METRICS.ready_errors.clone()).increment(1);
					if let Err(err) = error_hook(ctx, error_title, error.to_string()).await {
						error!("Failed to send connection error to webhook: {err}");
					}
				}
			}
			FullEvent::Message { new_message, .. } => {
				if !new_message.author.bot()
					&& let Some(guild_id) = new_message.guild_id
					&& let Err(error) = Box::pin(handle_message(ctx, new_message, guild_id)).await
				{
					let error_title = "# Error handling sent message";
					error!("{error_title}: {error}");
					counter!(METRICS.message_errors.clone()).increment(1);
					if let Err(err) = error_hook(ctx, error_title, error.to_string()).await {
						error!("Failed to send message error to webhook: {err}");
					}
				}
			}
			FullEvent::GuildCreate {
				guild,
				is_new: Some(is_new),
				..
			} => {
				if let Err(error) = handle_guild_create(ctx.data(), guild, *is_new).await {
					let error_title = "# Error handling newly created guild";
					error!("{error_title}: {error}");
					counter!(METRICS.new_guild_errors.clone()).increment(1);
					if let Err(err) = error_hook(ctx, error_title, error.to_string()).await {
						error!("Failed to send new guild error to webhook: {err}");
					}
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
					let error_title = "# Error handling deleted message";
					error!("{error_title}: {error}");
					counter!(METRICS.messages_deleted_errors.clone()).increment(1);
					if let Err(err) = error_hook(ctx, error_title, error.to_string()).await {
						error!("Failed to send deleted message error to webhook: {err}");
					}
				}
			}
			FullEvent::GuildMemberAddition { new_member, .. } => {
				if let Err(error) = handle_member_addition(ctx.data(), new_member).await {
					let error_title = "# Error handling new guild member";
					error!("{error_title}: {error}");
					counter!(METRICS.member_addition_errors.clone()).increment(1);
					if let Err(err) = error_hook(ctx, error_title, error.to_string()).await {
						error!("Failed to send new guild member error to webhook: {err}");
					}
				}
			}
			FullEvent::InteractionCreate { interaction, .. } => {
				if let Some(component_interaction) = interaction.as_message_component()
					&& component_interaction.data.custom_id == FEEDBACK_BUTTON_CUSTOM_ID
					&& let Err(error) =
						handle_feedback_modal_button(ctx, component_interaction).await
				{
					let error_title = "# Error handling feedback modal";
					error!("{error_title}: {error}");
					counter!(METRICS.feedback_modal_errors.clone()).increment(1);
					if let Err(err) = error_hook(ctx, error_title, error.to_string()).await {
						error!("Failed to send feedback modal error to webhook: {err}");
					}
				}
				if let Some(modal_interaction) = interaction.as_modal_submit()
					&& modal_interaction.data.custom_id == FEEDBACK_MODAL_CUSTOM_ID
					&& let Some(guild_id) = interaction.guild_id()
					&& let Err(error) =
						handle_feedback_modal_reply(ctx, modal_interaction, guild_id).await
				{
					let error_title = "# Error handling feedback reply";
					error!("{error_title}: {error}");
					counter!(METRICS.feedback_reply_errors.clone()).increment(1);
					if let Err(err) = error_hook(ctx, error_title, error.to_string()).await {
						error!("Failed to send feedback reply error to webhook: {err}");
					}
				}
			}
			_ => {}
		}
	}
}
