#![feature(iter_intersperse, float_algebraic)]

pub mod config;
mod events;
mod handlers;
pub mod utils;

use std::{
	str::FromStr as _,
	sync::Arc,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result as AResult};
use dashmap::DashMap;
use mini_moka::sync::Cache;
use poise::{Command, Framework, FrameworkOptions, Prefix, PrefixFrameworkOptions};
use serenity::{
	Client,
	all::{
		ActivityData, CreateAllowedMentions, CreateAttachment, EditProfile, GatewayIntents,
		GenericChannelId, GuildId, Http, OnlineStatus, Settings, Token,
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
		constants::PING_INTERVAL_SEC,
		settings::{APIConfig, BotConfig, ServerConfig},
		types::{
			CLIENT_DATA, ClientData, Data, Error as SError, HTTP_CLIENT, RNG, UTILS_CONFIG,
			UtilsConfig,
		},
	},
	handlers::{EventHandler, dynamic_prefix, initialize_counters, on_command, on_error},
	utils::helpers::{get_gifs, get_waifu},
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

async fn periodic_task(data: Arc<Data>, http: Arc<Http>) -> ! {
	let mut interval = interval(Duration::from_secs(3600));
	loop {
		interval.tick().await;
		if let Ok(system_time) = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map(|t| t.as_secs())
			&& let Ok(now_timestamp) = i64::try_from(system_time)
		{
			let guild_ids: Vec<GuildId> = data.guild_data.iter().map(|g| *g.key()).collect();
			for guild_id in guild_ids {
				if let Some(guild_data) = data.guild_data.get(&guild_id) {
					let guild_id_i64 = i64::from(guild_id);
					let mut needs_update = false;
					let mut modified_settings = guild_data.as_ref().clone();
					if let Some(last_waifu) = modified_settings.settings.last_waifu
						&& let Some(waifu_rate) = modified_settings.settings.waifu_rate
						&& now_timestamp.saturating_sub(last_waifu) >= waifu_rate
						&& let Some(waifu_channel) = modified_settings.settings.waifu_channel
						&& let Ok(waifu_channel_u64) = u64::try_from(waifu_channel)
					{
						if let Err(err) = GenericChannelId::new(waifu_channel_u64)
							.say(&http, get_waifu().await)
							.await
						{
							error!("Failed to send waifu: {:?}", &err);
						} else {
							modified_settings.settings.last_waifu = Some(now_timestamp);
							needs_update = true;
							if let Ok(mut db_conn) = data.db.acquire().await
								&& let Err(err) = query!(
									"INSERT INTO guild_settings (guild_id, last_waifu)
            					VALUES ($1, $2)
            					ON CONFLICT(guild_id)
            					DO UPDATE SET
                       				last_waifu = $2",
									guild_id_i64,
									now_timestamp
								)
								.execute(&mut *db_conn)
								.await
							{
								error!("Failed to update last_waifu in db: {:?}", &err);
							}
						}
					}
					if let Some(last_dead_chat) = modified_settings.settings.last_dead_chat
						&& let Some(dead_chat_rate) = modified_settings.settings.dead_chat_rate
						&& now_timestamp.saturating_sub(last_dead_chat) >= dead_chat_rate
						&& let Some(dead_chat_channel) =
							modified_settings.settings.dead_chat_channel
						&& let Ok(dead_chat_channel_u64) = u64::try_from(dead_chat_channel)
					{
						let gifs = get_gifs("dead chat".to_owned()).await;
						let index = RNG.lock().await.usize(..gifs.len());
						if let Some(gif) = gifs.get(index).map(|g| g.0.clone()) {
							if let Err(err) = GenericChannelId::new(dead_chat_channel_u64)
								.say(&http, gif)
								.await
							{
								error!("Failed to send dead chat gif: {:?}", &err);
							} else {
								modified_settings.settings.last_dead_chat = Some(now_timestamp);
								needs_update = true;
								if let Ok(mut db_conn) = data.db.acquire().await
									&& let Err(err) = query!(
										"INSERT INTO guild_settings (guild_id, last_dead_chat)
            						VALUES ($1, $2)
            						ON CONFLICT(guild_id)
            						DO UPDATE SET
                       					last_dead_chat = $2",
										guild_id_i64,
										now_timestamp
									)
									.execute(&mut *db_conn)
									.await
								{
									error!("Failed to update last_dead_chat in db: {:?}", &err);
								}
							}
						}
					}
					if needs_update {
						data.guild_data
							.insert(guild_id, Arc::new(modified_settings));
						break;
					}
				}
			}
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

	let bot_data = Arc::new(Data {
		db: postgres_pool,
		music_manager: music_manager.clone(),
		ai_chats: Cache::new(1000),
		global_chats: Cache::new(1000),
		channel_webhooks: Cache::builder()
			.max_capacity(100)
			.time_to_idle(Duration::from_secs(3600))
			.build(),
		guild_data: Cache::new(1000),
		user_settings: Cache::new(1000),
		track_metadata: DashMap::default(),
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
		.data(bot_data.clone())
		.await
		.context("Failed to create client")?;
	let shutdown_trigger = client.shard_manager.get_shutdown_trigger();
	spawn(async move { wait_and_shutdown(shutdown_trigger).await });
	let http_clone = client.http.clone();
	spawn(async move { periodic_task(bot_data, http_clone).await });
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
						.encode("image/gif")
						.await?,
				)
				.banner(
					CreateAttachment::url(&client.http, &utils_config.bot.banner, "bot_banner.gif")
						.await?
						.encode("image/gif")
						.await?,
				)
				.username(&utils_config.bot.username),
		)
		.await
		.context("Failed to edit bot profile")?;

	Ok(())
}
