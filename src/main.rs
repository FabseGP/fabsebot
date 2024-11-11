#![feature(let_chains, iter_intersperse)]

mod commands;
mod config;
mod core;
mod events;
mod utils;

use anyhow::{Context as _, Result as AResult};
use config::settings::{AIConfig, APIConfig, MainConfig, PostgresConfig};
use core::client::bot_start;
use opentelemetry::{global::set_tracer_provider, trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::{new_exporter, new_pipeline, WithExportConfig as _};
use opentelemetry_sdk::{runtime::Tokio, trace::Config, Resource};
use std::fs::read_to_string;
use toml::{Table, Value};
use tracing::Level;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
    filter::LevelFilter, fmt, layer::SubscriberExt as _, registry, util::SubscriberInitExt as _,
};

#[tokio::main]
async fn main() -> AResult<()> {
    let config_toml: Table = read_to_string("config.toml")?
        .parse()
        .context("config.toml not found")?;

    let bot_config: MainConfig = Value::try_into(config_toml["Main"].clone())?;
    let postgres_config: PostgresConfig = Value::try_into(config_toml["PostgreSQL-Info"].clone())?;
    let ai_config: AIConfig = Value::try_into(config_toml["AI-Info"].clone())?;
    let api_config: APIConfig = Value::try_into(config_toml["API-Info"].clone())?;

    let provider = new_pipeline()
        .tracing()
        .with_exporter(new_exporter().tonic().with_endpoint(&bot_config.jaeger))
        .with_trace_config(
            Config::default().with_resource(Resource::new(vec![KeyValue::new(
                "service.name",
                bot_config.username.clone(),
            )])),
        )
        .install_batch(Tokio)?;
    set_tracer_provider(provider.clone());
    registry()
        .with(LevelFilter::from_level(Level::INFO))
        .with(fmt::layer())
        .with(OpenTelemetryLayer::new(
            provider.tracer(bot_config.username.clone()),
        ))
        .init();

    bot_start(bot_config, postgres_config, ai_config, api_config).await?;

    Ok(())
}
