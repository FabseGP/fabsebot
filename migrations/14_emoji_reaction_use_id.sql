DROP INDEX idx_guild_emoji_reactions;

ALTER TABLE guild_emoji_reaction 
    DROP CONSTRAINT guild_emoji_reaction_pkey,
    DROP COLUMN emoji_name,
    ADD COLUMN emoji_id BIGINT NOT NULL,
    ADD PRIMARY KEY (guild_id, emoji_id);

CREATE INDEX idx_guild_emoji_reactions ON guild_emoji_reaction(guild_id, emoji_id);
