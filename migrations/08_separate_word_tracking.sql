ALTER TABLE guild_settings
DROP COLUMN word_tracked,
DROP COLUMN word_count;

CREATE TABLE guild_word_tracking (
    guild_id BIGINT NOT NULL,
    word TEXT NOT NULL,
    count BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, word),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id) ON DELETE CASCADE
);

CREATE INDEX idx_guild_words ON guild_word_tracking(guild_id, word);
