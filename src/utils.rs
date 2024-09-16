use crate::types::{get_http_client, ChatMessage, Error};

use ab_glyph::{FontArc, PxScale};
use anyhow::anyhow;
use image::{
    imageops::{overlay, resize, FilterType::Gaussian},
    load_from_memory, Rgba, RgbaImage,
};
use imageproc::drawing::{draw_text_mut, text_size};
use poise::serenity_prelude::{self as serenity, Context, CreateEmbed, ExecuteWebhook};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use serenity::{
    builder::CreateAttachment,
    model::{colour::Colour, prelude::Timestamp},
    Message,
};
use std::{cmp::Ordering, env, fs, fs::File, io::Write, path::Path};
use textwrap::wrap;
use urlencoding::encode;

#[derive(Deserialize)]
struct FabseAIImageDesc {
    result: AIResponseImageDesc,
}
#[derive(Deserialize)]
struct AIResponseImageDesc {
    description: String,
}

pub async fn ai_image_desc(content: Vec<u8>) -> Result<String, Error> {
    let client = get_http_client();
    let api_key = env::var("CLOUDFLARE_TOKEN")?;
    let gateway = env::var("CLOUDFLARE_GATEWAY")?;
    let resp = client
        .post(format!(
            "https://gateway.ai.cloudflare.com/v1/{}/workers-ai/@cf/llava-hf/llava-1.5-7b-hf",
            gateway
        ))
        .bearer_auth(api_key)
        .json(&json!({
            "image": content,
            "prompt": "Generate a detailed caption for this image"
        }))
        .send()
        .await?;

    let output = resp.json::<FabseAIImageDesc>().await?;
    Ok(output.result.description)
}

#[derive(Deserialize)]
struct FabseAIText {
    result: AIResponseText,
}
#[derive(Deserialize)]
struct AIResponseText {
    response: String,
}

pub async fn ai_response(content: Vec<ChatMessage>) -> Result<String, Error> {
    let client = get_http_client();
    let api_key = env::var("CLOUDFLARE_TOKEN")?;
    let gateway = env::var("CLOUDFLARE_GATEWAY")?;
    let resp = client
        .post(format!(
            "https://gateway.ai.cloudflare.com/v1/{}/workers-ai/@cf/meta/llama-3.1-8b-instruct",
            gateway
        ))
        .bearer_auth(api_key)
        .json(&json!({
            "messages": content
        }))
        .send()
        .await?;
    let output = resp.json::<FabseAIText>().await?;
    Ok(output.result.response)
}

#[derive(Deserialize)]
struct LocalAIResponse {
    message: LocalAIText,
}

#[derive(Deserialize)]
struct LocalAIText {
    content: String,
}

pub async fn ai_response_local(messages: Vec<ChatMessage>) -> Result<String, Error> {
    let client = get_http_client();
    let server = env::var("AI_SERVER")?;
    let resp = client
        .post(server)
        .json(&json!({
            "model": "meta-llama/Meta-Llama-3.1-8B-Instruct",
            "stream": false,
            "messages": messages,
        }))
        .send()
        .await?;

    if resp.status().is_success() {
        let output = resp.json::<LocalAIResponse>().await?;
        Ok(output.message.content)
    } else {
        ai_response(messages).await
    }
}

pub async fn ai_response_simple(role: String, prompt: String) -> Result<String, Error> {
    let client = get_http_client();
    let api_key = env::var("CLOUDFLARE_TOKEN")?;
    let gateway = env::var("CLOUDFLARE_GATEWAY")?;
    let resp = client
        .post(format!(
            "https://gateway.ai.cloudflare.com/v1/{}/workers-ai/@cf/meta/llama-3.1-8b-instruct",
            gateway
        ))
        .bearer_auth(api_key)
        .json(&json!({"messages": [
                { "role": "system", "content": role },
                { "role": "user", "content": prompt }
            ]
        }))
        .send()
        .await?;
    let output = resp.json::<FabseAIText>().await?;
    Ok(output.result.response)
}

pub fn embed_builder<'a>(title: &'a str, url: &'a str, colour: Colour) -> CreateEmbed<'a> {
    CreateEmbed::new()
        .title(title)
        .image(url)
        .color(colour)
        .timestamp(Timestamp::now())
}

