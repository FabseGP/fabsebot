mod config;

use anyhow::{Context as _, Result as AResult};
use config::MainConfig;
use fabsebot_commands::commands;
use fabsebot_core::{
	bot_start,
	config::settings::{APIConfig, BotConfig, HTTPAgent, ServerConfig},
};
use fabsebot_db::{PostgresConfig, PostgresConn};
use metrics_exporter_prometheus::PrometheusBuilder;
use mimalloc::MiMalloc;
use toml::{Table, Value};
use tracing::{Level, subscriber::set_global_default};
use tracing_subscriber::{filter::LevelFilter, fmt};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

const CONFIG_TOML: &str = include_str!("../config.toml");

fn setup_tracing(log_level_str: &str) -> AResult<()> {
	let log_level = match log_level_str {
		"trace" => Level::TRACE,
		"debug" => Level::DEBUG,
		"warn" => Level::WARN,
		"error" => Level::ERROR,
		_ => Level::INFO,
	};

	let subscriber = fmt()
		.with_max_level(LevelFilter::from_level(log_level))
		.finish();

	set_global_default(subscriber).context("Failed to set global subscriber")?;

	PrometheusBuilder::default()
		.install()
		.context("Failed to install Prometheus recorder")?;

	Ok(())
}

#[tokio::main]
async fn main() -> AResult<()> {
	let config_toml: Table = CONFIG_TOML.parse()?;

	let main_config: MainConfig = Value::try_into(config_toml["Main"].clone())?;
	let bot_config: BotConfig = Value::try_into(config_toml["Bot"].clone())?;
	let postgres_config: PostgresConfig = Value::try_into(config_toml["PostgreSQL"].clone())?;
	let server_config: ServerConfig = Value::try_into(config_toml["Server"].clone())?;
	let api_config: APIConfig = Value::try_into(config_toml["API-Info"].clone())?;
	let http_agent: HTTPAgent = Value::try_into(config_toml["HTTP-Agent"].clone())?;

	setup_tracing(&main_config.log_level)?;

	let postgres_pool = PostgresConn::new(postgres_config).await?;

	bot_start(
		bot_config,
		server_config,
		api_config,
		http_agent,
		postgres_pool.pool,
		commands(),
	)
	.await?;

	Ok(())
}
