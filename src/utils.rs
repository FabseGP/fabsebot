use crate::{
    commands::music::get_configured_handler,
    consts::{GIF_FALLBACK, WAIFU_FALLBACK, WAIFU_URL},
    types::{
        AIChatMessage, Data, Error, AI_SERVER, AI_TOKEN, CHANNEL_REGEX, HTTP_CLIENT,
        IMAGE_DESC_SERVER, QUOTE_REGEX, TENOR_TOKEN, TEXT_GEN_SERVER, TTS_SERVER,
    },
};

use ab_glyph::{FontArc, PxScale};
use anyhow::anyhow;
use base64::{engine::general_purpose, Engine};
use core::cmp::Ordering;
use dashmap::{DashMap, DashSet};
use image::{
    imageops::{overlay, resize, FilterType::Gaussian},
    load_from_memory, Rgba, RgbaImage,
};
use imageproc::drawing::{draw_text_mut, text_size};
use poise::serenity_prelude::{
    self as serenity, builder::CreateAttachment, ChannelId, ExecuteWebhook, GuildId, Http, Message,
    MessageId, Webhook,
};
use serde::{Deserialize, Serialize};
use songbird::{input::Input, Call};
use std::{fmt::Write, path::Path, sync::Arc};
use textwrap::wrap;
use tokio::{
    fs::{remove_file, File},
    io::AsyncWriteExt as _,
    sync::Mutex,
};
use urlencoding::encode;

