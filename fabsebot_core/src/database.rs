use std::process::exit;

use serde::Deserialize;
use sqlx::{
	Pool, Postgres,
	postgres::{PgConnectOptions, PgPoolOptions},
};
use tracing::error;

#[derive(Deserialize)]
pub struct PostgresConfig {
	pub host: String,
	pub port: u16,
	pub user: String,
	pub database: String,
	pub password: String,
	pub max_connections: u32,
}

pub struct PostgresConn {
	pub pool: Pool<Postgres>,
}

impl PostgresConn {
	pub async fn new(config: PostgresConfig) -> Self {
		let pool_options = PgConnectOptions::new()
			.host(&config.host)
			.port(config.port)
			.username(&config.user)
			.database(&config.database)
			.password(&config.password);
		match PgPoolOptions::default()
			.max_connections(config.max_connections)
			.connect_with(pool_options)
			.await
		{
			Ok(pool) => PostgresConn { pool },
			Err(err) => {
				error!("Failed to connect to database: {:?}", &err);
				exit(1);
			}
		}
	}
}
