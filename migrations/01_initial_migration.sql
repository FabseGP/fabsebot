CREATE TABLE guild_settings (
    guild_id BIGINT UNSIGNED NOT NULL,
    dead_chat_rate BIGINT UNSIGNED NOT NULL,
    dead_chat_channel INTEGER NOT NULL,
    PRIMARY KEY (guild_id)
);

