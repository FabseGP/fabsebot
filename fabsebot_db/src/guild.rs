use std::collections::HashSet;

use anyhow::{Context as _, Result as AResult};
use sqlx::{PgConnection, Postgres, Transaction, query};

#[derive(Default, Clone)]
pub struct GuildSettings {
	pub guild_id: i64,
	pub dead_chat_channel: Option<i64>,
	pub dead_chat_rate: Option<i64>,
	pub last_dead_chat: Option<i64>,
	pub quotes_channel: Option<i64>,
	pub spoiler_channel: Option<i64>,
	pub prefix: Option<String>,
	pub ai_chat_channel: Option<i64>,
	pub global_chat_channel: Option<i64>,
	pub global_chat: bool,
	pub global_music: bool,
	pub global_call: bool,
	pub music_channel: Option<i64>,
	pub waifu_channel: Option<i64>,
	pub waifu_rate: Option<i64>,
	pub last_waifu: Option<i64>,
}

impl GuildSettings {
	pub async fn set_music_channel(
		&self,
		guild_id: i64,
		channel_id: i64,
		conn: &mut PgConnection,
	) -> AResult<()> {
		query!(
			"INSERT INTO guild_settings (guild_id, music_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                music_channel = $2",
			guild_id,
			channel_id
		)
		.execute(conn)
		.await
		.context("Failed to set music channel")?;

		Ok(())
	}

	pub async fn set_spoiler_channel(
		&self,
		guild_id: i64,
		channel_id: i64,
		conn: &mut PgConnection,
	) -> AResult<()> {
		query!(
			"INSERT INTO guild_settings (guild_id, spoiler_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                spoiler_channel = $2",
			guild_id,
			channel_id
		)
		.execute(conn)
		.await
		.context("Failed to set spoiler channel")?;

		Ok(())
	}
}

#[derive(Default, Eq, Hash, PartialEq, Clone)]
pub struct WordReactions {
	pub guild_id: i64,
	pub word: String,
	pub content: Option<String>,
	pub media: Option<String>,
	pub emoji_id: Option<i64>,
	pub guild_emoji: bool,
}

#[derive(Default, Eq, Hash, PartialEq, Clone)]
pub struct WordTracking {
	pub guild_id: i64,
	pub word: String,
	pub count: i64,
}

#[derive(Default, Eq, Hash, PartialEq, Clone)]
pub struct EmojiReplies {
	pub guild_id: i64,
	pub emoji_id: i64,
	pub content_reaction: String,
	pub guild_emoji: bool,
}

#[derive(Clone, Default)]
pub struct GuildData {
	pub settings: GuildSettings,
	pub word_reactions: HashSet<WordReactions>,
	pub word_tracking: HashSet<WordTracking>,
	pub emoji_replies: HashSet<EmojiReplies>,
}

impl GuildData {
	pub async fn reset(
		&self,
		guild_id_i64: i64,
		mut tx: Transaction<'static, Postgres>,
	) -> AResult<()> {
		query!(
			"UPDATE guild_settings
            SET dead_chat_rate = NULL,
                dead_chat_channel = NULL,
                quotes_channel = NULL,
                spoiler_channel = NULL,
                prefix = NULL,
                ai_chat_channel = NULL,
                global_chat_channel = NULL,
                global_chat = FALSE,
                global_music = FALSE,
                global_call = FALSE,
                music_channel = NULL,
                waifu_channel = NULL
            WHERE guild_id = $1",
			guild_id_i64
		)
		.execute(&mut *tx)
		.await?;
		query!(
			"DELETE FROM guild_word_tracking
            WHERE guild_id = $1",
			guild_id_i64
		)
		.execute(&mut *tx)
		.await?;
		query!(
			"DELETE FROM guild_word_reaction
            WHERE guild_id = $1",
			guild_id_i64
		)
		.execute(&mut *tx)
		.await?;
		tx.commit()
			.await
			.context("Failed to commit sql-transaction")?;

		Ok(())
	}
}

pub async fn insert_guild(guild_id_i64: i64, conn: &mut PgConnection) -> AResult<()> {
	query!(
		"INSERT INTO guilds (guild_id)
                VALUES ($1)
                ON CONFLICT (guild_id)
                DO NOTHING",
		guild_id_i64
	)
	.execute(conn)
	.await?;

	Ok(())
}

pub async fn insert_user(user_id_i64: i64, conn: &mut PgConnection) -> AResult<()> {
	query!(
		"INSERT INTO users (user_id)
                VALUES ($1)
                ON CONFLICT (user_id)
                DO NOTHING",
		user_id_i64
	)
	.execute(conn)
	.await?;

	Ok(())
}
