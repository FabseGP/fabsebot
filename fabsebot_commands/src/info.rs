use std::fmt::Write as _;

use fabsebot_core::{
	config::{
		constants::MESSAGE_LIMIT,
		types::{Error, SContext},
	},
	utils::helpers::{member_pfp, reply_container, thumbnail_section},
};
use serenity::{
	all::{Colour, CreateContainer, Member, PremiumType},
	builder::{CreateComponent, CreateContainerComponent, CreateSection},
};

/// Get server information
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn server_info(ctx: SContext<'_>) -> Result<(), Error> {
	let mut output = String::with_capacity(512);

	let thumbnail = {
		let guild = ctx.guild().unwrap();

		writeln!(
			output,
			"# {}\n**Creation date:** {}\n**Emoji count:** {}\n**Stickers count:** {}\n**Members \
			 count:** {}\n**Role count:** {}\n**Channels:** {}\n**Server size:** {}\n**Guild \
			 ID:** {}\n**Owner:** <@{}>",
			guild.name,
			guild.id.created_at(),
			guild.emojis.len(),
			guild.stickers.len(),
			guild.member_count,
			guild.roles.len(),
			guild.channels.len(),
			if guild.large() { "Large" } else { "Not large" },
			guild.id,
			guild.owner_id
		)?;

		if let Some(description) = guild.description.as_ref() {
			writeln!(output, "**Guild description:** {description}")?;
		}

		if let Some(boosters) = guild.premium_subscription_count {
			write!(output, "**Guild boosters:** {boosters}")?;
		}

		guild
			.banner_url()
			.map(Cow::Owned)
			.or_else(|| {
				guild.icon.as_ref().map(|i| {
					Cow::Owned(format!(
						"https://cdn.discordapp.com/icons/{}/{i}.png",
						guild.id
					))
				})
			})
			.unwrap_or(Cow::Borrowed(
				"https://c.tenor.com/MZa0P_HjQOIAAAAC/tenor.gif",
			))
	};
	output.truncate(MESSAGE_LIMIT);

	let (text, thumbnail) = thumbnail_section(&output, thumbnail);
	let text_array = [text];
	let thumbnail_display = [CreateContainerComponent::Section(CreateSection::new(
		&text_array,
		thumbnail,
	))];

	let container = CreateContainer::new(&thumbnail_display).accent_colour(Colour::DARK_BLUE);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

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
	let avatar_url = member_pfp(&member);

	let mut output = String::with_capacity(512);

	if let Some(nick) = member.nick.as_ref() {
		writeln!(output, "# {nick} (aká {})", member.user.name)?;
	} else {
		writeln!(output, "# {}", member.display_name())?;
	}

	let premium_type = match member.user.premium_type {
		PremiumType::NitroBasic => "Basic nitro",
		PremiumType::Nitro => "Nitro",
		PremiumType::NitroClassic => "Classic nitro",
		_ => "Broke",
	};

	writeln!(
		output,
		"**ID:** {}\n**Creation date:** {}\n**Nitro tier:** {premium_type}",
		member.user.id,
		member.user.id.created_at()
	)?;

	if let Some(joined_at) = member.joined_at {
		writeln!(output, "**Joined date:** {joined_at}")?;
	}

	if let Some(verified_user) = member.user.verified() {
		writeln!(output, "**Verified:** {verified_user}")?;
	}

	if let Some(premium_since) = member.premium_since {
		writeln!(output, "**Last time boosting server:** {premium_since}")?;
	}

	let output = if let Some(roles) = member.roles(ctx.cache())
		&& !roles.is_empty()
	{
		output.push_str("**Roles:** ");
		for role in &roles {
			write!(output, "<@&{}>,", role.id)?;
		}
		output.strip_suffix(',').unwrap()
	} else {
		output.as_str()
	};

	let (text, thumbnail) = thumbnail_section(output, &avatar_url);
	let text_array = [text];
	let thumbnail_display = [CreateContainerComponent::Section(CreateSection::new(
		&text_array,
		thumbnail,
	))];

	let container = CreateContainer::new(&thumbnail_display)
		.accent_colour(member.user.accent_colour.unwrap_or_default());
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}
