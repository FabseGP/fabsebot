CREATE TABLE guild_settings (
    guild_id BIGINT UNSIGNED NOT NULL,
    dead_chat_rate BIGINT UNSIGNED NOT NULL,
    dead_chat_channel INTEGER NOT NULL,
    PRIMARY KEY (guild_id)
);

CREATE TABLE message_count (
    guild_id BIGINT UNSIGNED NOT NULL,
    user_name CHAR(200) NOT NULL UNIQUE,
    messages BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (user_name)
)
