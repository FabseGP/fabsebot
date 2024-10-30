use crate::{
    types::{Error, SContext},
    utils::webhook_find,
};

use poise::{
    serenity_prelude::{ChannelId, CreateMessage, ExecuteWebhook, Member, User},
    CreateReply,
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
    user.direct_message(ctx.http(), CreateMessage::default().content(message))
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
#[poise::command(slash_command)]
pub async fn user_misuse(
    ctx: SContext<'_>,
    #[description = "Target"] member: Member,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    let avatar_url = member
        .avatar_url()
        .unwrap_or_else(|| member.user.avatar_url().unwrap());
    let name = member.display_name();
    let channel_id = ctx.channel_id();
    let webhook_try = webhook_find(ctx.serenity_context(), channel_id).await?;
    if let Some(webhook) = webhook_try {
        webhook
            .execute(
                ctx.http(),
                false,
                ExecuteWebhook::default()
                    .username(name)
                    .avatar_url(avatar_url)
                    .content(message),
            )
            .await?;
    }
    ctx.send(
        CreateReply::default()
            .content("you're going to hell")
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
