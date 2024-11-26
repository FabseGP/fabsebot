use crate::{
    config::{
        constants::{COLOUR_BLUE, COLOUR_ORANGE, COLOUR_RED, COLOUR_YELLOW, DEFAULT_BOT_ROLE},
        types::{Data, Error, RNG},
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
use std::{borrow::Cow, sync::Arc};
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
    if let Some(guild_id) = new_message.guild_id {
        let guild_id_i64 = i64::from(guild_id);
        let user_id_i64 = i64::from(new_message.author.id);
        let mut tx = data
            .db
            .begin()
            .await
            .context("Failed to acquire savepoint")?;
        let mut bot_role = String::with_capacity(DEFAULT_BOT_ROLE.len());
        if let Some(user_settings) = data.user_settings.get(&guild_id) {
            for mut target in user_settings.iter_mut() {
                let user_id = UserId::new(
                    u64::try_from(target.user_id).expect("user id out of bounds for u64"),
                );
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
            bot_role = user_settings
                .get(&new_message.author.id)
                .and_then(|setting| setting.chatbot_role.clone())
                .unwrap_or_else(|| DEFAULT_BOT_ROLE.to_owned());
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
        }
        if let Some(mut guild_data) = data.guild_data.get_mut(&guild_id) {
            if let Some(spoiler_channel) = guild_data.settings.spoiler_channel
                && new_message.channel_id.get()
                    == u64::try_from(spoiler_channel).expect("channel id out of bounds for u64")
            {
                spoiler_message(ctx, new_message, &data).await?;
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
                ai_chatbot(
                    ctx,
                    new_message,
                    bot_role,
                    guild_id,
                    &data.ai_chats,
                    data.music_manager.get(guild_id),
                )
                .await?;
            }
            if let Some(global_chat_channel) = guild_data.settings.global_chat_channel
                && new_message.channel_id.get()
                    == u64::try_from(global_chat_channel).expect("channel id out of bounds for u64")
                && guild_data.settings.global_chat
            {
                let guild_global_chats: Vec<_> = data
                    .guild_data
                    .iter()
                    .filter(|entry| {
                        let settings = &entry.value().settings;
                        entry.key() != &guild_id
                            && settings.global_chat_channel.is_some()
                            && settings.global_chat
                    })
                    .map(|entry| {
                        (
                            entry.value().settings.guild_id,
                            entry.value().settings.global_chat_channel.unwrap(),
                        )
                    })
                    .collect();
                {
                    let global_chats_history = data.global_chats.entry(guild_id).or_default();
                    for (guild_id, _) in &guild_global_chats {
                        global_chats_history.insert(*guild_id, new_message.id);
                    }
                }
                for (guild_id, guild_channel_id) in &guild_global_chats {
                    let channel_id_type = ChannelId::new(
                        u64::try_from(*guild_channel_id).expect("channel id out of bounds for u64"),
                    );
                    if let Some(chat_channel) = channel_id_type
                        .to_channel(
                            &ctx.http,
                            Some(GuildId::new(
                                u64::try_from(*guild_id).expect("guild id out of bounds for u64"),
                            )),
                        )
                        .await?
                        .guild()
                    {
                        match webhook_find(ctx, chat_channel.id, &data).await {
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
                                            new_message.author.static_avatar_url().unwrap_or_else(
                                                || new_message.author.default_avatar_url(),
                                            )
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
            for record in &mut guild_data.word_tracking {
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
                    record.count += 1;
                }
            }
            for record in &guild_data.word_reactions {
                if content.contains(&record.word) {
                    let message = {
                        let base = CreateMessage::default()
                            .reference_message(new_message)
                            .allowed_mentions(CreateAllowedMentions::default().replied_user(false));
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
            for record in &mut guild_data.emoji_reactions {
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
                            .title("why ping me bitch, go get a life!")
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
            if let Ok(webhook) = webhook_find(ctx, new_message.channel_id, &data).await {
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
        if let Ok(webhook) = webhook_find(ctx, new_message.channel_id, &data).await {
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

    Ok(())
}
