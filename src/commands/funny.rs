use crate::types::{Context, Error};

use poise::serenity_prelude::{self as serenity, CreateMessage};
use poise::CreateReply;

/// Send an anonymous message
#[poise::command(slash_command, prefix_command)]
pub async fn anonymous(
    ctx: Context<'_>,
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
    ctx.channel_id().say(&ctx.http(), message).await?;
    Ok(())
}

/// Misuse other users dm
#[poise::command(slash_command, prefix_command)]
pub async fn user_dm(
    ctx: Context<'_>,
    #[description = "Target"] user: serenity::model::user::User,
    #[description = "Message to be sent"] message: String,
) -> Result<(), Error> {
    let dm_channel = user.create_dm_channel(ctx).await?;
    dm_channel
        .send_message(ctx, CreateMessage::default().content(message))
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
#[poise::command(slash_command, prefix_command)]
pub async fn user_misuse(
    ctx: Context<'_>,
    #[description = "Target"] user: poise::serenity_prelude::User,
    #[description = "Message to send"]
    #[rest]
    _message: String,
) -> Result<(), Error> {
    let _avatar_url = user.avatar_url();
    let _name = user.name;
    let _channel_id = ctx.channel_id();
    ctx.send(CreateReply::default().content("thou can't be trusted with this power"))
        .await?;
    Ok(())
}