pub async fn emoji_id(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    emoji_name: &str,
) -> Result<String, Error> {
    let guild_emojis = guild_id.emojis(&ctx.http).await?;
    let emoji = guild_emojis.iter().find(|e| e.name == emoji_name);
    match emoji {
        Some(emoji) => Ok(emoji.to_string()),
        None => Err(anyhow!("Emoji not found")),
    } 
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


pub async fn get_gifs(input: String) -> Result<Vec<String>, Error> {
    let api_key = env::var("TENOR_TOKEN")?;
    let request_url = format!(
        "https://tenor.googleapis.com/v2/search?q={}&key={}&contentfilter=medium&limit=40",
        encode(&input),
        api_key,
    );
    let client = get_http_client();
    let request = client.get(request_url).send().await?;
    let urls: GifResponse = request.json().await?;
    let payload = urls.results
        .iter()
        .filter_map(|result| result.media_formats.gif.as_ref())
        .map(|media| media.url.clone())
        .collect::<Vec<String>>(); 

    Ok(payload)
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
            let mut numbers: Vec<String> = Vec::new();
            while jindex < len {
                if quoted_content.chars().nth(jindex).unwrap() != '<'
                    && quoted_content.chars().nth(jindex).unwrap().is_ascii_digit()
                {
                    numbers.push(quoted_content.chars().nth(jindex).unwrap().to_string());
                } else {
                    break;
                }
                jindex += 1
            }
            emoji_id = numbers.join("");
            break;
        }
        index += 1;
    }

    let pattern = r#"<:[A-Za-z0-9_]+:[0-9]+>"#;
    let re = Regex::new(pattern).unwrap();

    let content_filtered = re.replace_all(quoted_content, "");

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
                    if wrapped_lines[0].len() < 10 {
                        text_offset += 64;
                    }
                } else {
                    total_text_height += 16;
                }
                break;
            }
        } else {
            content_scale_adjusted = PxScale::from(content_scale_adjusted.x - 1.0);
            if (content_scale_adjusted.x + 2.0) == author_scale.x {
                if author_scale.x.partial_cmp(&18.0) != Some(Ordering::Less) {
                    author_scale = PxScale::from(author_scale.x - 1.0);
                } else if line_width > max_content_width {
                    wrapped_length -= 2;
                    wrapped_lines = wrap(quoted_content, wrapped_length);
                } else {
                    wrapped_length += 2;
                    wrapped_lines = wrap(quoted_content, wrapped_length);
                    dimensions =
                        text_size(content_scale_adjusted, &font_content, &wrapped_lines[0]);
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
            "https://cdn.discordapp.com/emojis/{}.webp?size={}quality=lossless",
            emoji_id, emoji_height
        );
        let emoji_bytes = reqwest::get(&emoji_url)
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
        format!("- {}", author_name).as_str(),
    );

    img
}

pub async fn spoiler_message(ctx: &Context, message: &serenity::Message, text: &str) {
    let avatar_url = message.author.avatar_url().unwrap();
    let username = message.author.display_name();
    let mut is_first = true;
    let client = get_http_client();
    for attachment in &message.attachments {
        let target = &attachment.url.to_string();
        let response = client.get(target).send().await;
        let download = response.unwrap().bytes().await;
        let filename = format!("SPOILER_{}", &attachment.filename);
        let file = File::create(&filename);
        let download_bytes = match download {
            Ok(bytes) => bytes,
            Err(_) => {
                continue;
            }
        };
        let _ = file.unwrap().write_all(&download_bytes);
        let index = if is_first { 0 } else { 1 };
        webhook_file(ctx, message, username, &avatar_url, text, &filename, index).await;
        is_first = false;
        let _ = fs::remove_file(&filename);
    }
    let reason: Option<&str> = Some("");
    let _ = message.delete(&ctx.http, reason).await;
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
    let request_url = "https://api.waifu.im/search?height=>=2000&is_nsfw=false";
    let client = get_http_client();
    let request = client.get(request_url).send().await.unwrap();
    let url: WaifuResponse = request.json().await.unwrap();
    if !url.images[0].url.is_empty() {
        url.images[0].url.to_owned()
    } else {
        "https://media1.tenor.com/m/CzI4QNcXQ3YAAAAC/waifu-anime.gif".to_owned()
    }
}

pub async fn webhook_message(
    ctx: &Context,
    message: &Message,
    name: &str,
    url: &str,
    output: &str,
) {
    let channel_id = message.channel_id;
    let webhook_info = json!({
        "name": name,
        "avatar": url
    });
    let existing_webhooks = match channel_id.webhooks(&ctx.http).await {
        Ok(webhooks) => webhooks,
        Err(_) => {
            return;
        }
    };
    if existing_webhooks.len() >= 15 {
        let webhooks_to_delete = existing_webhooks.len() - 14;
        for webhook in existing_webhooks.iter().take(webhooks_to_delete) {
            let _ = (ctx.http).delete_webhook(webhook.id, None).await;
        }
    }
    let webhook = {
        if let Some(existing_webhook) = existing_webhooks
            .iter()
            .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
        {
            existing_webhook
        } else {
            &ctx.http
                .create_webhook(channel_id, &webhook_info, None)
                .await
                .unwrap()
        }
    };

    let _ = webhook
        .execute(
            &ctx.http,
            false,
            ExecuteWebhook::new()
                .username(name)
                .avatar_url(url)
                .content(output),
        )
        .await;
}

pub async fn webhook_file(
    ctx: &Context,
    message: &Message,
    name: &str,
    url: &str,
    text: &str,
    path: &str,
    mode: i32,
) {
    let channel_id = message.channel_id;
    let webhook_info = json!({
        "name": "test",
        "avatar": url
    });
    let attachment = CreateAttachment::path(Path::new(path)).await;
    let existing_webhooks = match channel_id.webhooks(&ctx.http).await {
        Ok(webhooks) => webhooks,
        Err(_) => {
            return;
        }
    };
    if existing_webhooks.len() >= 15 {
        let webhooks_to_delete = existing_webhooks.len() - 14;
        for webhook in existing_webhooks.iter().take(webhooks_to_delete) {
            let _ = (ctx.http).delete_webhook(webhook.id, None).await;
        }
    }

    let webhook = {
        if let Some(existing_webhook) = existing_webhooks
            .iter()
            .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
        {
            existing_webhook
        } else {
            &ctx.http
                .create_webhook(channel_id, &webhook_info, None)
                .await
                .unwrap()
        }
    };

    if mode == 0 {
        let _ = webhook
            .execute(
                &ctx.http,
                false,
                ExecuteWebhook::new()
                    .username(name)
                    .avatar_url(url)
                    .content(text)
                    .add_file(attachment.unwrap()),
            )
            .await;
    } else {
        let _ = webhook
            .execute(
                &ctx.http,
                false,
                ExecuteWebhook::new()
                    .username(name)
                    .avatar_url(url)
                    .add_file(attachment.unwrap()),
            )
            .await;
    }
}
