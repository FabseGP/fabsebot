use std::{
	borrow::Cow,
	collections::{HashMap, HashSet},
	sync::Arc,
};

use anyhow::{Context as _, Result as AResult, bail};
use fabsebot_db::guild::{GuildData, GuildSettings, WordTracking, insert_guild};
use serenity::all::{
	Context as SContext, CreateAllowedMentions, CreateAttachment, CreateEmbed, CreateEmbedAuthor,
	CreateEmbedFooter, CreateMessage, EditMessage, EmojiId, ExecuteWebhook, GenericChannelId,
	GuildId, Message, MessageId, ReactionType, UserId,
};
use songbird::{
	input::{Input, YoutubeDl},
	tracks::Track,
};
use sqlx::{Postgres, Transaction, query};
use tokio::task::spawn;
use tracing::warn;
use uuid::Uuid;
use winnow::Parser as _;

use crate::{
	config::{
		constants::{
			COLOUR_BLUE, COLOUR_ORANGE, COLOUR_RED, COLOUR_YELLOW, DEFAULT_BOT_ROLE,
			FABSEMAN_WEBHOOK_CONTENT, FABSEMAN_WEBHOOK_NAME, FABSEMAN_WEBHOOK_PFP, FLOPPAGANDA_GIF,
			VILBOT_NAME, VILBOT_PFP,
		},
		types::{Data, HTTP_CLIENT, RNG, UTILS_CONFIG},
	},
	utils::{
		ai::ai_chatbot,
		helpers::{discord_message_link, get_gifs, get_waifu, queue_song},
		webhook::{spoiler_message, webhook_find},
	},
};

async fn check_bot_ping(ctx: &SContext, new_message: &Message) -> AResult<()> {
	if new_message.mentions_user_id(ctx.cache.current_user().id)
		&& new_message.referenced_message.is_none()
		&& let Some(utils_config) = UTILS_CONFIG.get()
	{
		new_message
			.channel_id
			.send_message(
				&ctx.http,
				CreateMessage::default()
					.embed(
						CreateEmbed::default()
							.title(&utils_config.bot.ping_message)
							.image(&utils_config.bot.ping_payload)
							.colour(COLOUR_BLUE),
					)
					.reference_message(new_message)
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
			)
			.await?;
	}
	Ok(())
}

async fn emoji_reaction(
	ctx: &SContext,
	new_message: &Message,
	content: &str,
	data: Arc<Data>,
) -> AResult<()> {
	let app_emojis = ctx.get_application_emojis().await?;
	if content == "floppaganda" {
		new_message
			.channel_id
			.send_message(
				&ctx.http,
				CreateMessage::default()
					.content(FLOPPAGANDA_GIF)
					.reference_message(new_message)
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
			)
			.await?;
	} else if content.contains("kurukuru_seseren") {
		if let Some(emoji) = app_emojis
			.iter()
			.find(|emoji| emoji.name == "kurukuru_seseren")
		{
			let emoji_string = if emoji.animated() {
				format!("<a:{}:{}>", &emoji.name, emoji.id)
			} else {
				format!("<:{}:{}>", &emoji.name, emoji.id)
			};
			let count = content.matches("kurukuru_seseren").count();
			let response = emoji_string.repeat(count);
			if let Some(webhook) = webhook_find(
				ctx,
				new_message.guild_id,
				new_message.channel_id,
				data.channel_webhooks.clone(),
			)
			.await
			{
				webhook
					.execute(
						&ctx.http,
						false,
						ExecuteWebhook::default()
							.username(VILBOT_NAME)
							.avatar_url(VILBOT_PFP)
							.content(&response),
					)
					.await?;
			}
		}
	} else if content.contains("fabse") {
		if let Some(emoji) = app_emojis
			.iter()
			.find(|emoji| emoji.name == "fabseman_willbeatu")
		{
			let reaction = ReactionType::Custom {
				animated: emoji.animated(),
				id: emoji.id,
				name: Some(emoji.name.clone()),
			};
			new_message.react(&ctx.http, reaction).await?;
		}
		if (content == "fabse" || content == "fabseman")
			&& let Some(webhook) = webhook_find(
				ctx,
				new_message.guild_id,
				new_message.channel_id,
				data.channel_webhooks.clone(),
			)
			.await
		{
			webhook
				.execute(
					&ctx.http,
					false,
					ExecuteWebhook::default()
						.username(FABSEMAN_WEBHOOK_NAME)
						.avatar_url(FABSEMAN_WEBHOOK_PFP)
						.content(FABSEMAN_WEBHOOK_CONTENT),
				)
				.await?;
		}
	}

	Ok(())
}

