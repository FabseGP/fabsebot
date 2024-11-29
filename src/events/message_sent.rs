use crate::{
    config::{
        constants::{COLOUR_BLUE, COLOUR_ORANGE, COLOUR_RED, COLOUR_YELLOW, DEFAULT_BOT_ROLE},
        settings::WordTracking,
        types::{Data, Error, RNG, UTILS_CONFIG},
    },
    utils::{
        ai::ai_chatbot,
        helpers::{discord_message_link, get_gifs, get_waifu},
        webhook::{spoiler_message, webhook_find},
    },
};

use anyhow::Context as _;
use poise::serenity_prelude::{
    self as serenity, ChannelId, CreateAllowedMentions, CreateAttachment, CreateEmbed,
    CreateEmbedAuthor, CreateEmbedFooter, CreateMessage, EditMessage, EmojiId, ExecuteWebhook,
    GuildId, Message, MessageId, ReactionType, Timestamp, UserId,
};
use sqlx::query;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::task::spawn;
use winnow::Parser;

pub async fn handle_message(
    ctx: &serenity::Context,
    data: Arc<Data>,
    new_message: &Message,
) -> Result<(), Error> {
    if new_message.author.bot() {
        return Ok(());
    }
    let content = new_message.content.to_lowercase();
    if new_message.mentions_user_id(ctx.cache.current_user().id)
        && new_message.referenced_message.is_none()
    {
        new_message
            .channel_id
            .send_message(
                &ctx.http,
                CreateMessage::default()
                    .embed(
                        CreateEmbed::default()
                            .title(
                                &UTILS_CONFIG
                                    .get()
                                    .expect("UTILS_CONFIG must be set during initialization")
                                    .bot
                                    .ping_message,
                            )
                            .image("https://media.tenor.com/HNshDeQoEKsAAAAd/psyduck-hit-smash.gif")
                            .colour(COLOUR_BLUE),
                    )
                    .reference_message(new_message)
                    .allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
            )
            .await?;
    }
    let app_emojis = ctx.get_application_emojis().await?;
    if content == "floppaganda" {
        new_message
            .channel_id
            .send_message(
                &ctx.http,
                CreateMessage::default()
                    .content("https://i.imgur.com/Pys97pb.png")
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
            if let Ok(webhook) =
                webhook_find(ctx, new_message.channel_id, data.channel_webhooks.clone()).await
            {
                webhook
                    .execute(
                        &ctx.http,
                        false,
                        ExecuteWebhook::default()
                            .username("vilbot")
                            .avatar_url("https://i.postimg.cc/44t5vzWB/IMG-0014.png")
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
    }
    if content == "fabse" || content == "fabseman" {
        if let Ok(webhook) =
            webhook_find(ctx, new_message.channel_id, data.channel_webhooks.clone()).await
        {
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
    }
    if let Some(guild_id) = new_message.guild_id {
        let guild_id_i64 = i64::from(guild_id);
        let user_id_i64 = i64::from(new_message.author.id);
        let mut tx = data
            .db
            .begin()
            .await
            .context("Failed to acquire savepoint")?;
        let mut bot_role = String::with_capacity(DEFAULT_BOT_ROLE.len());
        {
            let mut modified_settings = data
                .user_settings
                .lock()
                .await
                .get(&guild_id)
                .unwrap_or_default()
                .as_ref()
                .clone();
            for target in modified_settings.iter_mut().map(|t| t.1) {
                let user_id = UserId::new(
                    u64::try_from(target.user_id).expect("user id out of bounds for u64"),
                );
                if user_id == new_message.author.id {
                    bot_role = target
                        .chatbot_role
                        .clone()
                        .unwrap_or_else(|| DEFAULT_BOT_ROLE.to_owned());
                }
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
                        "UPDATE user_settings SET afk = FALSE, afk_reason = NULL, pinged_links = NULL WHERE guild_id = $1 AND user_id = $2",
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
                                    let urls = get_gifs(gif_query).await;
                                    urls.get(RNG.lock().await.usize(..urls.len())).cloned()
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
                    target.message_count += 1;
                }
            }
            data.user_settings
                .lock()
                .await
                .insert(guild_id, Arc::new(modified_settings));
        }
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
        {
            let guild_data_opt = data.guild_data.lock().await.get(&guild_id);
            if let Some(guild_data) = guild_data_opt {
                if let Some(spoiler_channel) = guild_data.settings.spoiler_channel
                    && new_message.channel_id.get()
                        == u64::try_from(spoiler_channel).expect("channel id out of bounds for u64")
                {
                    spoiler_message(ctx, new_message, data.channel_webhooks.clone()).await?;
                }
                if let (Some(dead_channel), Some(dead_chat_rate)) = (
                    guild_data.settings.dead_chat_channel,
                    guild_data.settings.dead_chat_rate,
                ) {
                    let dead_chat_channel = ChannelId::new(
                        u64::try_from(dead_channel).expect("channel id out of bounds for u64"),
                    );
                    if let Ok(guild_channel) = dead_chat_channel
                        .to_guild_channel(&ctx.http, Some(guild_id))
                        .await
                        && let Some(message_id) = guild_channel.last_message_id
                    {
                        let last_message_time = guild_channel
                            .id
                            .message(&ctx.http, message_id)
                            .await?
                            .timestamp
                            .timestamp();
                        let current_time = Timestamp::now().timestamp();
                        if current_time - last_message_time > dead_chat_rate * 60 {
                            let urls = get_gifs("dead chat").await;
                            let index = RNG.lock().await.usize(..urls.len());
                            if let Some(url) = urls.get(index).cloned() {
                                dead_chat_channel.say(&ctx.http, url).await?;
                            }
                        }
                    }
                }
                if let Some(ai_chat_channel) = guild_data.settings.ai_chat_channel
                    && new_message.channel_id.get()
                        == u64::try_from(ai_chat_channel).expect("channel id out of bounds for u64")
                {
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
                    let ctx_clone = ctx.clone();
                    let music_manager_clone = data.music_manager.get(guild_id).clone();
                    let new_message_clone = new_message.clone();
                    let bot_role_clone = bot_role.clone();
                    let guild_id_clone = guild_id;
                    spawn(async move {
                        if let Err(e) = ai_chatbot(
                            &ctx_clone,
                            &new_message_clone,
                            bot_role_clone,
                            guild_id_clone,
                            &guild_ai_chats,
                            music_manager_clone,
                        )
                        .await
                        {
                            eprintln!("AI chatbot error: {e:?}");
                        }
                    });
                }
                if let Some(global_chat_channel) = guild_data.settings.global_chat_channel
                    && new_message.channel_id.get()
                        == u64::try_from(global_chat_channel)
                            .expect("channel id out of bounds for u64")
                    && guild_data.settings.global_chat
                {
                    let guild_global_chats: Vec<_> = data
                        .guild_data
                        .lock()
                        .await
                        .iter()
                        .filter(|entry| {
                            let settings = &entry.value().settings;
                            entry.key() != &guild_id
                                && settings.global_chat_channel.is_some()
                                && settings.global_chat
                        })
                        .map(|entry| {
                            (
                                GuildId::new(
                                    u64::try_from(entry.value().settings.guild_id)
                                        .expect("guild id out of bounds for u64"),
                                ),
                                entry.value().settings.global_chat_channel.unwrap(),
                            )
                        })
                        .collect();
                    {
                        if let Some(global_chats_history) = data.global_chats.get(&guild_id) {
                            let mut global_chats_history_clone =
                                global_chats_history.as_ref().clone();
                            for (target_guild_id, _) in &guild_global_chats {
                                global_chats_history_clone.insert(*target_guild_id, new_message.id);
                            }
                            data.global_chats
                                .insert(guild_id, Arc::new(global_chats_history_clone));
                        } else {
                            let mut new_history = HashMap::new();
                            for (target_guild_id, _) in &guild_global_chats {
                                new_history.insert(*target_guild_id, new_message.id);
                            }
                            data.global_chats.insert(guild_id, Arc::new(new_history));
                        }
                    }
                    for (guild_id, guild_channel_id) in &guild_global_chats {
                        let channel_id_type = ChannelId::new(
                            u64::try_from(*guild_channel_id)
                                .expect("channel id out of bounds for u64"),
                        );
                        if let Some(chat_channel) = channel_id_type
                            .to_channel(&ctx.http, Some(*guild_id))
                            .await?
                            .guild()
                        {
                            match webhook_find(ctx, chat_channel.id, data.channel_webhooks.clone())
                                .await
                            {
                                Ok(webhook) => {
                                    let content = if new_message.content.is_empty() {
                                        ""
                                    } else {
                                        new_message.content.as_str()
                                    };
                                    let mut message = ExecuteWebhook::default()
                                        .username(new_message.author.display_name())
                                        .avatar_url(new_message.author.avatar_url().unwrap_or_else(
                                            || {
                                                new_message
                                                    .author
                                                    .static_avatar_url()
                                                    .unwrap_or_else(|| {
                                                        new_message.author.default_avatar_url()
                                                    })
                                            },
                                        ))
                                        .content(content);
                                    if !new_message.attachments.is_empty() {
                                        for attachment in &new_message.attachments {
                                            if attachment.dimensions().is_some() {
                                                message = message.add_file(
                                                    CreateAttachment::url(
                                                        &ctx.http,
                                                        attachment.url.as_str(),
                                                        attachment.filename.to_string(),
                                                    )
                                                    .await?,
                                                );
                                            }
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
                                                    replied_message
                                                        .author
                                                        .avatar_url()
                                                        .unwrap_or_else(|| {
                                                            replied_message
                                                                .author
                                                                .default_avatar_url()
                                                        }),
                                                ),
                                            )
                                            .timestamp(new_message.timestamp);
                                        if let Some(attachment) =
                                            replied_message.attachments.first()
                                        {
                                            embed = embed.image(attachment.url.as_str());
                                        }
                                        message = message.embed(embed);
                                    }
                                    if webhook.execute(&ctx.http, false, message).await.is_err() {
                                        chat_channel
                                            .id
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
                                _ => {
                                    chat_channel
                                        .id
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
                }
                {
                    let mut word_tracking_updates: HashSet<WordTracking> =
                        guild_data.word_tracking.clone();
                    for record in &guild_data.word_tracking {
                        if content.contains(&record.word) {
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
                                updated_record.count += 1;
                                word_tracking_updates.insert(updated_record);
                            }
                        }
                    }
                    let guild_data_lock = data.guild_data.lock().await;
                    let mut current_settings_opt = guild_data_lock.get(&guild_id);
                    let mut modified_settings = current_settings_opt
                        .get_or_insert_default()
                        .as_ref()
                        .clone();
                    modified_settings.word_tracking = word_tracking_updates;
                    guild_data_lock.insert(guild_id, Arc::new(modified_settings));
                }
                for record in &guild_data.word_reactions {
                    if content.contains(&record.word) {
                        let message = {
                            let base = CreateMessage::default()
                                .reference_message(new_message)
                                .allowed_mentions(
                                    CreateAllowedMentions::default().replied_user(false),
                                );
                            match &record.media {
                                Some(media) if !media.is_empty() => {
                                    if let Some(gif_query) = media.strip_prefix("!gif") {
                                        let urls = get_gifs(gif_query).await;
                                        let mut embed = CreateEmbed::default()
                                            .title(&record.content)
                                            .colour(COLOUR_YELLOW);
                                        let index = RNG.lock().await.usize(..urls.len());
                                        if let Some(url) = urls.get(index).cloned() {
                                            embed = embed.image(url);
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
                }
                for record in &guild_data.emoji_reactions {
                    if content.contains(&record.content_reaction) {
                        let emoji_id_typed = EmojiId::new(
                            u64::try_from(record.emoji_id).expect("emoji id out of bounds for u64"),
                        );
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
                    }
                }
            }
        }
        tx.commit()
            .await
            .context("Failed to commit sql-transaction")?;
        if let Ok(link) = discord_message_link.parse_next(&mut content.as_str()) {
            let (guild_id, channel_id, message_id) = (
                GuildId::new(link.guild_id),
                ChannelId::new(link.channel_id),
                MessageId::new(link.message_id),
            );
            if let Ok(ref_channel) = channel_id.to_guild_channel(&ctx.http, Some(guild_id)).await {
                let (channel_name, ref_msg) = (
                    ref_channel.name.as_str(),
                    ref_channel.id.message(&ctx.http, message_id).await?,
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
                    if let Some(ref_embed) = ref_msg.embeds.first() {
                        preview_message =
                            preview_message.add_embed(CreateEmbed::from(ref_embed.clone()));
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
    }

    Ok(())
}
