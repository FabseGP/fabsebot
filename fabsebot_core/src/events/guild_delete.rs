use std::sync::Arc;

use anyhow::Result as AResult;
use fabsebot_db::guild::delete_guild;
use serenity::all::GuildId;

use crate::config::types::Data;

pub async fn handle_guild_delete(data: Arc<Data>, guild_id: GuildId) -> AResult<()> {
	delete_guild(i64::from(guild_id), &data.db).await?;

	Ok(())
}
