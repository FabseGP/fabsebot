use std::sync::{Arc, LazyLock};

use metrics::describe_counter;

use crate::config::types::utils_config;

pub static METRICS: LazyLock<Arc<Metrics>> = LazyLock::new(|| Arc::new(Metrics::new()));

pub struct Metrics {
	pub commands: String,
	pub command_errors: String,
	pub channel_triggers: String,
	pub words_tracked: String,
	pub word_reactions: String,
	pub periodic_dead_chat: String,
	pub periodic_waifu: String,
	pub bot_pings: String,
	pub floppaganda: String,
	pub message_previews: String,
	pub user_afks: String,
	pub custom_user_pings: String,
	pub prefix_errors: String,
	pub message_errors: String,
	pub new_guild_errors: String,
	pub member_addition_errors: String,
	pub ready_errors: String,
	pub messages_deleted_errors: String,
	pub playback_errors: String,
	pub feedback_modal_errors: String,
	pub feedback_reply_errors: String,
	pub bot_permissions_errors: String,
	pub user_permissions_errors: String,
	pub chatbot_errors: String,
	pub music_queue_errors: String,
	pub waifu_errors: String,
	pub lyrics_errors: String,
	pub gifs_errors: String,
}

impl Metrics {
	fn new() -> Self {
		let bot_name = utils_config().bot_name.as_str();
		Self {
			commands: format!("{bot_name}_commands_total"),
			command_errors: format!("{bot_name}_command_errors_total"),
			channel_triggers: format!("{bot_name}_channel_triggers_total"),
			words_tracked: format!("{bot_name}_words_tracked"),
			word_reactions: format!("{bot_name}_word_reactions"),
			periodic_dead_chat: format!("{bot_name}_periodic_dead_chat_total"),
			periodic_waifu: format!("{bot_name}_periodic_waifu_total"),
			bot_pings: format!("{bot_name}_bot_pings_total"),
			floppaganda: format!("{bot_name}_floppaganda_total"),
			message_previews: format!("{bot_name}_message_previews_total"),
			user_afks: format!("{bot_name}_user_afks_total"),
			custom_user_pings: format!("{bot_name}_custom_user_pings_total"),
			prefix_errors: format!("{bot_name}_prefix_errors_total"),
			message_errors: format!("{bot_name}_message_errors_total"),
			new_guild_errors: format!("{bot_name}_new_guild_errors_total"),
			member_addition_errors: format!("{bot_name}_member_addition_errors_total"),
			ready_errors: format!("{bot_name}_ready_errors_total"),
			messages_deleted_errors: format!("{bot_name}_messages_deleted_errors_total"),
			playback_errors: format!("{bot_name}_playback_errors_total"),
			feedback_modal_errors: format!("{bot_name}_feedback_modal_errors_total"),
			feedback_reply_errors: format!("{bot_name}_feedback_reply_errors_total"),
			bot_permissions_errors: format!("{bot_name}_bot_permissions_errors_total"),
			user_permissions_errors: format!("{bot_name}_user_permissions_errors_total"),
			chatbot_errors: format!("{bot_name}_chatbot_errors"),
			music_queue_errors: format!("{bot_name}_music_queue_errors"),
			waifu_errors: format!("{bot_name}_waifu_errors"),
			lyrics_errors: format!("{bot_name}_lyrics_errors"),
			gifs_errors: format!("{bot_name}_gifs_errors"),
		}
	}

	pub fn describe_all(&self) {
		describe_counter!(self.commands.clone(), "Counter for commands");
		describe_counter!(self.command_errors.clone(), "Counter for command errors");
		describe_counter!(
			self.channel_triggers.clone(),
			"Counter for channel triggers"
		);
		describe_counter!(self.words_tracked.clone(), "Counter for words tracked");
		describe_counter!(self.word_reactions.clone(), "Counter for word reactions");
		describe_counter!(
			self.periodic_dead_chat.clone(),
			"Counter for periodic dead chat"
		);
		describe_counter!(self.periodic_waifu.clone(), "Counter for periodic waifu");
		describe_counter!(self.bot_pings.clone(), "Counter for bot pings");
		describe_counter!(self.floppaganda.clone(), "Counter for floppaganda");
		describe_counter!(
			self.message_previews.clone(),
			"Counter for message previews"
		);
		describe_counter!(self.user_afks.clone(), "Counter for user afks");
		describe_counter!(
			self.custom_user_pings.clone(),
			"Counter for custom user pings"
		);
		describe_counter!(self.prefix_errors.clone(), "Counter for prefix errors");
		describe_counter!(self.message_errors.clone(), "Counter for message errors");
		describe_counter!(
			self.new_guild_errors.clone(),
			"Counter for new guild errors"
		);
		describe_counter!(
			self.member_addition_errors.clone(),
			"Counter for member addition errors"
		);
		describe_counter!(self.ready_errors.clone(), "Counter for ready errors");
		describe_counter!(
			self.messages_deleted_errors.clone(),
			"Counter for message deletion errors"
		);
		describe_counter!(self.playback_errors.clone(), "Counter for playback errors");
		describe_counter!(
			self.feedback_modal_errors.clone(),
			"Counter for feedback modal errors"
		);
		describe_counter!(
			self.feedback_reply_errors.clone(),
			"Counter for feedback reply errors"
		);
		describe_counter!(
			self.bot_permissions_errors.clone(),
			"Counter for bot permissions errors"
		);
		describe_counter!(
			self.user_permissions_errors.clone(),
			"Counter for user permissions errors"
		);
		describe_counter!(self.chatbot_errors.clone(), "Counter for chatbot errors");
		describe_counter!(
			self.music_queue_errors.clone(),
			"Counter for music queue errors"
		);
		describe_counter!(self.waifu_errors.clone(), "Counter for waifu errors");
		describe_counter!(self.lyrics_errors.clone(), "Counter for lyrics errors");
		describe_counter!(self.gifs_errors.clone(), "Counter for gifs errors");
	}
}
