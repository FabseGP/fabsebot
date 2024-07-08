use crate::types::{Context, Error};

use poise::{serenity_prelude as serenity, CreateReply};
use serenity::{
    ButtonStyle, ComponentInteractionCollector, CreateActionRow, CreateButton, CreateEmbed,
    CreateInteractionResponse, EditMessage,
};
use std::{collections::HashMap, time::Duration};

/// Get rekt by an another user in rps
#[poise::command(slash_command, prefix_command)]
pub async fn rps(
    ctx: Context<'_>,
    #[description = "Target"] user: serenity::User,
    #[description = "Your choice: rock, paper, or scissor"]
    #[rest]
    choice: String,
) -> Result<(), Error> {
    if !user.bot() && user.id != ctx.author().id {
        let author_choice = choice.to_lowercase();
        let valid_choices = ["rock", "paper", "scissor"];
        if !valid_choices.contains(&author_choice.as_str()) {
            ctx.say("can't you even do smth this simple correct?")
                .await?;
            return Ok(());
        }
        /*
            let options = [
                CreateSelectMenuOption::new("🪨", "rock"),
                CreateSelectMenuOption::new("🧻", "paper"),
                CreateSelectMenuOption::new("✂️", "scissor"),
            ];

            let components = vec![CreateActionRow::SelectMenu(CreateSelectMenu::new(
                "animal_select",
                CreateSelectMenuKind::String {
                    options: Cow::Borrowed(&options),
                },
            ))];
        */

        let components = vec![CreateActionRow::Buttons(vec![
            CreateButton::new("rock")
                .style(ButtonStyle::Primary)
                .label("🪨"),
            CreateButton::new("paper")
                .style(ButtonStyle::Primary)
                .label("🧻"),
            CreateButton::new("scissor")
                .style(ButtonStyle::Primary)
                .label("✂️"),
        ])];

        let embed = CreateEmbed::new()
            .title("Rock paper scissors...")
            .color(0xf6d32d)
            .description("Make a choice within 60s...");

        ctx.send(CreateReply::default().embed(embed).components(components))
            .await?;

        while let Some(interaction) =
            ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                .author_id(user.id)
                .timeout(Duration::from_secs(60))
                .await
        {
            let target_choice = match &interaction.data.custom_id[..] {
                "rock" | "paper" | "scissor" => interaction.data.custom_id.to_string(),
                _ => {
                    ctx.say("why you dumb? try again").await?;
                    continue;
                }
            };

            interaction
                .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                .await?;

            let outcomes =
                HashMap::from([("rock", "scissor"), ("paper", "rock"), ("scissor", "paper")]);
            let response = {
                let result = if author_choice == target_choice {
                    None
                } else if outcomes.get(&author_choice.as_str()) == Some(&target_choice.as_str()) {
                    Some(&author_choice)
                } else {
                    Some(&target_choice)
                };
                match result {
                    Some(winner) if winner == &author_choice => {
                        format!(
                            "{} won!",
                            ctx.author()
                                .nick_in(ctx.http(), ctx.guild_id().unwrap())
                                .await
                                .unwrap_or(ctx.author().name.to_string())
                        )
                    }
                    Some(_) => format!(
                        "{} won!",
                        user.nick_in(ctx.http(), ctx.guild_id().unwrap())
                            .await
                            .unwrap_or(user.name.to_string())
                    ),
                    None => "You both suck!".to_string(),
                }
            };

            let mut msg = interaction.message.clone();

            let new_embed = CreateEmbed::new()
                .title(&response)
                .color(0x00ff00)
                .description("Still no luck getting a life");

            msg.edit(
                ctx.http(),
                EditMessage::new().embed(new_embed).components(vec![]),
            )
            .await?;

            break;
        }
    } else {
        ctx.say("**invalid target, get some friends**").await?;
    }

    Ok(())
}
