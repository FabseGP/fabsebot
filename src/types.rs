use lazy_static::lazy_static;
use reqwest::Client;
use std::sync::Arc;

pub type Context<'a> = poise::Context<'a, Data, Error>;
pub struct Data {
    pub db: sqlx::MySqlPool,
    pub req_client: Client,
    pub music_manager: Arc<songbird::Songbird>,
}
pub type Error = Box<dyn std::error::Error + Send + Sync>;

lazy_static! {
    static ref HTTP_CLIENT: Arc<Client> = Arc::new(Client::new());
}

pub fn get_http_client() -> Arc<Client> {
    Arc::clone(&HTTP_CLIENT)
}
