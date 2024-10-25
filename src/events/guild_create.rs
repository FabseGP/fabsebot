use crate::types::{Data, Error};

use anyhow::Context as _;
use poise::serenity_prelude::Guild;
use sqlx::query;
use std::sync::Arc;

pub async fn handle_guild_create(
    data: Arc<Data>,
    guild: &Guild,
    is_new: Option<&bool>,
) -> Result<(), Error> {
    if let Some(new_guild) = is_new {
        if *new_guild {
            let mut conn = data
                .db
                .acquire()
                .await
                .context("Failed to acquire database connection")?;
            query!(
                "INSERT INTO guilds (guild_id)
                VALUES ($1)
                ON CONFLICT (guild_id)
                DO NOTHING",
                i64::from(guild.id)
            )
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}
