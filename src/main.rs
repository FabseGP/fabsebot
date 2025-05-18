#![feature(let_chains, iter_intersperse, float_algebraic)]

mod commands;
mod config;
mod core;
mod events;
mod utils;

use core::client::bot_start;
use std::{fs::read_to_string, time::Duration};

use anyhow::{Context as _, Result as AResult};
use config::{
	settings::{APIConfig, FabseserverConfig, MainConfig, PostgresConfig},
	types::HTTP_CLIENT,
};
use opentelemetry::{KeyValue, global::set_tracer_provider, trace::TracerProvider as _};
use opentelemetry_otlp::{SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
use tokio::{spawn, time::interval};
use toml::{Table, Value};
use tracing::{Level, warn};
use tracing_opentelemetry::layer;
use tracing_subscriber::{
	Registry, filter::LevelFilter, fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

async fn periodic_task(url: &String) -> AResult<()> {
	let mut interval = interval(Duration::from_secs(60));
	loop {
		interval.tick().await;
		HTTP_CLIENT.get(url).send().await?;
	}
}

#[tokio::main]
async fn main() -> AResult<()> {
	let config_toml: Table = read_to_string("config.toml")?
		.parse()
		.context("config.toml not found")?;

	let bot_config: MainConfig = Value::try_into(config_toml["Main"].clone())?;
	let postgres_config: PostgresConfig = Value::try_into(config_toml["PostgreSQL-Info"].clone())?;
	let fabseserver_config: FabseserverConfig =
		Value::try_into(config_toml["Fabseserver"].clone())?;
	let api_config: APIConfig = Value::try_into(config_toml["API-Info"].clone())?;

	let log_level = match bot_config.log_level.as_str() {
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
				.with_endpoint(&bot_config.jaeger)
				.build()?,
		)
		.with_resource(
			Resource::builder()
				.with_attribute(KeyValue::new("service.name", bot_config.username.clone()))
				.build(),
		)
		.build();

	set_tracer_provider(provider.clone());

	Registry::default()
		.with(LevelFilter::from_level(log_level))
		.with(fmt::layer())
		.with(layer().with_tracer(provider.tracer(bot_config.username.clone())))
		.init();

	let uptime_task_url = bot_config.uptime_url.clone();

	spawn(async move {
		if let Err(e) = periodic_task(&uptime_task_url).await {
			warn!("Failed to report uptime: {e}");
		}
	});

	bot_start(bot_config, postgres_config, fabseserver_config, api_config).await?;

	Ok(())
}
