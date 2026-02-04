CREATE TABLE IF NOT EXISTS emojis (
    emoji_id BIGINT NOT NULL PRIMARY KEY
);

ALTER TABLE guild_word_reaction
    DROP COLUMN emoji_name,
    ADD COLUMN emoji_id BIGINT NOT NULL,
    ADD CONSTRAINT fk_word_reaction_emoji_id
    FOREIGN KEY (emoji_id)
    REFERENCES emojis(emoji_id)
    ON DELETE CASCADE;
