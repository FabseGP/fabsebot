ALTER TABLE user_settings
DROP COLUMN chatbot_role;

ALTER TABLE guild_settings
ADD COLUMN chatbot_role TEXT NULL DEFAULT NULL;
