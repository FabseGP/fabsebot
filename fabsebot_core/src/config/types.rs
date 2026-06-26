use std::{
	sync::{Arc, LazyLock, OnceLock, atomic::AtomicBool},
	time::Duration,
};

use anyhow::Error as AError;
use dashmap::DashMap;
use lavalink_rs::client::LavalinkClient;
use mini_moka::sync::Cache;
use poise::Context as PContext;
use reqwest::Client;
use serde::Serialize;
use serenity::all::{
	Context, Emoji, EmojiId, GenericChannelId, GuildId, ShardId, ShardRunnerMetadata, Webhook,
};
use songbird::Songbird;
use sqlx::PgPool;
use systemstat::{Platform as _, System};
use tokio::sync::{Mutex, watch::Sender};

use crate::{
	config::settings::{APIConfig, HTTPAgent, ServerConfig},
	utils::{
		ai::{ContentPart, ToolCall},
		voice::TrackSignal,
	},
};

pub type WebhookMap = Cache<GenericChannelId, Webhook>;
pub type AIChats = Arc<Mutex<AIChatContext>>;

#[derive(Default)]
pub struct GuildCache {
	pub ai_chats: AIChats,
}

#[derive(Default)]
pub struct AIChatContext {
	pub messages: Vec<AIChatMessage>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum AIRole {
	System,
	User,
	Assistant,
	Tool,
}

#[derive(Serialize)]
pub struct AIChatMessage {
	role: AIRole,
	#[serde(skip_serializing_if = "Option::is_none")]
	content: Option<Vec<ContentPart>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	tool_call_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	tool_calls: Option<Vec<ToolCall>>,
}

impl AIChatMessage {
	#[must_use]
	const fn new(
		role: AIRole,
		content: Option<Vec<ContentPart>>,
		tool_call_id: Option<String>,
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
	pub fn system(content: String) -> Self {
		Self::new(
			AIRole::System,
			Some(vec![ContentPart::Text { text: content }]),
			None,
			None,
		)
	}

	#[must_use]
	pub const fn user(content: Vec<ContentPart>) -> Self {
		Self::new(AIRole::User, Some(content), None, None)
	}

	#[must_use]
	pub fn assistant(content: String) -> Self {
		Self::new(
			AIRole::Assistant,
			Some(vec![ContentPart::Text { text: content }]),
			None,
			None,
		)
	}

	#[must_use]
	pub const fn assistant_with_tools(
		content: Option<Vec<ContentPart>>,
		tool_calls: Vec<ToolCall>,
	) -> Self {
		Self::new(AIRole::Assistant, content, None, Some(tool_calls))
	}

	#[must_use]
	pub const fn tool(content: Vec<ContentPart>, call_id: String) -> Self {
		Self::new(AIRole::Tool, Some(content), Some(call_id), None)
	}
}

pub struct Data {
	pub db: PgPool,
	pub music_manager: Arc<Songbird>,
	pub channel_webhooks: WebhookMap,
	pub guilds: Cache<GuildId, Arc<GuildCache>>,
	pub app_emojis: Cache<EmojiId, Arc<Emoji>>,
	pub state_tracker: AtomicBool,
	pub lavalink_client: LavalinkClient,
	pub track_signals: DashMap<u64, Sender<TrackSignal>>,
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
	#[expect(clippy::expect_used)]
	UTILS_CONFIG.get().expect("UTILS_CONFIG not initialized!")
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
	#[expect(clippy::expect_used)]
	CLIENT_DATA.get().expect("CLIENT_DATA not initialized!")
}

pub static BOT_CONTEXT: OnceLock<Context> = OnceLock::new();

pub fn bot_context() -> &'static Context {
	#[expect(clippy::expect_used)]
	BOT_CONTEXT.get().expect("BOT_CONTEXT not initialized!")
}
