use crate::{
    config::{
        constants::{
            COLOUR_BLUE, COLOUR_ORANGE, COLOUR_RED, COLOUR_YELLOW, DEFAULT_BOT_ROLE, EMOJI_KURUKURU,
        },
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
    self as serenity, ChannelId, Colour, CreateAllowedMentions, CreateAttachment, CreateEmbed,
    CreateEmbedAuthor, CreateEmbedFooter, CreateMessage, EditMessage, ExecuteWebhook, GuildId,
    Message, MessageId, ReactionType, Timestamp, UserId,
};
use sqlx::query;
use std::sync::Arc;
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
    if let Some(id) = new_message.guild_id {
        let guild_id = i64::from(id);
        let user_id = i64::from(new_message.author.id);
        let mut tx = data
            .db
            .begin()
            .await
            .context("Failed to acquire savepoint")?;
        let mut bot_role = String::new();
        if let Some(user_settings) = data.user_settings.get(&id) {
            for mut target in user_settings.iter_mut() {
                if target.afk {
                    let user_id = UserId::new(
                        u64::try_from(target.user_id).expect("user id out of bounds for u64"),
                    );
                    if new_message.author.id == user_id {
                        let user = user_id.to_user(&ctx.http).await?;
                        let user_name = user.display_name();
                        let mut response = new_message
                        .reply(
                            &ctx.http,
                            format!(
                                "Ugh, welcome back {user_name}! Guess I didn't manage to kill you after all"
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
                            guild_id,
                            target.user_id,
                        )
                        .execute(&mut *tx)
                        .await?;
                        target.afk = false;
                        target.afk_reason = None;
                        target.pinged_links = None;
                    } else if new_message.mentions_user_id(user_id) {
                        let author_name = new_message.author.display_name();
                        let message_link = new_message.link();
                        let pinged_link = format!("{author_name};{message_link}");
                        query!(
                            "UPDATE user_settings 
                            SET pinged_links = COALESCE(pinged_links || ',' || $1, $1) 
                            WHERE guild_id = $2 AND user_id = $3",
                            pinged_link,
                            guild_id,
                            target.user_id,
                        )
                        .execute(&mut *tx)
                        .await?;
                        match target.pinged_links.as_mut() {
                            Some(existing_links) => {
                                existing_links.push(',');
                                existing_links.push_str(&pinged_link);
                            }
                            None => {
                                target.pinged_links = Some(pinged_link.to_string());
                            }
                        }
                        let reason = target
                            .afk_reason
                            .as_deref()
                            .unwrap_or("Didn't renew life subscription");
                        let user = user_id.to_user(&ctx.http).await?;
                        let user_name = user.display_name();
                        new_message
                            .reply(
                                &ctx.http,
                                format!("{user_name} is currently dead. Reason: {reason}"),
                            )
                            .await?;
                    }
                }
                let target_id = target.user_id;
                if content.contains(&format!("<@{target_id}>"))
                    && let Some(ping_content) = &target.ping_content
                {
                    let message = {
                        match &target.ping_media {
                            Some(ping_media) => {
                                let media = if ping_media.eq_ignore_ascii_case("waifu") {
                                    Some(get_waifu().await)
                                } else if let Some(gif_query) = ping_media.strip_prefix("!gif") {
                                    let urls = get_gifs(gif_query).await;
                                    urls.get(RNG.lock().await.usize(..urls.len())).cloned()
                                } else if !ping_media.is_empty() {
                                    Some(ping_media.to_owned())
                                } else {
                                    None
                                };
                                media.map_or_else(
                                    || CreateMessage::default().content(ping_content),
                                    |image| {
                                        CreateMessage::default().embed(
                                            CreateEmbed::default()
                                                .title(ping_content)
                                                .colour(COLOUR_BLUE)
                                                .image(image),
                                        )
                                    },
                                )
                            }
                            None => CreateMessage::default().content(ping_content),
                        }
                    };

                    new_message
                        .channel_id
                        .send_message(&ctx.http, message)
                        .await?;
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
                guild_id,
                user_id,
            )
            .execute(&mut *tx)
            .await?;
            if let Some(mut user_setting) = user_settings.get_mut(&new_message.author.id) {
                user_setting.message_count += 1;
            }
        }
        if let Some(mut guild_data) = data.guild_data.get_mut(&id) {
            if let Some(spoiler_channel) = guild_data.settings.spoiler_channel
                && new_message.channel_id.get()
                    == u64::try_from(spoiler_channel).expect("channel id out of bounds for u64")
            {
                spoiler_message(ctx, new_message, &new_message.content, &data).await?;
            }
            if let (Some(dead_channel), Some(dead_chat_rate)) = (
                guild_data.settings.dead_chat_channel,
                guild_data.settings.dead_chat_rate,
            ) {
                let dead_chat_channel = ChannelId::new(
                    u64::try_from(dead_channel).expect("channel id out of bounds for u64"),
                );
                if let Ok(guild_channel) = dead_chat_channel
                    .to_guild_channel(&ctx.http, Some(id))
                    .await
                {
                    if let Some(message_id) = guild_channel.last_message_id {
                        let last_message_time = guild_channel
                            .id
                            .message(&ctx.http, message_id)
                            .await?
                            .timestamp
                            .timestamp();
                        let current_time = Timestamp::now().timestamp();
                        if current_time - last_message_time > dead_chat_rate * 60 {
                            let urls = get_gifs("dead chat").await;
                            dead_chat_channel
                                .say(
                                    &ctx.http,
                                    urls[RNG.lock().await.usize(..urls.len())].as_str(),
                                )
                                .await?;
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
                    id,
                    &data.ai_chats,
                    data.music_manager.get(id),
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
                        entry.key() != &id
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
                    let global_chats_history = data.global_chats.entry(id).or_default();
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
                        if let Ok(webhook) = webhook_find(ctx, chat_channel.id, &data).await {
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
                        } else {
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
            for record in &mut guild_data.word_tracking {
                if content.contains(&record.word) {
                    query!(
                        "UPDATE guild_word_tracking 
                         SET count = count + 1 
                         WHERE guild_id = $1
                         AND word = $2",
                        guild_id,
                        record.word
                    )
                    .execute(&mut *tx)
                    .await?;
                    record.count += 1;
                }
            }
            for record in &guild_data.word_reactions {
                if content.contains(&record.word) {
                    let message = match &record.media {
                        Some(media) if !media.is_empty() => {
                            if let Some(gif_query) = media.strip_prefix("!gif") {
                                let urls = get_gifs(gif_query).await;
                                CreateMessage::default().embed(
                                    CreateEmbed::default()
                                        .title(&record.content)
                                        .colour(COLOUR_YELLOW)
                                        .image(urls[RNG.lock().await.usize(..urls.len())].clone()),
                                )
                            } else {
                                CreateMessage::default().embed(
                                    CreateEmbed::default()
                                        .title(&record.content)
                                        .colour(COLOUR_YELLOW)
                                        .image(media),
                                )
                            }
                        }
                        _ => CreateMessage::default().content(&record.content),
                    };
                    new_message
                        .channel_id
                        .send_message(&ctx.http, message)
                        .await?;
                }
            }
        }
        tx.commit()
            .await
            .context("Failed to commit sql-transaction")?;
        if let Ok(link) = discord_message_link.parse_next(&mut content.as_str()) {
            let guild_id = GuildId::new(link.guild_id);
            let channel_id = ChannelId::new(link.channel_id);
            let message_id = MessageId::new(link.message_id);
            if let Ok(ref_channel) = channel_id.to_guild_channel(&ctx.http, Some(guild_id)).await {
                let (channel_name, ref_msg) = (
                    ref_channel.name.as_str(),
                    ref_channel.id.message(&ctx.http, message_id).await?,
                );
                if ref_msg.poll.is_none() {
                    let author_accent = ctx.http.get_user(ref_msg.author.id).await?.accent_colour;
                    let mut embed = CreateEmbed::default()
                        .colour(author_accent.unwrap_or(Colour::new(COLOUR_ORANGE)))
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
                    let content_url = if let Some(attachment) = ref_msg.attachments.first() {
                        if let Some(content_type) = attachment.content_type.as_deref() {
                            if content_type.starts_with("image") {
                                embed = embed.image(attachment.url.as_str());
                                None
                            } else if content_type.starts_with("video") {
                                Some(attachment.url.as_str())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
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
    if content.contains(&ctx.cache.current_user().to_string()) {
        new_message
            .channel_id
            .send_message(
                &ctx.http,
                CreateMessage::default().embed(
                    CreateEmbed::default()
                        .title("why ping me bitch, go get a life!")
                        .image("https://media.tenor.com/HNshDeQoEKsAAAAd/psyduck-hit-smash.gif")
                        .colour(COLOUR_BLUE),
                ),
            )
            .await?;
    }
    if content == "floppaganda" {
        new_message
            .channel_id
            .send_message(
                &ctx.http,
                CreateMessage::default().content("https://i.imgur.com/Pys97pb.png"),
            )
            .await?;
    } else if content.contains("kurukuru_seseren") {
        let count = content.matches("kurukuru_seseren").count() - 1;
        let mut response = String::with_capacity(EMOJI_KURUKURU.len() * count);
        for _ in 0..count {
            response.push_str(EMOJI_KURUKURU);
        }
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
    } else if content.contains("fabse") {
        if let Ok(reaction) = ReactionType::try_from("<:fabseman_willbeatu:1284742390099480631>") {
            new_message.react(&ctx.http, reaction).await?;
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
    }

    Ok(())
}
