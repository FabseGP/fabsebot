use std::sync::Arc;

use anyhow::Result as AResult;
use fabsebot_db::guild::insert_guild;
use serenity::all::GuildId;

use crate::config::types::{Data, GuildCache};

pub async fn handle_guild_create(data: Arc<Data>, guild_id: GuildId) -> AResult<()> {
	insert_guild(i64::from(guild_id), &data.db).await?;
	data.guilds
		.insert(guild_id, Arc::new(GuildCache::default()));

	Ok(())
}
