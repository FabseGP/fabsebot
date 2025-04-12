use crate::config::settings::{
    AIConfig, APIConfig, EmojiReactions, GuildSettings, MainConfig, UserSettings, WordReactions,
    WordTracking,
};
use fastrand::Rng;
use mini_moka::sync::Cache;
use once_cell::sync::{Lazy, OnceCell};
use poise::{
    Context as PContext,
    serenity_prelude::{GenericChannelId, GuildId, MessageId, UserId, Webhook},
};
use reqwest::Client;
use serde::Serialize;
use songbird::Songbird;
use sqlx::PgPool;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::Mutex;

pub type AIChatMap = Cache<GuildId, Arc<Mutex<AIChatContext>>>;
type GlobalChatMap = Cache<GuildId, Arc<HashMap<GuildId, MessageId>>>;
pub type WebhookMap = Cache<GenericChannelId, Webhook>;
pub type GuildDataMap = Cache<GuildId, Arc<GuildData>>;
type UserSettingsMap = Cache<GuildId, Arc<HashMap<UserId, UserSettings>>>;

#[derive(Default)]
pub struct AIChatContext {
    pub messages: Vec<AIChatMessage>,
    pub static_info: AIChatStatic,
}

#[derive(Default)]
pub struct AIChatStatic {
    pub is_set: bool,
    pub chatbot_role: String,
    pub guild_desc: String,
    pub users: HashMap<u64, String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    System,
    User,
    Assistant,
}

impl Role {
    pub const fn is_system(&self) -> bool {
        matches!(self, Self::System)
    }

    pub const fn is_user(&self) -> bool {
        matches!(self, Self::User)
    }
}

#[derive(Serialize, Clone, Default)]
pub struct AIChatMessage {
    pub role: Role,
    pub content: String,
}

impl AIChatMessage {
    pub const fn new(role: Role, content: String) -> Self {
        Self { role, content }
    }

    pub const fn system(content: String) -> Self {
        Self::new(Role::System, content)
    }

    pub const fn user(content: String) -> Self {
        Self::new(Role::User, content)
    }

    pub const fn assistant(content: String) -> Self {
        Self::new(Role::Assistant, content)
    }
}

#[derive(Clone, Default)]
pub struct GuildData {
    pub settings: GuildSettings,
    pub word_reactions: HashSet<WordReactions>,
    pub word_tracking: HashSet<WordTracking>,
    pub emoji_reactions: HashSet<EmojiReactions>,
}

pub struct Data {
    pub db: PgPool,
    pub music_manager: Arc<Songbird>,
    pub ai_chats: Arc<AIChatMap>,
    pub global_chats: Arc<GlobalChatMap>,
    pub channel_webhooks: Arc<WebhookMap>,
    pub guild_data: Arc<Mutex<GuildDataMap>>,
    pub user_settings: Arc<Mutex<UserSettingsMap>>,
}

pub type Error = anyhow::Error;
pub type SContext<'a> = PContext<'a, Data, Error>;

pub struct UtilsConfig {
    pub bot: MainConfig,
    pub ai: AIConfig,
    pub api: APIConfig,
}

pub static UTILS_CONFIG: OnceCell<Arc<UtilsConfig>> = OnceCell::new();
pub static HTTP_CLIENT: Lazy<Client> = Lazy::new(Client::new);
pub static RNG: Lazy<Mutex<Rng>> = Lazy::new(|| Mutex::new(Rng::new()));
