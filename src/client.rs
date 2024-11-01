use crate::{
    commands::{animanga, api_calls, funny, games, info, misc, music, settings},
    events::{
        bot_ready::handle_ready, guild_create::handle_guild_create,
        http_ratelimit::handle_ratelimit, message_sent::handle_message,
    },
    types::{ClientData, Data, Error, CLIENT_DATA},
};
use anyhow::Context as _;
use core::time::Duration;
use dashmap::DashMap;
use poise::{
    builtins,
    serenity_prelude::{
        cache::Settings, Client, CreateAttachment, EditProfile, FullEvent, GatewayIntents,
        ShardManager,
    },
    EditTracker, Framework, FrameworkContext, FrameworkError, FrameworkOptions, PartialContext,
    Prefix, PrefixFrameworkOptions,
};
use songbird::{driver::DecodeMode::Decode, Config, Songbird};
use sqlx::{migrate, postgres::PgPoolOptions, query};
use std::{borrow::Cow, env, sync::Arc};
use tracing::{error, warn};

async fn on_error(error: FrameworkError<'_, Data, Error>) {
    match error {
        FrameworkError::Command { error, ctx, .. } => {
            warn!("Error in command `{}`: {:?}", ctx.command().name, error);
        }
        FrameworkError::UnknownCommand { .. } => {}
        error => {
            if let Err(e) = builtins::on_error(error).await {
                warn!("Error while handling error: {:?}", e);
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
                "SELECT prefix FROM guild_settings WHERE guild_id = $1",
                i64::from(id),
            )
            .fetch_optional(&mut *conn)
            .await
            .context("Failed to fetch prefix from database")?
            {
                record
                    .prefix
                    .map_or_else(|| "!".to_owned(), |prefix| prefix)
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
        FullEvent::GuildCreate { guild, is_new } => {
            handle_guild_create(data, guild, is_new.as_ref()).await?;
        }
        FullEvent::Ratelimit { data } => handle_ratelimit(data).await?,
        _ => {}
    }

    Ok(())
}

pub async fn start() -> anyhow::Result<()> {
    dotenvy::dotenv().context("Failed to load .env file")?;
    let database_url = env::var("DATABASE_URL").context("DATABASE_URL not set in environment")?;
    let max_db_conns: u32 = env::var("DATABASE_MAX_CONNS")
        .context("DATABASE_MAX_CONNS not set in environment")?
        .parse()
        .context("Failed to parse DATABASE_MAX_CONNS")?;
    let database = PgPoolOptions::new()
        .max_connections(max_db_conns)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;
    migrate!("./migrations")
        .run(&database)
        .await
        .context("Failed to run database migrations")?;
    let music_manager = Songbird::serenity();
    music_manager.set_config(Config::default().use_softclip(false).decode_mode(Decode));
    let user_data = Arc::new(Data {
        db: database,
        music_manager: Arc::<Songbird>::clone(&music_manager),
        ai_conversations: Arc::new(DashMap::default()),
        global_chat_last: Arc::new(DashMap::default()),
        webhook_cache: Arc::new(DashMap::default()),
    });
    let framework = Framework::builder()
        .options(FrameworkOptions {
            event_handler: |framework, event| Box::pin(event_handler(framework, event)),
            commands: vec![
                animanga::anime_scene(),
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
                api_calls::wiki(),
                funny::anonymous(),
                funny::user_dm(),
                funny::user_misuse(),
                games::rps(),
                info::user_info(),
                info::server_info(),
                misc::anony_poll(),
                misc::birthday(),
                misc::end_pgo(),
                misc::global_chat_end(),
                misc::global_chat_start(),
                misc::help(),
                misc::leaderboard(),
                misc::ohitsyou(),
                misc::quote(),
                misc::slow_mode(),
                misc::word_count(),
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
                settings::reset_settings(),
                settings::set_afk(),
                settings::set_chatbot_channel(),
                settings::set_chatbot_role(),
                settings::set_dead_chat(),
                settings::set_prefix(),
                settings::set_quote_channel(),
                settings::set_spoiler_channel(),
                settings::set_user_ping(),
                settings::set_word_react(),
                settings::set_word_track(),
            ],
            prefix_options: PrefixFrameworkOptions {
                dynamic_prefix: Some(|ctx| Box::pin(dynamic_prefix(ctx))),
                edit_tracker: Some(Arc::new(EditTracker::for_timespan(Duration::from_secs(60)))),
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
    let intents = GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT;
    let token = env::var("DISCORD_TOKEN").context("DISCORD_TOKEN not set in environment")?;
    let mut cache_settings = Settings::default();
    cache_settings.max_messages = 10;
    let client = Client::builder(&token, intents)
        .framework(framework)
        .voice_manager::<Songbird>(music_manager)
        .cache_settings(cache_settings)
        .data(user_data)
        .await;
    match client {
        Ok(mut client) => {
            if let Err(e) = client.start().await {
                warn!("Client error: {:?}", e);
            }
            let client_data = ClientData {
                shard_manager: Arc::<ShardManager>::clone(&client.shard_manager),
            };
            if CLIENT_DATA.set(client_data).is_err() {
                error!("Failed to set CLIENT_DATA");
            }
            let bot_username =
                env::var("BOT_USERNAME").context("BOT_USERNAME not set in environment")?;
            let bot_avatar = env::var("BOT_AVATAR").context("BOT_AVATAR not set in environment")?;
            let bot_banner = env::var("BOT_BANNER").context("BOT_BANNER not set in environment")?;
            let avatar =
                CreateAttachment::url(&client.http, bot_avatar, "fabsebot_avatar.gif").await?;
            let banner =
                CreateAttachment::url(&client.http, bot_banner, "fabsebot_banner.gif").await?;
            client
                .http
                .edit_profile(
                    &EditProfile::default()
                        .avatar(&avatar)
                        .banner(&banner)
                        .username(bot_username),
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
