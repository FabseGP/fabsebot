ALTER TABLE words_count
DROP PRIMARY KEY;

ALTER TABLE words_count
ADD PRIMARY KEY (guild_id);
