#![feature(iter_intersperse, const_convert, const_trait_impl)]

pub mod config;
pub mod errors;
mod events;
mod handlers;
pub mod stats;
pub mod utils;

use std::{
	str::FromStr as _,
	sync::Arc,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result as AResult};
use fabsebot_db::guild::GuildSettings;
use metrics::counter;
use mini_moka::sync::Cache;
use poise::{Command, Framework, FrameworkOptions, Prefix, PrefixFrameworkOptions};
use serenity::{
	Client,
	all::{
		ActivityData, CreateAllowedMentions, GatewayIntents, GenericChannelId, Http, OnlineStatus,
		Settings, Token, TransportCompression,
	},
};
use songbird::{Config, Songbird, driver::DecodeMode};
use sqlx::{Pool, Postgres, query, query_as};
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
		types::{CLIENT_DATA, ClientData, Data, Error as SError, HTTP_CLIENT},
	},
	handlers::{EventHandler, dynamic_prefix, on_command, on_error},
	stats::counters::METRICS,
	utils::helpers::{get_gifs, get_waifu},
};

const PING_INTERVAL_SEC: u64 = 60;

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

async fn periodic_task(data: Arc<Data>, http: Arc<Http>) -> ! {
	let mut interval = interval(Duration::from_hours(1));
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

		let mut tx = match data.db.begin().await {
			Ok(tx) => tx,
			Err(err) => {
				error!("Failed to start SQL-transaction: {err}");
				continue;
			}
		};

		let guilds = match query_as!(
			GuildSettings,
			r#"
			SELECT * FROM guild_settings
			WHERE waifu_channel IS NOT NULL
			OR dead_chat_channel IS NOT NULL
			"#
		)
		.fetch_all(&mut *tx)
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
					.say(&http, get_waifu().await)
					.await
				{
					error!("Failed to send waifu: {:?}", &err);
				} else if let Err(err) = query!(
					r#"
					INSERT INTO guild_settings (guild_id, last_waifu)
            		VALUES ($1, $2)
            		ON CONFLICT (guild_id)
            		DO UPDATE SET last_waifu = $2
                    "#,
					guild.guild_id,
					now_timestamp
				)
				.execute(&mut *tx)
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
				let gifs = get_gifs("dead chat").await;
				let index = fastrand::usize(..gifs.len());
				if let Some(gif) = gifs.get(index).map(|g| g.0.clone()) {
					if let Err(err) = GenericChannelId::new(dead_chat_channel.cast_unsigned())
						.say(&http, gif)
						.await
					{
						error!("Failed to send dead chat gif: {:?}", &err);
					} else if let Err(err) = query!(
						r#"
						INSERT INTO guild_settings (guild_id, last_dead_chat)
            			VALUES ($1, $2)
            			ON CONFLICT (guild_id)
            			DO UPDATE SET last_dead_chat = $2
            			"#,
						guild.guild_id,
						now_timestamp
					)
					.execute(&mut *tx)
					.await
					{
						error!("Failed to update last_dead_chat in db: {:?}", &err);
					}
				}
			}
		}
		if let Err(err) = tx.commit().await {
			error!("Failed to commit SQL-transaction: {err}");
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

	let bot_data = Arc::new(Data {
		db: postgres_pool,
		music_manager: music_manager.clone(),
		channel_webhooks: Cache::builder()
			.max_capacity(100)
			.time_to_idle(Duration::from_hours(12))
			.build(),
		guilds: Cache::builder()
			.max_capacity(1000)
			.time_to_idle(Duration::from_hours(12))
			.build(),
		track_metadata: Cache::builder()
			.max_capacity(1000)
			.time_to_idle(Duration::from_hours(12))
			.build(),
		app_emojis: Cache::builder()
			.max_capacity(1000)
			.time_to_idle(Duration::from_hours(12))
			.build(),
	});
	let additional_prefix: &'static str =
		Box::leak(format!("hey {}", &bot_config.username).into_boxed_str());
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
	let http_clone = client.http.clone();
	spawn(async move { periodic_task(bot_data, http_clone).await });
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
