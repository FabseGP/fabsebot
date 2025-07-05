use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, LazyLock, OnceLock},
};

use anyhow::Error as AError;
use fastrand::Rng;
use mini_moka::sync::Cache;
use poise::Context as PContext;
use reqwest::Client;
use serde::Serialize;
use serenity::all::{GenericChannelId, GuildId, MessageId, ShardManager, UserId, Webhook};
use songbird::Songbird;
use sqlx::PgPool;
use systemstat::{Platform as _, System};
use tokio::sync::Mutex;

use crate::config::settings::{
	APIConfig, EmojiReactions, FabseserverConfig, GuildSettings, MainConfig, UserSettings,
	WordReactions, WordTracking,
};

pub type AIChatMap = Cache<GuildId, Arc<Mutex<AIChatContext>>>;
type GlobalChatMap = Cache<GuildId, Arc<HashMap<GuildId, MessageId>>>;
pub type WebhookMap = Cache<GenericChannelId, Webhook>;
pub type GuildDataMap = Cache<GuildId, Arc<GuildData>>;
type UserSettingsMap = Cache<GuildId, Arc<HashMap<UserId, UserSettings>>>;

pub struct AIModelDefaults {
	pub temperature: f32,
	pub top_k: i32,
	pub min_p: f32,
	pub top_p: f32,
	pub repetition_penalty: f32,
	pub frequency_penalty: f32,
	pub presence_penalty: f32,
}

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
	Model,
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

	pub const fn model(content: String) -> Self {
		Self::new(Role::Model, content)
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

pub type Error = AError;
pub type SContext<'a> = PContext<'a, Data, Error>;

pub struct UtilsConfig {
	pub bot: MainConfig,
	pub fabseserver: FabseserverConfig,
	pub api: APIConfig,
}

pub static UTILS_CONFIG: OnceLock<Arc<UtilsConfig>> = OnceLock::new();
pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(Client::new);
pub static RNG: LazyLock<Mutex<Rng>> = LazyLock::new(|| Mutex::new(Rng::new()));
pub static SYSTEM_STATS: LazyLock<Arc<System>> = LazyLock::new(|| Arc::new(System::new()));

pub static GEMMA_DEFAULTS: LazyLock<AIModelDefaults> = LazyLock::new(|| AIModelDefaults {
	temperature: 1.0,
	top_k: 64,
	min_p: 0.01,
	top_p: 0.95,
	repetition_penalty: 1.0,
	frequency_penalty: 0.0,
	presence_penalty: 0.0,
});

pub static LLAMA_DEFAULTS: LazyLock<AIModelDefaults> = LazyLock::new(|| AIModelDefaults {
	temperature: 0.7,
	top_k: 40,
	min_p: 0.05,
	top_p: 0.9,
	repetition_penalty: 1.1,
	frequency_penalty: 0.0,
	presence_penalty: 0.0,
});

pub static QWEN_DEFAULTS: LazyLock<AIModelDefaults> = LazyLock::new(|| AIModelDefaults {
	temperature: 0.6,
	top_k: 40,
	min_p: 0.01,
	top_p: 0.9,
	repetition_penalty: 1.1,
	frequency_penalty: 0.0,
	presence_penalty: 0.0,
});

pub static CLIENT_DATA: OnceLock<Arc<ClientData>> = OnceLock::new();

pub struct ClientData {
	pub shard_manager: ShardManager,
}
