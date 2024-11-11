use crate::config::settings::{
    AIConfig, APIConfig, GuildSettings, UserSettings, WordReactions, WordTracking,
};

use dashmap::DashMap;
use fastrand::Rng;
use once_cell::sync::{Lazy, OnceCell};
use poise::{
    serenity_prelude::{ChannelId, GuildId, MessageId, ShardManager, UserId, Webhook},
    Context as PContext,
};
use reqwest::Client;
use serde::Serialize;
use songbird::Songbird;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;

type AIChatMap = DashMap<GuildId, Vec<AIChatMessage>>;
type GlobalChatMap = DashMap<GuildId, DashMap<i64, MessageId>>;
type WebhookMap = DashMap<ChannelId, Webhook>;
type GuildDataMap = DashMap<GuildId, GuildData>;
type UserSettingsMap = DashMap<GuildId, DashMap<UserId, UserSettings>>;

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
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

#[derive(Serialize)]
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

pub struct GuildData {
    pub settings: GuildSettings,
    pub word_reactions: Vec<WordReactions>,
    pub word_tracking: Vec<WordTracking>,
}

pub struct Data {
    pub db: PgPool,
    pub music_manager: Arc<Songbird>,
    pub ai_chats: Arc<AIChatMap>,
    pub global_chats: Arc<GlobalChatMap>,
    pub channel_webhooks: Arc<WebhookMap>,
    pub guild_data: Arc<GuildDataMap>,
    pub user_settings: Arc<UserSettingsMap>,
}

pub type Error = anyhow::Error;
pub type SContext<'a> = PContext<'a, Data, Error>;

pub struct UtilsConfig {
    pub ai: AIConfig,
    pub api: APIConfig,
}

pub static SHARD_MANAGER: OnceCell<Arc<ShardManager>> = OnceCell::new();
pub static UTILS_CONFIG: OnceCell<Arc<UtilsConfig>> = OnceCell::new();
pub static HTTP_CLIENT: Lazy<Client> = Lazy::new(Client::new);
pub static RNG: Lazy<Mutex<Rng>> = Lazy::new(|| Mutex::new(Rng::new()));
