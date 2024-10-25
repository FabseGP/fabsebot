use crate::types::{Error, SContext};

use dashmap::DashMap;
use poise::{
    futures_util::{Stream, StreamExt},
    serenity_prelude::{
        futures, ButtonStyle, ComponentInteractionCollector, CreateActionRow, CreateButton,
        CreateEmbed, CreateInteractionResponse, EditMessage, User,
    },
    CreateReply,
};
use std::{borrow::Cow, time::Duration};

async fn autocomplete_choice<'a>(
    _ctx: SContext<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    futures::stream::iter(&["rock", "paper", "scissor"])
        .filter(move |name| futures::future::ready(name.starts_with(partial)))
        .map(|name| (*name).to_string())
}

/// Get rekt by an another user in rps
#[poise::command(prefix_command, slash_command)]
pub async fn rps(
    ctx: SContext<'_>,
    #[description = "Target"] user: User,
    #[description = "Your choice: rock, paper, or scissor"]
    #[autocomplete = "autocomplete_choice"]
    #[rest]
    choice: String,
) -> Result<(), Error> {
    if !user.bot() {
        let valid_choices = ["rock", "paper", "scissor"];
        let author_choice = choice.to_lowercase();
        if !valid_choices.contains(&author_choice.as_str()) {
            ctx.reply("Can't you even do smth this simple correct?")
                .await?;
            return Ok(());
        }

        let ctx_id = ctx.id();
        let rock_id = format!("{ctx_id}_rock");
        let paper_id = format!("{ctx_id}_paper");
        let scissor_id = format!("{ctx_id}_scissor");

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

        let embed = CreateEmbed::default()
            .title("Rock paper scissors...")
            .color(0xf6d32d)
            .description("Make a choice within 60s...");

        ctx.send(
            CreateReply::default()
                .embed(embed)
                .components(&[CreateActionRow::Buttons(Cow::Borrowed(&buttons))]),
        )
        .await?;

        if let Some(interaction) =
            ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                .author_id(user.id)
                .timeout(Duration::from_secs(60))
                .filter(move |interaction| {
                    let id = interaction.data.custom_id.as_str();
                    id == rock_id.as_str() || id == paper_id.as_str() || id == scissor_id.as_str()
                })
                .await
        {
            let target_choice = &interaction.data.custom_id.to_string();

            interaction
                .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                .await?;

            let outcomes: DashMap<&str, &str> =
                [("rock", "scissor"), ("paper", "rock"), ("scissor", "paper")]
                    .into_iter()
                    .collect();

            let response = {
                let result = if target_choice == &author_choice {
                    None
                } else if let Some(v) = outcomes.get(&author_choice.as_str()) {
                    if target_choice == v.key() {
                        Some(&author_choice)
                    } else {
                        Some(target_choice)
                    }
                } else {
                    Some(target_choice)
                };
                match result {
                    Some(winner) if winner == &author_choice => {
                        let user_name = ctx
                            .author()
                            .nick_in(ctx.http(), ctx.guild_id().unwrap())
                            .await
                            .unwrap_or_else(|| ctx.author().display_name().to_owned());
                        format!("{user_name} won!")
                    }
                    Some(_) => {
                        let user_name = user
                            .nick_in(ctx.http(), ctx.guild_id().unwrap())
                            .await
                            .unwrap_or_else(|| user.name.into_string());
                        format!("{user_name} won!")
                    }
                    None => "You both suck!".to_owned(),
                }
            };

            let mut msg = interaction.message;

            let new_embed = CreateEmbed::default()
                .title(&response)
                .color(0x00ff00)
                .description("Still no luck getting a life");

            msg.edit(
                ctx.http(),
                EditMessage::default().embed(new_embed).components(vec![]),
            )
            .await?;
        }
    } else {
        ctx.reply("**Invalid target, get some friends**").await?;
    }

    Ok(())
}
