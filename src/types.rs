pub struct BotStorage {
    database: sqlx::SqlitePool,
}
pub type Context<'a> = poise::Context<'a, Data, Error>;
pub struct Data {}
pub type Error = Box<dyn std::error::Error + Send + Sync>;
