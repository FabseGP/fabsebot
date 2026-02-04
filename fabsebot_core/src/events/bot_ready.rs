use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};

use anyhow::{Context as _, Result as AResult};
use fabsebot_db::guild::{EmojiReplies, GuildData, GuildSettings, WordReactions, WordTracking};
use serenity::all::{Context as SContext, GuildId, Ready, UserId};
use sqlx::query_as;
use tracing::info;

use crate::config::{settings::UserSettings, types::Data};

pub async fn handle_ready(ctx: &SContext, data_about_bot: &Ready) -> AResult<()> {
	let data: Arc<Data> = ctx.data();
	let mut tx = data
		.db
		.begin()
		.await
		.context("Failed to acquire savepoint")?;
	let guild_settings = query_as!(GuildSettings, "SELECT * FROM guild_settings")
		.fetch_all(&mut *tx)
		.await?;
	let user_settings = query_as!(UserSettings, "SELECT * FROM user_settings")
		.fetch_all(&mut *tx)
		.await?;
	let word_reactions = query_as!(WordReactions, "SELECT * FROM guild_word_reaction")
		.fetch_all(&mut *tx)
		.await?;
	let word_tracking = query_as!(WordTracking, "SELECT * FROM guild_word_tracking")
		.fetch_all(&mut *tx)
		.await?;
	tx.commit()
		.await
		.context("Failed to commit sql-transaction")?;

	let mut grouped_word_reactions: HashMap<i64, HashSet<WordReactions>> = HashMap::default();
	let mut grouped_word_tracking: HashMap<i64, HashSet<WordTracking>> = HashMap::default();
	let mut grouped_emoji_replies: HashMap<i64, HashSet<EmojiReplies>> = HashMap::default();

	for reaction in word_reactions {
		grouped_word_reactions
			.entry(reaction.guild_id)
			.or_default()
			.insert(reaction);
	}

	for tracking in word_tracking {
		grouped_word_tracking
			.entry(tracking.guild_id)
			.or_default()
			.insert(tracking);
	}

	for settings in guild_settings {
		let guild_id = GuildId::new(settings.guild_id.cast_unsigned());
		let settings_guild_id = settings.guild_id;
		let guild_data = GuildData {
			settings,
			word_reactions: grouped_word_reactions
				.remove(&settings_guild_id)
				.unwrap_or_default(),
			word_tracking: grouped_word_tracking
				.remove(&settings_guild_id)
				.unwrap_or_default(),
			emoji_replies: grouped_emoji_replies
				.remove(&settings_guild_id)
				.unwrap_or_default(),
		};
		data.guilds.insert(guild_id, Arc::new(guild_data));
	}

	let mut guild_maps: HashMap<GuildId, HashMap<UserId, UserSettings>> = HashMap::default();
	for settings in user_settings {
		let guild_id = GuildId::new(settings.guild_id.cast_unsigned());
		let user_id = UserId::new(settings.user_id.cast_unsigned());
		guild_maps
			.entry(guild_id)
			.or_default()
			.insert(user_id, settings);
	}
	for (guild_id, map) in guild_maps {
		data.user_settings.insert(guild_id, Arc::new(map));
	}

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

	Ok(())
}
