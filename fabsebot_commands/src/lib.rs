#![feature(iter_intersperse)]

use std::{convert::Infallible, sync::Arc};

use anyhow::Result as AResult;
use fabsebot_core::{
	config::{
		constants::{HUMAN_ONLY_MSG, NOT_IN_GUILD_MSG, NOT_IN_VOICE_CHAN_MSG},
		types::{Data, Error, SContext},
	},
	errors::commands::{GuildError, InteractionError, MusicError},
};
use poise::Command;
use serenity::all::{CacheRef, Guild, GuildId, User};
use songbird::Call;
use tokio::sync::Mutex;

mod api_calls;
mod funny;
mod games;
mod info;
mod misc;
mod music;
mod settings;

#[must_use]
pub fn commands() -> Vec<Command<Data, Error>> {
	vec![
		api_calls::ai_image(),
		api_calls::ai_text(),
		api_calls::anime(),
		api_calls::anime_scene(),
		api_calls::eightball(),
		api_calls::gif(),
		api_calls::joke(),
		api_calls::manga(),
		api_calls::memegen(),
		api_calls::roast(),
		api_calls::roast_user(),
		api_calls::translate(),
		api_calls::urban(),
		api_calls::waifu(),
		api_calls::wiki(),
		funny::anonymous(),
		funny::anonymous_msg(),
		funny::user_dm(),
		funny::user_misuse(),
		games::rps(),
		info::server_info(),
		info::user_info(),
		misc::anony_poll(),
		misc::birthday(),
		misc::birthday_user(),
		misc::bot_control(),
		misc::debug(),
		misc::global_chat_end(),
		misc::global_chat_start(),
		misc::help(),
		misc::leaderboard(),
		misc::ohitsyou(),
		misc::quote(),
		misc::quote_menu(),
		misc::register_commands(),
		misc::respond(),
		misc::slow_mode(),
		misc::word_count(),
		music::add_deezer_playlist(),
		music::add_youtube_playlist(),
		music::join_voice(),
		music::join_voice_global(),
		music::leave_voice(),
		music::leave_voice_global(),
		music::play_song(),
		music::play_song_global(),
		music::seek_song(),
		music::text_to_voice(),
		settings::configure_server_settings(),
		settings::reset_user_settings(),
		settings::set_afk(),
		settings::set_chatbot_options(),
		settings::set_prefix(),
		settings::set_user_ping(),
		settings::set_word_react(),
		settings::set_word_track(),
	]
}

pub async fn require_guild(ctx: SContext<'_>) -> AResult<CacheRef<'_, GuildId, Guild, Infallible>> {
	let Some(guild) = ctx.guild() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Err(GuildError::NotInGuild.into());
	};
	Ok(guild)
}

pub async fn require_guild_id(ctx: SContext<'_>) -> AResult<GuildId> {
	let Some(guild_id) = ctx.guild_id() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Err(GuildError::NotInGuild.into());
	};
	Ok(guild_id)
}

pub async fn require_human(ctx: SContext<'_>, user: &User) -> AResult<()> {
	if user.bot() {
		ctx.reply(HUMAN_ONLY_MSG).await?;
		return Err(InteractionError::NotHuman.into());
	}
	Ok(())
}

pub async fn require_handler(ctx: SContext<'_>, guild_id: GuildId) -> AResult<Arc<Mutex<Call>>> {
	let Some(handler_lock) = ctx.data().music_manager.get(guild_id) else {
		ctx.reply(NOT_IN_VOICE_CHAN_MSG).await?;
		return Err(MusicError::NotInVoiceChan.into());
	};
	Ok(handler_lock)
}

pub async fn remove_handler(ctx: SContext<'_>, guild_id: GuildId) -> AResult<()> {
	if ctx.data().music_manager.remove(guild_id).await.is_err() {
		ctx.reply(NOT_IN_VOICE_CHAN_MSG).await?;
		return Err(MusicError::NotInVoiceChan.into());
	}
	Ok(())
}
