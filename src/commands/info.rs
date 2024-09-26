use crate::types::{Context, Error};

use poise::{
    serenity_prelude::{CreateEmbed, User},
    CreateReply,
};
use std::sync::Arc;

/// Get server information
#[poise::command(prefix_command, slash_command)]
pub async fn server_info(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(g) => Arc::new(g.clone()),
        None => {
            return Ok(());
        }
    };

    let size = if guild.large() {
        "Discord labels this server as being large"
    } else {
        "Discord doesn't label this server as being large"
    };

    let thumbnail = match &guild.banner {
        Some(banner) => banner.to_string(),
        None => match &guild.icon {
            Some(icon_hash) =>
        format!(
            "https://cdn.discordapp.com/icons/{}/{}.png",
            guild.id, icon_hash
        ),
        None =>
        "https://external-content.duckduckgo.com/iu/?u=http%3A%2F%2Fvignette1.wikia.nocookie.net%2Fpokemon%2Fimages%2Fe%2Fe2%2F054Psyduck_Pokemon_Mystery_Dungeon_Red_and_Blue_Rescue_Teams.png%2Frevision%2Flatest%3Fcb%3D20150106002458&f=1&nofb=1&ipt=b7e9fef392b547546f7aded0dbc11449fe38587bfc507022a8f103995eaf8dd0&ipo=images".to_owned()
        }
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
