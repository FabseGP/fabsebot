UPDATE user_settings
SET chatbot_internet_search = FALSE
WHERE chatbot_internet_search IS NULL;

ALTER TABLE user_settings
ALTER COLUMN chatbot_internet_search SET NOT NULL;

ALTER TABLE user_settings
ALTER COLUMN chatbot_internet_search SET DEFAULT FALSE;
