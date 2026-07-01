use sqlx::{Error, Pool, Postgres, Transaction, postgres::PgQueryResult, query, query_as};

pub struct GuildSettings {
	pub spoiler_channel: Option<i64>,
	pub ai_chat_channel: Option<i64>,
	pub global_chat_channel: Option<i64>,
	pub global_chat: bool,
	pub music_channel: Option<i64>,
	pub chatbot_role: Option<String>,
}

pub async fn set_music_channel(
	guild_id: i64,
	channel_id: i64,
	conn: &Pool<Postgres>,
) -> Result<PgQueryResult, Error> {
	query!(
		r#"
		UPDATE guild_settings
		SET music_channel = $2
		WHERE guild_id = $1
        "#,
		guild_id,
		channel_id
	)
	.execute(conn)
	.await
}

pub async fn set_spoiler_channel(
	guild_id: i64,
	channel_id: i64,
	conn: &Pool<Postgres>,
) -> Result<PgQueryResult, Error> {
	query!(
		r#"
		UPDATE guild_settings
		SET spoiler_channel = $2
		WHERE guild_id = $1
        "#,
		guild_id,
		channel_id
	)
	.execute(conn)
	.await
}

pub struct WordReactions {
	pub word: String,
	pub content: Option<String>,
	pub media: Option<String>,
	pub emoji_id: Option<i64>,
	pub guild_emoji: bool,
}

pub struct WordTracking {
	pub guild_id: i64,
	pub word: String,
	pub count: i64,
}

pub async fn reset_guild(
	guild_id: i64,
	tx: &mut Transaction<'static, Postgres>,
) -> Result<PgQueryResult, Error> {
	query!(
		r#"
		UPDATE guild_settings
        SET dead_chat_rate = NULL,
        dead_chat_channel = NULL,
        last_dead_chat = NULL,
        quotes_channel = NULL,
        spoiler_channel = NULL,
        prefix = NULL,
        ai_chat_channel = NULL,
        global_chat_channel = NULL,
        global_chat = FALSE,
        global_call = FALSE,
        music_channel = NULL,
        waifu_channel = NULL,
        waifu_rate = NULL,
        last_waifu = NULL,
        chatbot_role = NULL
    	WHERE guild_id = $1
    	"#,
		guild_id
	)
	.execute(tx.as_mut())
	.await?;
	query!(
		r#"
		DELETE FROM guild_word_tracking
        WHERE guild_id = $1
        "#,
		guild_id
	)
	.execute(tx.as_mut())
	.await?;
	query!(
		r#"
		DELETE FROM guild_word_reaction
		WHERE guild_id = $1
		"#,
		guild_id
	)
	.execute(tx.as_mut())
	.await
}

pub async fn delete_guild(guild_id: i64, conn: &Pool<Postgres>) -> Result<PgQueryResult, Error> {
	query!(
		r#"
		DELETE FROM guilds
		WHERE guild_id = $1
        "#,
		guild_id
	)
	.execute(conn)
	.await
}

pub async fn fetch_guild_settings(
	guild_id: i64,
	conn: &Pool<Postgres>,
) -> Result<Option<GuildSettings>, Error> {
	query_as!(
		GuildSettings,
		r#"
		SELECT spoiler_channel, ai_chat_channel, global_chat_channel,
			music_channel, chatbot_role, global_chat
		FROM guild_settings
		WHERE guild_id = $1
			AND (spoiler_channel IS NOT NULL
			OR ai_chat_channel IS NOT NULL
			OR global_chat_channel IS NOT NULL
			OR music_channel IS NOT NULL
			OR global_chat IS TRUE)
		"#,
		guild_id
	)
	.fetch_optional(conn)
	.await
}

pub async fn insert_guild_settings(
	guild_id: i64,
	conn: &Pool<Postgres>,
) -> Result<PgQueryResult, Error> {
	query!(
		r#"
		WITH ensured_guild AS (
			INSERT INTO guilds (guild_id)
			VALUES ($1)
			ON CONFLICT (guild_id) DO NOTHING
		)
		INSERT INTO guild_settings (guild_id)
		VALUES ($1)
		ON CONFLICT (guild_id)
		DO NOTHING
		"#,
		guild_id
	)
	.execute(conn)
	.await
}
