use poise::serenity_prelude::{
	Context, CreateEmbed, CreateMessage, GenericChannelId, GuildId, MessageId, audit_log,
};

use crate::config::types::Error;

pub async fn handle_message_delete(
	ctx: &Context,
	channel_id: GenericChannelId,
	guild_id_opt: Option<GuildId>,
	deleted_message_id: MessageId,
) -> Result<(), Error> {
	let message_author_id = ctx
		.cache
		.message(channel_id, deleted_message_id)
		.map(|msg| msg.author.id);
	if let (Some(author_id), Some(guild_id)) = (message_author_id, guild_id_opt)
		&& author_id == ctx.cache.current_user().id
	{
		let audit = guild_id
			.audit_logs(
				&ctx.http,
				Some(audit_log::Action::Message(audit_log::MessageAction::Delete)),
				None,
				None,
				None,
			)
			.await?;
		let deleted_content = ctx
			.cache
			.message(channel_id, deleted_message_id)
			.map(|msg| (msg.content.clone(), msg.embeds.first().cloned()));
		if let Some(entry) = audit.entries.first()
			&& let Some(user_id) = entry.user_id
			&& let Some((content, embed_opt)) = deleted_content
		{
			let (guild_owner_id, evil_person_id, evil_person_name, neccessary_perms) = {
				if let Some(guild) = ctx.cache.guild(guild_id).map(|g| g.clone())
					&& let Ok(channel) = channel_id.to_channel(&ctx.http, guild_id_opt).await
					&& let Some(guild_channel) = channel.guild()
					&& let Ok(member) = guild.member(&ctx.http, user_id).await
				{
					let user_perms = guild.user_permissions_in(&guild_channel, &member);
					(
						guild.owner_id,
						member.user.id,
						member.display_name().to_owned(),
						user_perms.administrator() || user_perms.moderate_members(),
					)
				} else {
					return Ok(());
				}
			};
			if evil_person_id != guild_owner_id && !neccessary_perms {
				channel_id
					.send_message(
						&ctx.http,
						CreateMessage::default().content(format!(
							"**Bruh, {evil_person_name} deleted my message while not being an \
							 admin or a mod!**\nSending it again",
						)),
					)
					.await?;
				let mut message = CreateMessage::default().content(content);
				if let Some(embed) = embed_opt {
					message = message.embed(CreateEmbed::from(embed));
				}
				channel_id.send_message(&ctx.http, message).await?;
			}
		}
	}
	Ok(())
}
