use std::{fmt::Write as _, sync::Arc};

use anyhow::Result as AResult;
use fabsebot_db::{
	guild::{GuildSettings, WordReactions, fetch_guild_settings, insert_guild_settings},
	user::{
		PingedLink, UserSettings, UserSettingsLimited, fetch_user_settings, insert_user_settings,
	},
};
use metrics::counter;
use serde_json::{Value, to_value};
use serenity::{
	all::{
		Colour, Context as SContext, CreateAllowedMentions, CreateContainer, CreateMessage,
		EmojiId, ExecuteWebhook, GenericChannelId, GuildId, Message, MessageId, ReactionType,
	},
	builder::{CreateComponent, EditMessage},
	model::{channel::MessageFlags, id::UserId},
};
use songbird::{Call, Songbird};
use sqlx::{Pool, Postgres, query, query_as, types::Json};
use tokio::{
	sync::{Mutex, mpsc},
	task::spawn,
	try_join,
};
use tracing::{error, warn};
use winnow::Parser as _;

use crate::{
	config::{
		constants::{
			DEFAULT_BOT_ROLE, EMPTY_VOICE_CHAN_MSG, FAILED_SONG_FETCH, MESSAGE_LIMIT, QUEUEING_MSG,
		},
		types::{AIQueue, Data, GuildCache, UsersMap, WebhookMap, utils_config},
	},
	log_error,
	stats::counters::METRICS,
	utils::{
		ai::{AIQueuePayload, ai_task},
		helpers::{
			channel_counter, discord_message_link, get_emoji, get_gif, get_user, get_waifu,
			media_gallery, message_container, separator, text_display, thumbnail_section, user_pfp,
		},
		voice::{add_voice_events, add_youtube_song},
		webhook::{spoiler_message, webhook_find},
	},
};

async fn check_bot_ping(ctx: &SContext, new_message: &Message) -> AResult<()> {
	if new_message.mentions_user_id(ctx.cache.current_user().id)
		&& new_message.referenced_message.is_none()
	{
		counter!(METRICS.bot_pings.clone()).increment(1);
		let (ping_message, ping_payload) = {
			let utils_config = utils_config();
			(
				utils_config.ping_message.as_str(),
				utils_config.ping_payload.as_str(),
			)
		};

		let text_display = [text_display(ping_message)];
		let image = media_gallery(ping_payload);
		let container = CreateContainer::new(&text_display)
			.add_component(image)
			.accent_colour(Colour::BLITZ_BLUE);

		new_message
			.channel_id
			.send_message(&ctx.http, message_container(Some(new_message), container))
			.await?;
	}

	Ok(())
}

async fn easter_eggs(
	ctx: &SContext,
	new_message: &Message,
	content: &str,
	webhooks: &WebhookMap,
) -> AResult<()> {
	if content == "floppaganda" {
		counter!(METRICS.floppaganda.clone()).increment(1);
		new_message
			.channel_id
			.send_message(
				&ctx.http,
				CreateMessage::default()
					.content("https://c.tenor.com/1y6DManILSYAAAAd/tenor.gif")
					.reference_message(new_message)
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
			)
			.await?;
	} else if content == "fabse" || content == "fabseman" {
		let webhook = match webhook_find(
			ctx,
			new_message.guild_id,
			new_message.channel_id,
			webhooks.clone(),
		)
		.await
		{
			Ok(webhook) => webhook,
			Err(err) => {
				warn!("{err}");
				return Ok(());
			}
		};
		webhook
			.execute(
				&ctx.http,
				false,
				ExecuteWebhook::default()
					.username("yotsuba")
					.avatar_url("https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png")
					.content("# such magnificence"),
			)
			.await?;
	}

	Ok(())
}

