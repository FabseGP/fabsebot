ALTER TABLE guild_settings
DROP PRIMARY KEY;

ALTER TABLE guild_settings
ADD CONSTRAINT fk_guild_settings_guild_id FOREIGN KEY (guild_id) REFERENCES guilds(guild_id)
ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE guild_settings
ADD PRIMARY KEY (guild_id);

CREATE INDEX idx_guild_settings_guild_id ON guild_settings(guild_id);
