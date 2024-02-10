use crate::commands::*;
use crate::handlers::event_handler;
use crate::types::{BotStorage, Data};

use poise::serenity_prelude as serenity;
use serenity::client::Client;
use songbird::SerenityInit;
use std::env;

pub async fn start() {
    dotenvy::dotenv().expect("Failed to load .env file");
    let options = poise::FrameworkOptions {
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
            //            music::add_queue(),
            //            music::join_voice(),
            //            music::leave_voice(),
            //            music::play_song(),
            //            music::skip_song(),
            //            music::stop_song(),
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
    };
    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename("database.sqlite")
                .create_if_missing(true),
        )
        .await
        .expect("Couldn't connect to database");
    sqlx::migrate!("./migrations")
        .run(&database)
        .await
        .expect("Couldn't run database migrations");
    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::GUILD_MESSAGE_REACTIONS
        | serenity::GatewayIntents::GUILD_MESSAGE_TYPING
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGE_REACTIONS
        | serenity::GatewayIntents::DIRECT_MESSAGE_TYPING
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_VOICE_STATES;
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let _ = Client::builder(&token, intents) /*.register_songbird()*/
        .await;
    let framework = poise::Framework::builder()
        //.client_settings(|c| c.register_songbird())
        .token(token)
        .intents(intents)
        .options(options)
        .setup(move |_ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(_ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        });
    framework.run().await.unwrap();
}
