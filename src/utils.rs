use poise::serenity_prelude::{self as serenity, CreateEmbed, CreateMessage, ExecuteWebhook};

use rand::Rng;
use serenity::{
    builder::CreateAttachment,
    json::json,
    model::{colour::Colour, prelude::Timestamp},
};
use std::{fs, fs::File, io::Write, path::Path};

pub async fn embed_builder(
    ctx: &serenity::Context,
    message: &serenity::Message,
    title: &str,
    url: &str,
    colour: Colour,
) {
    let _ = message
        .channel_id
        .send_message(
            &ctx.http,
            CreateMessage::default().embed(
                CreateEmbed::new()
                    .title(title)
                    .image(url)
                    .color(colour)
                    .timestamp(Timestamp::now()),
            ),
        )
        .await;
}

pub async fn emoji_id(
    ctx: &serenity::Context,
    message: &serenity::Message,
    emoji_name: &str,
) -> String {
    let guild = message.guild_id.unwrap();
    match guild.emojis(&ctx.http).await {
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

pub fn random_number(count: usize) -> usize {
    rand::thread_rng().gen_range(0..count)
}

pub async fn spoiler_message(ctx: &serenity::Context, message: &serenity::Message, text: &str) {
    let avatar_url = message.author.avatar_url().unwrap_or_default();
    let username = &message.author_nick(&ctx.http).await.unwrap_or_default();
    let mut index = 0;
    let client = reqwest::Client::new();
    for attachment in &message.attachments {
        let target = &attachment.url;
        let response = client.get(target).send().await;
        let download = response.unwrap().bytes().await;
        let filename = format!("SPOILER_{}", &attachment.filename);
        let file = File::create(&filename);
        let download_bytes = match download {
            Ok(bytes) => bytes,
            Err(_e) => {
                continue;
            }
        };
        let _ = file.unwrap().write_all(&download_bytes);
        if index == 0 {
            webhook_file(ctx, message, username, &avatar_url, text, &filename, 0).await;
            index = 1;
        } else {
            webhook_file(ctx, message, username, &avatar_url, text, &filename, 1).await;
        }
        let _ = fs::remove_file(&filename);
    }
    let _ = message.delete(&ctx).await;
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
        Err(err) => {
            eprintln!("Error retrieving webhooks: {:?}", err);
            return;
        }
    };
    if existing_webhooks.len() >= 15 {
        for webhook in &existing_webhooks {
            let _ = (ctx.http).delete_webhook(webhook.id, None).await;
        }
    }
    if let Some(existing_webhook) = existing_webhooks
        .iter()
        .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
    {
        let _ = existing_webhook
            .execute(
                &ctx.http,
                false,
                ExecuteWebhook::new()
                    .username(name)
                    .avatar_url(url)
                    .content(output),
            )
            .await;
    } else {
        let new_webhook = ctx
            .http
            .create_webhook(channel_id, &webhook_info, None)
            .await;
        let _ = new_webhook
            .unwrap()
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
        Err(err) => {
            eprintln!("Error retrieving webhooks: {:?}", err);
            return;
        }
    };
    if existing_webhooks.len() >= 15 {
        for webhook in &existing_webhooks {
            let _ = (ctx.http).delete_webhook(webhook.id, None).await;
        }
    }
    if mode == 0 {
        if let Some(existing_webhook) = existing_webhooks
            .iter()
            .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
        {
            let _ = existing_webhook
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
            let new_webhook = ctx
                .http
                .create_webhook(channel_id, &webhook_info, None)
                .await;

            let _ = new_webhook
                .unwrap()
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
        }
    } else if let Some(existing_webhook) = existing_webhooks
        .iter()
        .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
    {
        let _ = existing_webhook
            .execute(
                &ctx.http,
                false,
                ExecuteWebhook::new()
                    .username(name)
                    .avatar_url(url)
                    .add_file(attachment.unwrap()),
            )
            .await;
    } else {
        let new_webhook = ctx
            .http
            .create_webhook(channel_id, &webhook_info, None)
            .await;
        let _ = new_webhook
            .unwrap()
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
