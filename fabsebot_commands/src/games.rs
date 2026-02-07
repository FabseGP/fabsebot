use std::time::Duration;

use fabsebot_core::config::{
	constants::{COLOUR_ORANGE, NOT_IN_GUILD_MSG},
	types::{Error, SContext},
};
use poise::{ChoiceParameter, CreateReply};
use serenity::all::{
	ButtonStyle, ComponentInteractionCollector, CreateActionRow, CreateButton, CreateComponent,
	CreateEmbed, CreateInteractionResponse, EditMessage, User,
};

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
#[poise::command(prefix_command, slash_command)]
pub async fn rps(
	ctx: SContext<'_>,
	#[description = "Target"] user: User,
	#[description = "Your choice: rock, paper, or scissors"] author_choice: RpsChoice,
) -> Result<(), Error> {
	let Some(guild_id) = ctx.guild_id() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Ok(());
	};
	if user.bot() {
		ctx.reply("**Invalid target, get some friends**").await?;
		return Ok(());
	}

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

	let embed = CreateEmbed::default()
		.title("Rock paper scissors...")
		.colour(COLOUR_ORANGE)
		.description("Make a choice within 60s...");

	ctx.send(
		CreateReply::default()
			.embed(embed)
			.reply(true)
			.components(&[CreateComponent::ActionRow(CreateActionRow::Buttons(
				Cow::Borrowed(&buttons),
			))]),
	)
	.await?;

	let ctx_id_str = ctx_id.to_string();
	if let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
		.author_id(user.id)
		.timeout(Duration::from_secs(60))
		.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
		.await
	{
		let target_choice = RpsChoice::from_button_id(&interaction.data.custom_id).unwrap();

		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;

		let response = if author_choice == target_choice {
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

		let mut msg = interaction.message;
		let embed = CreateEmbed::default()
			.title(&response)
			.colour(COLOUR_ORANGE)
			.description("Still no luck getting a life");

		msg.edit(
			ctx.http(),
			EditMessage::default().embed(embed).components(vec![]),
		)
		.await?;
	}
	Ok(())
}
