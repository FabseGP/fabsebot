use fastrand::Rng;
use once_cell::sync::{Lazy, OnceCell};
use reqwest::Client;
use serde::Serialize;
use serenity::gateway::ShardManager;
use songbird::Songbird;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

type ChatHashMap = HashMap<u64, HashMap<u64, Vec<ChatMessage>>>;
pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub db: sqlx::MySqlPool,
    pub req_client: Client,
    pub music_manager: Arc<Songbird>,
    pub conversations: Arc<Mutex<ChatHashMap>>,
    pub rng_thread: Arc<Mutex<Rng>>,
}

static HTTP_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| Arc::new(Client::new()));

pub fn get_http_client() -> Arc<Client> {
    Arc::clone(&HTTP_CLIENT)
}

pub struct ClientData {
    pub shard_manager: Arc<ShardManager>,
}
pub static CLIENT_DATA: OnceCell<Arc<ClientData>> = OnceCell::new();
