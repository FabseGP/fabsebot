use std::{
	sync::Arc,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose};
use fabsebot_core::{
	config::{
		constants::COLOUR_RED,
		settings::UserSettings,
		types::{Error, HTTP_CLIENT, RNG, SContext},
	},
	utils::helpers::{get_gifs, get_waifu},
};
use fabsebot_db::guild::{EmojiReactions, GuildData, GuildSettings, WordReactions, WordTracking};
use poise::CreateReply;
use serde::Serialize;
use serenity::{
	all::{
		ButtonStyle, Channel, ComponentInteractionCollector, ComponentInteractionDataKind,
		CreateActionRow, CreateButton, CreateComponent, CreateEmbed, CreateInteractionResponse,
		CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, GuildId,
	},
	futures::StreamExt as _,
};
use sqlx::query;
use tracing::warn;

async fn reset_server_settings(ctx: SContext<'_>, guild_id: GuildId) -> Result<(), Error> {
	let guild_id_i64 = i64::from(guild_id);
	let tx = ctx
		.data()
		.db
		.begin()
		.await
		.context("Failed to acquire savepoint")?;
	if let Some(guild_data) = ctx.data().guild_data.get(&guild_id) {
		guild_data.reset(guild_id_i64, tx).await?;
	}
	ctx.data().guild_data.insert(
		guild_id,
		Arc::new(GuildData {
			settings: GuildSettings {
				guild_id: guild_id_i64,
				..Default::default()
			},
			..Default::default()
		}),
	);

	Ok(())
}

async fn configure_channels(
	music_channel_opt: Option<Channel>,
	spoiler_channel_opt: Option<Channel>,
	quote_channel_opt: Option<Channel>,
	chatbot_channel_opt: Option<Channel>,
	waifu_channel_opt: Option<(Channel, i64)>,
	dead_chat_gifs_opt: Option<(Channel, i64)>,
	ctx: SContext<'_>,
	guild_id: GuildId,
) -> Result<(), Error> {
	let mut modified_settings = ctx
		.data()
		.guild_data
		.get(&guild_id)
		.get_or_insert_default()
		.as_ref()
		.clone();
	let guild_id_i64 = i64::from(guild_id);
	let system_time = if let Ok(system_time) = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|t| t.as_secs())
		&& let Ok(now_timestamp_i64) = i64::try_from(system_time)
	{
		now_timestamp_i64
	} else {
		warn!("Failed to get system time");
		return Ok(());
	};

	let conn = &mut *ctx.data().db.acquire().await?;
	if let Some(music_channel) = music_channel_opt {
		let music_channel_id_i64 = i64::from(music_channel.id());
		modified_settings
			.settings
			.set_music_channel(guild_id_i64, music_channel_id_i64, conn)
			.await?;
		music_channel
			.id()
			.say(
				ctx.http(),
				"Once I'm in a voice channel with /join_voice, I'll start listen to your song \
				 requests!\nMessages prefixed with # will be ignored",
			)
			.await?;
		modified_settings.settings.music_channel = Some(music_channel_id_i64);
	}
	if let Some(spoiler_channel) = spoiler_channel_opt {
		let spoiler_channel_id_i64 = i64::from(spoiler_channel.id());
		modified_settings
			.settings
			.set_spoiler_channel(guild_id_i64, spoiler_channel_id_i64, conn)
			.await?;
		spoiler_channel
			.id()
			.say(
				ctx.http(),
				"Every attachment sent here will now be spoilered",
			)
			.await?;
		modified_settings.settings.spoiler_channel = Some(spoiler_channel_id_i64);
	}
	if let Some(quote_channel) = quote_channel_opt {
		let quote_channel_id_i64 = i64::from(quote_channel.id());
		set_quote_channel(ctx, quote_channel, guild_id_i64, quote_channel_id_i64).await?;
		modified_settings.settings.quotes_channel = Some(quote_channel_id_i64);
	}
	if let Some(chatbot_channel) = chatbot_channel_opt {
		let chatbot_channel_id_i64 = i64::from(chatbot_channel.id());
		set_chatbot_channel(ctx, chatbot_channel, guild_id_i64, chatbot_channel_id_i64).await?;
		modified_settings.settings.ai_chat_channel = Some(chatbot_channel_id_i64);
	}
	if let Some((waifu_channel, waifu_occurrence)) = waifu_channel_opt {
		let waifu_channel_id_i64 = i64::from(waifu_channel.id());
		set_waifu_channel(
			ctx,
			waifu_channel,
			guild_id_i64,
			waifu_channel_id_i64,
			waifu_occurrence,
			system_time,
		)
		.await?;
		modified_settings.settings.waifu_channel = Some(waifu_channel_id_i64);
		modified_settings.settings.waifu_rate = Some(waifu_occurrence);
		modified_settings.settings.last_waifu = Some(system_time);
	}
	if let Some((dead_chat_channel, dead_chat_occurrence)) = dead_chat_gifs_opt {
		let dead_chat_channel_id_i64 = i64::from(dead_chat_channel.id());
		set_dead_chat(
			ctx,
			dead_chat_channel,
			guild_id_i64,
			dead_chat_channel_id_i64,
			dead_chat_occurrence,
			system_time,
		)
		.await?;
		modified_settings.settings.dead_chat_channel = Some(dead_chat_channel_id_i64);
		modified_settings.settings.dead_chat_rate = Some(dead_chat_occurrence);
		modified_settings.settings.last_dead_chat = Some(system_time);
	}
	ctx.data()
		.guild_data
		.insert(guild_id, Arc::new(modified_settings));

	Ok(())
}

