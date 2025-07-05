use std::{collections::HashSet, sync::Arc};

use serenity::all::Guild;
use sqlx::query;

use crate::config::{
	settings::GuildSettings,
	types::{Data, Error, GuildData},
};

pub async fn handle_guild_create(
	data: Arc<Data>,
	guild: &Guild,
	is_new: Option<&bool>,
) -> Result<(), Error> {
	if let Some(new_guild) = is_new
		&& *new_guild
	{
		let guild_id_i64 = i64::from(guild.id);
		query!(
			"INSERT INTO guilds (guild_id)
                VALUES ($1)
                ON CONFLICT (guild_id)
                DO NOTHING",
			guild_id_i64
		)
		.execute(&mut *data.db.acquire().await?)
		.await?;
		let default_settings = GuildSettings {
			guild_id: guild_id_i64,
			..Default::default()
		};
		data.guild_data.lock().await.insert(
			guild.id,
			Arc::new(GuildData {
				settings: default_settings,
				word_reactions: HashSet::new(),
				word_tracking: HashSet::new(),
				emoji_reactions: HashSet::new(),
			}),
		);
	}

	Ok(())
}
