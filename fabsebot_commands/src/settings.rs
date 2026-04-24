use std::{
	sync::Arc,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose};
use fabsebot_core::{
	config::types::{Error, HTTP_CLIENT, SContext},
	utils::helpers::{
		get_gifs, get_waifu, send_container, separator, text_display, thumbnail_section,
	},
};
use fabsebot_db::guild::{reset_guild, set_music_channel, set_spoiler_channel};
use poise::CreateReply;
use serde::Serialize;
use serenity::{
	all::{
		ButtonStyle, Channel, Colour, ComponentInteractionCollector, ComponentInteractionDataKind,
		CreateActionRow, CreateButton, CreateComponent, CreateContainer, CreateInteractionResponse,
		CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, GuildId,
	},
	futures::StreamExt as _,
};
use sqlx::{PgConnection, query};
use tracing::warn;
use url::Url;

use crate::require_guild_id;

async fn reset_server_settings(ctx: SContext<'_>, guild_id: GuildId) -> Result<(), Error> {
	let guild_id_i64 = i64::from(guild_id);
	let mut tx = ctx
		.data()
		.db
		.begin()
		.await
		.context("Failed to acquire savepoint")?;

	reset_guild(guild_id_i64, &mut tx).await?;

	tx.commit()
		.await
		.context("Failed to commit sql-transaction")?;

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
	let guild_id_i64 = i64::from(guild_id);
	let system_time = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|t| t.as_secs().cast_signed())?;

	let mut tx = ctx
		.data()
		.db
		.begin()
		.await
		.context("Failed to acquire savepoint")?;
	if let Some(music_channel) = music_channel_opt {
		let music_channel_id_i64 = i64::from(music_channel.id());
		set_music_channel(guild_id_i64, music_channel_id_i64, &mut tx).await?;
		music_channel
			.id()
			.say(
				ctx.http(),
				"Once I'm in a voice channel with /join_voice, I'll start listen to your song \
				 requests!\nMessages prefixed with # will be ignored",
			)
			.await?;
	}
	if let Some(spoiler_channel) = spoiler_channel_opt {
		let spoiler_channel_id_i64 = i64::from(spoiler_channel.id());
		set_spoiler_channel(guild_id_i64, spoiler_channel_id_i64, &mut tx).await?;
		spoiler_channel
			.id()
			.say(
				ctx.http(),
				"Every attachment sent here will now be spoilered",
			)
			.await?;
	}
	if let Some(quote_channel) = quote_channel_opt {
		let quote_channel_id_i64 = i64::from(quote_channel.id());
		set_quote_channel(
			ctx,
			quote_channel,
			guild_id_i64,
			quote_channel_id_i64,
			&mut tx,
		)
		.await?;
	}
	if let Some(chatbot_channel) = chatbot_channel_opt {
		let chatbot_channel_id_i64 = i64::from(chatbot_channel.id());
		set_chatbot_channel(
			ctx,
			chatbot_channel,
			guild_id_i64,
			chatbot_channel_id_i64,
			&mut tx,
		)
		.await?;
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
			&mut tx,
		)
		.await?;
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
			&mut tx,
		)
		.await?;
	}

	tx.commit()
		.await
		.context("Failed to commit sql-transaction")?;

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
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS",
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn configure_server_settings(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;

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
		.timeout(Duration::from_mins(10))
		.filter(move |interaction| {
			interaction
				.data
				.custom_id
				.starts_with(ctx_id_copy.to_string().as_str())
		})
		.stream();

	let mut too_slow = true;
	let mut response = "";

	while let Some(interaction) = collector_stream.next().await {
		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;

		if interaction.data.custom_id.ends_with('c') {
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
			response = "Server settings set... probably";
			break;
		} else if interaction.data.custom_id.ends_with('d') {
			response = "Server settings not changed... coward";
			too_slow = false;
			break;
		} else if interaction.data.custom_id.ends_with('r') {
			reset_server_settings(ctx, guild_id).await?;
			response = "Server settings reset... probably";
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
		response = "You were too slow";
	}

	message
		.edit(
			ctx,
			CreateReply::default()
				.content(response)
				.reply(true)
				.components(&[]),
		)
		.await?;

	Ok(())
}

/// To reset or not to reset the user, that's the question
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn reset_user_settings(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	ctx.send(
		CreateReply::default()
			.content("User settings resetted... probably")
			.ephemeral(true),
	)
	.await?;
	query!(
		r#"
		UPDATE user_settings
        SET afk = FALSE, afk_reason = NULL,
        pinged_links = NULL, ping_content = NULL, ping_media = NULL
    	WHERE guild_id = $1 AND user_id = $2
    	"#,
		i64::from(guild_id),
		i64::from(ctx.author().id)
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;

	Ok(())
}

/// When you want to escape discord
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn set_afk(
	ctx: SContext<'_>,
	#[description = "Reason for afk"] reason: Option<String>,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let guild_id_i64 = i64::from(guild_id);
	let user_id_i64 = i64::from(ctx.author().id);
	query!(
		r#"
		INSERT INTO user_settings (guild_id, user_id, afk, afk_reason, pinged_links)
    	VALUES ($1, $2, TRUE, $3, NULL)
        ON CONFLICT (guild_id, user_id)
        DO UPDATE SET afk = TRUE,
        afk_reason = $3,
        pinged_links = NULL
        "#,
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

	let avatar_url = ctx.author().avatar_url().unwrap_or_else(|| {
		ctx.author()
			.static_avatar_url()
			.unwrap_or_else(|| ctx.author().default_avatar_url())
	});

	let title = format!("# {user_name} killed!");
	let thumbnail_section = [thumbnail_section(&title, &avatar_url)];

	let text = format!("**Reason:** {embed_reason}");

	let text_display = text_display(&text);

	let container = CreateContainer::new(&thumbnail_section)
		.add_component(separator())
		.add_component(text_display)
		.accent_colour(Colour::RED);

	send_container(&ctx, container).await?;

	Ok(())
}

async fn set_chatbot_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
	conn: &mut PgConnection,
) -> Result<(), Error> {
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, ai_chat_channel)
        VALUES ($1, $2)
        ON CONFLICT (guild_id)
        DO UPDATE SET ai_chat_channel = $2
        "#,
		guild_id_i64,
		channel_id_i64,
	)
	.execute(conn)
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
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn set_chatbot_options(
	ctx: SContext<'_>,
	#[description = "The role the bot should take; if not set, then default role"] role: Option<
		String,
	>,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let guild_id_i64 = i64::from(guild_id);
	let final_role = role.map(|role| format!("The current user wants you to act as: {role}"));
	query!(
		r#"
		INSERT INTO guild_settings
		(guild_id, chatbot_role)
        VALUES ($1, $2)
        ON CONFLICT (guild_id)
        DO UPDATE SET chatbot_role = $2
        "#,
		guild_id_i64,
		final_role,
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;
	ctx.send(
		CreateReply::default()
			.content(
				"Options for chatbot set... probably\nThe new role will not take effect until the \
				 chat history is cleared using \"clear\" in the chat channel",
			)
			.ephemeral(true),
	)
	.await?;

	Ok(())
}

async fn set_dead_chat(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
	occurrence: i64,
	system_time: i64,
	conn: &mut PgConnection,
) -> Result<(), Error> {
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel, last_dead_chat)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (guild_id)
        DO UPDATE SET dead_chat_rate = $2, 
        dead_chat_channel = $3,
        last_dead_chat = $4
        "#,
		guild_id_i64,
		occurrence,
		channel_id_i64,
		system_time
	)
	.execute(conn)
	.await?;
	let gifs = get_gifs(ctx.serenity_context(), "dead chat").await;
	let index = fastrand::usize(..gifs.len());
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

/// Configure which prefix to use for commands
#[poise::command(
	prefix_command,
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS",
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn set_prefix(
	ctx: SContext<'_>,
	#[description = "Character(s) to use as prefix for commands"]
	#[rest]
	characters: String,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, prefix)
        VALUES ($1, $2)
        ON CONFLICT (guild_id)
        DO UPDATE SET prefix = $2
        "#,
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

	Ok(())
}

