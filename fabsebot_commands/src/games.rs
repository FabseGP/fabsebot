use std::time::Duration;

use fabsebot_core::{
	config::types::{Error, SContext},
	utils::helpers::{send_container, separator, text_display},
};
use poise::ChoiceParameter;
use serenity::all::{
	ButtonStyle, Colour, ComponentInteractionCollector, CreateActionRow, CreateButton,
	CreateComponent, CreateContainer, CreateContainerComponent, CreateInteractionResponse,
	EditMessage, MessageFlags, User,
};

use crate::{require_guild_id, require_human};

#[derive(PartialEq, Eq, ChoiceParameter)]
pub enum RpsChoice {
	#[name = "🪨 Rock"]
	Rock,
	#[name = "🧻 Paper"]
	Paper,
	#[name = "✂️ Scissors"]
	Scissors,
}

impl RpsChoice {
	const fn beats(self, other: Self) -> bool {
		matches!(
			(self, other),
			(Self::Rock, Self::Scissors)
				| (Self::Paper, Self::Rock)
				| (Self::Scissors, Self::Paper)
		)
	}

	fn button_id(self, ctx_id: u64) -> String {
		match self {
			Self::Rock => format!("{ctx_id}_r"),
			Self::Paper => format!("{ctx_id}_p"),
			Self::Scissors => format!("{ctx_id}_s"),
		}
	}

	fn from_button_id(id: &str) -> Option<Self> {
		if id.ends_with("_r") {
			Some(Self::Rock)
		} else if id.ends_with("_p") {
			Some(Self::Paper)
		} else if id.ends_with("_s") {
			Some(Self::Scissors)
		} else {
			None
		}
	}

	const fn emoji(self) -> &'static str {
		match self {
			Self::Rock => "🪨",
			Self::Paper => "🧻",
			Self::Scissors => "✂️",
		}
	}
}

/// Get rekt by another user in rps
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn rps(
	ctx: SContext<'_>,
	#[description = "Target"] user: User,
	#[description = "Your choice: rock, paper, or scissors"] author_choice: RpsChoice,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	require_human(ctx, &user).await?;

	let ctx_id = ctx.id();
	let buttons = [
		CreateButton::new(RpsChoice::Rock.button_id(ctx_id))
			.style(ButtonStyle::Primary)
			.label(RpsChoice::Rock.emoji()),
		CreateButton::new(RpsChoice::Paper.button_id(ctx_id))
			.style(ButtonStyle::Primary)
			.label(RpsChoice::Paper.emoji()),
		CreateButton::new(RpsChoice::Scissors.button_id(ctx_id))
			.style(ButtonStyle::Primary)
			.label(RpsChoice::Scissors.emoji()),
	];

	let display = [text_display(
		"# Rock paper scissors...\nMake a choice within 60s...",
	)];
	let container = CreateContainer::new(&display)
		.add_component(separator())
		.add_component(CreateContainerComponent::ActionRow(
			CreateActionRow::Buttons(Cow::Borrowed(&buttons)),
		))
		.accent_colour(Colour::ORANGE);

	send_container(&ctx, container).await?;

	let ctx_id_str = ctx_id.to_string();
	if let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
		.author_id(user.id)
		.timeout(Duration::from_mins(1))
		.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
		.await
	{
		let target_choice = RpsChoice::from_button_id(&interaction.data.custom_id).unwrap();

		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;

		let response = {
			let title = if author_choice == target_choice {
				"You both suck!".to_owned()
			} else if author_choice.beats(target_choice) {
				let mut user_name = ctx
					.author()
					.nick_in(ctx.http(), guild_id)
					.await
					.unwrap_or_else(|| ctx.author().display_name().to_owned());
				user_name.push_str(" won!");
				user_name
			} else {
				let mut user_name = user
					.nick_in(ctx.http(), guild_id)
					.await
					.unwrap_or_else(|| user.display_name().to_owned());
				user_name.push_str(" won!");
				user_name
			};
			format!("# {title}\nStill no luck getting a life")
		};

		let mut msg = interaction.message;

		let text_display = [text_display(&response)];
		let container = CreateContainer::new(&text_display).accent_colour(Colour::ORANGE);

		msg.edit(
			ctx.http(),
			EditMessage::default()
				.components(&[CreateComponent::Container(container)])
				.flags(MessageFlags::IS_COMPONENTS_V2),
		)
		.await?;
	}
	Ok(())
}
