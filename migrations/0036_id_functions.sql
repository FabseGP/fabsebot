CREATE OR REPLACE FUNCTION ensure_guild(p_guild_id BIGINT) RETURNS VOID AS $$
    INSERT INTO guilds (guild_id)
    VALUES (p_guild_id)
    ON CONFLICT (guild_id) DO NOTHING;
$$ LANGUAGE sql;

CREATE OR REPLACE FUNCTION ensure_user(p_user_id BIGINT) RETURNS VOID AS $$
    INSERT INTO users (user_id)
    VALUES (p_user_id)
    ON CONFLICT (user_id) DO NOTHING;
$$ LANGUAGE sql;
