use anyhow::Result as AResult;
use sqlx::{Pool, Postgres, query_as};

pub struct UserSettings {
	pub user_id: i64,
	pub afk: bool,
	pub afk_reason: Option<String>,
	pub pinged_links: Option<String>,
	pub ping_content: Option<String>,
	pub ping_media: Option<String>,
}

pub async fn insert_user_settings(
	guild_id: i64,
	user_id: i64,
	conn: &Pool<Postgres>,
) -> AResult<UserSettings> {
	let user_settings = query_as!(
		UserSettings,
		r#"
		WITH ensure_user AS (
			INSERT INTO users (user_id)
			VALUES ($1)
			ON CONFLICT (user_id) DO NOTHING
		)
    	INSERT INTO user_settings (guild_id, user_id, message_count)
   		VALUES ($1, $2, 1)
    	ON CONFLICT (guild_id, user_id) 
    	DO UPDATE SET message_count = user_settings.message_count + 1
    	RETURNING user_id, afk_reason, pinged_links, ping_content, ping_media, afk
    	"#,
		guild_id,
		user_id
	)
	.fetch_one(conn)
	.await?;

	Ok(user_settings)
}
