ALTER TABLE guild_settings
ADD global_chat BOOLEAN DEFAULT FALSE,
DROP COLUMN global_call;
