use std::string::ToString;

use fabsebot_core::config::{
	constants::NOT_IN_GUILD_MSG,
	types::{Error, SContext},
};
use poise::CreateReply;
use serenity::all::{
	CreateComponent, CreateContainer, CreateContainerComponent, CreateEmbed, CreateSection,
	CreateSectionAccessory, CreateSectionComponent, CreateSeparator, CreateTextDisplay,
	CreateThumbnail, CreateUnfurledMediaItem, Member, MessageFlags, PremiumType,
};

/// Get server information
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "User|Guild",
	interaction_context = "Guild"
)]
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
		let Some(guild) = ctx.guild() else {
			ctx.reply(NOT_IN_GUILD_MSG).await?;
			return Ok(());
		};
		let id = guild.id;
		(
			id.to_string(),
			guild.name.clone(),
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
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "User|Guild",
	interaction_context = "Guild"
)]
pub async fn user_info(
	ctx: SContext<'_>,
	#[description = "Target"] member: Member,
) -> Result<(), Error> {
	let username_display = [CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
		if let Some(nick) = member.nick.as_ref() {
			format!(
				"# {nick} (aká {})\n ID: {}",
				member.user.name, member.user.id
			)
		} else {
			format!("# {}\n ID: {}", member.display_name(), member.user.id)
		},
	))];

	let thumbnail_section = [CreateContainerComponent::Section(CreateSection::new(
		&username_display,
		CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
			member.avatar_url().unwrap_or_else(|| {
				member
					.user
					.avatar_url()
					.unwrap_or_else(|| member.user.default_avatar_url())
			}),
		))),
	))];

	let separator = CreateContainerComponent::Separator(CreateSeparator::new(true));

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
		"### Creation date: {}\n### Joined date: {}\n### Roles: {}\n### Verified: {}\n### Last \
		 time boosting server: {}\n### Nitro tier: {}",
		&member.user.id.created_at(),
		&member.joined_at.unwrap_or_default(),
		&roles,
		&member.user.verified().unwrap_or_default(),
		&member.premium_since.unwrap_or_default(),
		premium_type
	);

	let info_display = CreateContainerComponent::TextDisplay(CreateTextDisplay::new(user_info));

	let container = CreateContainer::new(&thumbnail_section)
		.add_component(separator.clone())
		.add_component(info_display)
		.accent_colour(member.user.accent_colour.unwrap_or_default())
		.add_component(separator);

	ctx.send(
		CreateReply::default()
			.components(&[CreateComponent::Container(container)])
			.flags(MessageFlags::IS_COMPONENTS_V2),
	)
	.await?;

	Ok(())
}
