#![feature(iter_intersperse)]

use anyhow::Result as AResult;
use fabsebot_core::{
	config::{
		constants::HUMAN_ONLY_MSG,
		types::{Data, Error, SContext},
	},
	errors::commands::InteractionError,
	utils::helpers::correct_permissions,
};
use poise::Command;
use serenity::all::{Permissions, User};

mod api_calls;
mod funny;
mod games;
mod info;
mod misc;
mod music;
mod settings;

pub async fn command_permissions(ctx: &SContext<'_>) -> AResult<()> {
	if let Some(guild_id) = ctx.guild_id()
		&& ctx.channel().await.is_some()
	{
		let required_perms = Permissions::SEND_MESSAGES | Permissions::SEND_MESSAGES_IN_THREADS;
		correct_permissions(ctx, guild_id, required_perms).await?;
	}
	Ok(())
}

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
		funny::user_dm(),
		funny::user_misuse(),
		games::rps(),
		info::server_info(),
		info::user_info(),
		misc::birthday(),
		misc::bot_control(),
		misc::bot_personalize(),
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
		music::join_lavalink(),
		music::leave_voice(),
		music::leave_lavalink(),
		music::play_song(),
		music::play_lavalink(),
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

pub async fn require_human(ctx: SContext<'_>, user: &User) -> AResult<()> {
	if user.bot() {
		ctx.reply(HUMAN_ONLY_MSG).await?;
		return Err(InteractionError::NotHuman.into());
	}
	Ok(())
}
