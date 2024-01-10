use poise::serenity_prelude as serenity;
use rand::Rng;
use serenity::{
    builder::CreateButton,
    model::{
        application::{
            component::ButtonStyle,
            interaction::{Interaction, InteractionResponseType},
        },
        channel::{Channel, Message, ReactionType},
        id::EmojiId,
        prelude::{AttachmentType, ChannelId, GuildId},
        Timestamp,
    },
    utils::{Colour, MessageBuilder},
};
use std::{fs, fs::File, io::Write};

pub async fn dead_chat(
    ctx: &serenity::Context,
    channel_id: ChannelId,
) -> Result<(), serenity::Error> {
    let dead_gifs = [
        "https://media.tenor.com/PFyQ24Kux9UAAAAC/googas-wet.gif",
        "https://media.tenor.com/71DeLT3bO0AAAAAM/dead-chat-dead-chat-skeleton.gif",
        "https://media.tenor.com/yjAObClgNM4AAAAM/dead-chat-xd-dead-chat.gif",
        "https://media.tenor.com/dpXmFPj7PacAAAAM/dead-chat.gif",
        "https://media.tenor.com/XyZ3A8FKZpkAAAAM/dead-group-chat-dead-chat.gif",
        "https://media.tenor.com/bAfYpkySsqQAAAAd/rip-chat-chat-dead.gif",
        "https://media.tenor.com/ogIdtDgmJuUAAAAC/dead-chat-dead-chat-xd.gif",
        "https://media.tenor.com/NPVLum9UiXYAAAAM/cringe-dead-chat.gif",
        "https://media.tenor.com/AYJL7HPOy-EAAAAd/ayo-the-chat-is-dead.gif",
        "https://media.tenor.com/2u621yp8wg0AAAAC/dead-chat-xd-mugman.gif",
        "https://media.tenor.com/3VXXC59D2BYAAAAC/omori-dead-chat.gif",
        "https://media.tenor.com/FqJ2W5diczAAAAAd/dead-chat.gif",
        "https://media.tenor.com/KFZQqKXcujIAAAAd/minecraft-dead-chat.gif",
        "https://media.tenor.com/qQeE7sMPIRMAAAAC/dead-chat-xd-ded-chat.gif",
        "https://media.tenor.com/cX9CCITVZNQAAAAd/hello-goodbye.gif",
        "https://media.tenor.com/eW0bnOiDjSAAAAAC/deadchatxdrickroll.gif",
        "https://media.tenor.com/1wCIRabmVUUAAAAd/chat-ded.gif",
        "https://media.tenor.com/N502JNoV_poAAAAd/dead-chat-dead-chat-xd.gif",
    ];
    channel_id
        .say(&ctx.http, dead_gifs[random_number(dead_gifs.len())])
        .await?;
    Ok(())
}

pub async fn embed_builder<S: ToString>(
    ctx: &serenity::Context,
    message: &serenity::Message,
    title: S,
    url: S,
    colour: Colour,
) {
    let _ = message
        .channel_id
        .send_message(&ctx.http, |e| {
            e.embed(|b| {
                b.title(title)
                    .image(url)
                    .color(colour)
                    .timestamp(Timestamp::now())
            })
        })
        .await;
}

pub fn emoji_react(emoji: &str) -> ReactionType {
    let id = match emoji {
        "fabseman_willbeatu" => 1135252520785150012,
        _ => 1135252520785150012,
    };
    ReactionType::Custom {
        animated: false,
        id: EmojiId(id),
        name: Some(emoji.to_string()),
    }
}

pub fn random_number(count: usize) -> usize {
    let mut rng = rand::thread_rng();
    if count == 0 {
        rng.gen_range(0..1000)
    } else {
        rng.gen_range(0..count)
    }
}

