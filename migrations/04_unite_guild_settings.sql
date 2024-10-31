ALTER TABLE guild_settings
ADD word_tracked TEXT NULL DEFAULT NULL,
ADD word_count INT NOT NULL DEFAULT 0;

DROP TABLE words_count;