pub async fn ai_chatbot(
    ctx: &serenity::Context,
    message: &Message,
    bot_role: String,
    guild_id: GuildId,
    conversations: &Arc<DashMap<GuildId, Vec<AIChatMessage>>>,
    voice_handle: Option<Arc<Mutex<Call>>>,
) -> Result<(), Error> {
    if message.content.eq_ignore_ascii_case("clear") {
        if conversations.remove(&guild_id).is_some() {
            message.reply(&ctx.http, "Conversation cleared!").await?;
        } else {
            message.reply(&ctx.http, "Bruh, nothing to clear!").await?;
        }
        return Ok(());
    }
    if !message.content.starts_with('#') {
        let typing = message
            .channel_id
            .start_typing(Arc::<Http>::clone(&ctx.http));
        let author_name = message.author.display_name();
        let mut system_content = bot_role;
        if let Some(reply) = &message.referenced_message {
            let ref_name = reply.author.display_name();
            write!(
                system_content,
                "\n{author_name} replied to a message sent by: {ref_name} and had this content: {}",
                reply.content
            )?;
        }
        if let Ok(author_member) = guild_id.member(&ctx.http, message.author.id).await {
            if let Some(author_roles) = author_member.roles(&ctx.cache) {
                let roles_joined = author_roles
                    .iter()
                    .map(|role| role.name.as_str())
                    .intersperse(", ")
                    .collect::<String>();
                write!(
                    system_content,
                    "\n{author_name} has the following roles: {roles_joined}"
                )?;
            }
            if !message.mentions.is_empty() {
                write!(
                    system_content,
                    "\n{} user(s) were mentioned:",
                    message.mentions.len()
                )?;
                for target in &message.mentions {
                    if let Ok(target_member) = guild_id.member(&ctx.http, target.id).await {
                        let target_roles = target_member.roles(&ctx.cache).map_or_else(
                            || "No roles found".to_owned(),
                            |roles| {
                                roles
                                    .iter()
                                    .map(|role| role.name.as_str())
                                    .intersperse(", ")
                                    .collect::<String>()
                            },
                        );
                        let pfp_desc = match HTTP_CLIENT
                            .get(target.avatar_url().unwrap_or_else(|| target.static_face()))
                            .send()
                            .await
                        {
                            Ok(pfp) => {
                                let binary_pfp = pfp.bytes().await?;
                                (ai_image_desc(&binary_pfp, None).await)
                                    .map_or_else(|| "Unable to describe".to_owned(), |desc| desc)
                            }
                            Err(_) => "Unable to describe".to_owned(),
                        };
                        let target_name = target.display_name();
                        write!(
                            system_content,
                            "\n{target_name} was mentioned. Roles: {target_roles}. Profile picture: {pfp_desc}"
                        )?;
                    }
                }
            }
        }
        if !message.attachments.is_empty() {
            write!(
                system_content,
                "\n{} image(s) were sent:",
                message.attachments.len()
            )?;
            for attachment in &message.attachments {
                if let Some(content_type) = attachment.content_type.as_deref() {
                    if content_type.starts_with("image") {
                        let file = attachment.download().await?;
                        if let Some(desc) = ai_image_desc(&file, Some(&message.content)).await {
                            write!(system_content, "\n{desc}")?;
                        }
                    }
                }
            }
        }
        if let Some(url) = CHANNEL_REGEX.captures(&message.content) {
            let guild_id = GuildId::new(url[1].parse().unwrap());
            let channel_id = ChannelId::new(url[2].parse().unwrap());
            let message_id = MessageId::new(url[3].parse().unwrap());
            if let Ok(ref_channel) = channel_id.to_guild_channel(&ctx.http, Some(guild_id)).await {
                let (guild_name, ref_msg) = (
                    guild_id
                        .name(&ctx.cache)
                        .unwrap_or_else(|| "unknown".to_owned()),
                    ref_channel.message(&ctx.http, message_id).await,
                );
                match ref_msg {
                    Ok(linked_message) => {
                        let link_author = linked_message.author.display_name();
                        let link_content = linked_message.content;
                        write!(
                            system_content,
                            "\n{author_name} linked to a message sent in: {guild_name}, sent by: {link_author} and had this content: {link_content}"
                        )?;
                    }
                    Err(_) => {
                        write!(
                            system_content,
                            "\n{author_name} linked to a message in non-accessible guild"
                        )?;
                    }
                }
            }
        }
        let response_opt = {
            let content_safe = message.content_safe(&ctx.cache);
            let mut history = conversations.entry(guild_id).or_default();

            if history.iter().any(|message| message.role == "user") {
                system_content.push_str("\nCurrent users in the conversation");
                let mut is_first = true;
                let seen_users = DashSet::new();
                for message in history.iter() {
                    if message.role == "user" {
                        if let Some(user) = message.content.split(':').next().map(str::trim) {
                            if seen_users.insert(user) {
                                if !is_first {
                                    system_content.push('\n');
                                }
                                system_content.push_str(user);
                                is_first = false;
                            }
                        }
                    }
                }
            }

            let system_message = history.iter_mut().find(|msg| msg.role == "system");

            match system_message {
                Some(system_message) => {
                    system_message.content = system_content;
                }
                None => {
                    history.push(AIChatMessage {
                        role: "system".to_owned(),
                        content: system_content,
                    });
                }
            }
            history.push(AIChatMessage {
                role: "user".to_owned(),
                content: format!("User: {author_name}: {content_safe}"),
            });
            ai_response(&history).await
        };

        if let Some(response) = response_opt {
            if response.len() >= 2000 {
                let mut start = 0;
                while start < response.len() {
                    let end = response[start..]
                        .char_indices()
                        .take_while(|(i, _)| *i < 2000)
                        .last()
                        .map_or(response.len(), |(i, c)| start + i + c.len_utf8());
                    message.reply(&ctx.http, &response[start..end]).await?;
                    start = end;
                }
            } else {
                message.reply(&ctx.http, response.as_str()).await?;
            }
            if let Some(handler_lock) = voice_handle {
                if let Some(bytes) = ai_voice(&response).await {
                    get_configured_handler(&handler_lock)
                        .await
                        .enqueue_input(Input::from(bytes))
                        .await;
                }
            }
            conversations
                .entry(guild_id)
                .or_default()
                .push(AIChatMessage {
                    role: "assistant".to_owned(),
                    content: response,
                });
        } else {
            let error_msg = "Sorry, I had to forget our convo, too boring!";
            {
                let mut history = conversations.entry(guild_id).or_default();
                history.clear();
                history.push(AIChatMessage {
                    role: "assistant".to_owned(),
                    content: error_msg.to_owned(),
                });
            }
            message.reply(&ctx.http, error_msg).await?;
        }
        typing.stop();
    }
    Ok(())
}

#[derive(Deserialize)]
struct FabseAIText {
    result: AIResponseText,
}
#[derive(Deserialize)]
struct AIResponseText {
    response: String,
}

#[derive(Serialize)]
struct SimpleMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ImageDesc<'a> {
    messages: [SimpleMessage<'a>; 2],
    image: &'a [u8],
}

pub async fn ai_image_desc(content: &[u8], user_context: Option<&str>) -> Option<String> {
    let request = ImageDesc {
        messages: [
            SimpleMessage {
                role: "system",
                content: "Generate a detailed caption for this image",
            },
            SimpleMessage {
                role: "user",
                content: user_context.map_or("What is in this image?", |context| context),
            },
        ],
        image: content,
    };
    let resp = HTTP_CLIENT
        .post(&*IMAGE_DESC_SERVER)
        .bearer_auth(&*AI_TOKEN)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIText>()
        .await
        .ok()
        .map(|output| output.result.response)
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    messages: &'a [AIChatMessage],
}

