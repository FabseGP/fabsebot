use std::sync::Arc;

use anyhow::Result as AResult;
use serenity::all::{Context as SContext, GenericChannelId, GuildId, Ready};
use sqlx::query;
use tokio::spawn;
use tracing::{error, info};

use crate::{
	config::types::{BOT_CONTEXT, Data},
	periodic_task,
	utils::voice::{add_voice_events, join_handler},
};

pub async fn handle_ready(ctx: &SContext, data_about_bot: &Ready) -> AResult<()> {
	let data: Arc<Data> = ctx.data();

	if let Ok(app_emojis) = ctx.get_application_emojis().await {
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

	if BOT_CONTEXT.set(ctx.clone()).is_err() {
		error!("BOT_CONTEXT already initialized");
	}

	let persistent_voice_channels = query!(
		r#"
		SELECT guild_id, current_voice_channel FROM guild_settings
		WHERE current_voice_channel IS NOT NULL
		"#
	)
	.fetch_all(&data.db)
	.await?;

	for record in persistent_voice_channels {
		let guild_id = GuildId::new(record.guild_id.cast_unsigned());
		let channel_id =
			GenericChannelId::new(record.current_voice_channel.unwrap().cast_unsigned());
		let handler_lock =
			join_handler(&data.music_manager, guild_id, channel_id.expect_channel()).await?;
		add_voice_events(ctx, guild_id, channel_id, handler_lock).await;
	}

	spawn(async move { periodic_task(data).await });

	Ok(())
}
