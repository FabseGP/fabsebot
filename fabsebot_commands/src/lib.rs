#![feature(iter_intersperse)]

use fabsebot_core::config::types::{Data, Error};
use poise::Command;

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
		misc::anony_poll(),
		misc::birthday(),
		misc::bot_control(),
		misc::debug(),
		misc::end_pgo(),
		misc::global_chat_end(),
		misc::global_chat_start(),
		misc::help(),
		misc::leaderboard(),
		misc::ohitsyou(),
		misc::quote(),
		misc::register_commands(),
		misc::respond(),
		misc::slow_mode(),
		misc::word_count(),
		music::add_playlist(),
		music::global_music_end(),
		music::global_music_start(),
		music::join_voice(),
		music::join_voice_global(),
		music::leave_voice(),
		music::pause_continue_song(),
		music::play_song(),
		music::seek_song(),
		music::skip_song(),
		music::stop_song(),
		music::text_to_voice(),
		settings::reset_server_settings(),
		settings::reset_user_settings(),
		settings::set_afk(),
		settings::set_chatbot_channel(),
		settings::set_chatbot_options(),
		settings::set_dead_chat(),
		settings::set_emoji_react(),
		settings::set_prefix(),
		settings::set_quote_channel(),
		settings::set_spoiler_channel(),
		settings::set_user_ping(),
		settings::set_word_react(),
		settings::set_word_track(),
	]
}
