use std::time::Duration;

use fabsebot_core::{
	config::types::{Error, SContext},
	utils::helpers::{edit_message_container, reply_container, separator, text_display},
};
use poise::ChoiceParameter;
use serenity::{
	all::{
		ButtonStyle, Colour, ComponentInteractionCollector, CreateActionRow, CreateButton,
		CreateContainer, CreateContainerComponent, CreateInteractionResponse, User,
	},
	builder::CreateComponent,
};

use crate::require_human;

#[derive(PartialEq, Eq, ChoiceParameter)]
enum RpsChoice {
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

	const fn button_id(self) -> &'static str {
		match self {
			Self::Rock => "rock",
			Self::Paper => "paper",
			Self::Scissors => "scissors",
		}
	}

	fn from_button_id(id: &str) -> Self {
		if id == "rock" {
			Self::Rock
		} else if id == "paper" {
			Self::Paper
		} else {
			Self::Scissors
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
	guild_only,
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn rps(
	ctx: SContext<'_>,
	#[description = "Target"] user: User,
	#[description = "Your choice: rock, paper, or scissors"] choice: RpsChoice,
) -> Result<(), Error> {
	require_human(ctx, &user).await?;

	let buttons = [
		CreateButton::new(RpsChoice::Rock.button_id())
			.style(ButtonStyle::Primary)
			.label(RpsChoice::Rock.emoji()),
		CreateButton::new(RpsChoice::Paper.button_id())
			.style(ButtonStyle::Primary)
			.label(RpsChoice::Paper.emoji()),
		CreateButton::new(RpsChoice::Scissors.button_id())
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
	let component = [CreateComponent::Container(container)];

	let message = ctx.send(reply_container(&component)).await?;

	if let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
		.author_id(user.id)
		.timeout(Duration::from_mins(1))
		.message_id(message.message().await?.id)
		.await
	{
		let target_choice = RpsChoice::from_button_id(&interaction.data.custom_id);

		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;

		let response = if choice == target_choice {
			Cow::Borrowed("You both suck!")
		} else {
			let user_id = if choice.beats(target_choice) {
				ctx.author().id
			} else {
				user.id
			};
			Cow::Owned(format!("# <@{user_id}> won!\nStill no luck getting a life"))
		};

		let mut msg = interaction.message;

		let text_display = [text_display(response)];
		let container = CreateContainer::new(&text_display).accent_colour(Colour::ORANGE);
		let component = [CreateComponent::Container(container)];

		msg.edit(ctx.http(), edit_message_container(&component))
			.await?;
	}
	Ok(())
}
