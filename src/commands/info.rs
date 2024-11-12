use crate::config::types::{Error, SContext};

use poise::{
    serenity_prelude::{CreateEmbed, Member},
    CreateReply,
};
use std::string::ToString;

/// Get server information
#[poise::command(prefix_command, slash_command)]
pub async fn server_info(ctx: SContext<'_>) -> Result<(), Error> {
    let opt_embed = match ctx.guild() {
        Some(guild) => {
            let size = if guild.large() { "Large" } else { "Not large" }.to_owned();
            let guild_id = guild.id;
            let thumbnail = match &guild.banner {
                Some(banner) => banner.to_string(),
                None => guild.icon.as_ref().map_or(
                    "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned(),
                    |icon_hash| {
                        format!("https://cdn.discordapp.com/icons/{guild_id}/{icon_hash}.png")
                    },
                ),
            };
            let guild_description = guild
                .description
                .as_ref()
                .map_or_else(|| "Unknown description".to_owned(), ToString::to_string);
            let guild_member_count = format!(
                "{}/{}",
                guild.member_count,
                guild.max_members.unwrap_or_default()
            );
            Some(
                CreateEmbed::default()
                    .title(guild.name.to_string())
                    .description(guild_description)
                    .thumbnail(thumbnail)
                    .fields(vec![
                        ("Guild ID:", guild_id.to_string(), true),
                        (
                            "Guild boosters:",
                            guild
                                .premium_subscription_count
                                .unwrap_or_default()
                                .to_string(),
                            false,
                        ),
                        ("Owner id:", guild.owner_id.to_string(), true),
                        ("Creation date:", guild.id.created_at().to_string(), false),
                        ("Emoji count:", guild.emojis.len().to_string(), true),
                        ("Sticker count:", guild.stickers.len().to_string(), false),
                        ("Members count:", guild_member_count, true),
                        ("Role count:", guild.roles.len().to_string(), false),
                        ("Channels:", guild.channels.len().to_string(), true),
                        ("Server size:", size, true),
                    ]),
            )
        }
        None => None,
    };
    if let Some(embed) = opt_embed {
        ctx.send(CreateReply::default().reply(true).embed(embed))
            .await?;
    } else {
        ctx.reply("Discord refuse to share the info").await?;
    }
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
    let user_created = member.user.id.created_at().to_string();
    let member_joined = member.joined_at.unwrap_or_default().to_string();
    let user_mfa = if member.user.mfa_enabled() {
        "MFA enabled"
    } else {
        "MFA disabled"
    }
    .to_owned();
    let empty = String::new();
    let embed = CreateEmbed::default()
        .title(member.display_name())
        .thumbnail(member.avatar_url().unwrap_or_else(|| {
            member
                .user
                .avatar_url()
                .unwrap_or_else(|| member.user.default_avatar_url())
        }))
        .fields(vec![
            ("Creation date:", &user_created, true),
            ("Joined date:", &member_joined, true),
            ("", &empty, false),
            ("Security:", &user_mfa, false),
        ])
        .colour(member.user.accent_colour.unwrap_or_default());
    ctx.send(CreateReply::default().reply(true).embed(embed))
        .await?;

    Ok(())
}
