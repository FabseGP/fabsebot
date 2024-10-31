ALTER TABLE guild_settings
ADD global_music BOOLEAN DEFAULT FALSE,
DROP COLUMN bot_call,
DROP COLUMN global_bot_call;
