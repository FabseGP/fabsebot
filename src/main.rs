#![feature(iter_intersperse)]
#![feature(let_chains)]

mod client;
mod commands;
mod events;
mod types;
mod utils;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{new_exporter, new_pipeline, WithExportConfig as _};
use opentelemetry_sdk::{runtime, trace, Resource};
use tracing::Level;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{filter::LevelFilter, fmt, layer::SubscriberExt as _, registry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = new_pipeline()
        .tracing()
        .with_exporter(
            new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .with_trace_config(trace::Config::default().with_resource(Resource::new(vec![
            KeyValue::new("service.name", "fabsebot"),
        ])))
        .install_batch(runtime::Tokio)?;
    global::set_tracer_provider(provider.clone());
    let tracer = provider.tracer("fabsebot");
    let telemetry_layer = OpenTelemetryLayer::new(tracer);
    let fmt_layer = fmt::layer();
    registry()
        .with(LevelFilter::from_level(Level::INFO))
        .with(fmt_layer)
        .with(telemetry_layer)
        .init();
    client::start().await?;
    Ok(())
}
