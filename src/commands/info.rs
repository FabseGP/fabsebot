use crate::types::{Context, Error};

use poise::{
    serenity_prelude::{CreateEmbed, User},
    CreateReply,
};

/// Get server information
#[poise::command(prefix_command, slash_command)]
pub async fn server_info(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(g) => g.clone(),
        None => {
            return Ok(());
        }
    };
    let size = match guild.large() {
        true => "Discord labels this server as being large",
        _ => "Discord doesn't label this server as being large",
    };
    let thumbnail = match &guild.banner {
        Some(banner) => banner.to_string(),
        None => match &guild.icon {
            Some(icon_hash) => format!(
                "https://cdn.discordapp.com/icons/{}/{}.png",
                guild.id, icon_hash
            ),
            None => "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned(),
        },
    };
    let owner_user = guild.owner_id.to_user(&ctx.http()).await?;
    let embed = CreateEmbed::default()
        .title(guild.name.to_string())
        .thumbnail(thumbnail)
        .field("Owner ID: ", owner_user.display_name(), false)
        .field("Emoji count: ", guild.emojis.len().to_string(), false)
        .field("Role count: ", guild.roles.len().to_string(), false)
        .field("Sticker count: ", guild.stickers.len().to_string(), false)
        .field("Members count: ", guild.member_count.to_string(), false)
        .field("Server size: ", size, false);
    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Leak other users private data
#[poise::command(prefix_command, slash_command)]
pub async fn user_info(
    ctx: Context<'_>,
    #[description = "Target"]
    #[rest]
    user: User,
) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()),
    };
    let member = guild.member(ctx.http(), user.id).await?;
    let embed = CreateEmbed::default()
        .title(member.display_name())
        .thumbnail(member.avatar_url().unwrap_or(user.avatar_url().unwrap()))
        .field("Account created at: ", user.created_at().to_string(), false);
    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}
