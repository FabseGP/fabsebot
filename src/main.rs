mod client;
mod commands;
mod handlers;
mod types;
mod utils;

use tracing::subscriber;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::new();
    subscriber::set_global_default(subscriber)?;
    client::start().await?;
    Ok(())
}
