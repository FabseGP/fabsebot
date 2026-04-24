use anyhow::Result as AResult;
use sqlx::{PgConnection, query, query_as};

pub struct UserSettings {
	pub guild_id: i64,
	pub user_id: i64,
	pub message_count: i32,
	pub afk: bool,
	pub afk_reason: Option<String>,
	pub pinged_links: Option<String>,
	pub ping_content: Option<String>,
	pub ping_media: Option<String>,
}

pub async fn insert_user(user_id: i64, conn: &mut PgConnection) -> AResult<()> {
	query!(
		r#"
		INSERT INTO users (user_id)
        VALUES ($1)
        ON CONFLICT (user_id)
        DO NOTHING
        "#,
		user_id
	)
	.execute(conn)
	.await?;

	Ok(())
}

pub async fn insert_user_settings(
	guild_id: i64,
	user_id: i64,
	conn: &mut PgConnection,
) -> AResult<UserSettings> {
	let user_settings = query_as!(
		UserSettings,
		r#"
    	INSERT INTO user_settings (guild_id, user_id, message_count)
   		VALUES ($1, $2, 1)
    	ON CONFLICT (guild_id, user_id) 
    	DO UPDATE SET message_count = user_settings.message_count + 1
    	RETURNING *
    	"#,
		guild_id,
		user_id
	)
	.fetch_one(conn)
	.await?;

	Ok(user_settings)
}
