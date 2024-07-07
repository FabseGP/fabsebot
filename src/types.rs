use reqwest::Client;
use std::sync::Arc;

pub type Context<'a> = poise::Context<'a, Data, Error>;
pub struct Data {
    pub db: sqlx::MySqlPool,
    pub req_client: Client,
    pub music_manager: Arc<songbird::Songbird>,
}
pub type Error = Box<dyn std::error::Error + Send + Sync>;
