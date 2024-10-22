CREATE TABLE user_settings (
    guild_id BIGINT UNSIGNED NOT NULL,
    user_id BIGINT UNSIGNED NOT NULL,
    message_count BIGINT UNSIGNED NOT NULL,
    chatbot_role TEXT NULL DEFAULT NULL,
    PRIMARY KEY (guild_id, user_id),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id)
);

CREATE INDEX idx_user_settings_guild_id ON user_settings(guild_id);

DROP TABLE message_count;

ALTER TABLE guild_settings
DROP COLUMN chatbot_role;
