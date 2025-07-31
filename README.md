# Fabsebot

Multipurpose Discord bot written in Rust using Poise, Serenity and Songbird

## Features

- Music playback using YouTube with support for lyrics, playlists & request-mode, where the bot queues whatever song you write in a channel
- TTS using Kokoro running locally
- Chatbot using Gemma3 running locally
- Quoting user messages
- Global chat across servers like Yggdrasil-bot

And many more utilities including looking up animanga, Urbandictionary, play RPS etc.

## Installation

Either invite the bot with [this invite](https://discord.com/oauth2/authorize?client_id=1146382254927523861) or run it yourself:

```bash
git clone https://codeberg.org/fabseman/fabsebot.git
cd fabsebot
cp config-example.toml config.toml # configure to your needs
cargo run -r # requires the nightly toolchain for rust
```

## Contributing
Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## License
[AGPL-3.0](https://choosealicense.com/licenses/agpl-3.0/)

