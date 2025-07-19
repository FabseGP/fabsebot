#![feature(iter_intersperse, float_algebraic)]

pub mod config;
mod events;
mod handlers;
pub mod utils;

use std::{process::exit, str::FromStr as _, sync::Arc, time::Duration};

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
use songbird::{Config, Songbird, driver::DecodeMode::Decode};
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
		settings::{APIConfig, BotConfig, ServerConfig},
		types::{
			CLIENT_DATA, ClientData, Data, Error as SError, HTTP_CLIENT, UTILS_CONFIG, UtilsConfig,
		},
	},
	handlers::{EventHandler, dynamic_prefix, on_error},
};

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

async fn periodic_task(url: &str) -> ! {
	let mut interval = interval(Duration::from_secs(60));
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
	if let Err(err) = UTILS_CONFIG.set(Arc::new(UtilsConfig {
		bot: bot_config,
		fabseserver: server_config,
		api: api_config,
	})) {
		error!("Failed to set UTILS_CONFIG: {:?}", &err);
	}

	let Some(utils_config) = UTILS_CONFIG.get() else {
		error!("UTILS_CONFIG not set");
		exit(1);
	};

	spawn(async move {
		periodic_task(&utils_config.bot.uptime_url).await;
	});

	let music_manager = Songbird::serenity();
	music_manager.set_config(Config::default().use_softclip(false).decode_mode(Decode));
	let user_data = Arc::new(Data {
		db: postgres_pool,
		music_manager: Arc::<Songbird>::clone(&music_manager),
		ai_chats: Arc::new(Cache::new(1000)),
		global_chats: Arc::new(Cache::new(1000)),
		channel_webhooks: Arc::new(Cache::new(1000)),
		guild_data: Arc::new(Mutex::new(Cache::new(1000))),
		user_settings: Arc::new(Mutex::new(Cache::new(1000))),
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
	cache_settings.max_messages = utils_config.bot.cache_max_messages;
	let activity = ActivityData::listening(&utils_config.bot.activity);
	let client = Client::builder(Token::from_str(&utils_config.bot.token)?, intents)
		.framework(framework)
		.voice_manager::<Songbird>(music_manager)
		.cache_settings(cache_settings)
		.event_handler(EventHandler)
		.activity(activity)
		.status(OnlineStatus::Online)
		.data(user_data)
		.await;
	match client {
		Ok(mut client) => {
			let shutdown_trigger = client.shard_manager.get_shutdown_trigger();
			spawn(async move {
				wait_until_shutdown().await;
				warn!("Recieved control C and shutting down...");
				if shutdown_trigger() {
					warn!("Successfully triggered shutdown for all shards");
				} else {
					warn!("Failed to trigger shutdown, shards may have already stopped");
				}
			});
			if let Err(e) = client.start_autosharded().await {
				warn!("Client error: {:?}", e);
			}
			let client_data = Arc::new(ClientData {
				shard_manager: client.shard_manager,
			});
			if CLIENT_DATA.set(client_data).is_err() {
				error!("Failed to set CLIENT_DATA");
			}
			client
				.http
				.edit_profile(
					&EditProfile::default()
						.avatar(
							CreateAttachment::url(
								&client.http,
								&utils_config.bot.avatar,
								"bot_avatar.gif",
							)
							.await?
							.encode()
							.await?,
						)
						.banner(
							CreateAttachment::url(
								&client.http,
								&utils_config.bot.banner,
								"bot_banner.gif",
							)
							.await?
							.encode()
							.await?,
						)
						.username(&utils_config.bot.username),
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
