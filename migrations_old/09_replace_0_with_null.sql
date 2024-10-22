UPDATE guild_settings
SET 
    dead_chat_channel = NULLIF(dead_chat_channel, 0),
    dead_chat_rate = NULLIF(dead_chat_rate, 0),
    quotes_channel = NULLIF(quotes_channel, 0),
    spoiler_channel = NULLIF(spoiler_channel, 0),
    prefix = NULLIF(prefix, '0');
