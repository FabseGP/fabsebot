use std::{fmt::Write as _, sync::Arc};

use anyhow::{Context as _, Result as AResult};
use fabsebot_db::{
	guild::{WordReactions, insert_guild, insert_guild_settings},
	user::{UserSettings, insert_user, insert_user_settings},
};
use metrics::counter;
use serenity::all::{
	Colour, Context as SContext, CreateAllowedMentions, CreateAttachment, CreateComponent,
	CreateContainer, CreateEmbed, CreateEmbedAuthor, CreateMessage, EditMessage, EmojiId,
	ExecuteWebhook, GenericChannelId, GuildId, Message, MessageFlags, MessageId, ReactionType,
};
use songbird::{Call, Songbird, input::Compose as _};
use sqlx::{Postgres, Transaction, query, query_as, query_scalar};
use tokio::{join, sync::Mutex, task::spawn};
use tracing::error;
use winnow::Parser as _;

use crate::{
	config::{
		constants::{
			AI_CHAT_ERROR, DEFAULT_BOT_ROLE, FABSEMAN_WEBHOOK_CONTENT, FABSEMAN_WEBHOOK_NAME,
			FABSEMAN_WEBHOOK_PFP, FAILED_SONG_FETCH, FLOPPAGANDA_GIF, INVALID_TRACK_SOURCE,
			MISSING_METADATA_MSG, NOT_IN_VOICE_CHAN_MSG, QUEUE_MSG,
		},
		types::{AIChats, Data, GuildCache, WebhookMap, utils_config},
	},
	errors::commands::MusicError,
	log_error, log_errors,
	stats::counters::METRICS,
	utils::{
		ai::ai_chatbot,
		helpers::{
			channel_counter, discord_message_link, event_container, get_gif, get_waifu,
			media_gallery, queue_song, separator, text_display, thumbnail_section, youtube_source,
		},
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

		event_container(ctx, new_message, container).await?;
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
					.content(FLOPPAGANDA_GIF)
					.reference_message(new_message)
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
			)
			.await?;
	} else if content == "fabse" || content == "fabseman" {
		let webhook = webhook_find(
			ctx,
			new_message.guild_id,
			new_message.channel_id,
			webhooks.clone(),
		)
		.await?;
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

	Ok(())
}

async fn queue_track(
	ctx: &SContext,
	new_message: &Message,
	music_manager: Arc<Songbird>,
	guild_id: GuildId,
) -> AResult<()> {
	channel_counter("music".to_owned());
	if let Some(handler_lock) = music_manager.get(guild_id) {
		let ctx_clone = ctx.clone();
		let new_message_clone = new_message.clone();
		spawn(async move {
			let Some(mut src) = youtube_source(new_message_clone.content.to_string()).await else {
				if let Err(err) = new_message_clone
					.reply(&ctx_clone.http, INVALID_TRACK_SOURCE)
					.await
				{
					error!("Failed to send message: {err}");
				}
				return;
			};
			let audio = match src.create_async().await {
				Ok(audio) => audio,
				Err(err) => {
					log_error(
						"Failed to create source",
						MusicError::FailedFetch(err).to_string(),
						&ctx_clone,
						METRICS.music_queue_errors.clone(),
					)
					.await;
					if let Err(err) = new_message_clone
						.reply(&ctx_clone.http, FAILED_SONG_FETCH)
						.await
					{
						error!("Failed to send message: {err}");
					}
					return;
				}
			};
			let metadata = match src.aux_metadata().await {
				Ok(metadata) => metadata,
				Err(err) => {
					log_error(
						"# Failed to obtain metadata",
						MusicError::MissingMetadata(err).to_string(),
						&ctx_clone,
						METRICS.music_queue_errors.clone(),
					)
					.await;
					if let Err(err) = new_message_clone
						.reply(&ctx_clone.http, MISSING_METADATA_MSG)
						.await
					{
						error!("Failed to send message: {err}");
					}
					return;
				}
			};
			let msg = match new_message_clone.reply(&ctx_clone.http, QUEUE_MSG).await {
				Ok(msg) => msg,
				Err(err) => {
					error!("Failed to send message: {err}");
					return;
				}
			};
			queue_song(
				metadata,
				audio,
				src,
				handler_lock,
				guild_id,
				ctx_clone.data(),
				msg.id,
				msg.channel_id,
				new_message_clone.author.display_name(),
			)
			.await;
		});
	} else {
		new_message.reply(&ctx.http, NOT_IN_VOICE_CHAN_MSG).await?;
	}

	Ok(())
}

