use serde::Deserialize;

#[derive(Deserialize)]
pub struct MainConfig {
	pub log_level: String,
}
