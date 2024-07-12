use poise::serenity_prelude::{self as serenity, CreateEmbed, ExecuteWebhook};

use ab_glyph::{FontArc, PxScale};
use image::{
    imageops::{overlay, resize, FilterType::Gaussian},
    Rgba, RgbaImage,
};
use imageproc::drawing::{draw_text_mut, text_size};
use rand::Rng;
use serde_json::json;
use serenity::{
    builder::CreateAttachment,
    model::{colour::Colour, prelude::Timestamp},
};
use std::{fs, fs::File, io::Write, path::Path};
use textwrap::wrap;

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
) -> String {
    match guild_id.emojis(&ctx.http).await {
        Ok(emojis) => {
            if let Some(emoji) = emojis.iter().find(|e| e.name == emoji_name) {
                emoji.to_string()
            } else {
                "bruh".to_string()
            }
        }
        Err(_) => "bruh".to_string(),
    }
}

pub fn quote_image(avatar: &RgbaImage, author_name: &str, quoted_content: &str) -> RgbaImage {
    let width = 1200;
    let height = 630;

    let avatar_image = resize(avatar, height, height, Gaussian);

    let mut img = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));

    overlay(&mut img, &avatar_image, 0, 0);

    let font_content_data = include_bytes!("../fonts/NotoSansJP-Regular.ttf");
    let font_content = FontArc::try_from_slice(font_content_data as &[u8]).unwrap();

    let font_author_data = include_bytes!("../fonts/NotoSansJP-ExtraLight.ttf");
    let font_author = FontArc::try_from_slice(font_author_data as &[u8]).unwrap();

    let content_scale = PxScale::from(160.0);
    let author_scale = PxScale::from(40.0);
    let white = Rgba([255, 255, 255, 255]);

    let max_content_width = width - height - 96;
    let max_content_height = height - 128;

    let mut wrapped_length = 20;
    let mut wrapped_lines = wrap(quoted_content, wrapped_length);

    let mut total_text_height;
    let mut content_scale_adjusted = content_scale;

    loop {
        let mut all_fit = true;
        total_text_height = 0;
        let mut line_height = 0;

        for line in &wrapped_lines {
            let dimensions = text_size(content_scale_adjusted, &font_content, line);
            line_height = dimensions.1;
            let line_width = dimensions.0;

            if total_text_height + line_height > max_content_height
                || line_width > max_content_width
            {
                all_fit = false;
                break;
            }

            total_text_height += line_height + 10;
        }

        if all_fit {
            if wrapped_lines.len() > 14 {
                wrapped_length += 2;
                wrapped_lines = wrap(quoted_content, wrapped_length);
                content_scale_adjusted = content_scale;
            } else {
                if wrapped_lines.len() == 1 {
                    total_text_height = line_height + 30;
                }
                break;
            }
        }

        content_scale_adjusted = PxScale::from(content_scale_adjusted.x - 1.0);
    }

    let mut quoted_content_y = (height - total_text_height) / 2;
    let author_name_y = quoted_content_y + total_text_height + 16;

    let (author_name_width, _author_name_height) =
        text_size(author_scale, &font_author, author_name);

    let quoted_content_x = ((width - max_content_width) / 2) + 320;
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

pub fn random_number(count: usize) -> usize {
    rand::thread_rng().gen_range(0..count)
}

pub async fn spoiler_message(ctx: &serenity::Context, message: &serenity::Message, text: &str) {
    let avatar_url = message.author.avatar_url().unwrap();
    let nick = message.author_nick(&ctx.http).await;
    let username = nick.as_deref().unwrap_or(message.author.name.as_str());
    let mut is_first = true;
    let client = reqwest::Client::new();
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

pub async fn webhook_message(
    ctx: &serenity::Context,
    message: &serenity::Message,
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
        for webhook in &existing_webhooks {
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
    ctx: &serenity::Context,
    message: &serenity::Message,
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
        for webhook in &existing_webhooks {
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