fn ai_chats(
	ctx: &SContext,
	new_message: &Message,
	ai_chats: AIChats,
	music_manager: Option<Arc<Mutex<Call>>>,
	guild_id: GuildId,
	chatbot_role: String,
	chatbot_internet_search: bool,
) {
	channel_counter("chatbot".to_owned());

	let ctx_clone = ctx.clone();
	let new_message_clone = new_message.clone();

	spawn(async move {
		if let Err(error) = ai_chatbot(
			&ctx_clone,
			&new_message_clone,
			chatbot_role,
			chatbot_internet_search,
			guild_id,
			ai_chats,
			music_manager,
		)
		.await
		{
			log_error(
				"# Failed to send AI-chat",
				error.to_string(),
				&ctx_clone,
				METRICS.chatbot_errors.clone(),
			)
			.await;
			if let Err(err) = new_message_clone
				.reply(&ctx_clone.http, AI_CHAT_ERROR)
				.await
			{
				error!("Failed to send message: {err}");
			}
		}
	});
}

async fn global_chats(
	ctx: &SContext,
	new_message: &Message,
	data: Arc<Data>,
	channel_id: Option<i64>,
	global_chat: bool,
	guild_id: i64,
) -> AResult<()> {
	if let Some(global_chat_channel) = channel_id
		&& new_message.channel_id.get() == global_chat_channel.cast_unsigned()
		&& global_chat
	{
		channel_counter("global_chat".to_owned());
		let guild_global_chats = query!(
			r#"
			SELECT guild_id, global_chat_channel FROM guild_settings
			WHERE global_chat IS TRUE
			AND guild_id != $1
			"#,
			guild_id
		)
		.fetch_all(&mut *data.db.acquire().await?)
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
					data.channel_webhooks.clone(),
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
				let content = if new_message.content.is_empty() {
					""
				} else {
					new_message.content.as_str()
				};
				let mut message = ExecuteWebhook::default()
					.username(new_message.author.display_name())
					.avatar_url(new_message.author.avatar_url().unwrap_or_else(|| {
						new_message
							.author
							.static_avatar_url()
							.unwrap_or_else(|| new_message.author.default_avatar_url())
					}))
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
					let mut embed =
						CreateEmbed::default()
							.description(replied_message.content.as_str())
							.author(
								CreateEmbedAuthor::new(replied_message.author.display_name())
									.icon_url(replied_message.author.avatar_url().unwrap_or_else(
										|| replied_message.author.default_avatar_url(),
									)),
							)
							.timestamp(new_message.timestamp);
					if let Some(attachment) = replied_message.attachments.first() {
						embed = embed.image(attachment.url.as_str());
					}
					message = message.embed(embed);
				}
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
				let author_avatar = ref_msg
					.author
					.avatar_url()
					.unwrap_or_else(|| ref_msg.author.default_avatar_url());
				let thumbnail_display = [thumbnail_section(
					ref_msg.author.display_name(),
					&author_avatar,
				)];
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
				let media_opt = if let Some(attachment) = ref_msg.attachments.first()
					&& let Some(content_type) = &attachment.content_type
					&& (content_type.starts_with("image") || content_type.starts_with("video"))
				{
					Some(attachment.url.as_str())
				} else {
					None
				};
				let media_ref = media_opt.as_ref().map(AsRef::as_ref);
				if let Some(media) = media_ref {
					let image = media_gallery(media);
					container = container.add_component(image);
				}
				let mut preview_message = CreateMessage::default()
					.components(vec![CreateComponent::Container(container)])
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false))
					.flags(MessageFlags::IS_COMPONENTS_V2);
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
			}
		}
	}

	Ok(())
}