async fn spoiler_channel(
	ctx: &SContext,
	new_message: &Message,
	data: Arc<Data>,
	guild_data: Arc<GuildData>,
) -> AResult<()> {
	if let Some(spoiler_channel) = guild_data.settings.spoiler_channel {
		if let Ok(spoiler_channel_u64) = u64::try_from(spoiler_channel) {
			if new_message.channel_id.get() == spoiler_channel_u64 {
				spoiler_message(ctx, new_message, data.channel_webhooks.clone()).await?;
			}
		} else {
			warn!("Failed to convert spoiler channel id to u64");
		}
	}
	Ok(())
}

async fn music_channel(
	ctx: &SContext,
	new_message: &Message,
	data: Arc<Data>,
	guild_data: Arc<GuildData>,
	guild_id: GuildId,
) -> AResult<()> {
	if let Some(music_channel) = guild_data.settings.music_channel
		&& !new_message.content.starts_with('#')
	{
		if let Ok(music_channel_u64) = u64::try_from(music_channel) {
			if new_message.channel_id.get() == music_channel_u64 {
				if let Some(handler_lock) = data.music_manager.get(guild_id) {
					let ctx_clone = ctx.clone();
					let new_message_clone = new_message.clone();
					spawn(async move {
						let src = if new_message_clone.content.starts_with("https") {
							if new_message_clone.content.contains("youtu") {
								YoutubeDl::new(
									HTTP_CLIENT.clone(),
									new_message_clone.content.clone(),
								)
							} else {
								new_message_clone
									.reply(&ctx_clone.http, "Only YouTube-links are supported")
									.await?;
								return Ok(());
							}
						} else {
							YoutubeDl::new_search(
								HTTP_CLIENT.clone(),
								new_message_clone.content.clone(),
							)
						};
						let mut input = Input::from(src.clone());
						if let Ok(metadata) = input.aux_metadata().await {
							let msg = new_message_clone
								.reply(&ctx_clone.http, "Song added to queue")
								.await?;
							if let Err(err) = queue_song(
								Track::new_with_uuid(input, Uuid::new_v4()),
								metadata.clone(),
								handler_lock.clone(),
								guild_id,
								ctx_clone.data(),
								msg.id,
								msg.channel_id,
								new_message_clone.author.display_name().to_owned(),
							)
							.await
							{
								bail!(err);
							}
						} else if let Err(err) = new_message_clone
							.reply(&ctx_clone.http, "Nothing is known about this song")
							.await
						{
							bail!(err);
						}
						Ok(())
					});
				} else {
					new_message
						.reply(
							&ctx.http,
							"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a \
							 voice channel first",
						)
						.await?;
				}
			}
		} else {
			warn!("Failed to convert music channel id to u64");
		}
	}

	Ok(())
}