pub async fn ai_response(content: &[AIChatMessage]) -> Option<String> {
    let request = ChatRequest { messages: content };
    let resp = HTTP_CLIENT
        .post(&*TEXT_GEN_SERVER)
        .bearer_auth(&*AI_TOKEN)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIText>()
        .await
        .ok()
        .map(|output| output.result.response)
}

#[derive(Deserialize)]
struct LocalAIResponse {
    message: LocalAIText,
}

#[derive(Deserialize)]
struct LocalAIText {
    content: String,
}

#[derive(Serialize)]
struct LocalAIRequest<'a> {
    model: &'static str,
    stream: bool,
    messages: &'a [AIChatMessage],
}

pub async fn ai_response_local(messages: &[AIChatMessage]) -> Option<String> {
    let request = LocalAIRequest {
        model: "meta-llama/Meta-Llama-3.1-8B-Instruct",
        stream: false,
        messages,
    };
    let resp = HTTP_CLIENT
        .post(&*AI_SERVER)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<LocalAIResponse>()
        .await
        .ok()
        .map(|output| output.message.content)
}

#[derive(Serialize)]
struct SimpleAIRequest<'a> {
    messages: [SimpleMessage<'a>; 2],
}

pub async fn ai_response_simple(role: &str, prompt: &str) -> Option<String> {
    let request = SimpleAIRequest {
        messages: [
            SimpleMessage {
                role: "system",
                content: role,
            },
            SimpleMessage {
                role: "user",
                content: prompt,
            },
        ],
    };
    let resp = HTTP_CLIENT
        .post(&*TEXT_GEN_SERVER)
        .bearer_auth(&*AI_TOKEN)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIText>()
        .await
        .ok()
        .map(|output| output.result.response)
}

#[derive(Serialize)]
struct AIVoiceRequest<'a> {
    prompt: &'a str,
    lang: &'a str,
}

#[derive(Deserialize)]
struct FabseAIVoice {
    result: AIResponseVoice,
}

#[derive(Deserialize)]
struct AIResponseVoice {
    audio: String,
}

pub async fn ai_voice(prompt: &str) -> Option<Vec<u8>> {
    let request = AIVoiceRequest {
        prompt: &prompt.replace('\'', ""),
        lang: "en",
    };
    let resp = HTTP_CLIENT
        .post(&*TTS_SERVER)
        .bearer_auth(&*AI_TOKEN)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIVoice>()
        .await
        .ok()
        .and_then(|output| general_purpose::STANDARD.decode(output.result.audio).ok())
}

