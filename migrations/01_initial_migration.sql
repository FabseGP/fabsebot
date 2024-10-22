CREATE TABLE guilds (
    guild_id BIGINT NOT NULL PRIMARY KEY
);

CREATE TABLE guild_settings (
    guild_id BIGINT NOT NULL,
    dead_chat_rate BIGINT NULL DEFAULT NULL,
    dead_chat_channel BIGINT NULL DEFAULT NULL,
    quotes_channel BIGINT NULL DEFAULT NULL,
    spoiler_channel BIGINT NULL DEFAULT NULL,
    prefix TEXT NULL DEFAULT NULL,
    PRIMARY KEY (guild_id),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id) ON DELETE CASCADE
);

CREATE INDEX idx_guild_settings_guild_id ON guild_settings(guild_id);

CREATE TABLE user_settings (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    message_count INT NOT NULL DEFAULT 0,
    chatbot_role TEXT NULL DEFAULT NULL,
    afk BOOLEAN DEFAULT FALSE,
    afk_reason TEXT NULL DEFAULT NULL,
    pinged_links TEXT NULL DEFAULT NULL,
    ping_content TEXT NULL DEFAULT NULL,
    ping_media TEXT NULL DEFAULT NULL,
    PRIMARY KEY (guild_id, user_id),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id) ON DELETE CASCADE
);

CREATE INDEX idx_user_settings_guild_id ON user_settings(guild_id);

CREATE TABLE words_count (
    word TEXT NOT NULL,
    count INT NOT NULL DEFAULT 0,
    guild_id BIGINT NOT NULL,
    PRIMARY KEY (guild_id),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id) ON DELETE CASCADE
);

CREATE INDEX idx_words_count_guild_id ON words_count(guild_id);

