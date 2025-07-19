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
use opentelemetry::{KeyValue, global::set_tracer_provider, trace::TracerProvider as _};
use opentelemetry_otlp::{SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
use toml::{Table, Value};
use tracing::Level;
use tracing_opentelemetry::layer;
use tracing_subscriber::{
	Registry, filter::LevelFilter, fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

fn setup_tracing(jaeger: &str, bot_username: String, log_level_str: &str) -> AResult<()> {
	let log_level = match log_level_str {
		"trace" => Level::TRACE,
		"debug" => Level::DEBUG,
		"warn" => Level::WARN,
		"error" => Level::ERROR,
		_ => Level::INFO,
	};

	let provider = SdkTracerProvider::builder()
		.with_batch_exporter(
			SpanExporter::builder()
				.with_tonic()
				.with_endpoint(jaeger)
				.build()?,
		)
		.with_resource(
			Resource::builder()
				.with_attribute(KeyValue::new("service.name", bot_username.clone()))
				.build(),
		)
		.build();

	set_tracer_provider(provider.clone());

	Registry::default()
		.with(LevelFilter::from_level(log_level))
		.with(fmt::layer())
		.with(layer().with_tracer(provider.tracer(bot_username)))
		.init();

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

	setup_tracing(
		&main_config.jaeger,
		bot_config.username.clone(),
		&main_config.log_level,
	)?;

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
