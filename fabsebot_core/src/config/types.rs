use std::{
	collections::HashMap,
	sync::{Arc, LazyLock, OnceLock},
	time::Duration,
};

use anyhow::Error as AError;
use fabsebot_db::guild::GuildData;
use mini_moka::sync::Cache;
use poise::Context as PContext;
use reqwest::Client;
use serde::Serialize;
use serenity::all::{Emoji, GenericChannelId, GuildId, MessageId, ShardManager, UserId, Webhook};
use songbird::{Songbird, input::AuxMetadata};
use sqlx::PgPool;
use systemstat::{Platform as _, System};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::config::settings::{APIConfig, HTTPAgent, ServerConfig, UserSettings};

pub type AIChatMap = Cache<GuildId, Arc<Mutex<AIChatContext>>>;
type GlobalChatMap = Cache<GuildId, Arc<HashMap<GuildId, MessageId>>>;
pub type WebhookMap = Cache<GenericChannelId, Webhook>;
pub type GuildMap = Cache<GuildId, Arc<GuildData>>;
type UserSettingsMap = Cache<GuildId, Arc<HashMap<UserId, UserSettings>>>;

#[derive(Default)]
pub struct AIChatContext {
	pub messages: Vec<AIChatMessage>,
	pub static_info: AIChatStatic,
	pub system_msg_index: usize,
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
	#[must_use]
	pub const fn is_system(&self) -> bool {
		matches!(self, Self::System)
	}

	#[must_use]
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
	#[must_use]
	pub const fn new(role: Role, content: String) -> Self {
		Self { role, content }
	}

	#[must_use]
	pub const fn system(content: String) -> Self {
		Self::new(Role::System, content)
	}

	#[must_use]
	pub const fn user(content: String) -> Self {
		Self::new(Role::User, content)
	}

	#[must_use]
	pub const fn assistant(content: String) -> Self {
		Self::new(Role::Assistant, content)
	}

	#[must_use]
	pub const fn model(content: String) -> Self {
		Self::new(Role::Model, content)
	}
}

pub type Metadata = Arc<(
	AuxMetadata,
	HashMap<GuildId, (String, MessageId, GenericChannelId)>,
)>;

pub struct Data {
	pub db: PgPool,
	pub music_manager: Arc<Songbird>,
	pub ai_chats: AIChatMap,
	pub global_chats: GlobalChatMap,
	pub channel_webhooks: WebhookMap,
	pub guilds: GuildMap,
	pub user_settings: UserSettingsMap,
	pub track_metadata: Cache<Uuid, Metadata>,
	pub app_emojis: Cache<u64, Arc<Emoji>>,
}

pub type Error = AError;
pub type SContext<'a> = PContext<'a, Data, Error>;

pub struct UtilsConfig {
	pub owner_id: u64,
	pub ping_message: String,
	pub ping_payload: String,
	pub fabseserver: ServerConfig,
	pub api: APIConfig,
	pub http_agent: HTTPAgent,
	pub bot_name: String,
}

pub static UTILS_CONFIG: OnceLock<Arc<UtilsConfig>> = OnceLock::new();
pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
	let http_agent = &UTILS_CONFIG.get().unwrap().http_agent;
	Client::builder()
		.user_agent(format!(
			"{} ({}; {})",
			http_agent.title, http_agent.repo, http_agent.email
		))
		.zstd(true)
		.http3_congestion_bbr()
		.timeout(Duration::from_secs(300))
		.build()
		.unwrap()
});
pub static SYSTEM_STATS: LazyLock<Arc<System>> = LazyLock::new(|| Arc::new(System::new()));

pub static CLIENT_DATA: OnceLock<Arc<ClientData>> = OnceLock::new();

pub struct ClientData {
	pub shard_manager: ShardManager,
}
