mod client;
mod commands;
mod handlers;
mod types;
mod utils;

#[tokio::main]
async fn main() {
    client::start().await;
}
