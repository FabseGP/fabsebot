use crate::types::Error;

use poise::serenity_prelude::{Context, Reaction};

pub async fn handle_reaction_add(ctx: &Context, add_reaction: &Reaction) -> Result<(), Error> {
    if let Some(guild) = add_reaction.channel(&ctx.http).await?.guild() {
        if let Some(topic) = guild.topic {
            if topic.contains("ai-chat") {
                add_reaction
                    .message(&ctx.http)
                    .await?
                    .react(&ctx.http, add_reaction.emoji.clone())
                    .await?;
            }
        }
    }
    Ok(())
}