fn ai_chat_channel(
	ctx: &SContext,
	new_message: &Message,
	data: &Arc<Data>,
	guild_data: &Arc<GuildData>,
	guild_id: GuildId,
) {
	if let Some(ai_chat_channel) = guild_data.settings.ai_chat_channel {
		if let Ok(ai_chat_channel_u64) = u64::try_from(ai_chat_channel) {
			if new_message.channel_id.get() == ai_chat_channel_u64 {
				let guild_ai_chats = {
					let ai_chats_opt = data.ai_chats.get(&guild_id);
					if let Some(ai_chat) = ai_chats_opt {
						ai_chat
					} else {
						let modified_settings = ai_chats_opt.unwrap_or_default();
						data.ai_chats.insert(guild_id, modified_settings.clone());
						modified_settings
					}
				};
				let (
					chatbot_role,
					chatbot_internet_search,
					chatbot_temperature,
					chatbot_top_p,
					chatbot_top_k,
					chatbot_repetition_penalty,
					chatbot_frequency_penalty,
					chatbot_presence_penalty,
				) = {
					let user_settings_opt = data.user_settings.get(&guild_id);
					if let Some(user_settings) = user_settings_opt {
						user_settings
							.get(&new_message.author.id)
							.map(|a| {
								(
									a.chatbot_role.clone(),
									a.chatbot_internet_search,
									a.chatbot_temperature,
									a.chatbot_top_p,
									a.chatbot_top_k,
									a.chatbot_repetition_penalty,
									a.chatbot_frequency_penalty,
									a.chatbot_presence_penalty,
								)
							})
							.unwrap_or_default()
					} else {
						let new_settings = user_settings_opt.unwrap_or_default();
						data.user_settings.insert(guild_id, new_settings.clone());
						new_settings
							.get(&new_message.author.id)
							.map(|a| {
								(
									a.chatbot_role.clone(),
									a.chatbot_internet_search,
									a.chatbot_temperature,
									a.chatbot_top_p,
									a.chatbot_top_k,
									a.chatbot_repetition_penalty,
									a.chatbot_frequency_penalty,
									a.chatbot_presence_penalty,
								)
							})
							.unwrap_or_default()
					}
				};
				let ctx_clone = ctx.clone();
				let music_manager_clone = data.music_manager.get(guild_id);
				let new_message_clone = new_message.clone();
				let guild_id_clone = guild_id;
				spawn(async move {
					if let Err(e) = ai_chatbot(
						&ctx_clone,
						&new_message_clone,
						chatbot_role.map_or_else(
							|| DEFAULT_BOT_ROLE.to_owned(),
							|role| format!("The current user wants you to act as: {role}"),
						),
						chatbot_internet_search,
						chatbot_temperature,
						chatbot_top_p,
						chatbot_top_k,
						chatbot_repetition_penalty,
						chatbot_frequency_penalty,
						chatbot_presence_penalty,
						guild_id_clone,
						&guild_ai_chats,
						music_manager_clone,
					)
					.await
					{
						warn!("AI chatbot error: {e:?}");
					}
				});
			}
		} else {
			warn!("Failed to convert ai chat channel id to u64");
		}
	}
}

async fn global_chat_channel(
	ctx: &SContext,
	new_message: &Message,
	data: Arc<Data>,
	guild_data: Arc<GuildData>,
	guild_id: GuildId,
) -> AResult<()> {
	if let Some(global_chat_channel) = guild_data.settings.global_chat_channel {
		if let Ok(global_chat_channel_u64) = u64::try_from(global_chat_channel) {
			if new_message.channel_id.get() == global_chat_channel_u64
				&& guild_data.settings.global_chat
			{
				let guild_global_chats: Vec<_> = data
					.guild_data
					.iter()
					.filter_map(|entry| {
						let settings = &entry.value().settings;
						if entry.key() != &guild_id
							&& settings.global_chat_channel.is_some()
							&& settings.global_chat
						{
							u64::try_from(settings.guild_id).ok().map(|guild_id_u64| {
								(GuildId::new(guild_id_u64), settings.global_chat_channel)
							})
						} else {
							None
						}
					})
					.collect();
				{
					if let Some(global_chats_history) = data.global_chats.get(&guild_id) {
						let mut global_chats_history_clone = global_chats_history.as_ref().clone();
						for (target_guild_id, _) in &guild_global_chats {
							global_chats_history_clone.insert(*target_guild_id, new_message.id);
						}
						data.global_chats
							.insert(guild_id, Arc::new(global_chats_history_clone));
					} else {
						let mut new_history = HashMap::with_capacity(guild_global_chats.len());
						for (target_guild_id, _) in &guild_global_chats {
							new_history.insert(*target_guild_id, new_message.id);
						}
						data.global_chats.insert(guild_id, Arc::new(new_history));
					}
				}
				for (guild_id, guild_channel_id) in
					guild_global_chats
						.iter()
						.filter_map(|(guild_id, guild_channel_id)| {
							guild_channel_id.map(|channel_id| (*guild_id, channel_id))
						}) {
					if let Ok(guild_channel_id_u64) = u64::try_from(guild_channel_id) {
						let channel_id_type = GenericChannelId::new(guild_channel_id_u64);
						if let Ok(chat_channel) =
							channel_id_type.to_channel(&ctx.http, Some(guild_id)).await
						{
							if let Some(webhook) = webhook_find(
								ctx,
								new_message.guild_id,
								chat_channel.id(),
								data.channel_webhooks.clone(),
							)
							.await
							{
								let content = if new_message.content.is_empty() {
									""
								} else {
									new_message.content.as_str()
								};
								let mut message = ExecuteWebhook::default()
									.username(new_message.author.display_name())
									.avatar_url(new_message.author.avatar_url().unwrap_or_else(
										|| {
											new_message.author.static_avatar_url().unwrap_or_else(
												|| new_message.author.default_avatar_url(),
											)
										},
									))
									.content(content);
								if !new_message.attachments.is_empty() {
									for attachment in new_message
										.attachments
										.iter()
										.filter(|a| a.dimensions().is_some())
									{
										message = message.add_file(
											CreateAttachment::url(
												&ctx.http,
												attachment.url.as_str(),
												attachment.filename.clone(),
											)
											.await?,
										);
									}
								}
								if let Some(replied_message) = &new_message.referenced_message {
									let mut embed = CreateEmbed::default()
										.description(replied_message.content.as_str())
										.author(
											CreateEmbedAuthor::new(
												replied_message.author.display_name(),
											)
											.icon_url(
												replied_message.author.avatar_url().unwrap_or_else(
													|| replied_message.author.default_avatar_url(),
												),
											),
										)
										.timestamp(new_message.timestamp);
									if let Some(attachment) = replied_message.attachments.first() {
										embed = embed.image(attachment.url.as_str());
									}
									message = message.embed(embed);
								}
								if webhook.execute(&ctx.http, false, message).await.is_err() {
									chat_channel
										.id()
										.say(
											&ctx.http,
											format!(
												"{} sent this: {}",
												new_message.author.display_name(),
												new_message.content.as_str()
											),
										)
										.await?;
								}
							} else {
								chat_channel
									.id()
									.say(
										&ctx.http,
										format!(
											"{} sent this: {}",
											new_message.author.display_name(),
											new_message.content.as_str()
										),
									)
									.await?;
							}
						}
					} else {
						warn!("Failed to convert guild channel id to u64");
					}
				}
			}
		} else {
			warn!("Failed to convert global chat channel id to u64");
		}
	}

	Ok(())
}

