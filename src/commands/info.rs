use std::string::ToString;

use poise::CreateReply;
use serenity::all::{CreateEmbed, Member};

use crate::config::types::{Error, SContext};

/// Get server information
#[poise::command(prefix_command, slash_command)]
pub async fn server_info(ctx: SContext<'_>) -> Result<(), Error> {
	let (
		guild_id,
		guild_name,
		thumbnail,
		guild_description,
		guild_member_count,
		guild_boosters,
		guild_owner_id,
		guild_created_at,
		guild_size,
		guild_emojis,
		guild_stickers,
		guild_roles,
		guild_channels,
	) = {
		if let Some(guild) = ctx.guild() {
			let id = guild.id;
			(
				id.to_string(),
				guild.name.to_string(),
				guild
					.banner
					.as_ref()
					.map(ToString::to_string)
					.or_else(|| {
						guild
							.icon
							.as_ref()
							.map(|i| format!("https://cdn.discordapp.com/icons/{id}/{i}.png"))
					})
					.unwrap_or_else(|| "https://...".to_owned()),
				guild
					.description
					.as_ref()
					.map_or_else(|| "Unknown description".to_owned(), ToString::to_string),
				format!(
					"{}/{}",
					guild.member_count,
					guild.max_members.unwrap_or_default()
				),
				guild
					.premium_subscription_count
					.unwrap_or_default()
					.to_string(),
				guild.owner_id.to_string(),
				id.created_at().to_string(),
				if guild.large() { "Large" } else { "Not large" }.to_owned(),
				guild.emojis.len().to_string(),
				guild.stickers.len().to_string(),
				guild.roles.len().to_string(),
				guild.channels.len().to_string(),
			)
		} else {
			ctx.reply("Discord refuse to share the info").await?;
			return Ok(());
		}
	};

	let embed = CreateEmbed::default()
		.title(guild_name)
		.description(guild_description)
		.thumbnail(thumbnail)
		.fields(vec![
			("Guild ID", guild_id, true),
			("Guild boosters:", guild_boosters, false),
			("Owner id:", guild_owner_id, true),
			("Creation date:", guild_created_at, false),
			("Emoji count:", guild_emojis, true),
			("Sticker count:", guild_stickers, false),
			("Members count:", guild_member_count, true),
			("Role count:", guild_roles, false),
			("Channels:", guild_channels, true),
			("Server size:", guild_size, true),
		]);
	ctx.send(CreateReply::default().reply(true).embed(embed))
		.await?;
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
