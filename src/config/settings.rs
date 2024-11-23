use serde::Deserialize;

#[derive(Deserialize)]
pub struct MainConfig {
    pub token: String,
    pub jaeger: String,
    pub cache_max_messages: usize,
    pub username: String,
    pub avatar: String,
    pub banner: String,
    pub activity: String,
}

#[derive(Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub user: String,
    pub database: String,
    pub password: String,
    pub max_connections: u32,
}

#[derive(Deserialize)]
pub struct AIConfig {
    pub token: String,
    pub base: String,
    pub translate: String,
    pub image_desc: String,
    pub image_gen: String,
    pub summarize: String,
    pub text_gen: String,
    pub tts: String,
}

#[derive(Deserialize)]
pub struct APIConfig {
    pub tenor_token: String,
}

#[derive(Deserialize, Default)]
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

#[derive(Deserialize, Default)]
pub struct UserSettings {
    pub guild_id: i64,
    pub user_id: i64,
    pub message_count: i32,
    pub chatbot_role: Option<String>,
    pub afk: bool,
    pub afk_reason: Option<String>,
    pub pinged_links: Option<String>,
    pub ping_content: Option<String>,
    pub ping_media: Option<String>,
}

#[derive(Deserialize)]
pub struct WordReactions {
    pub guild_id: i64,
    pub word: String,
    pub content: String,
    pub media: Option<String>,
}

#[derive(Deserialize)]
pub struct WordTracking {
    pub guild_id: i64,
    pub word: String,
    pub count: i64,
}

#[derive(Deserialize)]
pub struct EmojiReactions {
    pub guild_id: i64,
    pub emoji_id: i64,
    pub content_reaction: String,
    pub guild_emoji: bool,
}