async fn message_preview(ctx: &SContext, new_message: &Message, mut content: &str) -> AResult<()> {
	if let Ok(link) = discord_message_link.parse_next(&mut content) {
		let (guild_id, channel_id, message_id) = (
			GuildId::new(link.guild_id),
			GenericChannelId::new(link.channel_id),
			MessageId::new(link.message_id),
		);
		if let Ok(channel) = channel_id.to_channel(&ctx.http, Some(guild_id)).await
			&& let Some(ref_channel_name) = channel.guild().map(|g| g.base.name)
		{
			let (channel_name, ref_msg) = (
				ref_channel_name,
				channel_id.message(&ctx.http, message_id).await?,
			);
			if ref_msg.poll.is_none() {
				let embed = CreateEmbed::default()
					.colour(COLOUR_ORANGE)
					.description(ref_msg.content.as_str())
					.author(
						CreateEmbedAuthor::new(ref_msg.author.display_name()).icon_url(
							ref_msg
								.author
								.avatar_url()
								.unwrap_or_else(|| ref_msg.author.default_avatar_url()),
						),
					)
					.footer(CreateEmbedFooter::new(channel_name))
					.timestamp(ref_msg.timestamp);
				let (embed, content_url) = match ref_msg.attachments.first() {
					Some(attachment) => match attachment.content_type.as_deref() {
						Some(content_type) => {
							if content_type.starts_with("image") {
								(embed.image(attachment.url.as_str()), None)
							} else if content_type.starts_with("video") {
								(embed, Some(attachment.url.as_str()))
							} else {
								(embed, None)
							}
						}
						_ => (embed, None),
					},
					_ => (embed, None),
				};
				let mut preview_message = CreateMessage::default()
					.embed(embed)
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false));
				if ref_msg.channel_id == new_message.channel_id {
					preview_message = preview_message.reference_message(&ref_msg);
				}
				if let Some(ref_embed) = ref_msg.embeds.into_iter().next() {
					preview_message = preview_message.add_embed(CreateEmbed::from(ref_embed));
				}
				new_message
					.channel_id
					.send_message(&ctx.http, preview_message)
					.await?;
				if let Some(url) = content_url {
					new_message.channel_id.say(&ctx.http, url).await?;
				}
			}
		}
	}

	Ok(())
}

