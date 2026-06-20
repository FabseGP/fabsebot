use serde::Deserialize;

#[derive(Deserialize)]
pub struct BotConfig {
	pub token: String,
	pub bot_id: u64,
	pub cache_max_messages: usize,
	pub username: String,
	pub activity: String,
	pub ping_message: String,
	pub ping_payload: String,
	pub uptime_url: String,
	pub uptime_token: String,
	pub owner_id: u64,
	pub error_webhook: String,
	pub feedback_webhook: String,
	pub lavalink_host: String,
	pub lavalink_password: String,
}

#[derive(Deserialize)]
pub struct ServerConfig {
	pub translate: String,
	pub search: String,
	pub llm_host_text: String,
	pub llm_host_tts: String,
	pub llm_host_stt: String,
	pub text_gen_model: String,
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
pub struct LogConfig {
	pub log_level: String,
	pub host: String,
	pub env: String,
}

#[derive(Deserialize)]
pub struct HTTPAgent {
	pub title: String,
	pub repo: String,
	pub email: String,
}
