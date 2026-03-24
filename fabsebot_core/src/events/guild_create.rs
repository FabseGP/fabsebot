use std::sync::Arc;

use anyhow::Result as AResult;
use fabsebot_db::guild::insert_guild;
use serenity::all::Guild;

use crate::config::types::{Data, GuildCache};

pub async fn handle_guild_create(data: Arc<Data>, guild: &Guild, is_new: bool) -> AResult<()> {
	if is_new {
		insert_guild(i64::from(guild.id), &mut *data.db.acquire().await?).await?;
		data.guilds
			.insert(guild.id, Arc::new(GuildCache::default()));
	}

	Ok(())
}
