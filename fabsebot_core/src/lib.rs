#![feature(iter_intersperse, const_convert, const_trait_impl)]

pub mod config;
pub mod errors;
pub mod events;
mod handlers;
pub mod stats;
pub mod utils;

use std::{
	str::FromStr as _,
	sync::{Arc, atomic::AtomicBool},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result as AResult};
use dashmap::DashMap;
use lavalink_rs::model::UserId;
use metrics::counter;
use mini_moka::sync::Cache;
use poise::{Command, Framework, FrameworkOptions, Prefix, PrefixFrameworkOptions};
use serenity::{
	Client,
	all::{
		ActivityData, Context as SContext, CreateAllowedMentions, GatewayIntents, GenericChannelId,
		OnlineStatus, Settings, Token, TransportCompression,
	},
};
use songbird::{Config, Songbird, driver::DecodeMode};
use sqlx::{Pool, Postgres, query};
use tokio::{
	select,
	signal::unix::{SignalKind, signal},
	spawn,
	time::interval,
};
use tracing::{error, warn};

use crate::{
	config::{
		settings::BotConfig,
		types::{CLIENT_DATA, ClientData, Data, Error as SError, HTTP_CLIENT, bot_context},
	},
	handlers::{EventHandler, dynamic_prefix, on_command, on_error},
	stats::counters::METRICS,
	utils::{
		helpers::{get_gif, get_waifu},
		voice::setup_lavalink,
		webhook::error_hook,
	},
};

const PING_INTERVAL_SEC: u64 = 60;
const CACHE_CAPACITY: u64 = 1000;
const CACHE_TIME_TO_IDLE_HOURS: u64 = 24;

pub async fn log_error(output: &str, ctx: &SContext) {
	error!("{output}");
	if let Err(err) = error_hook(ctx, output).await {
		error!("Failed to send error to webhook: {err}");
	}
}

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

async fn periodic_ping(url: &str, token: &str) -> ! {
	let mut interval = interval(Duration::from_secs(PING_INTERVAL_SEC));
	loop {
		interval.tick().await;
		if let Err(err) = HTTP_CLIENT.post(url).bearer_auth(token).send().await {
			error!("Failed to report uptime: {:?}", &err);
		}
	}
}

pub async fn periodic_task(bot_data: Arc<Data>) -> ! {
	let mut interval = interval(Duration::from_hours(1));
	let bot_context = bot_context();
	loop {
		interval.tick().await;
		let system_time = match SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map(|t| t.as_secs())
		{
			Ok(time) => time,
			Err(err) => {
				error!("Failed to get system time: {err}");
				continue;
			}
		};
		let now_timestamp = system_time.cast_signed();

		let guilds = match query!(
			r#"
			SELECT guild_id, last_waifu, waifu_rate, waifu_channel,
			last_dead_chat, dead_chat_rate, dead_chat_channel
			FROM guild_settings
			WHERE waifu_channel IS NOT NULL
				OR dead_chat_channel IS NOT NULL
			"#
		)
		.fetch_all(&bot_data.db)
		.await
		{
			Ok(guilds) => guilds,
			Err(err) => {
				error!("Failed to fetch guild settings: {err}");
				continue;
			}
		};

		for guild in guilds {
			if let Some(last_waifu) = guild.last_waifu
				&& let Some(waifu_rate) = guild.waifu_rate
				&& now_timestamp.saturating_sub(last_waifu) >= waifu_rate
				&& let Some(waifu_channel) = guild.waifu_channel
			{
				counter!(METRICS.periodic_waifu.clone()).increment(1);
				if let Err(err) = GenericChannelId::new(waifu_channel.cast_unsigned())
					.say(&bot_context.http, get_waifu(bot_context).await)
					.await
				{
					error!("Failed to send waifu: {:?}", &err);
				} else if let Err(err) = query!(
					r#"
					UPDATE guild_settings
					SET last_waifu = $2
					WHERE guild_id = $1
                    "#,
					guild.guild_id,
					now_timestamp
				)
				.execute(&bot_data.db)
				.await
				{
					error!("Failed to update last_waifu in db: {:?}", &err);
				}
			}
			if let Some(last_dead_chat) = guild.last_dead_chat
				&& let Some(dead_chat_rate) = guild.dead_chat_rate
				&& now_timestamp.saturating_sub(last_dead_chat) >= dead_chat_rate
				&& let Some(dead_chat_channel) = guild.dead_chat_channel
			{
				counter!(METRICS.periodic_dead_chat.clone()).increment(1);
				let gif = get_gif(bot_context, "dead chat").await;
				if let Err(err) = GenericChannelId::new(dead_chat_channel.cast_unsigned())
					.say(&bot_context.http, gif)
					.await
				{
					error!("Failed to send dead chat gif: {:?}", &err);
				} else if let Err(err) = query!(
					r#"
					UPDATE guild_settings
					SET last_dead_chat = $2
					WHERE guild_id = $1
            		"#,
					guild.guild_id,
					now_timestamp
				)
				.execute(&bot_data.db)
				.await
				{
					error!("Failed to update last_dead_chat in db: {:?}", &err);
				}
			}
		}
	}
}

