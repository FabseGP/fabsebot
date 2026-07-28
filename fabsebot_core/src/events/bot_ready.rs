use std::sync::{Arc, atomic::Ordering};

use serenity::all::{Context as SContext, Ready};
use tokio::spawn;
use tracing::{error, info};

use crate::{
	config::types::{BOT_CONTEXT, BotContext, Data},
	periodic_task,
};

pub async fn handle_ready(ctx: &SContext, data_about_bot: &Ready) {
	let bot_data: Arc<Data> = ctx.data();

	if bot_data.state_tracker.swap(false, Ordering::Relaxed) {
		if BOT_CONTEXT
			.set(BotContext {
				data: bot_data.clone(),
				http: ctx.http.clone(),
				cache: ctx.cache.clone(),
			})
			.is_err()
		{
			error!("BOT_CONTEXT already initialized");
		}
		spawn(async move { periodic_task().await });
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
}
