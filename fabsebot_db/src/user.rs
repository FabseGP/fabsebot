use serde::{Deserialize, Serialize};
use sqlx::{Error, Pool, Postgres, postgres::PgQueryResult, query, query_as, types::Json};

#[derive(Serialize, Deserialize)]
pub struct PingedLink {
	pub link: String,
	pub author: String,
}

pub struct UserSettings {
	pub user_id: i64,
	pub afk: bool,
	pub afk_reason: Option<String>,
	pub pinged_links: Json<Vec<PingedLink>>,
	pub ping_content: Option<String>,
	pub ping_media: Option<String>,
}

pub struct UserSettingsLimited {
	pub afk_reason: Option<String>,
	pub pinged_links: Json<Vec<PingedLink>>,
}

pub async fn fetch_user_settings(
	guild_id: i64,
	user_id: i64,
	conn: &Pool<Postgres>,
) -> Result<Option<UserSettingsLimited>, Error> {
	query_as!(
		UserSettingsLimited,
		r#"
		WITH updated AS (
    		UPDATE user_settings
    		SET message_count = message_count + 1
    		WHERE guild_id = $1
    		AND user_id = $2
    		RETURNING afk_reason, pinged_links, afk
		)
		SELECT afk_reason,
    		pinged_links as "pinged_links: Json<Vec<PingedLink>>"
		FROM updated
		WHERE afk = TRUE
        "#,
		guild_id,
		user_id
	)
	.fetch_optional(conn)
	.await
}

pub async fn insert_user_settings(
	guild_id: i64,
	user_id: i64,
	conn: &Pool<Postgres>,
) -> Result<PgQueryResult, Error> {
	query!(
		r#"
		WITH ensured_user AS (
			INSERT INTO users (user_id)
			VALUES ($2)
			ON CONFLICT (user_id) DO NOTHING
		)
		INSERT INTO user_settings (guild_id, user_id, message_count)
		VALUES ($1, $2, 1)
		ON CONFLICT (guild_id, user_id)
		DO NOTHING
    	"#,
		guild_id,
		user_id
	)
	.execute(conn)
	.await
}
