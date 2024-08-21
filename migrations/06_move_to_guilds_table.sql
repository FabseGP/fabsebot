ALTER TABLE words_count
DROP PRIMARY KEY;

ALTER TABLE words_count
ADD CONSTRAINT fk_words_count_guild_id FOREIGN KEY (guild_id) REFERENCES guilds(guild_id)
ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE words_count
ADD PRIMARY KEY (word, guild_id);

CREATE INDEX idx_words_count_guild_id ON words_count(guild_id);