pub async fn bot_start(
	bot_config: BotConfig,
	postgres_pool: Pool<Postgres>,
	commands: Vec<Command<Data, SError>>,
) -> AResult<()> {
	METRICS.describe_all();

	spawn(async move {
		periodic_ping(&bot_config.uptime_url, &bot_config.uptime_token).await;
	});

	let music_manager = Songbird::serenity();
	music_manager.set_config(Config::default().decode_mode(DecodeMode::Decrypt));

	let lavalink_client = setup_lavalink(
		bot_config.lavalink_host,
		bot_config.lavalink_password,
		UserId::from(bot_config.bot_id),
	)
	.await;

	let bot_data = Arc::new(Data {
		db: postgres_pool,
		music_manager: music_manager.clone(),
		channel_webhooks: Cache::builder()
			.max_capacity(CACHE_CAPACITY)
			.time_to_idle(Duration::from_hours(CACHE_TIME_TO_IDLE_HOURS))
			.build(),
		guilds: Cache::builder()
			.max_capacity(CACHE_CAPACITY)
			.time_to_idle(Duration::from_hours(CACHE_TIME_TO_IDLE_HOURS))
			.build(),
		app_emojis: Cache::builder()
			.max_capacity(CACHE_CAPACITY)
			.time_to_idle(Duration::from_hours(CACHE_TIME_TO_IDLE_HOURS))
			.build(),
		state_tracker: AtomicBool::new(true),
		lavalink_client,
		track_signals: DashMap::new(),
		users: Cache::builder()
			.max_capacity(CACHE_CAPACITY)
			.time_to_idle(Duration::from_hours(CACHE_TIME_TO_IDLE_HOURS))
			.build(),
	});
	let additional_prefix: &'static str =
		Box::leak(format!("hey {}", bot_config.username).into_boxed_str());
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
	cache_settings.max_messages = bot_config.cache_max_messages;
	let activity = ActivityData::listening(&bot_config.activity);
	let mut client = Client::builder(Token::from_str(&bot_config.token)?, intents)
		.compression(TransportCompression::Zstd)
		.framework(Box::new(framework))
		.voice_manager(music_manager)
		.cache_settings(cache_settings)
		.event_handler(Arc::new(EventHandler))
		.activity(activity)
		.status(OnlineStatus::Online)
		.data(bot_data.clone())
		.await
		.context("Failed to create client")?;
	let shutdown_trigger = client.shard_manager.get_shutdown_trigger();
	spawn(async move { wait_and_shutdown(shutdown_trigger).await });
	let client_data = ClientData {
		runners: client.shard_manager.runners.clone(),
	};
	if CLIENT_DATA.set(client_data).is_err() {
		error!("CLIENT_DATA already initialized");
	}
	client
		.start_autosharded()
		.await
		.context("Failed to shart client")?;

	Ok(())
}
