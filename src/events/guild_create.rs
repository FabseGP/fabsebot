use crate::types::{Data, Error};

use anyhow::Context;
use poise::serenity_prelude::Guild;
use sqlx::query;
use std::sync::Arc;

pub async fn handle_guild_create(
    data: Arc<Data>,
    guild: &Guild,
    is_new: &Option<bool>,
) -> Result<(), Error> {
    if is_new.is_some() {
        let mut conn = data
            .db
            .acquire()
            .await
            .context("Failed to acquire database connection")?;
        let guild_id: u64 = guild.id.into();
        query!("INSERT IGNORE INTO guilds (guild_id) VALUES (?)", guild_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}