enum SelectionState {
	MainMenu,
	SelectingMusicChannel,
	SelectingSpoilerChannel,
	SelectingQuoteChannel,
	SelectingChatbotChannel,
	SelectingWaifuChannel,
	ConfiguringDeadChatGifs,
}

impl SelectionState {
	fn label(&self) -> String {
		match self {
			Self::MainMenu => "main",
			Self::SelectingMusicChannel => "music",
			Self::SelectingSpoilerChannel => "spoiler",
			Self::SelectingQuoteChannel => "quote",
			Self::SelectingChatbotChannel => "chatbot",
			Self::SelectingWaifuChannel => "waifu",
			Self::ConfiguringDeadChatGifs => "dead gifs",
		}
		.to_owned()
	}

	fn description(&self) -> String {
		match self {
			Self::MainMenu => "Time to configure me",
			Self::SelectingMusicChannel => "Select a channel to listen for music requests",
			Self::SelectingSpoilerChannel => "Select a channel to spoiler attachments",
			Self::SelectingQuoteChannel => "Select a channel to redirect quotes too",
			Self::SelectingChatbotChannel => "Select a channel to respond to users as a chatbot",
			Self::SelectingWaifuChannel => "Select a channel to send waifus to every day",
			Self::ConfiguringDeadChatGifs => "Select a channel to send dead gifs to every day",
		}
		.to_owned()
	}
}

