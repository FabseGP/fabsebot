#![feature(iter_intersperse, float_algebraic)]

pub mod config;
mod events;
mod handlers;
pub mod utils;

use std::{collections::HashMap, str::FromStr as _, sync::Arc, time::Duration};

use anyhow::{Context as _, Result as AResult};
use mini_moka::sync::Cache;
use poise::{Command, Framework, FrameworkOptions, Prefix, PrefixFrameworkOptions};
use serenity::{
	Client,
	all::{
		ActivityData, CreateAllowedMentions, CreateAttachment, EditProfile, GatewayIntents,
		OnlineStatus, Settings, Token,
	},
};
use songbird::{Config, Songbird, driver::DecodeMode};
use sqlx::{Pool, Postgres};
use tokio::{
	select,
	signal::unix::{SignalKind, signal},
	spawn,
	sync::Mutex,
	time::interval,
};
use tracing::{error, warn};

use crate::{
	config::{
		constants::PING_INTERVAL_SEC,
		settings::{APIConfig, BotConfig, ServerConfig},
		types::{
			CLIENT_DATA, ClientData, Data, Error as SError, HTTP_CLIENT, UTILS_CONFIG, UtilsConfig,
		},
	},
	handlers::{EventHandler, dynamic_prefix, initialize_counters, on_command, on_error},
};

async fn wait_and_shutdown<F>(shutdown_trigger: F)
where
	F: FnOnce() -> bool + Send,
{
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

	if shutdown_trigger() {
		warn!("Successfully triggered shutdown for all shards");
	} else {
		warn!("Failed to trigger shutdown, shards may have already stopped");
	}
}

async fn periodic_ping(url: &str) -> ! {
	let mut interval = interval(Duration::from_secs(PING_INTERVAL_SEC));
	loop {
		interval.tick().await;
		if let Err(err) = HTTP_CLIENT.get(url).send().await {
			error!("Failed to report uptime: {:?}", &err);
		}
	}
}

pub async fn bot_start(
	bot_config: BotConfig,
	server_config: ServerConfig,
	api_config: APIConfig,
	postgres_pool: Pool<Postgres>,
	commands: Vec<Command<Data, SError>>,
) -> AResult<()> {
	let utils_config = UTILS_CONFIG.get_or_init(|| {
		Arc::new(UtilsConfig {
			bot: bot_config,
			fabseserver: server_config,
			api: api_config,
		})
	});

	spawn(async move {
		periodic_ping(&utils_config.bot.uptime_url).await;
	});

	initialize_counters();

	let music_manager = Songbird::serenity();
	music_manager.set_config(
		Config::default()
			.use_softclip(false)
			.decode_mode(DecodeMode::Pass),
	);

	let voice_manager = Songbird::serenity();
	voice_manager.set_config(
		Config::default()
			.use_softclip(false)
			.decode_mode(DecodeMode::Decode),
	);

	let user_data = Arc::new(Data {
		db: postgres_pool,
		music_manager: Arc::<Songbird>::clone(&music_manager),
		voice_manager: Arc::<Songbird>::clone(&voice_manager),
		ai_chats: Cache::new(1000),
		global_chats: Cache::new(1000),
		channel_webhooks: Cache::new(1000),
		guild_data: Cache::new(1000),
		user_settings: Cache::new(1000),
		track_metadata: Arc::new(Mutex::new(HashMap::default())),
	});
	let additional_prefix: &'static str =
		Box::leak(format!("hey {}", &utils_config.bot.username).into_boxed_str());
	let framework = Framework::builder()
		.options(FrameworkOptions {
			commands,
			prefix_options: PrefixFrameworkOptions {
				dynamic_prefix: Some(|ctx| Box::pin(dynamic_prefix(ctx))),
				additional_prefixes: vec![Prefix::Literal(additional_prefix)],
				..Default::default()
			},
			allowed_mentions: Some(CreateAllowedMentions::default().replied_user(false)),
			on_error: |error| Box::pin(on_error(error)),
			pre_command: |context| Box::pin(on_command(context)),
			..Default::default()
		})
		.build();
	let intents = GatewayIntents::GUILDS
		| GatewayIntents::GUILD_MEMBERS
		| GatewayIntents::GUILD_MESSAGES
		| GatewayIntents::GUILD_VOICE_STATES
		| GatewayIntents::MESSAGE_CONTENT;
	let mut cache_settings = Settings::default();
	cache_settings.max_messages = utils_config.bot.cache_max_messages;
	let activity = ActivityData::listening(&utils_config.bot.activity);
	let mut client = Client::builder(Token::from_str(&utils_config.bot.token)?, intents)
		.framework(framework)
		.voice_manager::<Songbird>(music_manager)
		.cache_settings(cache_settings)
		.event_handler(EventHandler)
		.activity(activity)
		.status(OnlineStatus::Online)
		.data(user_data)
		.await
		.context("Failed to create client")?;
	let shutdown_trigger = client.shard_manager.get_shutdown_trigger();
	spawn(async move { wait_and_shutdown(shutdown_trigger).await });
	client
		.start_autosharded()
		.await
		.context("Failed to shart client")?;
	let client_data = Arc::new(ClientData {
		shard_manager: client.shard_manager,
	});
	if CLIENT_DATA.set(client_data).is_err() {
		error!("CLIENT_DATA already initialized");
	}
	client
		.http
		.edit_profile(
			&EditProfile::default()
				.avatar(
					CreateAttachment::url(&client.http, &utils_config.bot.avatar, "bot_avatar.gif")
						.await?
						.encode()
						.await?,
				)
				.banner(
					CreateAttachment::url(&client.http, &utils_config.bot.banner, "bot_banner.gif")
						.await?
						.encode()
						.await?,
				)
				.username(&utils_config.bot.username),
		)
		.await
		.context("Failed to edit bot profile")?;

	Ok(())
}
