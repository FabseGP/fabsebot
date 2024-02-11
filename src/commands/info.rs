use crate::types::{Context, Error};
use poise::serenity_prelude as serenity;
use poise::CreateReply;

/// Get server information
#[poise::command(slash_command, prefix_command)]
pub async fn server_info(ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Leak other users private data
#[poise::command(slash_command, prefix_command)]
pub async fn user_info(
    ctx: Context<'_>,
    #[description = "Target"]
    #[rest]
    user: Option<serenity::User>,
) -> Result<(), Error> {
    let target = user.as_ref().unwrap_or_else(|| ctx.author());
    let response = format!(
        "{}'s account was created at {}",
        target.name,
        target.created_at()
    );
    ctx.send(CreateReply::default().content(response)).await?;
    Ok(())
}
