ALTER TABLE guild_word_reaction
DROP COLUMN IF EXISTS guild_emoji;

ALTER TABLE guild_word_reaction
ADD COLUMN guild_emoji BOOLEAN NOT NULL DEFAULT FALSE;
