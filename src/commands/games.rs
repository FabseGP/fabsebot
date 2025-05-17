use crate::config::{
    constants::COLOUR_ORANGE,
    types::{Error, SContext},
};

use poise::{
    CreateReply,
    serenity_prelude::{
        AutocompleteChoice, ButtonStyle, ComponentInteractionCollector, CreateActionRow,
        CreateAutocompleteResponse, CreateButton, CreateEmbed, CreateInteractionResponse,
        EditMessage, Member,
    },
};
use std::{string::ToString, time::Duration};

#[expect(clippy::unused_async)]
async fn autocomplete_choice<'a>(
    _ctx: SContext<'_>,
    partial: &'a str,
) -> CreateAutocompleteResponse<'a> {
    let choices: Vec<_> = ["rock", "paper", "scissors"]
        .into_iter()
        .filter(move |name| name.starts_with(partial))
        .map(AutocompleteChoice::from)
        .collect();
    CreateAutocompleteResponse::default().set_choices(choices)
}

/// Get rekt by an another user in rps
#[poise::command(prefix_command, slash_command)]
pub async fn rps(
    ctx: SContext<'_>,
    #[description = "Target"] user: Member,
    #[description = "Your choice: rock, paper, or scissor"]
    #[autocomplete = "autocomplete_choice"]
    #[rest]
    choice: String,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        if user.user.bot() {
            ctx.reply("**Invalid target, get some friends**").await?;
        } else {
            let valid_choices = ["rock", "paper", "scissors"];
            let author_choice = choice.to_lowercase();
            if !valid_choices.contains(&author_choice.as_str()) {
                ctx.reply("Can't you even do smth this simple correct?")
                    .await?;
                return Ok(());
            }

            let ctx_id = ctx.id();
            let rock_id = format!("{ctx_id}_r");
            let paper_id = format!("{ctx_id}_p");
            let scissor_id = format!("{ctx_id}_s");

            let buttons = [
                CreateButton::new(rock_id.as_str())
                    .style(ButtonStyle::Primary)
                    .label("🪨"),
                CreateButton::new(paper_id.as_str())
                    .style(ButtonStyle::Primary)
                    .label("🧻"),
                CreateButton::new(scissor_id.as_str())
                    .style(ButtonStyle::Primary)
                    .label("✂️"),
            ];

            let mut embed = CreateEmbed::default()
                .title("Rock paper scissors...")
                .colour(COLOUR_ORANGE)
                .description("Make a choice within 60s...");

            ctx.send(
                CreateReply::default()
                    .embed(embed)
                    .reply(true)
                    .components(&[CreateActionRow::Buttons(Cow::Borrowed(&buttons))]),
            )
            .await?;

            let ctx_id_copy = ctx.id();
            if let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
                .author_id(user.user.id)
                .timeout(Duration::from_secs(60))
                .filter(move |interaction| {
                    interaction
                        .data
                        .custom_id
                        .starts_with(ctx_id_copy.to_string().as_str())
                })
                .await
            {
                let target_choice = interaction.data.custom_id.as_str();

                interaction
                    .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                    .await?;

                let response = {
                    let author_choice_str = author_choice.as_str();
                    let result = match (author_choice_str, target_choice) {
                        ("rock", "scissors") | ("paper", "rock") | ("scissors", "paper") => {
                            Some(author_choice_str)
                        }
                        (a, b) if a == b => None,
                        _ => Some(target_choice),
                    };
                    match result {
                        Some(winner) if winner == author_choice => {
                            let user_name = ctx
                                .author()
                                .nick_in(ctx.http(), guild_id)
                                .await
                                .unwrap_or_else(|| ctx.author().display_name().to_owned());
                            format!("{user_name} won!")
                        }
                        Some(_) => {
                            let user_name = user.nick.as_ref().map_or_else(
                                || user.display_name().to_owned(),
                                ToString::to_string,
                            );
                            format!("{user_name} won!")
                        }
                        None => "You both suck!".to_owned(),
                    }
                };

                let mut msg = interaction.message;

                embed = CreateEmbed::default()
                    .title(&response)
                    .colour(COLOUR_ORANGE)
                    .description("Still no luck getting a life");

                msg.edit(
                    ctx.http(),
                    EditMessage::default().embed(embed).components(vec![]),
                )
                .await?;
            }
        }
    }

    Ok(())
}
