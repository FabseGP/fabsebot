ALTER TABLE guild_word_reaction 
DROP CONSTRAINT IF EXISTS fk_word_reaction_emoji_id;

ALTER TABLE guild_word_reaction 
ALTER COLUMN emoji_id DROP NOT NULL;

ALTER TABLE guild_word_reaction 
ALTER COLUMN emoji_id SET DEFAULT NULL,
ALTER COLUMN guild_emoji SET DEFAULT NULL;

ALTER TABLE guild_word_reaction
ADD CONSTRAINT fk_word_reaction_emoji_id
FOREIGN KEY (emoji_id)
REFERENCES emojis(emoji_id)
ON DELETE CASCADE;
