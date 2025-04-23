use crate::{
    commands::{api_calls, funny, games, info, misc, music, settings},
    config::{
        settings::{APIConfig, FabseserverConfig, MainConfig, PostgresConfig},
        types::{Data, UTILS_CONFIG, UtilsConfig},
    },
    core::handlers::{EventHandler, dynamic_prefix, on_error},
};
use anyhow::Context;
use mini_moka::sync::Cache;
use poise::{
    Framework, FrameworkOptions, Prefix, PrefixFrameworkOptions,
    serenity_prelude::{
        ActivityData, Client, CreateAllowedMentions, CreateAttachment, EditProfile, GatewayIntents,
        OnlineStatus::Online, Token, cache::Settings,
    },
};
use songbird::{Config, Songbird, driver::DecodeMode::Decode};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{str::FromStr, sync::Arc};
use tokio::{
    select,
    signal::unix::{SignalKind, signal},
    spawn,
    sync::Mutex,
};
use tracing::{error, warn};

async fn wait_until_shutdown() {
    let [mut s1, mut s2, mut s3] = [
        signal(SignalKind::hangup()).unwrap(),
        signal(SignalKind::interrupt()).unwrap(),
        signal(SignalKind::terminate()).unwrap(),
    ];

    select!(
        v = s1.recv() => v.unwrap(),
        v = s2.recv() => v.unwrap(),
        v = s3.recv() => v.unwrap(),
    );
}

pub async fn bot_start(
    bot_config: MainConfig,
    postgres_config: PostgresConfig,
    fabseserver_config: FabseserverConfig,
    api_config: APIConfig,
) -> anyhow::Result<()> {
    if UTILS_CONFIG
        .set(Arc::new(UtilsConfig {
            bot: bot_config.clone(),
            fabseserver: fabseserver_config,
            api: api_config,
        }))
        .is_err()
    {
        error!("Failed to set utils config");
        panic!();
    }
    let pool_options = PgConnectOptions::new()
        .host(&postgres_config.host)
        .port(postgres_config.port)
        .username(&postgres_config.user)
        .database(&postgres_config.database)
        .password(&postgres_config.password);
    let database = PgPoolOptions::default()
        .max_connections(postgres_config.max_connections)
        .connect_with(pool_options)
        .await
        .context("Failed to connect to database")?;
    let music_manager = Songbird::serenity();
    music_manager.set_config(Config::default().use_softclip(false).decode_mode(Decode));
    let user_data = Arc::new(Data {
        db: database,
        music_manager: Arc::<Songbird>::clone(&music_manager),
        ai_chats: Arc::new(Cache::new(1000)),
        global_chats: Arc::new(Cache::new(1000)),
        channel_webhooks: Arc::new(Cache::new(1000)),
        guild_data: Arc::new(Mutex::new(Cache::new(1000))),
        user_settings: Arc::new(Mutex::new(Cache::new(1000))),
    });
    let additional_prefix: &'static str =
        Box::leak(format!("hey {}", &bot_config.username).into_boxed_str());
    let framework = Framework::builder()
        .options(FrameworkOptions {
            commands: vec![
                api_calls::anime_scene(),
                api_calls::ai_image(),
                api_calls::ai_text(),
                api_calls::anime(),
                api_calls::eightball(),
                api_calls::gif(),
                api_calls::joke(),
                api_calls::manga(),
                api_calls::memegen(),
                api_calls::roast(),
                api_calls::translate(),
                api_calls::urban(),
                api_calls::waifu(),
                api_calls::wiki(),
                funny::anonymous(),
                funny::user_dm(),
                funny::user_misuse(),
                games::rps(),
                info::user_info(),
                info::server_info(),
                misc::anony_poll(),
                misc::birthday(),
                misc::debug(),
                misc::end_pgo(),
                misc::global_chat_end(),
                misc::global_chat_start(),
                misc::help(),
                misc::leaderboard(),
                misc::ohitsyou(),
                misc::quote(),
                misc::register_commands(),
                misc::slow_mode(),
                misc::word_count(),
                music::text_to_voice(),
                music::add_playlist(),
                music::global_music_end(),
                music::global_music_start(),
                music::join_voice(),
                music::join_voice_global(),
                music::leave_voice(),
                music::pause_continue_song(),
                music::play_song(),
                music::seek_song(),
                music::skip_song(),
                music::stop_song(),
                settings::reset_server_settings(),
                settings::reset_user_settings(),
                settings::set_afk(),
                settings::set_chatbot_channel(),
                settings::set_chatbot_options(),
                settings::set_dead_chat(),
                settings::set_emoji_react(),
                settings::set_prefix(),
                settings::set_quote_channel(),
                settings::set_spoiler_channel(),
                settings::set_user_ping(),
                settings::set_word_react(),
                settings::set_word_track(),
            ],
            prefix_options: PrefixFrameworkOptions {
                dynamic_prefix: Some(|ctx| Box::pin(dynamic_prefix(ctx))),
                additional_prefixes: vec![Prefix::Literal(additional_prefix)],
                ..Default::default()
            },
            allowed_mentions: Some(CreateAllowedMentions::default().replied_user(false)),
            on_error: |error| {
                Box::pin(async move {
                    on_error(error)
                        .await
                        .unwrap_or_else(|err| error!("on_error: {:?}", err));
                })
            },
            ..Default::default()
        })
        .build();
    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT;
    let mut cache_settings = Settings::default();
    cache_settings.max_messages = bot_config.cache_max_messages;
    let activity = ActivityData::listening(&bot_config.activity);
    let client = Client::builder(Token::from_str(&bot_config.token)?, intents)
        .framework(framework)
        .voice_manager::<Songbird>(music_manager)
        .cache_settings(cache_settings)
        .event_handler(EventHandler)
        .activity(activity)
        .status(Online)
        .data(user_data)
        .await;
    match client {
        Ok(mut client) => {
            spawn(async move {
                wait_until_shutdown().await;
                warn!("Recieved control C and shutting down...");
                // IMPLEMENT SHUTTING DOWN ALL SHARDS
            });
            if let Err(e) = client.start_autosharded().await {
                warn!("Client error: {:?}", e);
            }
            client
                .http
                .edit_profile(
                    &EditProfile::default()
                        .avatar(
                            CreateAttachment::url(
                                &client.http,
                                &bot_config.avatar,
                                "bot_avatar.gif",
                            )
                            .await?
                            .encode()
                            .await?,
                        )
                        .banner(
                            CreateAttachment::url(
                                &client.http,
                                &bot_config.banner,
                                "bot_banner.gif",
                            )
                            .await?
                            .encode()
                            .await?,
                        )
                        .username(&bot_config.username),
                )
                .await
                .context("Failed to edit bot profile")?;
        }
        Err(e) => {
            warn!("Error creating client: {:?}", e);
        }
    }
    Ok(())
}
