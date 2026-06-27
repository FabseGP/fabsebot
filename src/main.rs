use std::{fs::read_to_string, time::Duration};

use anyhow::{Context as _, Result as AResult};
use fabsebot_commands::commands;
use fabsebot_core::{
	bot_start,
	config::{
		settings::{APIConfig, BotConfig, HTTPAgent, LogConfig, ServerConfig},
		types::{UTILS_CONFIG, UtilsConfig},
	},
};
use fabsebot_db::{PostgresConfig, PostgresConn};
use metrics_exporter_prometheus::PrometheusBuilder;
use mimalloc::MiMalloc;
use rustls::crypto::aws_lc_rs;
use tokio::{spawn, time::MissedTickBehavior};
use toml::{Table, Value};
use tracing::{Level, error};
use tracing_loki_but_better::LokiBuilder;
use tracing_subscriber::{
	Layer as _, Registry, filter::LevelFilter, fmt, layer::SubscriberExt as _,
	util::SubscriberInitExt as _,
};
use url::Url;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

async fn setup_tracing(log_config: &LogConfig, service_name: &str) -> AResult<()> {
	let log_level = match log_config.log_level.as_str() {
		"trace" => Level::TRACE,
		"debug" => Level::DEBUG,
		"warn" => Level::WARN,
		"error" => Level::ERROR,
		_ => Level::INFO,
	};

	let level_filter = LevelFilter::from_level(log_level);

	let env_layer = fmt::layer().with_filter(level_filter);

	let builder = LokiBuilder::new()
		.batch_send_interval(Duration::from_secs(1))
		.missed_tick_behavior(MissedTickBehavior::Burst)
		.label("service_name", service_name)?
		.label("env", &log_config.env)?
		.add_vl_compat(true);

	let (loki_layer, task) = builder
		.build_without_strip(&Url::parse(&log_config.host)?)
		.await?;

	Registry::default()
		.with(loki_layer.with_filter(level_filter))
		.with(env_layer)
		.init();

	spawn(task);

	PrometheusBuilder::default()
		.install()
		.context("Failed to install Prometheus recorder")?;

	Ok(())
}

#[expect(clippy::expect_used)]
#[tokio::main]
async fn main() -> AResult<()> {
	aws_lc_rs::default_provider().install_default().unwrap();

	let config_toml: Table = read_to_string("config.toml")?.parse()?;

	let log_config: LogConfig = Value::try_into(
		config_toml
			.get("Logging")
			.expect("Missing Logging-field")
			.clone(),
	)?;
	let bot_config: BotConfig =
		Value::try_into(config_toml.get("Bot").expect("Missing Bot-field").clone())?;
	let postgres_config: PostgresConfig = Value::try_into(
		config_toml
			.get("PostgreSQL")
			.expect("Missing PostgreSQL-field")
			.clone(),
	)?;
	let server_config: ServerConfig = Value::try_into(
		config_toml
			.get("Server")
			.expect("Missing Server-field")
			.clone(),
	)?;
	let api_config: APIConfig = Value::try_into(
		config_toml
			.get("API-Info")
			.expect("Missing API-Info-field")
			.clone(),
	)?;
	let http_agent: HTTPAgent = Value::try_into(
		config_toml
			.get("HTTP-Agent")
			.expect("Missing HTTP-Agent-field")
			.clone(),
	)?;

	setup_tracing(&log_config, &bot_config.username).await?;

	let postgres_pool = PostgresConn::new(postgres_config).await?;

	if UTILS_CONFIG
		.set(UtilsConfig {
			owner_id: bot_config.owner_id,
			ping_message: bot_config.ping_message.clone(),
			ping_payload: bot_config.ping_payload.clone(),
			fabseserver: server_config,
			api: api_config,
			http_agent,
			bot_name: bot_config.username.clone(),
			error_webhook: bot_config.error_webhook.clone(),
			feedback_webhook: bot_config.feedback_webhook.clone(),
		})
		.is_err()
	{
		error!("UTILS_CONFIG already initialized");
	}

	bot_start(bot_config, postgres_pool.pool, commands()).await?;

	Ok(())
}
