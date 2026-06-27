ALTER TABLE song_plays
    DROP CONSTRAINT song_plays_requested_by_fkey;

ALTER TABLE song_plays
    ALTER COLUMN requested_by SET NOT NULL;

ALTER TABLE song_plays
    ADD CONSTRAINT song_plays_requested_by_fkey
    FOREIGN KEY (requested_by) REFERENCES users(user_id)
    ON DELETE CASCADE;
