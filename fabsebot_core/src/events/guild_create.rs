use std::sync::Arc;

use anyhow::Result as AResult;
use fabsebot_db::guild::{GuildData, GuildSettings, insert_guild};
use serenity::all::Guild;

use crate::config::types::Data;

pub async fn handle_guild_create(
	data: Arc<Data>,
	guild: &Guild,
	is_new: Option<&bool>,
) -> AResult<()> {
	if let Some(new_guild) = is_new
		&& *new_guild
	{
		let guild_id_i64 = i64::from(guild.id);
		insert_guild(guild_id_i64, &mut *data.db.acquire().await?).await?;
		let default_settings = GuildSettings {
			guild_id: guild_id_i64,
			..Default::default()
		};
		data.guilds.insert(
			guild.id,
			Arc::new(GuildData {
				settings: default_settings,
				..Default::default()
			}),
		);
	}

	Ok(())
}
