use anyhow::Result as AResult;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, query_as, types::Json};

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

pub async fn insert_user_settings(
	guild_id: i64,
	user_id: i64,
	conn: &Pool<Postgres>,
) -> AResult<UserSettings> {
	let user_settings = query_as!(
		UserSettings,
		r#"
		WITH ensure_guild AS (SELECT ensure_guild($1)),
     		ensure_user AS (SELECT ensure_user($2))
    	INSERT INTO user_settings (guild_id, user_id, message_count)
   		VALUES ($1, $2, 1)
    	ON CONFLICT (guild_id, user_id) 
    	DO UPDATE SET message_count = user_settings.message_count + 1
    	RETURNING user_id, afk_reason,
			pinged_links as "pinged_links: Json<Vec<PingedLink>>", 
			ping_content, ping_media, afk
    	"#,
		guild_id,
		user_id
	)
	.fetch_one(conn)
	.await?;

	Ok(user_settings)
}
