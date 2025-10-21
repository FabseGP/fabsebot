pub mod guild;

use anyhow::{Context as _, Result as AResult};
use serde::Deserialize;
use sqlx::{
	Pool, Postgres,
	postgres::{PgConnectOptions, PgPoolOptions},
};

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
	pub async fn new(config: PostgresConfig) -> AResult<Self> {
		let pool_options = PgConnectOptions::new()
			.host(&config.host)
			.port(config.port)
			.username(&config.user)
			.database(&config.database)
			.password(&config.password);
		let pool = PgPoolOptions::default()
			.max_connections(config.max_connections)
			.connect_with(pool_options)
			.await
			.context("Failed to connect to database")?;
		Ok(Self { pool })
	}
}