async fn queue_track(
	ctx: &SContext,
	new_message: &Message,
	music_manager: Arc<Songbird>,
	settings: Option<&GuildSettings>,
) -> AResult<()> {
	if let Some(settings) = settings
		&& let Some(music_channel) = settings.music_channel
		&& i64::from(new_message.channel_id) == music_channel
		&& !new_message.content.starts_with('#')
	{
		let guild_id = new_message.guild_id.unwrap();
		channel_counter("music".to_owned());
		let handler_lock = if let Some(lock) = music_manager.get(guild_id) {
			lock
		} else if let Ok(voice_state) = guild_id
			.get_user_voice_state(&ctx.http, new_message.author.id)
			.await && let Some(channel_id) = voice_state.channel_id
			&& let Ok(lock) = music_manager.join(guild_id, channel_id).await
		{
			add_voice_events(ctx, guild_id, new_message.channel_id, lock.clone(), false).await;
			lock
		} else {
			new_message.reply(&ctx.http, EMPTY_VOICE_CHAN_MSG).await?;
			return Ok(());
		};
		let mut msg = new_message.reply(&ctx.http, QUEUEING_MSG).await?;
		let bot_data: Arc<Data> = ctx.data();
		let (content, author_id) = (new_message.content.to_string(), new_message.author.id);
		let ctx_clone = ctx.clone();
		spawn(async move {
			if let Err(err) = add_youtube_song(
				content,
				handler_lock,
				guild_id,
				i64::from(msg.id),
				i64::from(msg.channel_id),
				i64::from(author_id),
				&bot_data.db,
				None,
			)
			.await
			{
				if let Err(err) = msg
					.edit(
						&ctx_clone.http,
						EditMessage::new().content(FAILED_SONG_FETCH),
					)
					.await
				{
					error!("Failed to send message: {err}");
				}
				let output = format!("# Failed to queue song\n{err}");
				counter!(METRICS.music_queue_errors.clone()).increment(1);
				log_error(&output, &ctx_clone).await;
			}
		});
	}

	Ok(())
}

async fn ai_chats(
	ctx: &SContext,
	message: &Message,
	ai_queue: AIQueue,
	voice_handle: Option<Arc<Mutex<Call>>>,
	settings: Option<&GuildSettings>,
) -> AResult<()> {
	if let Some(settings) = settings
		&& let Some(ai_chat_channel) = settings.ai_chat_channel
		&& i64::from(message.channel_id) == ai_chat_channel
		&& !message.content.starts_with('#')
	{
		channel_counter("chatbot".to_owned());
		let payload = AIQueuePayload {
			message: message.clone(),
			chatbot_role: settings
				.chatbot_role
				.clone()
				.unwrap_or_else(|| DEFAULT_BOT_ROLE.to_owned()),
			ctx: ctx.clone(),
			voice_handle,
		};
		ai_queue.send(payload).await?;
	}
	Ok(())
}

