use fastrand::Rng;
use once_cell::sync::{Lazy, OnceCell};
use poise::serenity_prelude::ShardManager;
use reqwest::Client;
use serde::Serialize;
use songbird::Songbird;
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

type ChatHashMap = HashMap<u64, HashMap<u64, Vec<ChatMessage>>>;
pub type Error = anyhow::Error;
pub type SContext<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub db: sqlx::MySqlPool,
    pub music_manager: Arc<Songbird>,
    pub conversations: Arc<Mutex<ChatHashMap>>,
}

pub struct ClientData {
    pub shard_manager: Arc<ShardManager>,
}

pub static CLIENT_DATA: OnceCell<ClientData> = OnceCell::new();
pub static HTTP_CLIENT: Lazy<Client> = Lazy::new(Client::new);
pub static RNG: Lazy<Mutex<Rng>> = Lazy::new(|| Mutex::new(Rng::new()));

macro_rules! load_env {
    ($name:expr) => {
        Lazy::new(|| env::var($name).unwrap_or_else(|_| panic!("{} must be set", $name)))
    };
}

pub static CLOUDFLARE_TOKEN: Lazy<String> = load_env!("CLOUDFLARE_TOKEN");
pub static CLOUDFLARE_GATEWAY: Lazy<String> = load_env!("CLOUDFLARE_GATEWAY");
pub static AI_SERVER: Lazy<String> = load_env!("AI_SERVER");
pub static TENOR_TOKEN: Lazy<String> = load_env!("TENOR_TOKEN");
pub static GITHUB_TOKEN: Lazy<String> = load_env!("GITHUB_TOKEN");
pub static TRANSLATE_SERVER: Lazy<String> = load_env!("TRANSLATE_SERVER");
