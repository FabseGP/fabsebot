use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct MainConfig {
	pub log_level: String,
	pub jaeger: String,
}
