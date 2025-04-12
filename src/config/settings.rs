use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct MainConfig {
    pub log_level: String,
    pub token: String,
    pub jaeger: String,
    pub cache_max_messages: usize,
    pub username: String,
    pub avatar: String,
    pub banner: String,
    pub activity: String,
    pub ping_message: String,
    pub uptime_url: String,
}

#[derive(Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub database: String,
    pub password: String,
    pub max_connections: u32,
}

#[derive(Deserialize)]
pub struct AIConfig {
    pub token: String,
    pub token_fallback: String,
    pub token_huggingface: String,
    pub translate: String,
    pub image_desc: String,
    pub image_desc_fallback: String,
    pub image_gen: String,
    pub image_gen_fallback: String,
    pub text_gen: String,
    pub text_gen_fallback: String,
    pub text_gen_local: String,
    pub text_gen_local_model: String,
    pub tts: String,
    pub tts_fallback: String,
    pub search: String,
}

#[derive(Deserialize)]
pub struct APIConfig {
    pub tenor_token: String,
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
}

#[derive(Default, Clone)]
pub struct UserSettings {
    pub guild_id: i64,
    pub user_id: i64,
    pub message_count: i32,
    pub chatbot_role: Option<String>,
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
