use std::{borrow::Cow, fmt::Write as _, sync::Arc};

use anyhow::Result as AResult;
use fabsebot_db::{
	guild::{WordReactions, fetch_guild_settings},
	user::{PingedLink, UserSettings, fetch_user_settings},
};
use metrics::counter;
use serde_json::{Value, to_value};
use serenity::{
	all::{
		Colour, Context as SContext, CreateContainer, EmojiId, ExecuteWebhook, GenericChannelId,
		GuildId, Message, MessageId, ReactionType,
	},
	builder::{
		CreateComponent, CreateContainerComponent, CreateMediaGallery, CreateSection, EditMessage,
	},
	model::channel::MessageFlags,
};
use sqlx::{Pool, Postgres, query, query_as, types::Json};
use tokio::{sync::mpsc::error::SendError, try_join};
use tracing::error;
use winnow::Parser as _;

use crate::{
	config::{
		constants::{DEFAULT_AFK_REASON, FAILED_SONG_FETCH, MESSAGE_LIMIT, QUEUEING_MSG},
		types::{AIQueue, ContextType, Data, WebhookMap, utils_config},
	},
	stats::counters::METRICS,
	utils::{
		ai::AIQueuePayload,
		helpers::{
			channel_counter, discord_message_link, get_emoji, get_gif, get_waifu, guild_cache,
			media_gallery, message_container, separator, silent_message, text_display,
			thumbnail_section,
		},
		voice::{lavalink_play, lavalink_try_join},
		webhook::{spoiler_message, webhook_find},
	},
};

async fn check_bot_ping(ctx: &SContext, new_message: &Message) -> AResult<()> {
	if new_message.mentions_user_id(ctx.cache.current_user().id)
		&& new_message.referenced_message.is_none()
	{
		counter!(METRICS.bot_pings.as_str()).increment(1);
		let (ping_message, ping_payload) = {
			let utils_config = utils_config();
			(
				utils_config.ping_message.as_str(),
				utils_config.ping_payload.as_str(),
			)
		};

		let text_display = [text_display(ping_message)];
		let image = [media_gallery(ping_payload)];
		let container = CreateContainer::new(&text_display)
			.add_component(CreateContainerComponent::MediaGallery(
				CreateMediaGallery::new(&image),
			))
			.accent_colour(Colour::BLITZ_BLUE);
		let component = [CreateComponent::Container(container)];

		new_message
			.channel_id
			.send_message(
				&ctx.http,
				message_container(&component).reference_message(new_message),
			)
			.await?;
	}

	Ok(())
}

