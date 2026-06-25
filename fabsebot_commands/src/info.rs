use std::string::ToString;

use fabsebot_core::{
	config::types::{Error, SContext},
	utils::helpers::{member_pfp, reply_container, separator, text_display, thumbnail_section},
};
use serenity::all::{Colour, CreateContainer, Member, PremiumType};

/// Get server information
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn server_info(ctx: SContext<'_>) -> Result<(), Error> {
	let (
		guild_id,
		guild_name,
		thumbnail,
		guild_description,
		guild_member_count,
		guild_max_size,
		guild_boosters,
		guild_owner_id,
		guild_created_at,
		guild_size,
		guild_emojis,
		guild_stickers,
		guild_roles,
		guild_channels,
	) = {
		let guild = ctx.guild().unwrap();
		let id = guild.id;
		(
			id,
			format!("# {}", guild.name),
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
			guild.member_count,
			guild.max_members.unwrap_or_default(),
			guild.premium_subscription_count.unwrap_or_default(),
			guild.owner_id,
			id.created_at(),
			if guild.large() { "Large" } else { "Not large" }.to_owned(),
			guild.emojis.len(),
			guild.stickers.len(),
			guild.roles.len(),
			guild.channels.len(),
		)
	};

	let thumbnail_section = [thumbnail_section(&guild_name, &thumbnail)];

	let guild_info = format!(
		"**Guild description:** {guild_description}\n**Guild ID:** {guild_id}\n**Owner id:** \
		 {guild_owner_id}\n**Guild boosters:** {guild_boosters}\n**Creation date:** \
		 {guild_created_at}\n**Emoji count:** {guild_emojis}\n**Sticker count:** \
		 {guild_stickers}\n**Members count:** {guild_member_count}/{guild_max_size}\n**Role \
		 count:** {guild_roles}\n**Channels:** {guild_channels}\n**Server size:** {guild_size}",
	);

	let container = CreateContainer::new(&thumbnail_section)
		.add_component(separator())
		.add_component(text_display(&guild_info))
		.accent_colour(Colour::DARK_BLUE);

	ctx.send(reply_container(container)).await?;

	Ok(())
}

/// Leak an user's private data
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn user_info(
	ctx: SContext<'_>,
	#[description = "Target"] member: Member,
) -> Result<(), Error> {
	let avatar_url = member_pfp(&ctx, &member).await?;
	let username = if let Some(nick) = member.nick.as_ref() {
		format!(
			"# {nick} (aká {})\n**ID:** {}",
			member.user.name, member.user.id
		)
	} else {
		format!("# {}\n ID: {}", member.display_name(), member.user.id)
	};

	let thumbnail_section = [thumbnail_section(&username, &avatar_url)];

	let premium_type = match member.user.premium_type {
		PremiumType::NitroBasic => "Basic nitro",
		PremiumType::Nitro => "Nitro",
		PremiumType::NitroClassic => "Classic nitro",
		_ => "Broke",
	};

	let roles = member
		.roles(ctx.cache())
		.map(|r| {
			r.iter()
				.map(|role| format!("<@&{}>", role.id))
				.collect::<Vec<String>>()
				.join(" ")
		})
		.unwrap_or_default();

	let user_info = format!(
		"**Creation date:** {}\n**Joined date:** {}\n**Roles:** {}\n**Verified:** {}\n**Last time \
		 boosting server:** {}\n**Nitro tier: {}**",
		member.user.id.created_at(),
		member.joined_at.unwrap_or_default(),
		roles,
		member.user.verified().unwrap_or_default(),
		member.premium_since.unwrap_or_default(),
		premium_type
	);

	let container = CreateContainer::new(&thumbnail_section)
		.add_component(separator())
		.add_component(text_display(&user_info))
		.accent_colour(member.user.accent_colour.unwrap_or_default());

	ctx.send(reply_container(container)).await?;

	Ok(())
}