/// Configure settings related to channels
#[poise::command(
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn configure_server_settings(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let mut current_state = SelectionState::MainMenu;

		let settings_options = vec![
			CreateSelectMenuOption::new("Chatbot channel", "ch_chan"),
			CreateSelectMenuOption::new("Music channel", "mu_chan"),
			CreateSelectMenuOption::new("Quote channel", "qu_chan"),
			CreateSelectMenuOption::new("Spoiler channel", "sp_chan"),
			CreateSelectMenuOption::new("Waifu channel", "wu_chan"),
			CreateSelectMenuOption::new("Dead chat gifs", "dc_gifs"),
		];

		let settings_menu = CreateSelectMenu::new(
			format!("{}_settings_menu", ctx.id()),
			CreateSelectMenuKind::String {
				options: Cow::from(settings_options),
			},
		)
		.placeholder("Select server setting to configure")
		.min_values(1)
		.max_values(1);

		let settings_component = [
			CreateComponent::ActionRow(CreateActionRow::SelectMenu(settings_menu)),
			CreateComponent::ActionRow(CreateActionRow::Buttons(Cow::from(vec![
				CreateButton::new(format!("{}_c", ctx.id()))
					.label("Confirm changes")
					.style(ButtonStyle::Success),
				CreateButton::new(format!("{}_d", ctx.id()))
					.label("Cancel")
					.style(ButtonStyle::Danger),
				CreateButton::new(format!("{}_r", ctx.id()))
					.label("Reset")
					.style(ButtonStyle::Danger),
			]))),
		];

		let channel_menu = CreateSelectMenu::new(
			format!("{}_channels_menu", ctx.id()),
			CreateSelectMenuKind::Channel {
				channel_types: None,
				default_channels: None,
			},
		)
		.placeholder("Select channel")
		.min_values(1)
		.max_values(1);

		let channels_component = [
			CreateComponent::ActionRow(CreateActionRow::SelectMenu(channel_menu)),
			CreateComponent::ActionRow(CreateActionRow::Buttons(Cow::from(vec![
				CreateButton::new(format!("{}_d", ctx.id()))
					.label("Back")
					.style(ButtonStyle::Secondary),
			]))),
		];

		let message = ctx
			.send(
				CreateReply::default()
					.content(current_state.description())
					.components(&settings_component),
			)
			.await?;

		let ctx_id_copy = ctx.id();

		let mut music_channel_opt = None;
		let mut spoiler_channel_opt = None;
		let mut quote_channel_opt = None;
		let mut chatbot_channel_opt = None;
		let mut waifu_channel_opt = None;
		let mut dead_chat_gifs_opt = None;

		let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
			.timeout(Duration::from_secs(600))
			.filter(move |interaction| {
				interaction
					.data
					.custom_id
					.starts_with(ctx_id_copy.to_string().as_str())
			})
			.stream();

		let mut too_slow = true;

		while let Some(interaction) = collector_stream.next().await {
			interaction
				.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
				.await?;

			if interaction.data.custom_id.ends_with('c') {
				message
					.edit(
						ctx,
						CreateReply::default()
							.content("Server settings set... probably")
							.reply(true)
							.components(&[]),
					)
					.await?;
				configure_channels(
					music_channel_opt,
					spoiler_channel_opt,
					quote_channel_opt,
					chatbot_channel_opt,
					waifu_channel_opt,
					dead_chat_gifs_opt,
					ctx,
					guild_id,
				)
				.await?;
				too_slow = false;
				break;
			} else if interaction.data.custom_id.ends_with('d') {
				message
					.edit(
						ctx,
						CreateReply::default()
							.content("Server settings not changed... coward")
							.reply(true)
							.components(&[]),
					)
					.await?;
				too_slow = false;
				break;
			} else if interaction.data.custom_id.ends_with('r') {
				reset_server_settings(ctx, guild_id).await?;
				message
					.edit(
						ctx,
						CreateReply::default()
							.content("Server settings reset... probably")
							.reply(true)
							.components(&[]),
					)
					.await?;
				too_slow = false;
				break;
			}

			match &interaction.data.kind {
				ComponentInteractionDataKind::StringSelect { values } => {
					let Some(menu_choice) = values.first() else {
						continue;
					};
					current_state = if menu_choice == "mu_chan" {
						SelectionState::SelectingMusicChannel
					} else if menu_choice == "sp_chan" {
						SelectionState::SelectingSpoilerChannel
					} else if menu_choice == "qu_chan" {
						SelectionState::SelectingQuoteChannel
					} else if menu_choice == "ch_chan" {
						SelectionState::SelectingChatbotChannel
					} else if menu_choice == "wu_chan" {
						SelectionState::SelectingWaifuChannel
					} else if menu_choice == "dc_gifs" {
						SelectionState::ConfiguringDeadChatGifs
					} else {
						continue;
					};
					message
						.edit(
							ctx,
							CreateReply::default()
								.content(current_state.description())
								.components(&channels_component),
						)
						.await?;
				}
				ComponentInteractionDataKind::ChannelSelect { values } => {
					let Some(channel_id) = values.first() else {
						continue;
					};

					let channel_name = if let Ok(channel) = channel_id
						.widen()
						.to_channel(ctx.http(), ctx.guild_id())
						.await
					{
						match current_state {
							SelectionState::SelectingMusicChannel => {
								music_channel_opt = Some(channel.clone());
							}
							SelectionState::SelectingSpoilerChannel => {
								spoiler_channel_opt = Some(channel.clone());
							}
							SelectionState::SelectingQuoteChannel => {
								quote_channel_opt = Some(channel.clone());
							}
							SelectionState::SelectingChatbotChannel => {
								chatbot_channel_opt = Some(channel.clone());
							}
							SelectionState::SelectingWaifuChannel => {
								waifu_channel_opt = Some((channel.clone(), 3600 * 24));
							}
							SelectionState::ConfiguringDeadChatGifs => {
								dead_chat_gifs_opt = Some((channel.clone(), 3600 * 24));
							}
							SelectionState::MainMenu => {
								continue;
							}
						}
						if let Some(guild_channel) = channel.guild() {
							guild_channel.base.name.into_string()
						} else {
							channel_id.to_string()
						}
					} else {
						channel_id.to_string()
					};

					message
						.edit(
							ctx,
							CreateReply::default()
								.content(format!(
									"\"{channel_name}\" chosen as {} channel",
									current_state.label()
								))
								.components(&settings_component),
						)
						.await?;
					current_state = SelectionState::MainMenu;
				}
				_ => {}
			}
		}
		if too_slow {
			message
				.edit(
					ctx,
					CreateReply::default()
						.content("You were too slow")
						.components(&[]),
				)
				.await?;
		}
	}
	Ok(())
}