async fn db_queries(
	ctx: &SContext,
	new_message: &Message,
	data: Arc<Data>,
	guild_id: GuildId,
	guild_id_i64: i64,
	user_id_i64: i64,
	mut tx: Transaction<'static, Postgres>,
	content: &str,
) -> AResult<()> {
	let mut modified_settings = {
		if !data.user_settings.contains_key(&guild_id) {
			query!(
				"INSERT INTO guilds (guild_id)
                        VALUES ($1)
                        ON CONFLICT (guild_id)
                        DO NOTHING",
				guild_id_i64
			)
			.execute(&mut *tx)
			.await?;
		}
		data.user_settings
			.get(&guild_id)
			.unwrap_or_default()
			.as_ref()
			.clone()
	};

	for target in modified_settings.iter_mut().map(|t| t.1) {
		if let Ok(user_id_u64) = u64::try_from(target.user_id) {
			let user_id = UserId::new(user_id_u64);
			if target.afk {
				if user_id_i64 == target.user_id {
					let mut response = new_message
						.reply(
							&ctx.http,
							format!(
								"Ugh, welcome back {}! Guess I didn't manage to kill you after all",
								user_id.to_user(&ctx.http).await?.display_name()
							),
						)
						.await?;
					if let Some(links) = target.pinged_links.as_deref()
						&& !links.is_empty()
					{
						let mut e = CreateEmbed::default()
							.colour(COLOUR_RED)
							.title("Pings you retrieved:");
						for entry in links.split(',') {
							if let Some((name, role)) = entry.split_once(';') {
								e = e.field(name, role, false);
							}
						}
						response
							.edit(&ctx.http, EditMessage::default().embed(e))
							.await?;
					}
					query!(
						"UPDATE user_settings SET afk = FALSE, afk_reason = NULL, pinged_links = \
						 NULL WHERE guild_id = $1 AND user_id = $2",
						guild_id_i64,
						target.user_id,
					)
					.execute(&mut *tx)
					.await?;
					target.afk = false;
					target.afk_reason = None;
					target.pinged_links = None;
				} else if new_message.mentions_user_id(user_id)
					&& new_message.referenced_message.is_none()
				{
					let pinged_link = format!(
						"{};{},",
						new_message.link(),
						new_message.author.display_name()
					);
					query!(
						"UPDATE user_settings
                            SET pinged_links = COALESCE(pinged_links || ',' || $1, $1) 
                            WHERE guild_id = $2 AND user_id = $3",
						pinged_link,
						guild_id_i64,
						target.user_id,
					)
					.execute(&mut *tx)
					.await?;
					match target.pinged_links.as_mut() {
						Some(existing_links) => {
							existing_links.push_str(&pinged_link);
						}
						None => {
							target.pinged_links = Some(pinged_link);
						}
					}
					let reason = target
						.afk_reason
						.as_deref()
						.unwrap_or("Didn't renew life subscription");
					new_message
						.reply(
							&ctx.http,
							format!(
								"{} is currently dead. Reason: {reason}",
								user_id.to_user(&ctx.http).await?.display_name()
							),
						)
						.await?;
				}
			}
			if new_message.mentions_user_id(user_id)
				&& new_message.referenced_message.is_none()
				&& let Some(ping_content) = &target.ping_content
			{
				let message = {
					let base = CreateMessage::default()
						.reference_message(new_message)
						.allowed_mentions(CreateAllowedMentions::default().replied_user(false));
					match &target.ping_media {
						Some(ping_media) => {
							let media = if ping_media.eq_ignore_ascii_case("waifu") {
								Some(get_waifu().await)
							} else if let Some(gif_query) = ping_media.strip_prefix("!gif") {
								let gifs = get_gifs(gif_query.to_owned()).await;
								gifs.get(RNG.lock().await.usize(..gifs.len()))
									.map(|g| g.0.clone())
							} else if !ping_media.is_empty() {
								Some(Cow::Borrowed(ping_media.as_str()))
							} else {
								None
							};
							if let Some(image) = media {
								base.embed(
									CreateEmbed::default()
										.title(ping_content)
										.colour(COLOUR_BLUE)
										.image(image),
								)
							} else {
								base.content(ping_content)
							}
						}
						None => base.content(ping_content),
					}
				};
				new_message
					.channel_id
					.send_message(&ctx.http, message)
					.await?;
			}
			if user_id_i64 == target.user_id {
				target.message_count = target.message_count.saturating_add(1);
			}
		} else {
			warn!("Failed to convert user id to u64");
		}
	}
	data.user_settings
		.insert(guild_id, Arc::new(modified_settings));

	query!(
		"INSERT INTO user_settings (guild_id, user_id, message_count) VALUES ($1, $2, 1)
                ON CONFLICT(guild_id, user_id) 
                DO UPDATE SET
                    message_count = user_settings.message_count + 1",
		guild_id_i64,
		user_id_i64,
	)
	.execute(&mut *tx)
	.await?;

	let guild_data_opt = data.guild_data.get(&guild_id);
	if let Some(guild_data) = guild_data_opt {
		spoiler_channel(ctx, new_message, data.clone(), guild_data.clone()).await?;
		music_channel(ctx, new_message, data.clone(), guild_data.clone(), guild_id).await?;
		ai_chat_channel(ctx, new_message, &data, &guild_data, guild_id);
		global_chat_channel(ctx, new_message, data.clone(), guild_data.clone(), guild_id).await?;
		{
			let mut word_tracking_updates: HashSet<WordTracking> = guild_data.word_tracking.clone();
			for record in guild_data
				.word_tracking
				.iter()
				.filter(|r| content.contains(&r.word))
			{
				query!(
					"UPDATE guild_word_tracking
                                 SET count = count + 1 
                                 WHERE guild_id = $1
                                 AND word = $2",
					guild_id_i64,
					record.word
				)
				.execute(&mut *tx)
				.await?;
				if let Some(mut updated_record) = word_tracking_updates.take(record) {
					updated_record.count = updated_record.count.saturating_add(1);
					word_tracking_updates.insert(updated_record);
				}
			}
			let mut modified_settings = data
				.guild_data
				.get(&guild_id)
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.word_tracking = word_tracking_updates;
			data.guild_data
				.insert(guild_id, Arc::new(modified_settings));
		}
		for record in guild_data
			.word_reactions
			.iter()
			.filter(|r| content.contains(&r.word))
		{
			let message = {
				let base = CreateMessage::default()
					.reference_message(new_message)
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false));
				match &record.media {
					Some(media) if !media.is_empty() => {
						if let Some(gif_query) = media.strip_prefix("!gif") {
							let gifs = get_gifs(gif_query.to_owned()).await;
							let mut embed = CreateEmbed::default()
								.title(&record.content)
								.colour(COLOUR_YELLOW);
							let index = RNG.lock().await.usize(..gifs.len());
							if let Some(gif) = gifs.get(index).map(|g| g.0.clone()) {
								embed = embed.image(gif);
							}
							base.embed(embed)
						} else {
							base.embed(
								CreateEmbed::default()
									.title(&record.content)
									.colour(COLOUR_YELLOW)
									.image(media),
							)
						}
					}
					_ => base.content(&record.content),
				}
			};
			new_message
				.channel_id
				.send_message(&ctx.http, message)
				.await?;
		}
		for record in guild_data
			.emoji_reactions
			.iter()
			.filter(|r| content.contains(&r.content_reaction))
		{
			if let Ok(emoji_id_u64) = u64::try_from(record.emoji_id) {
				let emoji_id_typed = EmojiId::new(emoji_id_u64);
				let emoji = if record.guild_emoji {
					guild_id.emoji(&ctx.http, emoji_id_typed).await?
				} else {
					ctx.get_application_emoji(emoji_id_typed).await?
				};
				let reaction = ReactionType::Custom {
					animated: emoji.animated(),
					id: emoji.id,
					name: Some(emoji.name),
				};
				new_message.react(&ctx.http, reaction).await?;
			} else {
				warn!("Failed to convert emoji id to u64");
			}
		}
	}

	tx.commit()
		.await
		.context("Failed to commit sql-transaction")?;

	Ok(())
}

