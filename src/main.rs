mod config;

use std::fs::read_to_string;

use anyhow::Result as AResult;
use config::MainConfig;
use fabsebot_commands::commands;
use fabsebot_core::{
	bot_start,
	config::settings::{APIConfig, BotConfig, ServerConfig},
};
use fabsebot_db::{PostgresConfig, PostgresConn};
use metrics_exporter_prometheus::PrometheusBuilder;
use toml::{Table, Value};
use tracing::{Level, error, subscriber};
use tracing_subscriber::filter::LevelFilter;

fn setup_tracing(log_level_str: &str) -> AResult<()> {
	let log_level = match log_level_str {
		"trace" => Level::TRACE,
		"debug" => Level::DEBUG,
		"warn" => Level::WARN,
		"error" => Level::ERROR,
		_ => Level::INFO,
	};

	let subscriber = tracing_subscriber::fmt()
		.with_max_level(LevelFilter::from_level(log_level))
		.finish();
	subscriber::set_global_default(subscriber)?;

	if let Err(err) = PrometheusBuilder::default().install() {
		error!("Failed to install Prometheus recorder: {:?}", &err);
	}

	Ok(())
}

#[tokio::main]
async fn main() -> AResult<()> {
	let config_toml: Table = read_to_string("config.toml")?.parse()?;

	let main_config: MainConfig = Value::try_into(config_toml["Main"].clone())?;
	let bot_config: BotConfig = Value::try_into(config_toml["Bot"].clone())?;
	let postgres_config: PostgresConfig = Value::try_into(config_toml["PostgreSQL"].clone())?;
	let server_config: ServerConfig = Value::try_into(config_toml["Server"].clone())?;
	let api_config: APIConfig = Value::try_into(config_toml["API-Info"].clone())?;

	setup_tracing(&main_config.log_level)?;

	let postgres_pool = PostgresConn::new(postgres_config).await;

	bot_start(
		bot_config,
		server_config,
		api_config,
		postgres_pool.pool,
		commands(),
	)
	.await?;

	Ok(())
}
