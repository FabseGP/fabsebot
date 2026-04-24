use anyhow::{Context as _, Result as AResult};
use sqlx::{PgConnection, Postgres, Transaction, query, query_as};

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
	pub chatbot_role: Option<String>,
}

pub async fn set_music_channel(
	guild_id: i64,
	channel_id: i64,
	conn: &mut PgConnection,
) -> AResult<()> {
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, music_channel)
        VALUES ($1, $2)
        ON CONFLICT (guild_id)
        DO UPDATE SET music_channel = $2
        "#,
		guild_id,
		channel_id
	)
	.execute(conn)
	.await
	.context("Failed to set music channel")?;

	Ok(())
}

pub async fn set_spoiler_channel(
	guild_id: i64,
	channel_id: i64,
	conn: &mut PgConnection,
) -> AResult<()> {
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, spoiler_channel)
        VALUES ($1, $2)
        ON CONFLICT (guild_id)
        DO UPDATE SET spoiler_channel = $2
        "#,
		guild_id,
		channel_id
	)
	.execute(conn)
	.await
	.context("Failed to set spoiler channel")?;

	Ok(())
}

pub struct WordReactions {
	pub guild_id: i64,
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

pub async fn reset_guild(guild_id: i64, tx: &mut Transaction<'static, Postgres>) -> AResult<()> {
	query!(
		r#"
		UPDATE guild_settings
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
        waifu_channel = NULL,
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
	.await?;

	Ok(())
}

pub async fn insert_guild(guild_id: i64, conn: &mut PgConnection) -> AResult<()> {
	query!(
		r#"
		INSERT INTO guilds (guild_id)
        VALUES ($1)
        ON CONFLICT (guild_id)
        DO NOTHING
        "#,
		guild_id
	)
	.execute(conn)
	.await?;

	Ok(())
}

pub async fn insert_guild_settings(
	guild_id: i64,
	conn: &mut PgConnection,
) -> AResult<GuildSettings> {
	let guild_settings = query_as!(
		GuildSettings,
		r#"
    	INSERT INTO guild_settings (guild_id)
    	VALUES ($1)
    	ON CONFLICT (guild_id) 
    	DO UPDATE SET guild_id = guild_settings.guild_id 
    	RETURNING *
    	"#,
		guild_id
	)
	.fetch_one(conn)
	.await?;

	Ok(guild_settings)
}
