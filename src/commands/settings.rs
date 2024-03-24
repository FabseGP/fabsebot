use crate::types::{Context, Error};

use poise::CreateReply;

/// Configure the occurence of dead chat gifs
#[poise::command(slash_command, prefix_command)]
pub async fn dead_chat(
    ctx: Context<'_>,
    #[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: u8,
) -> Result<(), Error> {
    let mut conn = ctx.data().db.acquire().await?;
    sqlx::query!(
        "REPLACE INTO guild_settings (guild_id, dead_chat_rate) VALUES (?, ?)",
        ctx.guild_id().unwrap().get(),
        occurrence
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    ctx.send(CreateReply::default().content(format!(
        "Dead chat gifs will only be sent every {} minute(s)... probably",
        occurrence,
    )))
    .await?;
    Ok(())
}
