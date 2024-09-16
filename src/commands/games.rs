use crate::types::{Context, Error};

use poise::{
    futures_util::{Stream, StreamExt},
    serenity_prelude as serenity, CreateReply,
};
use serenity::{
    futures, ButtonStyle, ComponentInteractionCollector, CreateActionRow, CreateButton,
    CreateEmbed, CreateInteractionResponse, EditMessage,
};
use std::{collections::HashMap, time::Duration};

async fn autocomplete_choice<'a>(
    _ctx: Context<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    futures::stream::iter(&["rock", "paper", "scissor"])
        .filter(move |name| futures::future::ready(name.starts_with(partial)))
        .map(|name| name.to_string())
}

/// Get rekt by an another user in rps
#[poise::command(prefix_command, slash_command)]
pub async fn rps(
    ctx: Context<'_>,
    #[description = "Target"] user: serenity::User,
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

        let rock_id = format!("{}_rock", ctx.id());
        let paper_id = format!("{}_paper", ctx.id());
        let scissor_id = format!("{}_scissor", ctx.id());

        let components = vec![CreateActionRow::Buttons(vec![
            CreateButton::new(rock_id.clone())
                .style(ButtonStyle::Primary)
                .label("🪨"),
            CreateButton::new(paper_id.clone())
                .style(ButtonStyle::Primary)
                .label("🧻"),
            CreateButton::new(scissor_id.clone())
                .style(ButtonStyle::Primary)
                .label("✂️"),
        ])];

        let embed = CreateEmbed::default()
            .title("Rock paper scissors...")
            .color(0xf6d32d)
            .description("Make a choice within 60s...");

        ctx.send(CreateReply::default().embed(embed).components(components))
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

            let outcomes =
                HashMap::from([("rock", "scissor"), ("paper", "rock"), ("scissor", "paper")]);

            let response = {
                let result = if target_choice.contains(&author_choice) {
                    None
                } else if let Some(&v) = outcomes.get(&author_choice.as_str()) {
                    if target_choice.contains(v) {
                        Some(&author_choice)
                    } else {
                        Some(target_choice)
                    }
                } else {
                    Some(target_choice)
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

            let new_embed = CreateEmbed::default()
                .title(&response)
                .color(0x00ff00)
                .description("Still no luck getting a life");

            msg.edit(
                ctx.http(),
                EditMessage::new().embed(new_embed).components(vec![]),
            )
            .await?;
        }
    } else {
        ctx.reply("**Invalid target, get some friends**").await?;
    }

    Ok(())
}
