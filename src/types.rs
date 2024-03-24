pub type Context<'a> = poise::Context<'a, Data, Error>;
pub struct Data {
    pub db: sqlx::MySqlPool,
}
pub type Error = Box<dyn std::error::Error + Send + Sync>;
