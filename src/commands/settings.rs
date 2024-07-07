use crate::types::{Context, Error};

use poise::CreateReply;

/// Configure the occurence of dead chat gifs
#[poise::command(slash_command, prefix_command)]
pub async fn dead_chat(
    ctx: Context<'_>,
    #[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: u8,
    #[description = "Channel to send dead chat gifs to"] channel: serenity::model::channel::Channel,
) -> Result<(), Error> {
    let mut conn = ctx.data().db.acquire().await?;
    sqlx::query(
        "REPLACE INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel) VALUES (?, ?, ?)"
    )
    .bind(ctx.guild_id().unwrap().get())
    .bind(occurrence)
    .bind(channel.id().to_string())
    .execute(&mut *conn)
    .await
    .unwrap();
    ctx.send(CreateReply::default().content(format!(
        "Dead chat gifs will only be sent every {} minute(s) in {}... probably",
        occurrence, channel
    )))
    .await?;
    Ok(())
}

