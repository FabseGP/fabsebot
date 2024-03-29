use crate::commands::{animanga, api_calls, funny, games, info, misc, music};
use crate::handlers::event_handler;
use crate::types::Data;

use poise::serenity_prelude as serenity;
use serenity::{client::Client, prelude::GatewayIntents};
use songbird::SerenityInit;
use std::env;

pub async fn start() {
    dotenvy::dotenv().unwrap();
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            event_handler: |_ctx, event, _framework, _data| {
                Box::pin(event_handler(_ctx, event, _framework, _data))
            },
            commands: vec![
                animanga::anime_scene(),
                api_calls::anilist_anime(),
                api_calls::bored(),
                api_calls::eightball(),
                api_calls::gif(),
                api_calls::imgbb(),
                api_calls::imgur(),
                api_calls::joke(),
                api_calls::memegen(),
                api_calls::picsur(),
                api_calls::translate(),
                api_calls::urban(),
                funny::anonymous(),
                funny::bot_dm(),
                funny::user_dm(),
                funny::user_misuse(),
                games::rps(),
                info::user_info(),
                info::server_info(),
                misc::birthday(),
                misc::fabseman(),
                misc::help(),
                misc::quote(),
                misc::sensei_status(),
                misc::troll(),
                music::add_queue(),
                music::join_voice(),
                music::leave_voice(),
                music::play_song(),
                music::skip_song(),
                music::stop_song(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("!".into()),
                ..Default::default()
            },
            on_error: |error| {
                Box::pin(async move {
                    match error {
                        poise::FrameworkError::ArgumentParse { error, .. } => {
                            if let Some(error) = error.downcast_ref::<serenity::RoleParseError>() {
                                println!("Found a RoleParseError: {:?}", error);
                            } else {
                                println!("Not a RoleParseError :(");
                            }
                        }
                        other => poise::builtins::on_error(other).await.unwrap(),
                    }
                })
            },
            ..Default::default()
        })
        .setup(move |_ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(_ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        })
        .build();
    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename("database.sqlite")
                .create_if_missing(true),
        )
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&database).await.unwrap();
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::DIRECT_MESSAGE_REACTIONS
        | GatewayIntents::DIRECT_MESSAGE_TYPING
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_EMOJIS_AND_STICKERS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::GUILD_MESSAGE_TYPING
        | GatewayIntents::GUILD_PRESENCES
        | GatewayIntents::GUILD_SCHEDULED_EVENTS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT;
    let token = env::var("DISCORD_TOKEN").unwrap();
    let client = Client::builder(&token, intents)
        .framework(framework)
        .register_songbird()
        .await;
    client.unwrap().start().await.unwrap();
}
