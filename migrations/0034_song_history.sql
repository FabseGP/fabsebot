CREATE TABLE channels (
    guild_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    PRIMARY KEY (guild_id, channel_id),
    FOREIGN KEY (guild_id) REFERENCES guilds(guild_id) ON DELETE CASCADE
);

CREATE TABLE tracks (
    track_uuid UUID PRIMARY KEY,
    title TEXT NULL DEFAULT NULL,
    artist TEXT NULL DEFAULT NULL,
    source_url TEXT UNIQUE NULL DEFAULT NULL,
    duration_sec BIGINT NULL DEFAULT NULL,
    thumbnail_url TEXT NULL DEFAULT NULL,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE song_plays (
    play_id BIGSERIAL PRIMARY KEY,
    track_uuid UUID NOT NULL REFERENCES tracks(track_uuid) ON DELETE RESTRICT,
    guild_id BIGINT NOT NULL REFERENCES guilds(guild_id) ON DELETE CASCADE,
    requested_by BIGINT NULL REFERENCES users(user_id) ON DELETE SET NULL,
    requested_channel BIGINT NOT NULL,
    request_message_id BIGINT NOT NULL,
    played_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_song_plays_channel 
        FOREIGN KEY (guild_id, requested_channel) 
        REFERENCES channels(guild_id, channel_id) 
        ON DELETE RESTRICT
);

CREATE INDEX idx_song_plays_guild_recent ON song_plays(guild_id, played_at);
CREATE INDEX idx_song_plays_track        ON song_plays(track_uuid);
CREATE INDEX idx_song_plays_user ON song_plays(requested_by);