async fn user_queries(
	ctx: &SContext,
	new_message: &Message,
	guild_id: i64,
	author_settings: UserSettings,
	tx: &mut Transaction<'static, Postgres>,
) -> AResult<()> {
	let user_id_i64 = i64::from(new_message.author.id);

	if author_settings.afk {
		counter!(METRICS.user_afks.clone()).increment(1);
		let mut response = new_message
			.reply(
				&ctx.http,
				format!(
					"Ugh, welcome back {}! Guess I didn't manage to kill you after all",
					new_message.author.display_name()
				),
			)
			.await?;
		if let Some(links) = author_settings.pinged_links.as_deref() {
			let title_display = [text_display("# Pings you retrieved: ")];

			let mut list = String::with_capacity(links.len());

			for entry in links.split(',') {
				if let Some((name, role)) = entry.split_once(';') {
					writeln!(list, "**{name}**: {role}")?;
				}
			}
			let text_display = text_display(&list);

			let container = CreateContainer::new(&title_display)
				.add_component(separator())
				.add_component(text_display)
				.accent_colour(Colour::BLITZ_BLUE);

			response
				.edit(
					&ctx.http,
					EditMessage::default()
						.components(&[CreateComponent::Container(container)])
						.flags(MessageFlags::IS_COMPONENTS_V2),
				)
				.await?;
		}
		query!(
			r#"
			UPDATE user_settings
			SET afk = FALSE,
			afk_reason = NULL,
			pinged_links = NULL
			WHERE guild_id = $1
			AND user_id = $2
			"#,
			guild_id,
			user_id_i64,
		)
		.execute(tx.as_mut())
		.await?;
	}

	if new_message.referenced_message.is_none() {
		for mentioned_user in &new_message.mentions {
			let mentioned_user_id_i64 = mentioned_user.id.get().cast_signed();
			let Some(mentioned_user_settings) = query_as!(
				UserSettings,
				r#"
    			SELECT * from user_settings
    			WHERE guild_id = $1
    			AND user_id = $2
    			AND (afk IS TRUE OR ping_content IS NOT NULL)
    			"#,
				guild_id,
				mentioned_user_id_i64
			)
			.fetch_optional(tx.as_mut())
			.await?
			else {
				continue;
			};
			if mentioned_user_settings.afk {
				let pinged_link = format!(
					"{};{},",
					new_message.link(),
					new_message.author.display_name()
				);
				query!(
					r#"
					UPDATE user_settings
                    SET pinged_links = COALESCE(pinged_links || ',' || $1, $1) 
                    WHERE guild_id = $2
                    AND user_id = $3
                    "#,
					pinged_link,
					guild_id,
					mentioned_user_id_i64,
				)
				.execute(tx.as_mut())
				.await?;
				let reason = mentioned_user_settings
					.afk_reason
					.as_deref()
					.unwrap_or("Didn't renew life subscription");
				new_message
					.reply(
						&ctx.http,
						format!(
							"{} is currently dead. Reason: {reason}",
							new_message.author.display_name()
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
				let media_opt = if let Some(ping_media) = &mentioned_user_settings.ping_media {
					let media = if ping_media.eq_ignore_ascii_case("waifu") {
						get_waifu(ctx).await
					} else if let Some(gif_query) = ping_media.strip_prefix("!gif") {
						get_gif(ctx, gif_query).await
					} else {
						ping_media.to_owned()
					};
					Some(media)
				} else {
					None
				};
				let media_ref = media_opt.as_ref().map(AsRef::as_ref);
				if let Some(media) = media_ref {
					let image = media_gallery(media);
					container = container.add_component(image);
				}
				event_container(ctx, new_message, container).await?;
			}
		}
	}

	Ok(())
}

async fn guild_queries(
	ctx: &SContext,
	new_message: &Message,
	data: Arc<Data>,
	word_tracking: &[String],
	word_reactions: &[WordReactions],
	guild_id: GuildId,
	guild_id_i64: i64,
	tx: &mut Transaction<'static, Postgres>,
) -> AResult<()> {
	for record in word_tracking {
		counter!(METRICS.words_tracked.clone()).increment(1);
		query!(
			r#"
			UPDATE guild_word_tracking
            SET count = count + 1 
            WHERE guild_id = $1
            AND word = $2
            "#,
			guild_id_i64,
			record
		)
		.execute(tx.as_mut())
		.await?;
	}
	for record in word_reactions {
		counter!(METRICS.word_reactions.clone()).increment(1);
		if let Some(content) = &record.content {
			let title = format!("# {content}");
			let text_display = [text_display(&title)];
			let mut container = CreateContainer::new(&text_display).accent_colour(Colour::GOLD);
			let media_opt = if let Some(reaction_media) = &record.media {
				let media = if let Some(gif_query) = reaction_media.strip_prefix("!gif") {
					get_gif(ctx, gif_query).await
				} else {
					reaction_media.to_owned()
				};
				Some(media)
			} else {
				None
			};
			let media_ref = media_opt.as_ref().map(AsRef::as_ref);
			if let Some(media) = media_ref {
				let image = media_gallery(media);
				container = container.add_component(image);
			}
			event_container(ctx, new_message, container).await?;
		} else if let Some(emoji_id) = &record.emoji_id {
			let emoji_id_typed = EmojiId::new(emoji_id.cast_unsigned());
			let (is_animated, emoji_id, emoji_name) = if record.guild_emoji
				&& let Ok(guild_emoji) = guild_id.emoji(&ctx.http, emoji_id_typed).await
			{
				(guild_emoji.animated(), guild_emoji.id, guild_emoji.name)
			} else if let Some(cache_emoji) = data.app_emojis.get(&emoji_id.cast_unsigned()) {
				(
					cache_emoji.animated(),
					cache_emoji.id,
					cache_emoji.name.clone(),
				)
			} else {
				match ctx.get_application_emoji(emoji_id_typed).await {
					Ok(http_emoji) => (http_emoji.animated(), http_emoji.id, http_emoji.name),
					Err(err) => {
						error!("Failed to get app emojis: {err}");
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
	data: Arc<Data>,
	guild_id: GuildId,
	guild_id_i64: i64,
	author_settings: UserSettings,
	mut tx: Transaction<'static, Postgres>,
) -> AResult<()> {
	user_queries(ctx, new_message, guild_id_i64, author_settings, &mut tx).await?;

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

	let word_reactions = query_as!(
		WordReactions,
		r#"
		SELECT * FROM guild_word_reaction
		WHERE guild_id = $1
		AND word ILIKE ANY($2)
		"#,
		guild_id_i64,
		&words
	)
	.fetch_all(&mut *tx)
	.await?;

	let word_tracking = query_scalar!(
		r#"
		SELECT word FROM guild_word_tracking
		WHERE guild_id = $1
		AND word ILIKE ANY($2)
		"#,
		guild_id_i64,
		&words
	)
	.fetch_all(&mut *tx)
	.await?;

	guild_queries(
		ctx,
		new_message,
		data,
		&word_tracking,
		&word_reactions,
		guild_id,
		guild_id_i64,
		&mut tx,
	)
	.await?;

	tx.commit()
		.await
		.context("Failed to commit sql-transaction")?;

	Ok(())
}

pub async fn handle_message(
	ctx: &SContext,
	new_message: &Message,
	guild_id: GuildId,
) -> AResult<()> {
	let data: Arc<Data> = ctx.data();
	let mut tx = data
		.db
		.begin()
		.await
		.context("Failed to acquire savepoint")?;

	let guild_id_i64 = i64::from(guild_id);
	let user_id_i64 = i64::from(new_message.author.id);

	let guild_cache = data.guilds.get(&guild_id).unwrap_or_else(|| {
		let new_data = Arc::new(GuildCache::default());
		data.guilds.insert(guild_id, new_data.clone());
		new_data
	});

	let guild_settings =
		match insert_guild_settings(guild_id_i64, &mut *data.db.acquire().await?).await {
			Ok(settings) => settings,
			Err(err) => {
				error!("Failed to fetch guild settings: {err}");
				insert_guild(guild_id_i64, &mut tx).await?;
				insert_guild_settings(guild_id_i64, &mut tx).await?
			}
		};

	let author_settings =
		match insert_user_settings(guild_id_i64, user_id_i64, &mut *data.db.acquire().await?).await
		{
			Ok(settings) => settings,
			Err(err) => {
				error!("Failed to fetch user settings: {err}");
				insert_user(user_id_i64, &mut tx).await?;
				insert_user_settings(guild_id_i64, user_id_i64, &mut tx).await?
			}
		};

	if !new_message.content.starts_with('#') {
		if let Some(music_channel) = guild_settings.music_channel
			&& new_message.channel_id.get() == music_channel.cast_unsigned()
		{
			queue_track(ctx, new_message, data.music_manager.clone(), guild_id).await?;
		}
		if let Some(ai_chat_channel) = guild_settings.ai_chat_channel
			&& new_message.channel_id.get() == ai_chat_channel.cast_unsigned()
		{
			ai_chats(
				ctx,
				new_message,
				guild_cache.ai_chats.clone(),
				data.music_manager.get(guild_id),
				guild_id,
				author_settings
					.chatbot_role
					.clone()
					.unwrap_or_else(|| DEFAULT_BOT_ROLE.to_owned()),
				author_settings.chatbot_internet_search,
			);
		}
	}

	let content = new_message.content.to_lowercase();
	let (bot_ping, easter_eggs, message_preview, spoiler_message, global_chat) = join!(
		check_bot_ping(ctx, new_message),
		easter_eggs(ctx, new_message, &content, &data.channel_webhooks),
		message_preview(ctx, new_message, &content),
		spoiler_message(
			ctx,
			new_message,
			guild_settings.spoiler_channel,
			data.channel_webhooks.clone()
		),
		global_chats(
			ctx,
			new_message,
			data.clone(),
			guild_settings.global_chat_channel,
			guild_settings.global_chat,
			guild_id_i64
		)
	);

	log_errors!(
		bot_ping,
		easter_eggs,
		message_preview,
		spoiler_message,
		global_chat
	);

	db_queries(
		ctx,
		new_message,
		data,
		guild_id,
		guild_id_i64,
		author_settings,
		tx,
	)
	.await?;

	Ok(())
}