async fn set_quote_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
	conn: &mut PgConnection,
) -> Result<(), Error> {
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, quotes_channel)
        VALUES ($1, $2)
        ON CONFLICT (guild_id)
        DO UPDATE SET quotes_channel = $2
        "#,
		guild_id_i64,
		channel_id_i64,
	)
	.execute(conn)
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

/// Configure custom embed sent on user ping
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn set_user_ping(
	ctx: SContext<'_>,
	#[description = "Message to send"] content: String,
	#[description = "Image/gif to send; write waifu for a random waifu or !gif query for a gif of \
	                 query"]
	media: Option<String>,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
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
	let response = if valid {
		let guild_id_i64 = i64::from(guild_id);
		let user_id_i64 = i64::from(ctx.author().id);
		query!(
			r#"
			INSERT INTO user_settings (guild_id, user_id, ping_content, ping_media)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (guild_id, user_id)
            DO UPDATE SET ping_content = $3, 
            ping_media = $4
            "#,
			guild_id_i64,
			user_id_i64,
			content,
			media,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		"Custom user ping created... probably"
	} else {
		"Invalid media given... really bro?"
	};

	ctx.send(CreateReply::default().content(response).ephemeral(true))
		.await?;

	Ok(())
}

async fn set_waifu_channel(
	ctx: SContext<'_>,
	channel: Channel,
	guild_id_i64: i64,
	channel_id_i64: i64,
	occurrence: i64,
	system_time: i64,
	conn: &mut PgConnection,
) -> Result<(), Error> {
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, waifu_channel, waifu_rate, last_waifu)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (guild_id)
        DO UPDATE SET waifu_channel = $2,
        waifu_rate = $3,
        last_waifu = $4
        "#,
		guild_id_i64,
		channel_id_i64,
		occurrence,
		system_time
	)
	.execute(conn)
	.await?;
	channel
		.id()
		.say(ctx.http(), get_waifu(ctx.serenity_context()).await)
		.await?;
	Ok(())
}

