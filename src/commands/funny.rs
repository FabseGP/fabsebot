use crate::{
    types::{Context, Error},
    utils::webhook_find,
};

use poise::{
    serenity_prelude::{ChannelId, ExecuteWebhook, User},
    CreateReply,
};

/// Send an anonymous message
#[poise::command(slash_command)]
pub async fn anonymous(
    ctx: Context<'_>,
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

/*
/// Misuse other users dm
#[poise::command(prefix_command, slash_command)]
pub async fn user_dm(
    ctx: Context<'_>,
    #[description = "Target"] user: User,
    #[description = "Message to be sent"] message: String,
) -> Result<(), Error> {
    user.direct_message(ctx, CreateMessage::default().content(message))
        .await?;
    ctx.send(
        CreateReply::default()
            .content("DM sent successfully, RUN!")
            .ephemeral(true),
    )
    .await?;
    Ok(())
}
*/

/// Send message as an another user
#[poise::command(prefix_command, slash_command)]
pub async fn user_misuse(
    ctx: Context<'_>,
    #[description = "Target"] user: User,
    #[description = "Message to send"]
    #[rest]
    message: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        if guild_id != 1103723321683611698
            || ctx.author().id == 1014524859532980255
            || ctx.author().id == 999604056072929321
        {
            let member = {
                let guild = ctx.partial_guild().await.unwrap();
                guild.member(&ctx.http(), user.id).await?.clone()
            };
            let avatar_url = member.avatar_url().unwrap_or(user.avatar_url().unwrap());
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
            if ctx.prefix() != "/" {
                let reason: Option<&str> = Some("anonymous");
                ctx.channel_id()
                    .delete_message(ctx.http(), ctx.id().into(), reason)
                    .await?;
            } else {
                ctx.send(
                    CreateReply::default()
                        .content("you're going to hell")
                        .ephemeral(true),
                )
                .await?;
            }
        } else {
            ctx.send(
                CreateReply::default()
                    .content("you're not fabseman, hush!")
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}
