use fabsebot_core::{
	config::types::{Error, SContext},
	utils::webhook::webhook_find,
};
use poise::CreateReply;
use serenity::all::{CreateMessage, ExecuteWebhook, Member, User};

use crate::command_permissions;

/// Send an anonymous message
#[poise::command(
	slash_command,
	install_context = "User | Guild",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn anonymous(
	ctx: SContext<'_>,
	#[description = "Message to send"]
	#[rest]
	message: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	ctx.send(
		CreateReply::default()
			.ephemeral(true)
			.content("with big power comes big responsibility"),
	)
	.await?;
	ctx.say(message).await?;
	Ok(())
}

/// Misuse other users dm
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS",
	owners_only
)]
pub async fn user_dm(
	ctx: SContext<'_>,
	#[description = "Target"] user: User,
	#[description = "Message to send"] message: String,
) -> Result<(), Error> {
	user.id
		.direct_message(ctx.http(), CreateMessage::default().content(message))
		.await?;
	ctx.send(
		CreateReply::default()
			.content("DM sent successfully, RUN!")
			.ephemeral(true),
	)
	.await?;
	Ok(())
}

/// Send message as an another user
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | \
	                            MANAGE_WEBHOOKS"
)]
pub async fn user_misuse(
	ctx: SContext<'_>,
	#[description = "Target"] member: Member,
	#[description = "Message to send"]
	#[rest]
	message: String,
) -> Result<(), Error> {
	let webhook = match webhook_find(
		ctx.serenity_context(),
		ctx.guild_id(),
		ctx.channel_id(),
		ctx.data().channel_webhooks.clone(),
	)
	.await
	{
		Ok(webhook) => webhook,
		Err(err) => {
			ctx.send(
				CreateReply::default()
					.content("No misuse for now")
					.ephemeral(true),
			)
			.await?;
			return Err(err);
		}
	};
	ctx.send(
		CreateReply::default()
			.content("you're going to hell")
			.ephemeral(true),
	)
	.await?;
	let avatar_url = member.avatar_url().unwrap_or_else(|| {
		member
			.user
			.avatar_url()
			.unwrap_or_else(|| member.user.default_avatar_url())
	});
	webhook
		.execute(
			ctx.http(),
			false,
			ExecuteWebhook::default()
				.username(member.display_name())
				.avatar_url(avatar_url)
				.content(message),
		)
		.await?;

	Ok(())
}
