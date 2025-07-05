use std::sync::Arc;

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose};
use poise::CreateReply;
use serde::Serialize;
use serenity::all::{Channel, CreateEmbed};
use sqlx::query;

use crate::config::{
	constants::COLOUR_RED,
	settings::{EmojiReactions, GuildSettings, UserSettings, WordReactions, WordTracking},
	types::{Error, GuildData, HTTP_CLIENT, SContext},
};

/// To reset or not to reset the server, that's the question
#[poise::command(
	prefix_command,
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn reset_server_settings(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let guild_id_i64 = i64::from(guild_id);
		let mut tx = ctx
			.data()
			.db
			.begin()
			.await
			.context("Failed to acquire savepoint")?;
		ctx.send(
			CreateReply::default()
				.content("Server settings resetted... probably")
				.ephemeral(true),
		)
		.await?;
		query!(
			"UPDATE guild_settings
            SET dead_chat_rate = NULL,
                dead_chat_channel = NULL,
                quotes_channel = NULL,
                spoiler_channel = NULL,
                prefix = NULL,
                ai_chat_channel = NULL,
                global_chat_channel = NULL,
                global_chat = FALSE,
                global_music = FALSE,
                global_call = FALSE
            WHERE guild_id = $1",
			guild_id_i64
		)
		.execute(&mut *tx)
		.await?;
		query!(
			"DELETE FROM guild_word_tracking
            WHERE guild_id = $1",
			guild_id_i64
		)
		.execute(&mut *tx)
		.await?;
		query!(
			"DELETE FROM guild_word_reaction
            WHERE guild_id = $1",
			guild_id_i64
		)
		.execute(&mut *tx)
		.await?;
		tx.commit()
			.await
			.context("Failed to commit sql-transaction")?;
		ctx.data().guild_data.lock().await.insert(
			guild_id,
			Arc::new(GuildData {
				settings: GuildSettings {
					guild_id: guild_id_i64,
					..Default::default()
				},
				..Default::default()
			}),
		);
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
		let ctx_data = ctx.data();
		let user_settings_lock = ctx_data.user_settings.lock().await;
		let mut guild_user_settings_opt = user_settings_lock.get(&guild_id);
		let mut modified_settings = guild_user_settings_opt
			.get_or_insert_default()
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
		user_settings_lock.insert(guild_id, Arc::new(modified_settings));
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
		let ctx_data = ctx.data();
		let user_settings_lock = ctx_data.user_settings.lock().await;
		let current_settings = user_settings_lock.get(&guild_id).unwrap_or_default();
		let mut modified_settings = current_settings.as_ref().clone();
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
		user_settings_lock.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

/// When you need ai in your life
#[poise::command(
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_chatbot_channel(
	ctx: SContext<'_>,
	#[description = "Channel to act as chatbot in"] channel: Channel,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let channel_id_i64 = i64::from(channel.id());
		query!(
			"INSERT INTO guild_settings (guild_id, ai_chat_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                ai_chat_channel = $2",
			i64::from(guild_id),
			channel_id_i64,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content(format!("AI-sama is alive in {channel}... probably"))
				.ephemeral(true),
		)
		.await?;
		let ctx_data = ctx.data();
		let guild_settings_lock = ctx_data.guild_data.lock().await;
		let mut current_settings_opt = guild_settings_lock.get(&guild_id);
		let mut modified_settings = current_settings_opt
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.settings.ai_chat_channel = Some(channel_id_i64);
		guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
	}
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
		let ctx_data = ctx.data();
		let user_settings_lock = ctx_data.user_settings.lock().await;
		let current_settings = user_settings_lock.get(&guild_id).unwrap_or_default();
		let mut modified_settings = current_settings.as_ref().clone();
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
		user_settings_lock.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

/// Configure the occurence of dead chat gifs
#[poise::command(
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_dead_chat(
	ctx: SContext<'_>,
	#[description = "How often (in minutes) a dead chat gif should be sent"] occurrence: i64,
	#[description = "Channel to send dead chat gifs to"] channel: Channel,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let channel_id_i64 = i64::from(channel.id());
		query!(
			"INSERT INTO guild_settings (guild_id, dead_chat_rate, dead_chat_channel)
            VALUES ($1, $2, $3)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                dead_chat_rate = $2, 
                dead_chat_channel = $3",
			i64::from(guild_id),
			occurrence,
			channel_id_i64,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content(format!(
					"Dead chat gifs will only be sent every {occurrence} minute(s) in \
					 {channel}... probably",
				))
				.ephemeral(true),
		)
		.await?;
		let ctx_data = ctx.data();
		let guild_settings_lock = ctx_data.guild_data.lock().await;
		let mut current_settings_opt = guild_settings_lock.get(&guild_id);
		let mut modified_settings = current_settings_opt
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.settings.dead_chat_channel = Some(channel_id_i64);
		modified_settings.settings.dead_chat_rate = Some(occurrence);
		guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
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
			let ctx_data = ctx.data();
			let guild_settings_lock = ctx_data.guild_data.lock().await;
			let mut current_settings_opt = guild_settings_lock.get(&guild_id);
			let mut modified_settings = current_settings_opt
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.emoji_reactions.insert(EmojiReactions {
				guild_id: guild_id_i64,
				emoji_id: emoji_id_i64,
				guild_emoji: true,
				content_reaction: content,
			});
			guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
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
				let ctx_data = ctx.data();
				let guild_settings_lock = ctx_data.guild_data.lock().await;
				let mut current_settings_opt = guild_settings_lock.get(&guild_id);
				let mut modified_settings = current_settings_opt
					.get_or_insert_default()
					.as_ref()
					.clone();
				modified_settings.emoji_reactions.insert(EmojiReactions {
					guild_id: guild_id_i64,
					emoji_id: emoji_id_i64,
					guild_emoji: false,
					content_reaction: content,
				});
				guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
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
		let ctx_data = ctx.data();
		let guild_settings_lock = ctx_data.guild_data.lock().await;
		let mut current_settings_opt = guild_settings_lock.get(&guild_id);
		let mut modified_settings = current_settings_opt
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.settings.prefix = Some(characters);
		guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

/// Configure where to send quotes
#[poise::command(
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_quote_channel(
	ctx: SContext<'_>,
	#[description = "Channel to send quoted messages to"] channel: Channel,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let channel_id_i64 = i64::from(channel.id());
		query!(
			"INSERT INTO guild_settings (guild_id, quotes_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                quotes_channel = $2",
			i64::from(guild_id),
			channel_id_i64,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content(format!(
					"Quoted messages will be sent to {channel}... probably"
				))
				.ephemeral(true),
		)
		.await?;
		let ctx_data = ctx.data();
		let guild_settings_lock = ctx_data.guild_data.lock().await;
		let mut current_settings_opt = guild_settings_lock.get(&guild_id);
		let mut modified_settings = current_settings_opt
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.settings.quotes_channel = Some(channel_id_i64);
		guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}

/// Configure a channel to always spoiler messages
#[poise::command(
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn set_spoiler_channel(
	ctx: SContext<'_>,
	#[description = "Channel to send spoilered messages to"] channel: Channel,
) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let channel_id_i64 = i64::from(channel.id());
		query!(
			"INSERT INTO guild_settings (guild_id, spoiler_channel)
            VALUES ($1, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                spoiler_channel = $2",
			i64::from(guild_id),
			channel_id_i64,
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
		ctx.send(
			CreateReply::default()
				.content(format!(
					"Spoilered messages will be sent to {channel}... probably"
				))
				.ephemeral(true),
		)
		.await?;
		let ctx_data = ctx.data();
		let guild_settings_lock = ctx_data.guild_data.lock().await;
		let mut current_settings_opt = guild_settings_lock.get(&guild_id);
		let mut modified_settings = current_settings_opt
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.settings.spoiler_channel = Some(channel_id_i64);
		guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
	}
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
			let ctx_data = ctx.data();
			let user_settings_lock = ctx_data.user_settings.lock().await;
			let current_settings = user_settings_lock.get(&guild_id).unwrap_or_default();
			let mut modified_settings = current_settings.as_ref().clone();
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
			user_settings_lock.insert(guild_id, Arc::new(modified_settings));
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
			let ctx_data = ctx.data();
			let guild_settings_lock = ctx_data.guild_data.lock().await;
			let mut current_settings_opt = guild_settings_lock.get(&guild_id);
			let mut modified_settings = current_settings_opt
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.word_reactions.insert(WordReactions {
				guild_id: guild_id_i64,
				word,
				content,
				media,
			});
			guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
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
		let ctx_data = ctx.data();
		let guild_settings_lock = ctx_data.guild_data.lock().await;
		let mut current_settings_opt = guild_settings_lock.get(&guild_id);
		let mut modified_settings = current_settings_opt
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.word_tracking.insert(WordTracking {
			guild_id: guild_id_i64,
			word,
			count: 0,
		});
		guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
	}
	Ok(())
}