async fn global_chats(
	ctx: &SContext,
	new_message: &Message,
	settings: Option<&GuildSettings>,
	guild_id: i64,
) -> AResult<()> {
	if let Some(settings) = settings
		&& let Some(global_chat_channel) = settings.global_chat_channel
		&& i64::from(new_message.channel_id) == global_chat_channel
		&& settings.global_chat
	{
		let bot_data: Arc<Data> = ctx.data();
		channel_counter("global_chat".to_owned());
		let guild_global_chats = query!(
			r#"
			SELECT guild_id, global_chat_channel
			FROM guild_settings
			WHERE global_chat IS TRUE
				AND guild_id != $1
			LIMIT 10
			"#,
			guild_id
		)
		.fetch_all(&bot_data.db)
		.await?;
		for (guild_id, guild_channel_id) in guild_global_chats.iter().filter_map(|record| {
			record
				.global_chat_channel
				.map(|channel_id| (GuildId::new(record.guild_id.cast_unsigned()), channel_id))
		}) {
			let channel_id_type = GenericChannelId::new(guild_channel_id.cast_unsigned());
			if let Ok(chat_channel) = channel_id_type.to_channel(&ctx.http, Some(guild_id)).await {
				let webhook = match webhook_find(
					ctx,
					new_message.guild_id,
					chat_channel.id(),
					bot_data.channel_webhooks.clone(),
				)
				.await
				{
					Ok(webhook) => webhook,
					Err(err) => {
						error!("Failed to find webhook: {err}");
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
						return Ok(());
					}
				};
				let display = [text_display(&new_message.content)];
				let mut container = CreateContainer::new(&display);
				if let Some(attachment) = new_message
					.attachments
					.iter()
					.find(|a| a.dimensions().is_some())
				{
					let image = media_gallery(&attachment.url);
					container = container.add_component(separator()).add_component(image);
				}
				if let Some(replied_message) = &new_message.referenced_message {
					let mut text = format!(
						"# Referencing message sent by {}\n{}\n*Timestamp:*{}",
						replied_message.author.display_name(),
						replied_message.content.as_str(),
						new_message.timestamp
					);
					text.truncate(MESSAGE_LIMIT);
					let avatar = user_pfp(&replied_message.author);
					let thumbnail_section = thumbnail_section(text, avatar);
					container = container
						.add_component(separator())
						.add_component(thumbnail_section);
					if let Some(attachment) = replied_message
						.attachments
						.iter()
						.find(|a| a.dimensions().is_some())
					{
						let image = media_gallery(&attachment.url);
						container = container.add_component(separator()).add_component(image);
					}
				}
				let component = [CreateComponent::Container(container)];
				let avatar = user_pfp(&new_message.author);
				let message = ExecuteWebhook::default()
					.with_components(true)
					.flags(MessageFlags::IS_COMPONENTS_V2)
					.username(new_message.author.display_name())
					.components(&component)
					.avatar_url(avatar);
				if let Err(err) = webhook.execute(&ctx.http, false, message).await {
					error!("Failed to execute webhook: {err}");
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
		}
	}

	Ok(())
}

async fn message_preview(ctx: &SContext, new_message: &Message, mut content: &str) -> AResult<()> {
	if let Ok(link) = discord_message_link.parse_next(&mut content) {
		counter!(METRICS.message_previews.clone()).increment(1);
		let (guild_id, channel_id, message_id) = (
			GuildId::new(link.guild),
			GenericChannelId::new(link.channel),
			MessageId::new(link.message),
		);
		if let Ok(channel) = channel_id.to_channel(&ctx.http, Some(guild_id)).await
			&& let Some(channel_name) = channel.guild().map(|g| g.base.name)
		{
			let ref_msg = channel_id.message(&ctx.http, message_id).await?;
			if ref_msg.poll.is_none() {
				let avatar = user_pfp(&ref_msg.author);
				let thumbnail_display = [thumbnail_section(ref_msg.author.display_name(), &avatar)];
				let reply_display = text_display(&ref_msg.content);
				let timestamp = ref_msg.timestamp.to_string();
				let time_display = text_display(&timestamp);
				let channel_display = text_display(&channel_name);
				let mut container = CreateContainer::new(&thumbnail_display)
					.add_component(separator())
					.add_component(reply_display)
					.add_component(separator())
					.add_component(time_display)
					.add_component(separator())
					.add_component(channel_display)
					.accent_colour(Colour::ORANGE);
				if let Some(attachment) = ref_msg.attachments.first()
					&& let Some(content_type) = &attachment.content_type
					&& (content_type.starts_with("image") || content_type.starts_with("video"))
				{
					let image = media_gallery(attachment.url.as_str());
					container = container.add_component(image);
				}
				let mut preview_message = message_container(None, container);
				if ref_msg.channel_id == new_message.channel_id {
					preview_message = preview_message.reference_message(&ref_msg);
				}
				new_message
					.channel_id
					.send_message(&ctx.http, preview_message)
					.await?;
			}
		}
	}

	Ok(())
}

async fn user_queries(
	ctx: &SContext,
	users: &UsersMap,
	new_message: &Message,
	guild_id: i64,
	author_settings: Option<&UserSettingsLimited>,
	conn: &Pool<Postgres>,
) -> AResult<()> {
	let user_id_i64 = i64::from(new_message.author.id);

	if let Some(settings) = author_settings {
		counter!(METRICS.user_afks.clone()).increment(1);
		let text = format!(
			"# Ugh, welcome back {}! Guess I didn't manage to kill you after all",
			new_message.author.display_name()
		);
		let title_display = [text_display(text)];
		let mut container = CreateContainer::new(&title_display).accent_colour(Colour::BLITZ_BLUE);

		if !settings.pinged_links.0.is_empty() {
			let mut list = String::with_capacity(settings.pinged_links.0.len().saturating_add(1));

			writeln!(list, "## Pinged links:")?;

			for entry in &settings.pinged_links.0 {
				writeln!(list, "**{}**: {}", entry.author, entry.link)?;
			}
			let text_display = text_display(list);

			container = container
				.add_component(separator())
				.add_component(text_display);
		}

		new_message
			.channel_id
			.send_message(&ctx.http, message_container(Some(new_message), container))
			.await?;

		query!(
			r#"
			UPDATE user_settings
			SET afk = FALSE,
				afk_reason = NULL,
    			pinged_links = '[]'::jsonb
			WHERE guild_id = $1
			AND user_id = $2
			"#,
			guild_id,
			user_id_i64,
		)
		.execute(conn)
		.await?;
	}

	if new_message.referenced_message.is_none() && !new_message.mentions.is_empty() {
		let mentioned_ids: Vec<i64> = new_message
			.mentions
			.iter()
			.map(|u| i64::from(u.id))
			.collect();

		let mentioned_settings = query_as!(
			UserSettings,
			r#"
        	SELECT user_id, afk_reason,
        	    pinged_links as "pinged_links: Json<Vec<PingedLink>>",
        	    ping_content, ping_media, afk
        	FROM user_settings
        	WHERE guild_id = $1
          	AND user_id = ANY($2)
          	AND (afk IS TRUE OR ping_content IS NOT NULL)
        	"#,
			guild_id,
			&mentioned_ids[..]
		)
		.fetch_all(conn)
		.await?;

		let ping_updates: Vec<(Value, i64)> = mentioned_settings
			.iter()
			.filter(|s| s.afk)
			.map(|s| {
				let entry = PingedLink {
					link: new_message.link().to_string(),
					author: new_message.author.display_name().to_owned(),
				};
				(to_value(entry).unwrap(), s.user_id)
			})
			.collect();

		if !ping_updates.is_empty() {
			let (entries, user_ids): (Vec<Value>, Vec<i64>) = ping_updates.into_iter().unzip();
			query!(
				r#"
        		UPDATE user_settings
        		SET pinged_links = COALESCE(pinged_links, '[]'::jsonb) || jsonb_build_array(u.entry)
        		FROM UNNEST($1::jsonb[], $2::bigint[]) AS u(entry, user_id)
        		WHERE user_settings.guild_id = $3
        		AND user_settings.user_id = u.user_id
        		"#,
				&entries[..],
				&user_ids[..],
				guild_id
			)
			.execute(conn)
			.await?;
		}

		for mentioned_user_settings in mentioned_settings {
			if mentioned_user_settings.afk
				&& let Ok(user) = get_user(
					&ctx.http,
					users,
					UserId::new(mentioned_user_settings.user_id.cast_unsigned()),
				)
				.await
			{
				let reason = mentioned_user_settings
					.afk_reason
					.as_deref()
					.unwrap_or("Didn't renew life subscription");
				new_message
					.reply(
						&ctx.http,
						format!(
							"{} is currently dead. Reason: {reason}",
							user.display_name()
						),
					)
					.await?;
			}
			if let Some(ping_content) = &mentioned_user_settings.ping_content {
				counter!(METRICS.custom_user_pings.clone()).increment(1);
				let title = format!("# {ping_content}");
				let text_display = [text_display(&title)];
				let mut container =
					CreateContainer::new(&text_display).accent_colour(Colour::BLITZ_BLUE);
				if let Some(ping_media) = mentioned_user_settings.ping_media {
					let media = if ping_media.eq_ignore_ascii_case("waifu") {
						get_waifu(ctx).await
					} else if let Some(gif_query) = ping_media.strip_prefix("!gif") {
						get_gif(ctx, gif_query).await
					} else {
						ping_media
					};
					let image = media_gallery(media);
					container = container.add_component(image);
				}
				new_message
					.channel_id
					.send_message(&ctx.http, message_container(Some(new_message), container))
					.await?;
			}
		}
	}

	Ok(())
}

async fn guild_queries(
	ctx: &SContext,
	new_message: &Message,
	word_reactions: &[WordReactions],
	guild_id: GuildId,
) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();

	for record in word_reactions {
		counter!(METRICS.word_reactions.clone()).increment(1);
		if let Some(content) = &record.content {
			let title = format!("# {content}");
			let text_display = [text_display(&title)];
			let mut container = CreateContainer::new(&text_display).accent_colour(Colour::GOLD);
			if let Some(reaction_media) = &record.media {
				let media = if let Some(gif_query) = reaction_media.strip_prefix("!gif") {
					get_gif(ctx, gif_query).await
				} else {
					reaction_media.to_owned()
				};
				let image = media_gallery(media);
				container = container.add_component(image);
			}
			new_message
				.channel_id
				.send_message(&ctx.http, message_container(Some(new_message), container))
				.await?;
		} else if let Some(emoji_id) = &record.emoji_id {
			let emoji_id_typed = EmojiId::new(emoji_id.cast_unsigned());
			let (is_animated, emoji_id, emoji_name) = if record.guild_emoji
				&& let Ok(guild_emoji) = guild_id.emoji(&ctx.http, emoji_id_typed).await
			{
				(guild_emoji.animated(), guild_emoji.id, guild_emoji.name)
			} else {
				match get_emoji(ctx, &bot_data.app_emojis, emoji_id_typed).await {
					Ok(emoji) => (emoji.animated(), emoji.id, emoji.name.clone()),
					Err(err) => {
						error!("{err}");
						continue;
					}
				}
			};
			let reaction = ReactionType::Custom {
				animated: is_animated,
				id: emoji_id,
				name: Some(emoji_name),
			};
			new_message.react(&ctx.http, reaction).await?;
		}
	}

	Ok(())
}

async fn db_queries(
	ctx: &SContext,
	new_message: &Message,
	guild_id: GuildId,
	guild_id_i64: i64,
	author_settings: Option<&UserSettingsLimited>,
) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();

	user_queries(
		ctx,
		&bot_data.users,
		new_message,
		guild_id_i64,
		author_settings,
		&bot_data.db,
	)
	.await?;

	let words: Vec<String> = new_message
		.content
		.split_whitespace()
		.map(|s| {
			s.chars()
				.filter(|c| c.is_alphanumeric())
				.collect::<String>()
		})
		.filter(|s| !s.is_empty())
		.collect();

	let (word_reactions, updated_words) = try_join!(
		query_as!(
			WordReactions,
			r#"
        	SELECT word, content, media, emoji_id, guild_emoji
        	FROM guild_word_reaction
        	WHERE guild_id = $1
        	AND word ILIKE ANY($2)
        	"#,
			guild_id_i64,
			&words
		)
		.fetch_all(&bot_data.db),
		query!(
			r#"
    		UPDATE guild_word_tracking
    		SET count = count + 1
    		WHERE guild_id = $1
    		AND word ILIKE ANY($2)
    		"#,
			guild_id_i64,
			&words
		)
		.execute(&bot_data.db)
	)?;

	if updated_words.rows_affected() > 0 {
		counter!(METRICS.words_tracked.clone()).increment(updated_words.rows_affected());
	}

	guild_queries(ctx, new_message, &word_reactions, guild_id).await?;

	Ok(())
}