pub async fn handle_message(ctx: &SContext, new_message: &Message) -> AResult<()> {
	if new_message.author.bot() {
		return Ok(());
	}
	let data: Arc<Data> = ctx.data();
	check_bot_ping(ctx, new_message).await?;
	let content = new_message.content.to_lowercase();
	emoji_reaction(ctx, new_message, &content, data.clone()).await?;
	if let Some(guild_id) = new_message.guild_id {
		let guild_id_i64 = i64::from(guild_id);
		if !data.guild_data.contains_key(&guild_id) {
			insert_guild(guild_id_i64, &mut *data.db.acquire().await?).await?;
			let default_settings = GuildSettings {
				guild_id: guild_id_i64,
				..Default::default()
			};
			data.guild_data.insert(
				guild_id,
				Arc::new(GuildData {
					settings: default_settings,
					..Default::default()
				}),
			);
		}
		let guild_id_i64 = i64::from(guild_id);
		let user_id_i64 = i64::from(new_message.author.id);
		let tx = data
			.db
			.begin()
			.await
			.context("Failed to acquire savepoint")?;

		db_queries(
			ctx,
			new_message,
			data.clone(),
			guild_id,
			guild_id_i64,
			user_id_i64,
			tx,
			&content,
		)
		.await?;

		message_preview(ctx, new_message, &content).await?;
	}

	Ok(())
}
