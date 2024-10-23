use crate::types::{Error, SContext};

use poise::{
    serenity_prelude::{CreateEmbed, Member},
    CreateReply,
};

/// Get server information
#[poise::command(prefix_command, slash_command)]
pub async fn server_info(ctx: SContext<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(g) => g.clone(),
        None => {
            return Ok(());
        }
    };
    let size = if guild.large() { "Large" } else { "Not large" }.to_owned();
    let guild_id = guild.id;
    let thumbnail = match &guild.banner {
        Some(banner) => banner.as_str(),
        None => match &guild.icon {
            Some(icon_hash) => {
                &format!("https://cdn.discordapp.com/icons/{guild_id}/{icon_hash}.png")
            }
            None => "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif",
        },
    };
    let owner_user = guild.owner_id.to_user(&ctx.http()).await?;
    let guild_description = guild.description.unwrap_or_default().into_string();
    let guild_id = guild_id.to_string();
    let guild_boosters = guild.premium_subscription_count.unwrap().to_string();
    let owner_name = owner_user.display_name().to_owned();
    let guild_creation = guild.id.created_at().to_string();
    let guild_emojis_len = guild.emojis.len().to_string();
    let guild_roles_len = guild.roles.len().to_string();
    let guild_stickers_len = guild.stickers.len().to_string();
    let member_count = guild.member_count;
    let max_member_count = guild.max_members.unwrap_or_default();
    let guild_member_count = format!("{member_count}/{max_member_count}");
    let guild_channels = guild.channels.len().to_string();
    let empty = String::new();
    let embed = CreateEmbed::default()
        .title(guild.name.into_string())
        .description(guild_description)
        .thumbnail(thumbnail)
        .fields(vec![
            ("Guild ID:", &guild_id, true),
            ("Guild boosters:", &guild_boosters, true),
            ("", &empty, false),
            ("Owner:", &owner_name, true),
            ("Creation date:", &guild_creation, true),
            ("", &empty, false),
            ("Emoji count:", &guild_emojis_len, true),
            ("Sticker count:", &guild_stickers_len, true),
            ("", &empty, false),
            ("Members count:", &guild_member_count, true),
            ("Role count:", &guild_roles_len, true),
            ("", &empty, false),
            ("Channels:", &guild_channels, true),
            ("Server size:", &size, true),
        ]);
    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Leak other users private data
#[poise::command(prefix_command, slash_command)]
pub async fn user_info(
    ctx: SContext<'_>,
    #[description = "Target"]
    #[rest]
    member: Member,
) -> Result<(), Error> {
    let user = ctx.http().get_user(member.user.id).await?;
    let user_created = user.created_at().to_string();
    let member_joined = member.joined_at.unwrap().to_string();
    let user_mfa = if user.mfa_enabled() {
        "MFA enabled"
    } else {
        "MFA disabled"
    }
    .to_owned();
    let empty = String::new();
    let embed = CreateEmbed::default()
        .title(member.display_name())
        .thumbnail(member.avatar_url().unwrap_or(user.avatar_url().unwrap()))
        .fields(vec![
            ("Creation date:", &user_created, true),
            ("Joined date:", &member_joined, true),
            ("", &empty, false),
            ("Security:", &user_mfa, false),
        ])
        .colour(user.accent_colour.unwrap());
    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}