/// To reset or not to reset the user, that's the question
#[poise::command(prefix_command, slash_command)]
pub async fn reset_user_settings(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		ctx.send(
			CreateReply::default()
				.content("User settings resetted... probably")
				.ephemeral(true),
		)
		.await?;
		query!(
			"UPDATE user_settings
            SET chatbot_role = NULL,
                chatbot_internet_search = NULL,
                chatbot_temperature = NULL,
                chatbot_top_p = NULL,
                chatbot_top_k = NULL,
                chatbot_repetition_penalty = NULL,
                chatbot_frequency_penalty = NULL,
                chatbot_presence_penalty = NULL,
                afk = FALSE,
                afk_reason = NULL,
                pinged_links = NULL,
                ping_content = NULL,
                ping_media = NULL
            WHERE guild_id = $1
            AND user_id = $2",
			i64::from(guild_id),
			i64::from(ctx.author().id)
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		let mut modified_settings = ctx
			.data()
			.user_settings
			.get(&guild_id)
			.unwrap_or_default()
			.as_ref()
			.clone();
		modified_settings.insert(
			ctx.author().id,
			UserSettings {
				guild_id: i64::from(guild_id),
				user_id: i64::from(ctx.author().id),
				..Default::default()
			},
		);
		ctx.data()
			.user_settings
			.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

/// When you want to escape discord
#[poise::command(slash_command)]
pub async fn set_afk(
	ctx: SContext<'_>,
	#[description = "Reason for afk"] reason: Option<String>,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let guild_id_i64 = i64::from(guild_id);
		let user_id_i64 = i64::from(ctx.author().id);
		query!(
			"INSERT INTO user_settings (guild_id, user_id, afk, afk_reason, pinged_links)
            VALUES ($1, $2, TRUE, $3, NULL)
            ON CONFLICT(guild_id, user_id)
            DO UPDATE SET
                afk = TRUE,
                afk_reason = $3,
                pinged_links = NULL",
			guild_id_i64,
			user_id_i64,
			reason,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		let embed_reason = reason
			.as_deref()
			.unwrap_or("Didn't renew life subscription");
		let user_name = ctx.author().display_name();
		ctx.send(
			CreateReply::default()
				.embed(
					CreateEmbed::default()
						.title(format!("{user_name} killed!"))
						.description(format!("Reason: {embed_reason}"))
						.thumbnail(ctx.author().avatar_url().unwrap_or_else(|| {
							ctx.author()
								.static_avatar_url()
								.unwrap_or_else(|| ctx.author().default_avatar_url())
						}))
						.color(COLOUR_RED),
				)
				.reply(true),
		)
		.await?;
		let mut modified_settings = ctx
			.data()
			.user_settings
			.get(&guild_id)
			.unwrap_or_default()
			.as_ref()
			.clone();
		if let Some(user_settings) = modified_settings.get_mut(&ctx.author().id) {
			user_settings.afk = true;
			user_settings.afk_reason = reason;
		} else {
			modified_settings.insert(
				ctx.author().id,
				UserSettings {
					guild_id: guild_id_i64,
					user_id: user_id_i64,
					afk: true,
					afk_reason: reason,
					..Default::default()
				},
			);
		}
		ctx.data()
			.user_settings
			.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

async fn set_chatbot_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
) -> Result<(), Error> {
	query!(
		"INSERT INTO guild_settings (guild_id, ai_chat_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                ai_chat_channel = $2",
		guild_id_i64,
		channel_id_i64,
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;
	channel
		.id()
		.say(
			ctx.http(),
			"Roses are red, violets are blue, I'm a human behind a screen",
		)
		.await?;

	Ok(())
}

/// Configure the chatbot to your preferences; an empty field forces the default
/// value
#[poise::command(slash_command)]
pub async fn set_chatbot_options(
	ctx: SContext<'_>,
	#[description = "The role the bot should take; if not set, then default role"] role: Option<
		String,
	>,
	#[description = "Enable bot to search online every message sent"] internet_search: Option<bool>,
	#[description = "Higher values produce more random results; 0 - 5 (1.1 default)"]
	temperature: Option<f32>,
	#[description = "Higher values = more creative, but less predictable; 0 - 1 (0.9 default)"]
	top_p: Option<f32>,
	#[description = "Higher values produces more varied word choices; 0 - 50 (45 default)"]
	top_k: Option<i32>,
	#[description = "Higher values forces more different phrases; 0 - 2 (1.2 default)"]
	repetition_penalty: Option<f32>,
	#[description = "Higher values avoids reusing the same words; 0 - 1 (0.5 default)"]
	frequency_penalty: Option<f32>,
	#[description = "Higher values = more new topics, but less related; 0 - 1 (0.5 default)"]
	presence_penalty: Option<f32>,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		if temperature.is_some_and(|temp| !(0.0..=5.0).contains(&temp))
			|| top_p.is_some_and(|p| !(0.0..=1.0).contains(&p))
			|| top_k.is_some_and(|k| !(0..=50).contains(&k))
			|| repetition_penalty.is_some_and(|rp| !(0.0..=2.0).contains(&rp))
			|| frequency_penalty.is_some_and(|fp| !(0.0..=2.0).contains(&fp))
			|| presence_penalty.is_some_and(|pp| !(0.0..=2.0).contains(&pp))
		{
			ctx.send(
				CreateReply::default()
					.content("Bruh, invalid values for the parameters!")
					.ephemeral(true),
			)
			.await?;
		}
		let guild_id_i64 = i64::from(guild_id);
		let user_id_i64 = i64::from(ctx.author().id);
		query!(
			"INSERT INTO user_settings 
            (guild_id, user_id, chatbot_role, chatbot_internet_search, chatbot_temperature, \
			 chatbot_top_p, chatbot_top_k, chatbot_repetition_penalty, chatbot_frequency_penalty, \
			 chatbot_presence_penalty)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT(guild_id, user_id)
            DO UPDATE SET
                chatbot_role = $3,
                chatbot_internet_search = $4,
                chatbot_temperature = $5,
                chatbot_top_p = $6,
                chatbot_top_k = $7,
                chatbot_repetition_penalty = $8,
                chatbot_frequency_penalty = $9,
                chatbot_presence_penalty = $10",
			guild_id_i64,
			user_id_i64,
			role,
			internet_search,
			temperature,
			top_p,
			top_k,
			repetition_penalty,
			frequency_penalty,
			presence_penalty
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content("Options for chatbot set... probably")
				.ephemeral(true),
		)
		.await?;
		let mut modified_settings = ctx
			.data()
			.user_settings
			.get(&guild_id)
			.unwrap_or_default()
			.as_ref()
			.clone();
		if let Some(user_settings) = modified_settings.get_mut(&ctx.author().id) {
			user_settings.chatbot_role = role;
			user_settings.chatbot_internet_search = internet_search;
			user_settings.chatbot_temperature = temperature;
			user_settings.chatbot_top_p = top_p;
			user_settings.chatbot_top_k = top_k;
			user_settings.chatbot_repetition_penalty = repetition_penalty;
			user_settings.chatbot_frequency_penalty = frequency_penalty;
			user_settings.chatbot_presence_penalty = presence_penalty;
		} else {
			modified_settings.insert(
				ctx.author().id,
				UserSettings {
					guild_id: guild_id_i64,
					user_id: user_id_i64,
					chatbot_role: role,
					chatbot_internet_search: internet_search,
					chatbot_temperature: temperature,
					chatbot_top_p: top_p,
					chatbot_top_k: top_k,
					chatbot_repetition_penalty: repetition_penalty,
					chatbot_frequency_penalty: frequency_penalty,
					chatbot_presence_penalty: presence_penalty,
					..Default::default()
				},
			);
		}
		ctx.data()
			.user_settings
			.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

async fn set_dead_chat(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
	occurrence: i64,
	system_time: i64,
) -> Result<(), Error> {
	query!(
		"INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, last_dead_chat)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                dead_chat_rate = $2, 
                dead_chat_channel = $3,
                last_dead_chat = $4",
		guild_id_i64,
		occurrence,
		channel_id_i64,
		system_time
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;
	let gifs = get_gifs("dead chat".to_owned()).await;
	let index = RNG.lock().await.usize(..gifs.len());
	if let Some(gif) = gifs.get(index).map(|g| g.0.clone()) {
		channel.id().say(ctx.http(), gif).await?;
	}
	Ok(())
}

#[derive(Serialize)]
struct CreateApplicationEmoji<'a> {
	name: &'a str,
	image: &'a str,
}

/// Configure content to react to with a certain emoji
#[poise::command(slash_command)]
pub async fn set_emoji_react(
	ctx: SContext<'_>,
	#[description = "Word/sentence to react to"] content: String,
	#[description = "Name of emoji to react with"] emoji_name: String,
	#[description = "Image/gif for emoji if not in current server"] media: Option<String>,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let guild_id_i64 = i64::from(guild_id);
		let emoji_opt = match ctx.guild() {
			Some(guild) => guild
				.emojis
				.iter()
				.find(|emoji| emoji.name == emoji_name)
				.cloned(),
			_ => None,
		};
		if let Some(emoji) = emoji_opt {
			let emoji_id_i64 = i64::from(emoji.id);
			query!(
				"INSERT INTO guild_emoji_reaction (guild_id, emoji_id, guild_emoji, \
				 content_reaction)
                    VALUES ($1, $2, TRUE, $3)
                    ON CONFLICT(guild_id, emoji_id)
                    DO UPDATE SET
                        emoji_id = $2,
                        guild_emoji = TRUE,
                        content_reaction = $3",
				guild_id_i64,
				emoji_id_i64,
				content,
			)
			.execute(&mut *ctx.data().db.acquire().await?)
			.await?;
			ctx.send(
				CreateReply::default()
					.content(format!(
						"Every time {content} is sent, {} will be reacted with... probably",
						emoji.name
					))
					.ephemeral(true),
			)
			.await?;
			let mut modified_settings = ctx
				.data()
				.guild_data
				.get(&guild_id)
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.emoji_reactions.insert(EmojiReactions {
				guild_id: guild_id_i64,
				emoji_id: emoji_id_i64,
				guild_emoji: true,
				content_reaction: content,
			});
			ctx.data()
				.guild_data
				.insert(guild_id, Arc::new(modified_settings));
		} else if let Some(emoji_media) = media {
			let content_type_opt = if emoji_media.starts_with("https") {
				ctx.defer().await?;
				let response = HTTP_CLIENT.head(&emoji_media).send().await?;
				let content_type = response
					.headers()
					.get("content-type")
					.and_then(|ct| ct.to_str().ok())
					.unwrap_or("image/png")
					.to_string();
				(content_type.starts_with("image/") || content_type == "application/gif")
					.then_some(content_type)
			} else {
				None
			};
			if let Some(content_type) = content_type_opt {
				let image_bytes = HTTP_CLIENT.get(&emoji_media).send().await?.bytes().await?;
				let base64_str = general_purpose::STANDARD.encode(&image_bytes);
				let image_data = format!("data:{};base64,{}", &content_type, base64_str);
				let params = CreateApplicationEmoji {
					name: &emoji_name,
					image: &image_data,
				};
				let emoji = match ctx.http().create_application_emoji(&params).await {
					Ok(result) => result,
					Err(e) => {
						ctx.send(
							CreateReply::default()
								.content(format!("No can do, Discord gave this error: {e}"))
								.ephemeral(true),
						)
						.await?;
						return Ok(());
					}
				};
				let emoji_id_i64 = i64::from(emoji.id);
				query!(
					"INSERT INTO guild_emoji_reaction (guild_id, emoji_id, guild_emoji, \
					 content_reaction)
                    VALUES ($1, $2, FALSE, $3)
                    ON CONFLICT(guild_id, emoji_id)
                    DO UPDATE SET
                        emoji_id = $2,
                        guild_emoji = FALSE,
                        content_reaction = $3",
					guild_id_i64,
					emoji_id_i64,
					content,
				)
				.execute(&mut *ctx.data().db.acquire().await?)
				.await?;
				ctx.send(
					CreateReply::default()
						.content(format!(
							"Every time {content} is sent, {} will be reacted with... probably",
							emoji.name
						))
						.ephemeral(true),
				)
				.await?;
				let mut modified_settings = ctx
					.data()
					.guild_data
					.get(&guild_id)
					.get_or_insert_default()
					.as_ref()
					.clone();
				modified_settings.emoji_reactions.insert(EmojiReactions {
					guild_id: guild_id_i64,
					emoji_id: emoji_id_i64,
					guild_emoji: false,
					content_reaction: content,
				});
				ctx.data()
					.guild_data
					.insert(guild_id, Arc::new(modified_settings));
			} else {
				ctx.send(
					CreateReply::default()
						.content("Bruh, invalid media was given!")
						.ephemeral(true),
				)
				.await?;
			}
		} else {
			ctx.send(
				CreateReply::default()
					.content("Bruh, the emoji doesn't exist + no media was given!")
					.ephemeral(true),
			)
			.await?;
		}
	}
	Ok(())
}

async fn set_music_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
) -> Result<(), Error> {
	channel
		.id()
		.say(
			ctx.http(),
			"Once I'm in a voice channel with /join_voice, I'll start listen to your song \
			 requests!\nMessages prefixed with # will be ignored",
		)
		.await?;
	Ok(())
}

/// Configure which prefix to use for commands
#[poise::command(
	prefix_command,
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_prefix(
	ctx: SContext<'_>,
	#[description = "Character(s) to use as prefix for commands"]
	#[rest]
	characters: String,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		query!(
			"INSERT INTO guild_settings (guild_id, prefix)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                prefix = $2",
			i64::from(guild_id),
			characters,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content(format!(
					"{characters} set as the prefix for commands... probably"
				))
				.ephemeral(true),
		)
		.await?;
		let mut modified_settings = ctx
			.data()
			.guild_data
			.get(&guild_id)
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.settings.prefix = Some(characters);
		ctx.data()
			.guild_data
			.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

async fn set_quote_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
) -> Result<(), Error> {
	query!(
		"INSERT INTO guild_settings (guild_id, quotes_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                quotes_channel = $2",
		guild_id_i64,
		channel_id_i64,
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;
	channel
		.id()
		.say(
			ctx.http(),
			"Every quoted message will be redirected here too",
		)
		.await?;

	Ok(())
}

async fn set_spoiler_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
) -> Result<(), Error> {
	query!(
		"INSERT INTO guild_settings (guild_id, spoiler_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                spoiler_channel = $2",
		guild_id_i64,
		channel_id_i64
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;
	channel
		.id()
		.say(
			ctx.http(),
			"Every attachment sent here will now be spoilered",
		)
		.await?;

	Ok(())
}

/// Configure custom embed sent on user ping
#[poise::command(slash_command)]
pub async fn set_user_ping(
	ctx: SContext<'_>,
	#[description = "Message to send"] content: String,
	#[description = "Image/gif to send; write waifu for a random waifu or !gif query for a gif of \
	                 query"]
	media: Option<String>,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let valid = if let Some(user_media) = &media {
			if user_media.starts_with("https") {
				ctx.defer().await?;
				HTTP_CLIENT
					.head(user_media)
					.send()
					.await
					.is_ok_and(|response| {
						response
							.headers()
							.get("content-type")
							.and_then(|ct| ct.to_str().ok())
							.is_some_and(|ct| ct.starts_with("image/") || ct == "application/gif")
					})
			} else if let Some(media_stripped) = user_media.strip_prefix("!gif") {
				!media_stripped.is_empty()
			} else {
				user_media == "waifu"
			}
		} else {
			true
		};
		if valid {
			let guild_id_i64 = i64::from(guild_id);
			let user_id_i64 = i64::from(ctx.author().id);
			query!(
				"INSERT INTO user_settings (guild_id, user_id, ping_content, ping_media)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT(guild_id, user_id)
                DO UPDATE SET 
                    ping_content = $3, 
                    ping_media = $4",
				guild_id_i64,
				user_id_i64,
				content,
				media,
			)
			.execute(&mut *ctx.data().db.acquire().await?)
			.await?;
			ctx.send(
				CreateReply::default()
					.content("Custom user ping created... probably")
					.ephemeral(true),
			)
			.await?;
			let mut modified_settings = ctx
				.data()
				.user_settings
				.get(&guild_id)
				.unwrap_or_default()
				.as_ref()
				.clone();
			if let Some(user_settings) = modified_settings.get_mut(&ctx.author().id) {
				user_settings.ping_content = Some(content);
				user_settings.ping_media = media;
			} else {
				modified_settings.insert(
					ctx.author().id,
					UserSettings {
						guild_id: guild_id_i64,
						user_id: user_id_i64,
						ping_content: Some(content),
						ping_media: media,
						..Default::default()
					},
				);
			}
			ctx.data()
				.user_settings
				.insert(guild_id, Arc::new(modified_settings));
		} else {
			ctx.send(
				CreateReply::default()
					.content("Invalid media given... really bro?")
					.ephemeral(true),
			)
			.await?;
		}
	}
	Ok(())
}

async fn set_waifu_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
	occurrence: i64,
	system_time: i64,
) -> Result<(), Error> {
	query!(
		"INSERT INTO guild_settings (guild_id, waifu_channel, waifu_rate, last_waifu)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                waifu_channel = $2,
                waifu_rate = $3,
                last_waifu = $4",
		guild_id_i64,
		channel_id_i64,
		occurrence,
		system_time
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;
	channel.id().say(ctx.http(), get_waifu().await).await?;
	Ok(())
}

/// Configure words to react to with custom content
#[poise::command(slash_command)]
pub async fn set_word_react(
	ctx: SContext<'_>,
	#[description = "Word to react to"] word: String,
	#[description = "Text to send on react"] content: String,
	#[description = "Media to send on react; use !gif query for a random gif of query"]
	media: Option<String>,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let valid = if let Some(user_media) = &media {
			if user_media.starts_with("https") {
				ctx.defer().await?;
				HTTP_CLIENT
					.head(user_media)
					.send()
					.await
					.is_ok_and(|response| {
						response
							.headers()
							.get("content-type")
							.and_then(|ct| ct.to_str().ok())
							.is_some_and(|ct| ct.starts_with("image/") || ct == "application/gif")
					})
			} else if let Some(media_stripped) = user_media.strip_prefix("!gif") {
				!media_stripped.is_empty()
			} else {
				false
			}
		} else {
			true
		};
		if valid {
			let guild_id_i64 = i64::from(guild_id);
			query!(
				"INSERT INTO guild_word_reaction (guild_id, word, content, media)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT(guild_id, word)
                DO UPDATE SET
                    word = $2,
                    content = $3,
                    media = $4",
				guild_id_i64,
				word,
				content,
				media
			)
			.execute(&mut *ctx.data().db.acquire().await?)
			.await?;
			ctx.send(
				CreateReply::default()
					.content(format!("{word} will be reacted to from now on... probably"))
					.ephemeral(true),
			)
			.await?;
			let mut modified_settings = ctx
				.data()
				.guild_data
				.get(&guild_id)
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.word_reactions.insert(WordReactions {
				guild_id: guild_id_i64,
				word,
				content,
				media,
			});
			ctx.data()
				.guild_data
				.insert(guild_id, Arc::new(modified_settings));
		} else {
			ctx.send(
				CreateReply::default()
					.content("Invalid media given... really bro?")
					.ephemeral(true),
			)
			.await?;
		}
	}
	Ok(())
}

/// Configure words to track count of
#[poise::command(slash_command)]
pub async fn set_word_track(
	ctx: SContext<'_>,
	#[description = "Word to track count of"] word: String,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let guild_id_i64 = i64::from(guild_id);
		query!(
			"INSERT INTO guild_word_tracking (guild_id, word)
            VALUES ($1, $2)
            ON CONFLICT(guild_id, word)
            DO UPDATE SET
                word = $2, 
                count = 0",
			guild_id_i64,
			word,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content(format!("The count of {word} will be tracked... probably"))
				.ephemeral(true),
		)
		.await?;
		let mut modified_settings = ctx
			.data()
			.guild_data
			.get(&guild_id)
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.word_tracking.insert(WordTracking {
			guild_id: guild_id_i64,
			word,
			count: 0,
		});
		ctx.data()
			.guild_data
			.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}
