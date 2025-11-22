use serde::Deserialize;

#[derive(Deserialize)]
pub struct BotConfig {
	pub token: String,
	pub cache_max_messages: usize,
	pub username: String,
	pub avatar: String,
	pub banner: String,
	pub activity: String,
	pub ping_message: String,
	pub ping_payload: String,
	pub uptime_url: String,
	pub uptime_token: String,
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
	pub tenor_token: String,
	pub cloudflare_token: String,
	pub cloudflare_token_fallback: String,
	pub cloudflare_image_gen: String,
	pub cloudflare_image_gen_fallback: String,
}

#[derive(Default, Clone)]
pub struct UserSettings {
	pub guild_id: i64,
	pub user_id: i64,
	pub message_count: i32,
	pub chatbot_role: Option<String>,
	pub chatbot_internet_search: Option<bool>,
	pub chatbot_temperature: Option<f32>,
	pub chatbot_top_p: Option<f32>,
	pub chatbot_top_k: Option<i32>,
	pub chatbot_repetition_penalty: Option<f32>,
	pub chatbot_frequency_penalty: Option<f32>,
	pub chatbot_presence_penalty: Option<f32>,
	pub afk: bool,
	pub afk_reason: Option<String>,
	pub pinged_links: Option<String>,
	pub ping_content: Option<String>,
	pub ping_media: Option<String>,
}
