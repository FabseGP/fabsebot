use std::{
	borrow::Cow,
	sync::{Arc, LazyLock, OnceLock, RwLock, atomic::AtomicBool},
	time::Duration,
};

use anyhow::Error as AError;
use dashmap::DashMap;
use lavalink_rs::client::LavalinkClient;
use mini_moka::sync::Cache;
use poise::Context as PContext;
use reqwest::Client;
use serde::Serialize;
use serenity::{
	all::{
		Cache as SerenityCache, Emoji, EmojiId, GenericChannelId, GuildId, ShardId,
		ShardRunnerMetadata, Webhook,
	},
	http::Http,
};
use songbird::Songbird;
use sqlx::PgPool;
use systemstat::{Platform as _, System};
use tokio::sync::{
	Mutex, mpsc,
	watch::{self},
};
use tracing::error;

use crate::{
	config::settings::{APIConfig, HTTPAgent, ServerConfig},
	utils::{
		ai::{AIQueuePayload, ContentPart, ToolCall},
		voice::{ConnectionStatus, QueueData, TrackSignal},
	},
};

pub type AIQueue = mpsc::Sender<AIQueuePayload>;

pub type MusicQueueData = Arc<QueueData>;
pub type MusicQueue = mpsc::Sender<MusicQueueData>;

pub struct MusicData {
	pub queue: MusicQueue,
	pub global: AtomicBool,
	pub track_signals: watch::Sender<TrackSignal>,
	pub connection_signals: watch::Sender<ConnectionStatus>,
}

impl MusicData {
	pub fn connected(&self, status: ConnectionStatus) {
		if let Err(err) = self.connection_signals.send(status) {
			error!("Failed to notify about connected status: {err}");
		}
	}

	pub fn disconnected(&self) {
		if let Err(err) = self.connection_signals.send(ConnectionStatus::Disconnected) {
			error!("Failed to notify about disconnected status: {err}");
		}
	}

	pub fn is_disconnected(&self) -> bool {
		*self.connection_signals.borrow() == ConnectionStatus::Disconnected
	}

	pub fn is_songbird_connected(&self) -> bool {
		*self.connection_signals.borrow() == ConnectionStatus::SongbirdConnected
	}

	pub fn is_lavalink_connected(&self) -> bool {
		*self.connection_signals.borrow() == ConnectionStatus::LavalinkConnected
	}

	pub fn has_track_exception(&self) -> bool {
		*self.track_signals.borrow() == TrackSignal::Exception
	}
}

pub struct GuildCache {
	pub ai_queue: AIQueue,
	pub music_data: MusicData,
	pub prefix: RwLock<Cow<'static, str>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "lowercase")]
enum AIRole {
	System,
	User,
	Assistant,
	Tool,
}

#[derive(Serialize, Clone)]
#[serde(untagged)]
pub enum ChatContent {
	Text(Cow<'static, str>),
	Parts(Vec<ContentPart>),
}

#[derive(Serialize, Clone)]
pub struct AIChatMessage {
	role: AIRole,
	#[serde(skip_serializing_if = "Option::is_none")]
	content: Option<ChatContent>,
	#[serde(skip_serializing_if = "Option::is_none")]
	tool_call_id: Option<Cow<'static, str>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	tool_calls: Option<Vec<ToolCall>>,
}

impl AIChatMessage {
	#[must_use]
	const fn new(
		role: AIRole,
		content: Option<ChatContent>,
		tool_call_id: Option<Cow<'static, str>>,
		tool_calls: Option<Vec<ToolCall>>,
	) -> Self {
		Self {
			role,
			content,
			tool_call_id,
			tool_calls,
		}
	}

	#[must_use]
	pub const fn system(content: Cow<'static, str>) -> Self {
		Self::new(AIRole::System, Some(ChatContent::Text(content)), None, None)
	}

	#[must_use]
	pub const fn user_text(content: Cow<'static, str>) -> Self {
		Self::new(AIRole::User, Some(ChatContent::Text(content)), None, None)
	}

	#[must_use]
	pub const fn user_parts(parts: Vec<ContentPart>) -> Self {
		Self::new(AIRole::User, Some(ChatContent::Parts(parts)), None, None)
	}

	#[must_use]
	pub const fn assistant(content: Cow<'static, str>) -> Self {
		Self::new(
			AIRole::Assistant,
			Some(ChatContent::Text(content)),
			None,
			None,
		)
	}

	#[must_use]
	pub const fn assistant_with_tools(
		content: Option<ChatContent>,
		tool_calls: Vec<ToolCall>,
	) -> Self {
		Self::new(AIRole::Assistant, content, None, Some(tool_calls))
	}

	#[must_use]
	pub const fn tool_text(content: Cow<'static, str>, call_id: Cow<'static, str>) -> Self {
		Self::new(
			AIRole::Tool,
			Some(ChatContent::Text(content)),
			Some(call_id),
			None,
		)
	}

	#[must_use]
	pub const fn tool_parts(content: Vec<ContentPart>, call_id: Cow<'static, str>) -> Self {
		Self::new(
			AIRole::Tool,
			Some(ChatContent::Parts(content)),
			Some(call_id),
			None,
		)
	}
}

pub type WebhookMap = Cache<GenericChannelId, Arc<Webhook>>;
pub type EmojisMap = Cache<EmojiId, Arc<Emoji>>;

pub struct Data {
	pub db: PgPool,
	pub music_manager: Arc<Songbird>,
	pub channel_webhooks: WebhookMap,
	pub guilds: Cache<GuildId, Arc<GuildCache>>,
	pub app_emojis: EmojisMap,
	pub state_tracker: AtomicBool,
	pub lavalink_client: LavalinkClient,
	pub guild_cache_lock: Arc<Mutex<()>>,
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
	pub error_webhook: String,
	pub feedback_webhook: String,
}

pub static UTILS_CONFIG: OnceLock<UtilsConfig> = OnceLock::new();

pub fn utils_config() -> &'static UtilsConfig {
	UTILS_CONFIG.get().unwrap()
}

pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
	let http_agent = &utils_config().http_agent;
	#[expect(clippy::expect_used)]
	Client::builder()
		.user_agent(format!(
			"{} ({}; {})",
			http_agent.title, http_agent.repo, http_agent.email
		))
		.zstd(true)
		.http3_congestion_bbr()
		.timeout(Duration::from_mins(5))
		.build()
		.expect("Failed to build HTTP-client!")
});

pub static SYSTEM_STATS: LazyLock<System> = LazyLock::new(System::new);

pub static CLIENT_DATA: OnceLock<ClientData> = OnceLock::new();

pub struct ClientData {
	pub runners: Arc<DashMap<ShardId, ShardRunnerMetadata>>,
}

pub fn client_data() -> &'static ClientData {
	CLIENT_DATA.get().unwrap()
}

pub static BOT_CONTEXT: OnceLock<BotContext> = OnceLock::new();

pub struct BotContext {
	pub data: Arc<Data>,
	pub http: Arc<Http>,
	pub cache: Arc<SerenityCache>,
}

pub fn bot_context() -> &'static BotContext {
	BOT_CONTEXT.get().unwrap()
}
