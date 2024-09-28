use crate::{
    commands::{animanga, api_calls, funny, games, info, misc, music, settings},
    events::{
        bot_ready::handle_ready, guild_create::handle_guild_create,
        message_delete::handle_message_delete, message_sent::handle_message,
        reaction_add::handle_reaction_add,
    },
    types::{ClientData, Data, Error, CLIENT_DATA},
};
use anyhow::Context;
use fastrand::Rng;
use poise::{
    builtins,
    serenity_prelude::{cache::Settings, Client, FullEvent, GatewayIntents},
    EditTracker, Framework, FrameworkContext, FrameworkError, FrameworkOptions, PartialContext,
    PrefixFrameworkOptions, Prefix,
};
use reqwest::Client as http_client;
use songbird::Songbird;
use sqlx::query;
use std::{borrow::Cow, collections::HashMap, env, sync::Arc, time::Duration};
use tokio::sync::Mutex;

async fn on_error(error: FrameworkError<'_, Data, Error>) {
    match error {
        FrameworkError::Command { error, ctx, .. } => {
            tracing::warn!("Error in command `{}`: {:?}", ctx.command().name, error);
        }
        FrameworkError::UnknownCommand { .. } => {}
        error => {
            if let Err(e) = builtins::on_error(error).await {
                tracing::warn!("Error while handling error: {:?}", e);
            }
        }
    }
}

async fn dynamic_prefix(
    ctx: PartialContext<'_, Data, Error>,
) -> anyhow::Result<Option<Cow<'static, str>>> {
    let prefix = match ctx.guild_id {
        Some(id) => {
            let mut conn = ctx
                .framework
                .user_data()
                .db
                .acquire()
                .await
                .context("Failed to acquire database connection")?;
            if let Some(record) = query!(
                "SELECT prefix FROM guild_settings WHERE guild_id = ?",
                id.get()
            )
            .fetch_optional(&mut *conn)
            .await
            .context("Failed to fetch prefix from database")?
            {
                if let Some(prefix) = record.prefix {
                    prefix
                } else {
                    "!".to_owned()
                }
            } else {
                "!".to_owned()
            }
        }
        _ => "!".to_owned(),
    };

    Ok(Some(Cow::Owned(prefix)))
}

async fn event_handler(
    framework: FrameworkContext<'_, Data, Error>,
    event: &FullEvent,
) -> Result<(), Error> {
    let data = framework.user_data();
    let ctx = framework.serenity_context;

    match event {
        FullEvent::Ready { data_about_bot } => handle_ready(ctx, data_about_bot, framework).await?,
        FullEvent::Message { new_message } => handle_message(ctx, data, new_message).await?,
        FullEvent::ReactionAdd { add_reaction } => handle_reaction_add(ctx, add_reaction).await?,
        FullEvent::GuildCreate { guild, is_new } => {
            handle_guild_create(data, guild, is_new).await?
        }
        FullEvent::MessageDelete {
            channel_id,
            guild_id,
            deleted_message_id,
        } => handle_message_delete(ctx, *channel_id, *guild_id, *deleted_message_id).await?,
        _ => {}
    }

    Ok(())
}

pub async fn start() -> anyhow::Result<()> {
    dotenvy::dotenv().context("Failed to load .env file")?;
    let sql_user = env::var("MARIADB_USER").context("MARIADB_USER not set in environment")?;
    let sql_password =
        env::var("MARIADB_PASSWORD").context("MARIADB_PASSWORD not set in environment")?;
    let sql_database =
        env::var("MARIADB_DATABASE").context("MARIADB_DATABASE not set in environment")?;
    let database = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&format!(
            "mariadb://{}:{}@localhost/{}",
            sql_user, sql_password, sql_database
        ))
        .await
        .context("Failed to connect to database")?;
    sqlx::migrate!("./migrations")
        .run(&database)
        .await
        .context("Failed to run database migrations")?;
    let manager = Songbird::serenity();
    let user_data = Data {
        db: database,
        req_client: http_client::new(),
        music_manager: Arc::clone(&manager),
        conversations: Arc::new(Mutex::new(HashMap::new())),
        rng_thread: Arc::new(Mutex::new(Rng::new())),
    };
    let framework = Framework::builder()
        .options(FrameworkOptions {
            event_handler: |framework, event| Box::pin(event_handler(framework, event)),
            commands: vec![
                animanga::anime_scene(),
                api_calls::ai_anime(),
                api_calls::ai_image(),
                api_calls::ai_summarize(),
                api_calls::ai_text(),
                api_calls::anilist_anime(),
                api_calls::eightball(),
                api_calls::gif(),
                api_calls::github_search(),
                api_calls::joke(),
                api_calls::memegen(),
                api_calls::roast(),
                api_calls::translate(),
                api_calls::urban(),
                api_calls::waifu(),
                funny::anonymous(),
                // funny::user_dm(),
                funny::user_misuse(),
                games::rps(),
                info::user_info(),
                info::server_info(),
                misc::anony_poll(),
                misc::birthday(),
                misc::end_pgo(),
                misc::help(),
                misc::leaderboard(),
                misc::ohitsyou(),
                misc::quote(),
                misc::slow_mode(),
                misc::troll(),
                misc::word_count(),
                music::add_playlist(),
                music::join_voice(),
                music::leave_voice(),
                music::pause_continue_song(),
                music::play_song(),
                music::seek_song_backward(),
                music::seek_song_forward(),
                music::skip_song(),
                music::stop_song(),
                settings::reset_settings(),
                settings::set_afk(),
                settings::set_chatbot_channel(),
                settings::set_chatbot_role(),
                settings::set_dead_chat(),
                settings::set_prefix(),
                settings::set_quote_channel(),
                settings::set_spoiler_channel(),
                settings::set_user_ping(),
                settings::set_word_track(),
            ],
            prefix_options: PrefixFrameworkOptions {
                dynamic_prefix: Some(|ctx| Box::pin(dynamic_prefix(ctx))),
                edit_tracker: Some(Arc::new(EditTracker::for_timespan(Duration::from_secs(
                    3600,
                )))),
                additional_prefixes: vec![
                    Prefix::Literal("fabsebot"),
                    Prefix::Literal("hey fabsebot"),
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
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT;
    let token = env::var("DISCORD_TOKEN").context("DISCORD_TOKEN not set in environment")?;
    let mut cache_settings = Settings::default();
    cache_settings.max_messages = 10;
    let client = Client::builder(&token, intents)
        .framework(framework)
        .voice_manager::<Songbird>(manager)
        .cache_settings(cache_settings)
        .data(Arc::new(user_data) as _)
        .await;
    match client {
        Ok(mut client) => {
            if let Err(e) = client.start().await {
                tracing::warn!("Client error: {:?}", e);
            }
            let client_data = Arc::new(ClientData {
                shard_manager: client.shard_manager.clone(),
            });
            if CLIENT_DATA.set(client_data).is_err() {
                tracing::error!("Failed to set CLIENT_DATA");
            }
        }
        Err(e) => {
            tracing::warn!("Error creating client: {:?}", e);
        }
    }
    Ok(())
}
