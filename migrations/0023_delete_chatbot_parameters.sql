ALTER TABLE user_settings
DROP COLUMN chatbot_temperature,
DROP COLUMN chatbot_top_p,
DROP COLUMN chatbot_top_k,
DROP COLUMN chatbot_repetition_penalty,
DROP COLUMN chatbot_frequency_penalty,
DROP COLUMN chatbot_presence_penalty;
