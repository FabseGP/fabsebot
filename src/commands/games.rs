use crate::types::{Context, Error};

use poise::serenity_prelude as serenity;
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
    let valid_choices = ["rock", "paper", "scissors"];
    if !valid_choices.contains(&choice.as_str()) {
        ctx.say("can't you even do smth this simple correct?")
            .await?;
        return Ok(());
    }

    let _ = ctx
        .say(format!(
        "{}, you're going down! \nPlease type 'rock', 'paper' or 'scissors' within the next 60s",
        user
    ))
        .await;

    let start = std::time::Instant::now();
    let mut proceed = false;

    while start.elapsed() < Duration::from_secs(60) && !proceed {
        if let Some(reply) = user
            .await_reply(ctx)
            .timeout(Duration::from_secs(60) - start.elapsed())
            .await
        {
            if valid_choices.contains(&reply.content.to_lowercase().as_str()) {
                let author_choice = choice.to_lowercase();
                let target_choice = reply.content.to_lowercase();
                let outcomes = HashMap::from([
                    ("rock", "scissors"),
                    ("paper", "rock"),
                    ("scissors", "paper"),
                ]);
                let response = {
                    let result = if author_choice == target_choice {
                        None
                    } else if outcomes.get(&author_choice.as_str()) == Some(&target_choice.as_str())
                    {
                        Some(&author_choice)
                    } else {
                        Some(&target_choice)
                    };
                    match result {
                        Some(winner) if winner == author_choice.as_str() => {
                            format!("{} won!", ctx.author())
                        }
                        Some(_) => format!("{} won!", user),
                        None => "you both suck".to_string(),
                    }
                };

                reply
                    .reply(
                        &ctx,
                        format!(
                            "{} chose {}, {} chose {}. {}",
                            ctx.author(),
                            author_choice,
                            user,
                            target_choice,
                            response
                        ),
                    )
                    .await?;
                proceed = true;
            } else {
                ctx.say("why you dumb? try again").await?;
            }
        } else {
            ctx.say(format!("you will not be missed, {}", user)).await?;
            return Ok(());
        }
    }

    Ok(())
}
