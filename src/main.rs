#![feature(let_chains, iter_intersperse)]

mod commands;
mod config;
mod core;
mod events;
mod utils;

use anyhow::{Context as _, Result as AResult};
use config::{
    settings::{AIConfig, APIConfig, MainConfig, PostgresConfig},
    types::HTTP_CLIENT,
};
use core::client::bot_start;
use opentelemetry::{KeyValue, global::set_tracer_provider, trace::TracerProvider as _};
use opentelemetry_otlp::{SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::{Resource, runtime::Tokio, trace::TracerProvider};
use std::{fs::read_to_string, time::Duration};
use tokio::{spawn, time::interval};
use toml::{Table, Value};
use tracing::Level;
use tracing_opentelemetry::layer;
use tracing_subscriber::{
    Registry, filter::LevelFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt as _,
};

async fn periodic_task(url: &String) {
    let mut interval = interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let _ = HTTP_CLIENT.get(url).send().await;
    }
}

#[tokio::main]
async fn main() -> AResult<()> {
    let config_toml: Table = read_to_string("config.toml")?
        .parse()
        .context("config.toml not found")?;

    let bot_config: MainConfig = Value::try_into(config_toml["Main"].clone())?;
    let postgres_config: PostgresConfig = Value::try_into(config_toml["PostgreSQL-Info"].clone())?;
    let ai_config: AIConfig = Value::try_into(config_toml["AI-Info"].clone())?;
    let api_config: APIConfig = Value::try_into(config_toml["API-Info"].clone())?;

    let log_level = match bot_config.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let new_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&bot_config.jaeger)
        .build()?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(new_exporter, Tokio)
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            bot_config.username.clone(),
        )]))
        .build();

    set_tracer_provider(provider.clone());

    Registry::default()
        .with(LevelFilter::from_level(log_level))
        .with(fmt::layer())
        .with(layer().with_tracer(provider.tracer(bot_config.username.clone())))
        .init();

    let uptime_task_url = bot_config.uptime_url.clone();

    spawn(async move {
        periodic_task(&uptime_task_url).await;
    });

    bot_start(bot_config, postgres_config, ai_config, api_config).await?;

    Ok(())
}
