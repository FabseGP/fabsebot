CREATE TABLE guild_emoji_reaction (
    guild_id BIGINT NOT NULL,
    emoji_name TEXT NOT NULL,
    guild_emoji BOOLEAN NOT NULL DEFAULT FALSE,
    content_reaction TEXT NULL DEFAULT NULL,
    PRIMARY KEY (guild_id, emoji_name),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id) ON DELETE CASCADE
);

CREATE INDEX idx_guild_emoji_reactions ON guild_emoji_reaction(guild_id, emoji_name);