pub async fn handle_message(
	ctx: &SContext,
	new_message: &Message,
	guild_id: GuildId,
) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();

	let guild_id_i64 = i64::from(guild_id);
	let user_id_i64 = i64::from(new_message.author.id);

	let guild_cache = if let Some(cache) = bot_data.guilds.get(&guild_id) {
		cache
	} else {
		insert_guild_settings(guild_id_i64, &bot_data.db).await?;
		insert_user_settings(guild_id_i64, user_id_i64, &bot_data.db).await?;
		let channel = mpsc::channel(100);
		let cache = Arc::new(GuildCache {
			ai_queue: channel.0,
		});
		spawn(async move { ai_task(channel.1).await });
		bot_data.guilds.insert(guild_id, cache.clone());
		cache
	};

	let (guild_settings, author_settings) = try_join!(
		fetch_guild_settings(guild_id_i64, &bot_data.db),
		fetch_user_settings(guild_id_i64, user_id_i64, &bot_data.db)
	)?;

	let content = new_message.content.to_lowercase();

	try_join!(
		check_bot_ping(ctx, new_message),
		easter_eggs(ctx, new_message, &content, &bot_data.channel_webhooks),
		message_preview(ctx, new_message, &content),
		spoiler_message(
			ctx,
			new_message,
			guild_settings.as_ref(),
			bot_data.channel_webhooks.clone()
		),
		global_chats(ctx, new_message, guild_settings.as_ref(), guild_id_i64),
		ai_chats(
			ctx,
			new_message,
			guild_cache.ai_queue.clone(),
			bot_data.music_manager.get(guild_id),
			guild_settings.as_ref()
		),
		queue_track(
			ctx,
			new_message,
			bot_data.music_manager.clone(),
			guild_settings.as_ref()
		)
	)?;

	db_queries(
		ctx,
		new_message,
		guild_id,
		guild_id_i64,
		author_settings.as_ref(),
	)
	.await?;

	Ok(())
}