/// Configure words to react to with custom content
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn set_word_react(
	ctx: SContext<'_>,
	#[description = "Word to react to"] word: String,
	#[description = "Text to send on react"] content: Option<String>,
	#[description = "Media to send on react; use !gif query for a random gif of query"]
	media: Option<String>,
	#[description = "Name of emoji to react with"] emoji_name: Option<String>,
	#[description = "Link to image/gif for emoji if not in current server"] emoji_media: Option<
		String,
	>,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let mut emoji_id = None;
	let mut guild_emoji = false;
	let valid = if content.is_some() {
		if let Some(user_media) = &media {
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
		}
	} else if let Some(emoji_name) = emoji_name {
		if let Some(guild) = ctx.guild()
			&& guild.emojis.iter().any(|emoji| emoji.name == emoji_name)
		{
			guild_emoji = true;
			true
		} else if let Some(emoji_media) = emoji_media {
			if Url::parse(&emoji_media).is_ok() {
				ctx.defer().await?;
				let response = HTTP_CLIENT.head(&emoji_media).send().await?;
				let content_type = response
					.headers()
					.get("content-type")
					.and_then(|ct| ct.to_str().ok())
					.unwrap_or("image/png")
					.to_string();
				if content_type.starts_with("image/") || content_type == "application/gif" {
					let image_bytes = HTTP_CLIENT.get(&emoji_media).send().await?.bytes().await?;
					let base64_str = general_purpose::STANDARD.encode(&image_bytes);
					let image_data = format!("data:{};base64,{}", &content_type, base64_str);
					let params = CreateApplicationEmoji {
						name: &emoji_name,
						image: &image_data,
					};
					match ctx.http().create_application_emoji(&params).await {
						Ok(http_emoji) => {
							emoji_id = Some(http_emoji.id.get().cast_signed());
							ctx.data()
								.app_emojis
								.insert(http_emoji.id.get(), Arc::new(http_emoji));

							true
						}
						Err(err) => {
							warn!("Failed to get app emojis: {err}");
							false
						}
					}
				} else {
					false
				}
			} else {
				false
			}
		} else {
			false
		}
	} else {
		false
	};
	if valid {
		let guild_id_i64 = i64::from(guild_id);
		query!(
			r#"
			INSERT INTO guild_word_reaction (guild_id, word, content, media, emoji_id, guild_emoji)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (guild_id, word)
            DO UPDATE SET word = $2, content = $3, media = $4, emoji_id = $5, guild_emoji = $6
            "#,
			guild_id_i64,
			word,
			content,
			media,
			emoji_id,
			guild_emoji
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content(format!("{word} will be reacted to from now on... probably"))
				.ephemeral(true),
		)
		.await?;
	} else {
		ctx.send(
			CreateReply::default()
				.content("Invalid media given... really bro?")
				.ephemeral(true),
		)
		.await?;
	}
	Ok(())
}

/// Configure words to track count of
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn set_word_track(
	ctx: SContext<'_>,
	#[description = "Word to track count of"] word: String,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let guild_id_i64 = i64::from(guild_id);
	query!(
		r#"
		INSERT INTO guild_word_tracking (guild_id, word)
        VALUES ($1, $2)
        ON CONFLICT (guild_id, word)
        DO UPDATE SET word = $2, count = 0
        "#,
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

	Ok(())
}
