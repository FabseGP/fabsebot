CREATE TABLE guild_word_reaction (
    guild_id BIGINT NOT NULL,
    word TEXT NOT NULL,
    content TEXT NOT NULL,
    media TEXT NULL DEFAULT NULL,
    PRIMARY KEY (guild_id, word),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id) ON DELETE CASCADE
);

CREATE INDEX idx_guild_reactions ON guild_word_reaction(guild_id, word);