pub async fn emoji_id(
    ctx: &serenity::Context,
    guild_id: GuildId,
    emoji_name: &str,
) -> Result<String, Error> {
    let guild_emojis = guild_id.emojis(&ctx.http).await?;
    guild_emojis
        .iter()
        .find_map(|e| {
            if e.name == emoji_name {
                Some(e.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("Emoji not found"))
}

#[derive(Deserialize)]
struct GifResponse {
    results: Vec<GifResult>,
}

#[derive(Deserialize)]
struct GifResult {
    media_formats: MediaFormat,
}

#[derive(Deserialize)]
struct MediaFormat {
    gif: Option<GifObject>,
}

#[derive(Deserialize)]
struct GifObject {
    url: String,
}

pub async fn get_gifs(input: &str) -> Vec<String> {
    let request_url = {
        let encoded_input = encode(input);
        format!(
            "https://tenor.googleapis.com/v2/search?q={encoded_input}&key={}&contentfilter=medium&limit=40",
            *TENOR_TOKEN,
        )
    };
    let Ok(response) = HTTP_CLIENT.get(request_url).send().await else {
        return vec![GIF_FALLBACK.to_owned()];
    };
    response.json::<GifResponse>().await.ok().map_or_else(
        || vec![GIF_FALLBACK.to_owned()],
        |urls| {
            urls.results
                .into_iter()
                .filter_map(|result| result.media_formats.gif.map(|media| media.url))
                .collect()
        },
    )
}

#[derive(Deserialize)]
struct WaifuResponse {
    images: Vec<WaifuData>,
}
#[derive(Deserialize)]
struct WaifuData {
    url: String,
}

pub async fn get_waifu() -> String {
    let Ok(response) = HTTP_CLIENT.get(WAIFU_URL).send().await else {
        return WAIFU_FALLBACK.to_owned();
    };
    response
        .json::<WaifuResponse>()
        .await
        .ok()
        .and_then(|urls| urls.images.into_iter().next().map(|img| img.url))
        .unwrap_or_else(|| WAIFU_FALLBACK.to_owned())
}

pub async fn quote_image(avatar: &RgbaImage, author_name: &str, quoted_content: &str) -> RgbaImage {
    let width = 1200;
    let height = 630;

    let avatar_image = resize(avatar, height, height, Gaussian);

    let mut img = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));

    overlay(&mut img, &avatar_image, 0, 0);

    let font_content_data = include_bytes!("../fonts/NotoSansJP-Regular.ttf");
    let font_content = FontArc::try_from_slice(font_content_data as &[u8]).unwrap();
    overlay(&mut img, &avatar_image, 0, 0);

    let font_author_data = include_bytes!("../fonts/NotoSansJP-ExtraLight.ttf");
    let font_author = FontArc::try_from_slice(font_author_data as &[u8]).unwrap();

    let content_scale = PxScale::from(128.0);
    let mut author_scale = PxScale::from(40.0);
    let white = Rgba([255, 255, 255, 255]);

    let max_content_width = width - height - 96;
    let max_content_height = height - 64;

    let mut emoji_id = String::new();
    let mut index = 0;
    let len = quoted_content.len();
    while index < len {
        if quoted_content.chars().nth(index).unwrap_or_default() == ':'
            && index + 1 < len
            && quoted_content
                .chars()
                .nth(index + 1)
                .unwrap()
                .is_ascii_digit()
        {
            let mut jindex = index + 1;
            while jindex < len {
                let current_char = quoted_content.chars().nth(jindex).unwrap();
                if current_char != '<' && current_char.is_ascii_digit() {
                    emoji_id.push(current_char);
                } else {
                    break;
                }
                jindex += 1;
            }
            break;
        }
        index += 1;
    }

    let content_filtered = QUOTE_REGEX.replace_all(quoted_content, "");

    let mut wrapped_length = 20;
    let mut wrapped_lines = wrap(&content_filtered, wrapped_length);

    let mut text_offset = 320;

    let mut total_text_height;
    let mut content_scale_adjusted = content_scale;

    loop {
        let mut all_fit = true;
        total_text_height = 0;
        let mut line_height = 0;
        let mut line_width = 0;
        let mut dimensions;
        let padding = if wrapped_lines.len() == 1 { 32 } else { 16 };

        for line in &wrapped_lines {
            dimensions = text_size(content_scale_adjusted, &font_content, line);
            line_height = dimensions.1;
            line_width = dimensions.0;

            if total_text_height + line_height + padding > max_content_height
                || line_width > max_content_width
            {
                all_fit = false;
                break;
            }

            total_text_height += line_height + 10;
        }

        if all_fit {
            if wrapped_lines.len() > 18 {
                wrapped_length += 2;
                wrapped_lines = wrap(quoted_content, wrapped_length);
                content_scale_adjusted = content_scale;
            } else {
                if wrapped_lines.len() == 1 {
                    total_text_height = line_height + 40;
                    if wrapped_lines.first().unwrap().len() < 10 {
                        text_offset += 64;
                    }
                } else {
                    total_text_height += 16;
                }
                break;
            }
        } else {
            content_scale_adjusted = PxScale::from(content_scale_adjusted.x - 1.0);
            if (content_scale_adjusted.x + 2.0 - author_scale.x).abs() < 0.1 {
                if author_scale.x.partial_cmp(&18.0) != Some(Ordering::Less) {
                    author_scale = PxScale::from(author_scale.x - 1.0);
                } else if line_width > max_content_width {
                    wrapped_length -= 2;
                    wrapped_lines = wrap(quoted_content, wrapped_length);
                } else {
                    wrapped_length += 2;
                    wrapped_lines = wrap(quoted_content, wrapped_length);
                    dimensions = text_size(
                        content_scale_adjusted,
                        &font_content,
                        wrapped_lines.first().unwrap(),
                    );
                    if dimensions.0 > max_content_width {
                        wrapped_length -= 2;
                        wrapped_lines = wrap(quoted_content, wrapped_length);
                    }
                }
                content_scale_adjusted = content_scale;
            }
        }
    }

    let (_, emoji_height) = text_size(
        content_scale_adjusted,
        &font_content,
        wrapped_lines.join("").as_str(),
    );

    let emoji_image = if !emoji_id.is_empty() {
        let emoji_url = format!(
            "https://cdn.discordapp.com/emojis/{emoji_id}.webp?size={emoji_height}quality=lossless"
        );
        let emoji_bytes = HTTP_CLIENT
            .get(&emoji_url)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        Some(load_from_memory(&emoji_bytes).unwrap().to_rgba8())
    } else {
        None
    };

    if let Some(emoji) = emoji_image {
        overlay(
            &mut img,
            &emoji,
            (width - emoji.width()).into(),
            (height - emoji.height()).into(),
        );
    }

    let mut quoted_content_y = (height - total_text_height) / 2;
    let author_name_y = quoted_content_y + total_text_height + 16;

    let (author_name_width, _author_name_height) =
        text_size(author_scale, &font_author, author_name);

    let quoted_content_x = ((width - max_content_width) / 2) + text_offset;
    let author_name_x = ((width - author_name_width) / 2) + 320;

    for line in wrapped_lines {
        draw_text_mut(
            &mut img,
            white,
            quoted_content_x.try_into().unwrap(),
            quoted_content_y.try_into().unwrap(),
            content_scale_adjusted,
            &font_content,
            &line,
        );

        let dimensions = text_size(content_scale_adjusted, &font_content, &line);
        quoted_content_y += dimensions.1 + 10;
    }

    draw_text_mut(
        &mut img,
        white,
        author_name_x.try_into().unwrap(),
        author_name_y.try_into().unwrap(),
        author_scale,
        &font_author,
        author_name,
    );

    img
}

pub async fn spoiler_message(
    ctx: &serenity::Context,
    message: &Message,
    text: &str,
    data: &Arc<Data>,
) -> Result<(), Error> {
    if let Some(avatar_url) = message.author.avatar_url() {
        let username = message.author.display_name();
        let mut is_first = true;
        for attachment in &message.attachments {
            let target = attachment.url.as_str();
            let download = HTTP_CLIENT.get(target).send().await?.bytes().await;
            let attachment_name = &attachment.filename;
            let filename = format!("SPOILER_{attachment_name}");
            let mut file = File::create(&filename).await?;
            let Ok(download_bytes) = download else {
                continue;
            };
            file.write_all(&download_bytes).await?;
            let webhook_try = webhook_find(ctx, message.channel_id, data).await;
            if let Ok(webhook) = webhook_try {
                let attachment = CreateAttachment::path(Path::new(&filename)).await?;
                if is_first {
                    webhook
                        .execute(
                            &ctx.http,
                            false,
                            ExecuteWebhook::default()
                                .username(username)
                                .avatar_url(avatar_url.as_str())
                                .content(text)
                                .add_file(attachment),
                        )
                        .await?;
                    is_first = false;
                } else {
                    webhook
                        .execute(
                            &ctx.http,
                            false,
                            ExecuteWebhook::default()
                                .username(username)
                                .avatar_url(avatar_url.as_str())
                                .add_file(attachment),
                        )
                        .await?;
                }
            }
            remove_file(&filename).await?;
        }
        message.delete(&ctx.http, None).await?;
    }
    Ok(())
}

#[derive(Serialize)]
struct WebhookInfo {
    name: &'static str,
    avatar: &'static str,
}

pub async fn webhook_find(
    ctx: &serenity::Context,
    channel_id: ChannelId,
    data: &Arc<Data>,
) -> Result<Webhook, Error> {
    if let Some(webhook) = data.webhook_cache.get(&channel_id) {
        return Ok(webhook.clone());
    }
    let existing_webhooks_get = channel_id.webhooks(&ctx.http).await;
    match existing_webhooks_get {
        Ok(existing_webhooks) => {
            if existing_webhooks.len() >= 15 {
                ctx.http
                    .delete_webhook(existing_webhooks.first().unwrap().id, None)
                    .await?;
            }
            let webhook_info = WebhookInfo {
                name: "fabsebot",
                avatar: "http://img2.wikia.nocookie.net/__cb20150611192544/pokemon/images/e/ef/Psyduck_Confusion.png",
            };
            (ctx.http
                .create_webhook(channel_id, &webhook_info, None)
                .await)
                .map_or_else(
                    |_| Err(anyhow!("")),
                    |webhook| {
                        data.webhook_cache.insert(channel_id, webhook.clone());
                        Ok(webhook)
                    },
                )
        }
        Err(_) => Err(anyhow!("")),
    }
}
