use crate::types::{Context, Error};
use poise::serenity_prelude as serenity;
//use poise::serenity_prelude::CreateEmbed;
use poise::CreateReply;

/// Get server information
#[poise::command(slash_command, prefix_command)]
pub async fn server_info(ctx: Context<'_>) -> Result<(), Error> {
    /*   let guild = ctx.partial_guild().await;
    let response = CreateReply::default().embed(
        CreateEmbed::new()
            .title(guild.clone().unwrap().name)
            .image(guild.clone().unwrap().banner.unwrap())
            .field(
                "Owner: ",
                guild.clone().unwrap().owner_id.to_string(),
                false,
            )
            .field(
                "Emoji count: ",
                guild.clone().unwrap().emojis.len().to_string(),
                true,
            )
            .field(
                "Emojis: ",
                guild
                    .clone()
                    .unwrap()
                    .emojis
                    .values()
                    .map(|e| e.to_string())
                    .collect::<String>(),
                true,
            )
            .field(
                "Role count: ",
                guild.clone().unwrap().roles.len().to_string(),
                false,
            )
            .field(
                "Roles: ",
                guild
                    .clone()
                    .unwrap()
                    .roles
                    .values()
                    .map(|e| e.to_string())
                    .collect::<String>(),
                false,
            )
            .field(
                "Sticker count: ",
                guild.clone().unwrap().stickers.len().to_string(),
                false,
            )
            .field(
                "Members count: ",
                guild
                    .clone()
                    .unwrap()
                    .approximate_member_count
                    .unwrap()
                    .to_string(),
                true,
            )
            .field(
                "Actual alive members count: ",
                guild
                    .unwrap()
                    .approximate_presence_count
                    .unwrap()
                    .to_string(),
                true,
            ),
    );*/
    let response = CreateReply::default().content("idk, ask the owner");
    ctx.send(response).await?;
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
