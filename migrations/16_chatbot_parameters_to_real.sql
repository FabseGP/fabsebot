ALTER TABLE user_settings
ALTER COLUMN chatbot_temperature TYPE real,
ALTER COLUMN chatbot_top_p TYPE real,
ALTER COLUMN chatbot_top_k TYPE real,
ALTER COLUMN chatbot_repetition_penalty TYPE real,
ALTER COLUMN chatbot_frequency_penalty TYPE real,
ALTER COLUMN chatbot_presence_penalty TYPE real;
