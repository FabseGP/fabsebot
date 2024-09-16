use fastrand::Rng;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Serialize;
use songbird::Songbird;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub db: sqlx::MySqlPool,
    pub req_client: Client,
    pub music_manager: Arc<Songbird>,
    pub conversations: Arc<Mutex<HashMap<u64, HashMap<u64, Vec<ChatMessage>>>>>,
    pub rng_thread: Arc<Mutex<Rng>>,
}

static HTTP_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| Arc::new(Client::new()));

pub fn get_http_client() -> Arc<Client> {
    Arc::clone(&HTTP_CLIENT)
}
