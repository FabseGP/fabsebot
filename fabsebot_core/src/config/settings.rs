use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
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
}

#[derive(Deserialize, Debug)]
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

#[derive(Deserialize, Debug)]
pub struct APIConfig {
	pub tenor_token: String,
	pub cloudflare_token: String,
	pub cloudflare_token_fallback: String,
	pub cloudflare_image_gen: String,
	pub cloudflare_image_gen_fallback: String,
}

#[derive(Default, Clone)]
pub struct GuildSettings {
	pub guild_id: i64,
	pub dead_chat_rate: Option<i64>,
	pub dead_chat_channel: Option<i64>,
	pub quotes_channel: Option<i64>,
	pub spoiler_channel: Option<i64>,
	pub prefix: Option<String>,
	pub ai_chat_channel: Option<i64>,
	pub global_chat_channel: Option<i64>,
	pub global_chat: bool,
	pub global_music: bool,
	pub global_call: bool,
	pub music_channel: Option<i64>,
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

#[derive(Default, Eq, Hash, PartialEq, Clone)]
pub struct WordReactions {
	pub guild_id: i64,
	pub word: String,
	pub content: String,
	pub media: Option<String>,
}

#[derive(Default, Eq, Hash, PartialEq, Clone)]
pub struct WordTracking {
	pub guild_id: i64,
	pub word: String,
	pub count: i64,
}

#[derive(Default, Eq, Hash, PartialEq, Clone)]
pub struct EmojiReactions {
	pub guild_id: i64,
	pub emoji_id: i64,
	pub content_reaction: String,
	pub guild_emoji: bool,
}
