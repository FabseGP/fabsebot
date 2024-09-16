use crate::types::Error;

use poise::serenity_prelude::{
    audit_log, ChannelId, Context, CreateMessage, GuildId, MessageId, UserId,
};

pub async fn handle_message_delete(
    ctx: &Context,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
    deleted_message_id: MessageId,
) -> Result<(), Error> {
    let message_author_id = ctx
        .cache
        .message(channel_id, deleted_message_id)
        .map(|msg| msg.author.id);

    let deleted_content = ctx
        .cache
        .message(channel_id, deleted_message_id)
        .map(|msg| (msg.content.clone(), msg.embeds.clone()));

    if let (Some(author_id), Some(guild_id)) = (message_author_id, guild_id) {
        if author_id == UserId::new(1146382254927523861) {
            let guild = ctx.http.get_guild(guild_id).await?;
            let audit = guild
                .audit_logs(
                    &ctx.http,
                    Some(audit_log::Action::Message(audit_log::MessageAction::Delete)),
                    None,
                    None,
                    None,
                )
                .await?;

            if let Some(entry) = audit.entries.first() {
                if let Some(user_id) = entry.user_id {
                    let evil_person = ctx.http.get_user(user_id).await?;
                    if let Ok(member) = ctx.http.get_member(guild_id, user_id).await {
                        if let Some(permissions) = member.permissions {
                            let admin_perms = permissions.administrator();
                            if evil_person.id != guild.owner_id && !admin_perms {
                                let name = evil_person.display_name();
                                channel_id
                                    .send_message(
                                        &ctx.http,
                                        CreateMessage::default().content(format!(
                                            "Bruh, {} deleted my message, sending it again",
                                            name
                                        )),
                                    )
                                    .await?;

                                if let Some((content, embeds)) = deleted_content {
                                    let mut message = CreateMessage::default().content(content);
                                    if !embeds.is_empty() {
                                        message = message.embed(embeds[0].clone().into());
                                    }
                                    channel_id.send_message(&ctx.http, message).await?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
