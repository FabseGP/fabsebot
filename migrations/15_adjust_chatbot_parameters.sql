ALTER TABLE user_settings
ADD chatbot_temperature numeric(3,2) DEFAULT NULL,
ADD chatbot_top_p numeric(3,2) DEFAULT NULL,
ADD chatbot_top_k numeric(3,2) DEFAULT NULL,
ADD chatbot_repetition_penalty numeric(3,2) DEFAULT NULL,
ADD chatbot_frequency_penalty numeric(3,2) DEFAULT NULL,
ADD chatbot_presence_penalty numeric(3,2) DEFAULT NULL;
