use crate::commands::{animanga, api_calls, funny, games, info, misc, music, settings};
use crate::handlers::event_handler;
use crate::types::{Context, Data, Error};

use poise::serenity_prelude as serenity;
use reqwest::Client as http_client;
use serenity::{cache::Settings, client::Client, prelude::GatewayIntents};
use sqlx::query;
use std::{borrow::Cow, env, sync::Arc, time::Duration};

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, ctx, .. } => {
            println!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                println!("Error while handling error: {}", e)
            }
        }
    }
}

#[poise::command(prefix_command, owners_only)]
async fn register_commands(ctx: Context<'_>) -> Result<(), Error> {
    let commands = &ctx.framework().options().commands;
    poise::builtins::register_globally(ctx.http(), commands).await?;
    ctx.say("Successfully registered slash commands!").await?;
    Ok(())
}

pub async fn dynamic_prefix(
    ctx: poise::PartialContext<'_, Data, Error>,
) -> Result<Option<Cow<'static, str>>, Error> {
    let prefix = {
        if let Ok(record) = query!(
            "SELECT prefix FROM guild_settings WHERE guild_id = ?",
            ctx.guild_id.unwrap().get()
        )
        .fetch_one(&mut *ctx.framework.user_data().db.acquire().await?)
        .await
        {
            if record.prefix.is_empty() {
                "!".to_string()
            } else {
                record.prefix
            }
        } else {
            "!".to_string()
        }
    };

    Ok(Some(Cow::Owned(prefix)))
}

pub async fn start() {
    dotenvy::dotenv().unwrap();
    let sql_user = env::var("MARIADB_USER").unwrap();
    let sql_password = env::var("MARIADB_PASSWORD").unwrap();
    let sql_database = env::var("MARIADB_DATABASE").unwrap();
    let database = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&format!(
            "mariadb://{username}:{password}@localhost/{database}",
            username = sql_user,
            password = sql_password,
            database = sql_database
        ))
        .await
        .expect("oof, couldn't connect to database");
    sqlx::migrate!("./migrations")
        .run(&database)
        .await
        .expect("oof, couldn't run database migrations");
    let manager = songbird::Songbird::serenity();
    let user_data = Data {
        db: database,
        req_client: http_client::new(),
        music_manager: Arc::clone(&manager),
    };
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            event_handler: |framework, event| Box::pin(event_handler(framework, event)),
            commands: vec![
                register_commands(),
                animanga::anime_scene(),
                api_calls::ai_anime(),
                api_calls::ai_image(),
                api_calls::ai_midjourney(),
                api_calls::ai_summarize(),
                api_calls::ai_text(),
                api_calls::anilist_anime(),
                api_calls::eightball(),
                api_calls::gif(),
                api_calls::joke(),
                api_calls::memegen(),
                api_calls::roast(),
                api_calls::translate(),
                api_calls::urban(),
                api_calls::waifu(),
                funny::anonymous(),
                //     funny::user_dm(),
                funny::user_misuse(),
                games::rps(),
                info::user_info(),
                info::server_info(),
                misc::birthday(),
                misc::end_pgo(),
                misc::help(),
                misc::leaderboard(),
                misc::quote(),
                misc::slow_mode(),
                misc::troll(),
                misc::pure_count(),
                music::add_playlist(),
                music::join_voice(),
                music::leave_voice(),
                music::play_song(),
                music::skip_song(),
                music::stop_song(),
                settings::dead_chat(),
                settings::prefix(),
                settings::quote_channel(),
                settings::reset_settings(),
                settings::spoiler_channel(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                dynamic_prefix: Some(|ctx| Box::pin(dynamic_prefix(ctx))),
                edit_tracker: Some(Arc::new(poise::EditTracker::for_timespan(
                    Duration::from_secs(3600),
                ))),
                additional_prefixes: vec![
                    poise::Prefix::Literal("fabsebot"),
                    poise::Prefix::Literal("hey fabsebot"),
                ],
                ..Default::default()
            },
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        })
        .build();
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
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT;
    let token = env::var("DISCORD_TOKEN").unwrap();
    let mut cache_settings = Settings::default();
    cache_settings.max_messages = 10;
    let client = Client::builder(&token, intents)
        .framework(framework)
        .voice_manager::<songbird::Songbird>(manager)
        .cache_settings(cache_settings)
        .data(Arc::new(user_data) as _)
        .await;
    client.unwrap().start().await.unwrap();
}