pub async fn spoiler_message(ctx: &serenity::Context, message: &serenity::Message, text: String) {
    let avatar_url = message.author.avatar_url().unwrap_or_default().to_string();
    let username = &message.author_nick(&ctx.http).await.unwrap_or_default();
    let mut index = 0;
    for attachment in &message.attachments {
        let target = &attachment.url;
        let response = reqwest::get(target).await;
        let download = response.expect("couldn't download file").bytes().await;
        let filename = format!("SPOILER_{}", &attachment.filename);
        let file = File::create(filename.clone());
        let download_bytes = match download {
            Ok(bytes) => bytes,
            Err(_e) => {
                continue;
            }
        };
        let _ = file.expect("file not found").write_all(&download_bytes);
        if index == 0 {
            webhook_file(
                ctx,
                message,
                username,
                &avatar_url,
                &text,
                filename.to_string(),
                0,
            )
            .await;
            index = 1;
        } else {
            webhook_file(
                ctx,
                message,
                username,
                &avatar_url,
                &text,
                filename.to_string(),
                1,
            )
            .await;
        }
        let _ = fs::remove_file(filename);
    }
    let _ = message.delete(&ctx).await;
}

pub async fn webhook_message<S: ToString>(
    ctx: &serenity::Context,
    message: &serenity::Message,
    name: S,
    url: &str,
    output: S,
) {
    let channel_id = message.channel_id;
    let existing_webhooks = match channel_id.webhooks(&ctx.http).await {
        Ok(webhooks) => webhooks,
        Err(err) => {
            eprintln!("Error retrieving webhooks: {:?}", err);
            return;
        }
    };
    if existing_webhooks.len() >= 15 {
        for webhook in &existing_webhooks {
            let _ = (ctx.http).delete_webhook(webhook.id.into()).await;
        }
    }
    if let Some(existing_webhook) = existing_webhooks
        .iter()
        .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
    {
        let _ = existing_webhook
            .execute(&ctx.http, false, |w| {
                w.username(name).avatar_url(url).content(output)
            })
            .await;
    } else {
        let new_webhook = message
            .channel_id
            .create_webhook_with_avatar(ctx, "fabsemanbots", url)
            .await;
        let _ = new_webhook
            .expect("rip webhooks")
            .execute(&ctx.http, false, |w| {
                w.username(name).avatar_url(url).content(output)
            })
            .await;
    }
}

pub async fn webhook_file<S: ToString>(
    ctx: &serenity::Context,
    message: &serenity::Message,
    name: S,
    url: &str,
    text: S,
    path: String,
    mode: i32,
) {
    let channel_id = message.channel_id;
    let existing_webhooks = match channel_id.webhooks(&ctx.http).await {
        Ok(webhooks) => webhooks,
        Err(err) => {
            eprintln!("Error retrieving webhooks: {:?}", err);
            return;
        }
    };
    if existing_webhooks.len() >= 15 {
        for webhook in &existing_webhooks {
            let _ = (ctx.http).delete_webhook(webhook.id.into()).await;
        }
    }
    if mode == 0 {
        if let Some(existing_webhook) = existing_webhooks
            .iter()
            .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
        {
            let _ = existing_webhook
                .execute(&ctx.http, false, |w| {
                    w.username(name)
                        .avatar_url(url)
                        .content(text)
                        .add_file(AttachmentType::Path(path.as_ref()))
                })
                .await;
        } else {
            let new_webhook = message
                .channel_id
                .create_webhook_with_avatar(ctx, "fabsemanbots", url)
                .await;
            let _ = new_webhook
                .expect("rip webhooks")
                .execute(&ctx.http, false, |w| {
                    w.username(name)
                        .avatar_url(url)
                        .content(text)
                        .add_file(AttachmentType::Path(path.as_ref()))
                })
                .await;
        }
    } else if let Some(existing_webhook) = existing_webhooks
        .iter()
        .find(|webhook| webhook.name.as_deref() == Some("fabsemanbots"))
    {
        let _ = existing_webhook
            .execute(&ctx.http, false, |w| {
                w.username(name)
                    .avatar_url(url)
                    .add_file(AttachmentType::Path(path.as_ref()))
            })
            .await;
    } else {
        let new_webhook = message
            .channel_id
            .create_webhook_with_avatar(ctx, "fabsemanbots", url)
            .await;
        let _ = new_webhook
            .expect("rip webhooks")
            .execute(&ctx.http, false, |w| {
                w.username(name)
                    .avatar_url(url)
                    .add_file(AttachmentType::Path(path.as_ref()))
            })
            .await;
    }
}
