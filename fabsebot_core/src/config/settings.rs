use serde::Deserialize;

#[derive(Deserialize)]
pub struct BotConfig {
	pub token: String,
	pub cache_max_messages: usize,
	pub username: String,
	pub activity: String,
	pub ping_message: String,
	pub ping_payload: String,
	pub uptime_url: String,
	pub uptime_token: String,
	pub owner_id: u64,
}

#[derive(Deserialize)]
pub struct ServerConfig {
	pub translate: String,
	pub search: String,
	pub llm_host_text: String,
	pub llm_host_tts: String,
	pub llm_host_stt: String,
	pub text_gen_model: String,
	pub image_to_text_model: String,
	pub text_to_speech_model: String,
	pub speech_to_text_model: String,
}

#[derive(Deserialize)]
pub struct APIConfig {
	pub gif_token: String,
	pub cloudflare_token: String,
	pub cloudflare_image_gen: String,
}

#[derive(Deserialize)]
pub struct HTTPAgent {
	pub title: String,
	pub repo: String,
	pub email: String,
}

#[derive(Default, Clone)]
pub struct UserSettings {
	pub guild_id: i64,
	pub user_id: i64,
	pub message_count: i32,
	pub chatbot_role: Option<String>,
	pub chatbot_internet_search: bool,
	pub afk: bool,
	pub afk_reason: Option<String>,
	pub pinged_links: Option<String>,
	pub ping_content: Option<String>,
	pub ping_media: Option<String>,
}

#[derive(Default, Clone)]
pub struct UserSettingsInternal {
	pub message_count: i32,
	pub chatbot_role: Option<String>,
	pub chatbot_internet_search: bool,
	pub afk: bool,
	pub afk_reason: Option<String>,
	pub pinged_links: Option<String>,
	pub ping_content: Option<String>,
	pub ping_media: Option<String>,
}

impl From<UserSettings> for UserSettingsInternal {
	fn from(db: UserSettings) -> Self {
		Self {
			message_count: db.message_count,
			chatbot_role: db.chatbot_role,
			chatbot_internet_search: db.chatbot_internet_search,
			afk: db.afk,
			afk_reason: db.afk_reason,
			pinged_links: db.pinged_links,
			ping_content: db.ping_content,
			ping_media: db.ping_media,
		}
	}
}
