use crate::types::Error;

use poise::serenity_prelude::{
    self as serenity, audit_log, ChannelId, CreateEmbed, CreateMessage, GuildId, MessageId,
};

pub async fn handle_message_delete(
    ctx: &serenity::Context,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
    deleted_message_id: MessageId,
) -> anyhow::Result<(), Error> {
    let message_author_id = ctx
        .cache
        .message(channel_id, deleted_message_id)
        .map(|msg| msg.author.id);
    if let (Some(author_id), Some(guild_id)) = (message_author_id, guild_id) {
        let bot_id = ctx.cache.current_user().id;
        if author_id == bot_id {
            let audit = ctx
                .http
                .get_audit_logs(
                    guild_id,
                    Some(audit_log::Action::Message(audit_log::MessageAction::Delete)),
                    None,
                    None,
                    None,
                )
                .await?;
            if let Some(entry) = audit.entries.first() {
                if let Some(user_id) = entry.user_id {
                    let guild = match ctx.cache.guild(guild_id) {
                        Some(guild) => guild.clone(),
                        None => {
                            return Ok(());
                        }
                    };
                    if let Ok(member) = guild.member(&ctx.http, user_id).await {
                        if let Some(permissions) = member.permissions {
                            let necessary_perms =
                                permissions.administrator() && permissions.moderate_members();
                            let evil_person = &member.user;
                            if evil_person.id != guild.owner_id && !necessary_perms {
                                let deleted_content = ctx
                                    .cache
                                    .message(channel_id, deleted_message_id)
                                    .map(|msg| (msg.content.clone(), msg.embeds.clone()));
                                let evil_name = evil_person.display_name();
                                channel_id
                                    .send_message(
                                        &ctx.http,
                                        CreateMessage::default().content(format!(
                                            "Bruh, {evil_name} deleted my message, sending it again"
                                        )),
                                    )
                                    .await?;
                                if let Some((content, embeds)) = deleted_content {
                                    let mut message = CreateMessage::default().content(content);
                                    if let Some(deleted_embed) = embeds.into_iter().next() {
                                        message = message.embed(CreateEmbed::from(deleted_embed));
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
