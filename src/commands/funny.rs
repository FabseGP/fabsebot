use crate::{
    config::types::{Error, SContext},
    utils::webhook::webhook_find,
};

use poise::{
    CreateReply,
    serenity_prelude::{ChannelId, CreateMessage, ExecuteWebhook, Member, User},
};

/// Send an anonymous message
#[poise::command(slash_command)]
pub async fn anonymous(
    ctx: SContext<'_>,
    #[description = "Channel to send message"] channel: ChannelId,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    ctx.send(
        CreateReply::default()
            .ephemeral(true)
            .content("with big power comes big responsibility"),
    )
    .await?;
    channel.say(ctx.http(), message).await?;
    Ok(())
}

/// Misuse other users dm
#[poise::command(slash_command, owners_only)]
pub async fn user_dm(
    ctx: SContext<'_>,
    #[description = "Target"] user: User,
    #[description = "Message to be sent"] message: String,
) -> Result<(), Error> {
    user.id
        .direct_message(ctx.http(), CreateMessage::default().content(message))
        .await?;
    ctx.send(
        CreateReply::default()
            .content("DM sent successfully, RUN!")
            .ephemeral(true)
            .reply(true),
    )
    .await?;
    Ok(())
}

/// Send message as an another user
#[poise::command(slash_command)]
pub async fn user_misuse(
    ctx: SContext<'_>,
    #[description = "Target"] member: Member,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    if ctx.guild_id().is_some() {
        match webhook_find(
            ctx.serenity_context(),
            ctx.channel_id(),
            ctx.data().channel_webhooks.clone(),
        )
        .await
        {
            Ok(webhook) => {
                ctx.send(
                    CreateReply::default()
                        .content("you're going to hell")
                        .ephemeral(true),
                )
                .await?;
                let avatar_url = member.avatar_url().unwrap_or_else(|| {
                    member.user.avatar_url().unwrap_or_else(|| {
                        member
                            .user
                            .avatar_url()
                            .unwrap_or_else(|| member.user.default_avatar_url())
                    })
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
            }
            _ => {
                ctx.send(
                    CreateReply::default()
                        .content("no misuse for now")
                        .ephemeral(true),
                )
                .await?;
            }
        }
    }
    Ok(())
}
