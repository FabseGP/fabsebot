use std::{self, sync::Arc};

use anyhow::Result as AResult;
use serenity::all::{Context as SContext, Ready};
use tracing::info;

use crate::config::types::Data;

pub async fn handle_ready(ctx: &SContext, data_about_bot: &Ready) -> AResult<()> {
	if let Ok(app_emojis) = ctx.get_application_emojis().await {
		let data: Arc<Data> = ctx.data();
		for emoji in app_emojis {
			data.app_emojis.insert(emoji.id.get(), Arc::new(emoji));
		}
	}

	let user_count = ctx
		.http
		.get_current_application_info()
		.await
		.map_or(0, |info| info.approximate_user_install_count.unwrap_or(0));

	info!(
		"Logged in as {} in {} server(s) and installed for {user_count} user(s)",
		data_about_bot.user.name,
		data_about_bot.guilds.len(),
	);

	Ok(())
}
