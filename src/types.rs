use lazy_static::lazy_static;
use reqwest::Client;
use serde::Serialize;
use std::{collections::HashMap, error::Error as StdError, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub type Context<'a> = poise::Context<'a, Data, Error>;
pub struct Data {
    pub db: sqlx::MySqlPool,
    pub req_client: Client,
    pub music_manager: Arc<songbird::Songbird>,
    pub conversations: Arc<Mutex<HashMap<u64, HashMap<u64, Vec<ChatMessage>>>>>,
}

pub type Error = Box<dyn StdError + Send + Sync>;

lazy_static! {
    static ref HTTP_CLIENT: Arc<Client> = Arc::new(Client::new());
}

pub fn get_http_client() -> Arc<Client> {
    Arc::clone(&HTTP_CLIENT)
}
