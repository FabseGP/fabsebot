CREATE TABLE IF NOT EXISTS users (
    user_id BIGINT NOT NULL PRIMARY KEY
);

INSERT INTO users (user_id)
SELECT DISTINCT user_id FROM user_settings
ON CONFLICT (user_id) DO NOTHING;

DELETE FROM user_settings 
WHERE user_id NOT IN (SELECT user_id FROM users);

ALTER TABLE user_settings
ADD CONSTRAINT fk_user_settings_user_id 
FOREIGN KEY (user_id) 
REFERENCES users(user_id) 
ON DELETE CASCADE;

CREATE INDEX idx_user_settings_user_id ON user_settings(user_id);
