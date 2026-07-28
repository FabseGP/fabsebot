use std::borrow::Cow;

use anyhow::Result as AResult;
use serenity::all::{
	Context as SContext, CreateMessage, GenericChannelId, GuildId, MessageId,
	audit_log::{Action::Message, MessageAction::Delete},
};

use crate::{errors::commands::GuildError, utils::helpers::silent_message};

pub async fn handle_message_delete(
	ctx: &SContext,
	channel_id: GenericChannelId,
	guild_id: GuildId,
	deleted_message_id: MessageId,
) -> AResult<()> {
	let audit = guild_id
		.audit_logs(&ctx.http, Some(Message(Delete)), None, None, None, None)
		.await?;
	if let Some(entry) = audit.entries.first()
		&& let Some(user_id) = entry.user_id
	{
		if let Ok(channel) = channel_id.to_channel(&ctx.http, Some(guild_id)).await
			&& let Some(guild_channel) = channel.guild()
			&& let Some(guild) = ctx.cache.guild(guild_id).map(|g| g.clone())
			&& let Ok(member) = guild.member(&ctx.http, user_id).await
		{
			let user_perms = guild.user_permissions_in(&guild_channel, &member);
			if member.user.id == guild.owner_id
				|| (user_perms.administrator() || user_perms.moderate_members())
			{
				return Ok(());
			}
		} else {
			return Err(GuildError::FailedFetch.into());
		}
		let Some(content_opt) = ctx
			.cache
			.message(channel_id, deleted_message_id)
			.map(|msg| (!msg.components.is_empty()).then(|| msg.content.to_string()))
		else {
			return Ok(());
		};
		channel_id
			.send_message(
				&ctx.http,
				CreateMessage::new().content(format!(
					"**Bruh, <@{user_id}> deleted my message while not being an admin or a \
					 mod!**\nSending it again",
				)),
			)
			.await?;
		let message = content_opt.map_or(
			Cow::Borrowed("Discord didn't allow me to resend my message smh"),
			Cow::Owned,
		);
		channel_id
			.send_message(&ctx.http, silent_message(&message))
			.await?;
	}
	Ok(())
}
