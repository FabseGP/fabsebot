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
    pub rng_thread: Arc<Mutex<Rng>>,
}

pub struct ClientData {
    pub shard_manager: Arc<ShardManager>,
}

pub static CLIENT_DATA: OnceCell<ClientData> = OnceCell::new();
pub static HTTP_CLIENT: Lazy<Client> = Lazy::new(Client::new);
pub static CLOUDFLARE_TOKEN: Lazy<String> =
    Lazy::new(|| env::var("CLOUDFLARE_TOKEN").expect("CLOUDFLARE_TOKEN must be set"));
pub static CLOUDFLARE_GATEWAY: Lazy<String> =
    Lazy::new(|| env::var("CLOUDFLARE_GATEWAY").expect("CLOUDFLARE_GATEWAY must be set"));
pub static AI_SERVER: Lazy<String> =
    Lazy::new(|| env::var("AI_SERVER").expect("AI_SERVER must be set"));
pub static TENOR_TOKEN: Lazy<String> =
    Lazy::new(|| env::var("TENOR_TOKEN").expect("TENOR_TOKEN must be set"));
pub static GITHUB_TOKEN: Lazy<String> =
    Lazy::new(|| env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set"));
pub static TRANSLATE_SERVER: Lazy<String> =
    Lazy::new(|| env::var("TRANSLATE_SERVER").expect("TRANSLATE_SERVER must be set"));
