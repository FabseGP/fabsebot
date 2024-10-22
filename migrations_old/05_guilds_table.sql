CREATE TABLE guilds (
    guild_id BIGINT UNSIGNED NOT NULL PRIMARY KEY
);

INSERT INTO guilds (guild_id)
SELECT DISTINCT guild_id FROM message_count;

ALTER TABLE message_count
DROP PRIMARY KEY;

ALTER TABLE message_count
ADD CONSTRAINT fk_guild_id FOREIGN KEY (guild_id) REFERENCES guilds(guild_id)
ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE message_count
ADD PRIMARY KEY (guild_id, user_name);

CREATE INDEX idx_guild_id ON message_count(guild_id);