async fn easter_eggs(ctx: &SContext, new_message: &Message, webhooks: &WebhookMap) -> AResult<()> {
	let content = new_message.content.as_str();
	if content.eq_ignore_ascii_case("floppaganda") {
		counter!(METRICS.floppaganda.as_str()).increment(1);
		new_message
			.channel_id
			.send_message(
				&ctx.http,
				silent_message("https://c.tenor.com/1y6DManILSYAAAAd/tenor.gif")
					.reference_message(new_message),
			)
			.await?;
	} else if (content.eq_ignore_ascii_case("fabse") || content.eq_ignore_ascii_case("fabseman"))
		&& let Some(webhook) =
			webhook_find(ctx, new_message.guild_id, new_message.channel_id, webhooks).await?
	{
		webhook
			.execute(
				&ctx.http,
				false,
				ExecuteWebhook::new()
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
	conn: &Pool<Postgres>,
	guild_id: GuildId,
) -> AResult<()> {
	channel_counter("music");
	let Some((_typing, player_context)) =
		lavalink_try_join(ContextType::Serenity(ctx), guild_id, new_message.author.id).await?
	else {
		return Ok(());
	};
	let mut msg = new_message.reply(&ctx.http, QUEUEING_MSG).await?;
	if let Err(err) = lavalink_play(
		ctx,
		guild_id,
		msg.id,
		msg.channel_id,
		new_message.author.id,
		&new_message.content,
		player_context,
		conn,
	)
	.await
	{
		msg.edit(&ctx.http, EditMessage::new().content(FAILED_SONG_FETCH))
			.await?;
		return Err(err);
	}

	Ok(())
}

#[expect(clippy::result_large_err)]
async fn ai_chats(
	message: Message,
	ai_queue: AIQueue,
	chatbot_role: Option<String>,
) -> Result<(), SendError<AIQueuePayload>> {
	channel_counter("chatbot");
	let payload = AIQueuePayload {
		message,
		chatbot_role,
	};
	ai_queue.send(payload).await
}

async fn global_chats(ctx: &SContext, new_message: &Message, guild_id: i64) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();
	channel_counter("global_chat");
	let guild_global_chats = query!(
		r#"
		SELECT guild_id, global_chat_channel as "global_chat_channel!"
		FROM guild_settings
		WHERE global_chat IS TRUE
			AND global_chat_channel IS NOT NULL
			AND guild_id != $1
		LIMIT 10
		"#,
		guild_id
	)
	.fetch_all(&bot_data.db)
	.await?;
	let display = [text_display(&new_message.content)];
	let mut container = CreateContainer::new(&display);
	if let Some(attachment) = new_message
		.attachments
		.iter()
		.find(|a| a.dimensions().is_some())
	{
		let image = vec![media_gallery(&attachment.url)];
		container = container.add_component(separator()).add_component(
			CreateContainerComponent::MediaGallery(CreateMediaGallery::new(image)),
		);
	}
	if let Some(replied_message) = &new_message.referenced_message {
		let mut text = String::with_capacity(
			usize::from(replied_message.author.name.len())
				.saturating_add(usize::from(replied_message.content.len()))
				.saturating_add(128),
		);
		write!(
			text,
			"# Referencing message sent by {}\n**Timestamp:**<t:{}:F>\n\n*{}*",
			replied_message.author.name,
			replied_message.content.as_str(),
			new_message.timestamp.timestamp()
		)?;
		text.truncate(MESSAGE_LIMIT);
		let avatar = replied_message.author.face();
		let (text, thumbnail) = thumbnail_section(text, avatar);
		container =
			container
				.add_component(separator())
				.add_component(CreateContainerComponent::Section(CreateSection::new(
					vec![text],
					thumbnail,
				)));
		if let Some(attachment) = replied_message
			.attachments
			.iter()
			.find(|a| a.dimensions().is_some())
		{
			let image = vec![media_gallery(&attachment.url)];
			container = container.add_component(separator()).add_component(
				CreateContainerComponent::MediaGallery(CreateMediaGallery::new(image)),
			);
		}
	}
	let component = [CreateComponent::Container(container)];
	let avatar = new_message.author.face();
	let message = ExecuteWebhook::new()
		.with_components(true)
		.flags(MessageFlags::IS_COMPONENTS_V2)
		.username(&new_message.author.name)
		.components(&component)
		.avatar_url(avatar);
	for (guild_id, guild_channel_id) in guild_global_chats.iter().map(|record| {
		(
			GuildId::new(record.guild_id.cast_unsigned()),
			GenericChannelId::new(record.global_chat_channel.cast_unsigned()),
		)
	}) {
		let webhook = match webhook_find(
			ctx,
			Some(guild_id),
			guild_channel_id,
			&bot_data.channel_webhooks,
		)
		.await
		{
			Ok(Some(webhook)) => webhook,
			Ok(None) => continue,
			Err(err) => {
				error!("Failed to find webhook: {err}");
				guild_channel_id
					.say(
						&ctx.http,
						format!(
							"{} sent this: {}",
							new_message.author.name,
							new_message.content.as_str()
						),
					)
					.await?;
				continue;
			}
		};
		if let Err(err) = webhook.execute(&ctx.http, false, message.clone()).await {
			error!("Failed to execute webhook: {err}");
			guild_channel_id
				.say(
					&ctx.http,
					format!(
						"{} sent this: {}",
						new_message.author.name,
						new_message.content.as_str()
					),
				)
				.await?;
		}
	}

	Ok(())
}

async fn message_preview(ctx: &SContext, new_message: &Message) -> AResult<()> {
	if let Ok(link) = discord_message_link.parse_next(&mut new_message.content.as_str()) {
		let (channel_id, message_id) = (
			GenericChannelId::new(link.channel),
			MessageId::new(link.message),
		);
		let ref_msg = channel_id.message(&ctx.http, message_id).await?;
		if ref_msg.poll.is_none() {
			counter!(METRICS.message_previews.as_str()).increment(1);
			let avatar = ref_msg.author.face();
			let mut text = String::with_capacity(usize::from(
				u16::from(ref_msg.author.name.len())
					.saturating_add(ref_msg.content.len())
					.saturating_add(128),
			));

			write!(
				text,
				"# {}\n**Timestamp:** <t:{}:F>\n**Channel:** <#{channel_id}>\n\n*{}*",
				ref_msg.author.name,
				ref_msg.timestamp.timestamp(),
				ref_msg.content
			)?;
			let (text, thumbnail) = thumbnail_section(&text, &avatar);
			let text_array = [text];
			let thumbnail_display = [CreateContainerComponent::Section(CreateSection::new(
				&text_array,
				thumbnail,
			))];
			let mut container =
				CreateContainer::new(&thumbnail_display).accent_colour(Colour::ORANGE);
			if let Some(attachment) = ref_msg
				.attachments
				.iter()
				.find(|a| a.dimensions().is_some())
			{
				let image = vec![media_gallery(attachment.url.as_str())];
				container = container.add_component(CreateContainerComponent::MediaGallery(
					CreateMediaGallery::new(image),
				));
			}
			let component = [CreateComponent::Container(container)];
			let mut message = message_container(&component);
			if ref_msg.channel_id == new_message.channel_id {
				message = message.reference_message(&ref_msg);
			}
			new_message
				.channel_id
				.send_message(&ctx.http, message)
				.await?;
		}
	}

	Ok(())
}

async fn user_queries(
	ctx: &SContext,
	new_message: &Message,
	guild_id: i64,
	conn: &Pool<Postgres>,
) -> AResult<()> {
	let user_id_i64 = i64::from(new_message.author.id);
	let author_settings_opt = fetch_user_settings(guild_id, user_id_i64, conn).await?;

	if let Some(settings) = author_settings_opt {
		counter!(METRICS.user_afks.as_str()).increment(1);
		let text = format!(
			"# Ugh, welcome back <@{}>! Guess I didn't manage to kill you after all",
			new_message.author.id
		);
		let title_display = [text_display(text)];
		let mut container = CreateContainer::new(&title_display).accent_colour(Colour::BLITZ_BLUE);

		if !settings.pinged_links.0.is_empty() {
			let mut list = String::with_capacity(512);
			list.push_str("## Pinged links:\n");

			for entry in &settings.pinged_links.0 {
				writeln!(list, "**<@{}>**: {}", entry.author_id, entry.link)?;
			}
			list.truncate(MESSAGE_LIMIT);
			let text_display = text_display(list);
			container = container
				.add_component(separator())
				.add_component(text_display);
		}

		let component = [CreateComponent::Container(container)];
		new_message
			.channel_id
			.send_message(
				&ctx.http,
				message_container(&component).reference_message(new_message),
			)
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

		if mentioned_settings.is_empty() {
			return Ok(());
		}

		let new_message_link = new_message.link().to_string();

		let (entries, user_ids): (Vec<Value>, Vec<i64>) = mentioned_settings
			.iter()
			.filter(|s| s.afk)
			.map(|s| {
				let entry = PingedLink {
					link: new_message_link.clone(),
					author_id: new_message.author.id.get().cast_signed(),
				};
				(to_value(entry).unwrap(), s.user_id)
			})
			.unzip();

		if !entries.is_empty() {
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
			if mentioned_user_settings.afk {
				let reason = mentioned_user_settings
					.afk_reason
					.as_deref()
					.unwrap_or(DEFAULT_AFK_REASON);
				new_message
					.channel_id
					.send_message(
						&ctx.http,
						silent_message(&format!(
							"<@{}> is currently dead. Reason: {reason}",
							mentioned_user_settings.user_id
						)),
					)
					.await?;
			}
			if let Some(ping_content) = &mentioned_user_settings.ping_content {
				counter!(METRICS.custom_user_pings.as_str()).increment(1);
				let title = format!("# {ping_content}");
				let text_display = [text_display(&title)];
				let mut container =
					CreateContainer::new(&text_display).accent_colour(Colour::BLITZ_BLUE);
				if let Some(ping_media) = mentioned_user_settings.ping_media {
					let media = if ping_media.eq_ignore_ascii_case("waifu") {
						get_waifu().await
					} else if let Some(gif_query) = ping_media.strip_prefix("!gif") {
						get_gif(gif_query).await
					} else {
						Cow::Owned(ping_media)
					};
					let image = vec![media_gallery(media)];
					container = container.add_component(CreateContainerComponent::MediaGallery(
						CreateMediaGallery::new(image),
					));
				}
				let component = [CreateComponent::Container(container)];
				new_message
					.channel_id
					.send_message(
						&ctx.http,
						message_container(&component).reference_message(new_message),
					)
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

	counter!(METRICS.word_reactions.as_str())
		.increment(u64::try_from(word_reactions.len()).unwrap());
	for record in word_reactions {
		if let Some(content) = &record.content {
			let title = format!("# {content}");
			let text_display = [text_display(&title)];
			let mut container = CreateContainer::new(&text_display).accent_colour(Colour::GOLD);
			if let Some(reaction_media) = &record.media {
				let media = if let Some(gif_query) = reaction_media.strip_prefix("!gif") {
					get_gif(gif_query).await
				} else {
					Cow::Borrowed(reaction_media.as_str())
				};
				let image = vec![media_gallery(media)];
				container = container.add_component(CreateContainerComponent::MediaGallery(
					CreateMediaGallery::new(image),
				));
			}
			let component = [CreateComponent::Container(container)];
			new_message
				.channel_id
				.send_message(
					&ctx.http,
					message_container(&component).reference_message(new_message),
				)
				.await?;
		} else if let Some(emoji_id) = &record.emoji_id {
			let emoji_id_typed = EmojiId::new(emoji_id.cast_unsigned());
			let (is_animated, emoji_name) = if record.guild_emoji
				&& let Ok(guild_emoji) = guild_id.emoji(&ctx.http, emoji_id_typed).await
			{
				(guild_emoji.animated(), guild_emoji.name)
			} else if let Some(emoji) = get_emoji(ctx, &bot_data.app_emojis, emoji_id_typed).await {
				(emoji.is_animated, emoji.name.clone())
			} else {
				continue;
			};
			let reaction = ReactionType::Custom {
				animated: is_animated,
				id: emoji_id_typed,
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
) -> AResult<()> {
	let bot_data: Arc<Data> = ctx.data();

	user_queries(ctx, new_message, guild_id_i64, &bot_data.db).await?;

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
		counter!(METRICS.words_tracked.as_str()).increment(updated_words.rows_affected());
	}

	if !word_reactions.is_empty() {
		guild_queries(ctx, new_message, &word_reactions, guild_id).await?;
	}

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
	let channel_id_i64 = i64::from(new_message.channel_id);

	let guild_settings_opt =
		fetch_guild_settings(guild_id_i64, channel_id_i64, &bot_data.db).await?;

	if let Some(guild_settings) = guild_settings_opt {
		if let Some(spoiler_channel) = guild_settings.spoiler_channel
			&& spoiler_channel == channel_id_i64
		{
			spoiler_message(ctx, new_message, &bot_data.channel_webhooks).await?;
		}

		if let Some(global_chat_channel) = guild_settings.global_chat_channel
			&& global_chat_channel == channel_id_i64
		{
			global_chats(ctx, new_message, guild_id_i64).await?;
		}

		if !new_message.content.starts_with('#') {
			if let Some(ai_chat_channel) = guild_settings.ai_chat_channel
				&& ai_chat_channel == channel_id_i64
			{
				let guild_cache = guild_cache(&bot_data, guild_id, Some(user_id_i64), ctx).await?;
				ai_chats(
					new_message.clone(),
					guild_cache.ai_queue.clone(),
					guild_settings.chatbot_role.clone(),
				)
				.await?;
			}
			if let Some(music_channel) = guild_settings.music_channel
				&& music_channel == channel_id_i64
			{
				queue_track(ctx, new_message, &bot_data.db, guild_id).await?;
			}
		}
	}

	try_join!(
		check_bot_ping(ctx, new_message),
		easter_eggs(ctx, new_message, &bot_data.channel_webhooks),
		message_preview(ctx, new_message),
	)?;

	db_queries(ctx, new_message, guild_id, guild_id_i64).await?;

	Ok(())
}
