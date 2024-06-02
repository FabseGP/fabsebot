use crate::types::{Data, Error};
use crate::utils::{
    dead_chat, embed_builder, emoji_react, random_number, spoiler_message, webhook_message,
};
use poise::serenity_prelude::{self as serenity, Colour, CreateEmbed, CreateMessage, FullEvent};
use serenity::{
    gateway::ActivityData,
    model::{prelude::ChannelId, user::OnlineStatus},
};
use sqlx::Row;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

lazy_static::lazy_static! {
    static ref LAST_MESSAGE_TIMESTAMP: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
}

pub async fn event_handler(
    ctx: &serenity::Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        FullEvent::Ready { data_about_bot } => {
            println!("Logged in as {}", data_about_bot.user.name);
            let activity = ActivityData::listening("YMCA");
            let status = OnlineStatus::Online;
            ctx.set_presence(Some(activity), status);
        }
        FullEvent::Message { new_message } => {
            let content = new_message.content.to_lowercase();
            if new_message.author.bot {
            } else if (new_message.channel_id == 1103729117184139325)
                && (new_message.author.id == 1014524859532980255
                    || new_message.author.id == 538731291970109471)
                && !new_message.attachments.is_empty()
            {
                spoiler_message(ctx, new_message, &new_message.content).await;
            } else if content.contains(&ctx.cache.current_user().to_string()) {
                embed_builder(
                    ctx,
                    new_message,
                    "why ping me bitch, go get a life!",
                    "https://media.tenor.com/HNshDeQoEKsAAAAd/psyduck-hit-smash.gif",
                    Colour(0x00b0f4),
                )
                .await;
            } else if new_message.content.contains("<@1014524859532980255>")
                && !content.contains("!user")
            {
                let fabse_life_gifs = [
                    "https://media1.tenor.com/m/hcjOU7y8RgMAAAAd/pokemon-psyduck.gif",
                    "https://media1.tenor.com/m/z0ZTwNfJJDAAAAAC/psyduck-psyduck-x.gif",
                    "https://media1.tenor.com/m/7lgxLiGtCX4AAAAC/psyduck-psyduck-x.gif",
                    "https://media1.tenor.com/m/yhO7PxBKUVoAAAAC/pokemon-hole.gif",
                    "https://media1.tenor.com/m/t--85A1qznIAAAAd/pupuce-cat.gif",
                ];
                embed_builder(
                    ctx,
                    new_message,
                    "fabseman is out to open source life",
                    fabse_life_gifs[random_number(fabse_life_gifs.len())],
                    Colour(0xf8e45c),
                )
                .await;
                //embed_builder(&ctx, new_message, "one fabseman coming up", "https://media.tenor.com/rdkYJPdWkyAAAAAC/psychokwak-psyduck.gif", Colour(0xf8e45c)).await;
            } else if (new_message.content.contains("<@409113157550997515>")
                || content == "nito"
                || content == "denito")
                && !content.contains("!user")
            {
                embed_builder(
                    ctx,
                    new_message,
                    "haiiii ^_^ hi!! hiiiii<3 haii :3 meow",
                    "https://i.postimg.cc/xC0pBhR1/gifntext-gif.gif",
                    Colour(0x00b0f4),
                )
                .await;
            } else if (new_message.content.contains("<@1110757956775051294>")
                || content == "kato"
                || content == "kachooow"
                || content == "kachoow")
                && !content.contains("!user")
            {
                embed_builder(
                    ctx,
                    new_message,
                    "kachooow",
                    "https://i.postimg.cc/m2YSQ8RL/022106-tofushop.gif",
                    Colour(0x00b0f4),
                )
                .await;
            } else if (new_message.content.contains("<@701838215757299772>")
                || content == "harsh g")
                && !content.contains("!user")
            {
                embed_builder(
                    ctx,
                    new_message,
                    "don't be harsh on me",
                    "https://media1.tenor.com/m/JYSs-svHAaMAAAAC/sunglasses-men-in-black.gif",
                    Colour(0x00b0f4),
                )
                .await;
            } else if (new_message.content.contains("<@749949941975089213>") || content == "bread")
                && !content.contains("!user")
            {
                embed_builder(
                            ctx,
                            new_message,
                            "not expired",
                            "https://media1.tenor.com/m/wmmJSYZqcPIAAAAC/lets-get-this-bread-praise-the-loaf.gif",
                            Colour(0x00b0f4),
                        )
                        .await;
            } else if (new_message.content.contains("<@287809220210851851>")
                || content == "ant1hero"
                || content == "antihero")
                && !content.contains("!user")
            {
                embed_builder(
                    ctx,
                    new_message,
                    "It's me, hi",
                    "https://i.postimg.cc/25Lhr6KQ/ezgif-1-c18da48d4b.gif",
                    Colour(0x00b0f4),
                )
                .await;
            } else if content == "sensei is here" {
                embed_builder(
                    ctx,
                    new_message,
                    "shrugging",
                    "https://media.tenor.com/rEgYW314NQ0AAAAi/shruggers-shrug.gif",
                    Colour(0x00b0f4),
                )
                .await;
            } else if content.contains("fabseman_willbeatu")
                || content.contains(":fabseman_willbeatu:")
                || content.contains("fabse")
            {
                new_message
                    .react(&ctx.http, emoji_react("fabseman_willbeatu"))
                    .await?;
            } else if content.contains("kurukuru_seseren") {
                let count = new_message.content.matches("kurukuru_seseren").count();
                let response = "<a:kurukuru_seseren:1153742599220375634>".repeat(count);
                webhook_message(
                    ctx,
                    new_message,
                    "vilbot",
                    "https://i.postimg.cc/44t5vzWB/IMG-0014.png",
                    &response,
                )
                .await;
            }
            match content.as_str() {
                "fabse" | "fabseman" => {
                    webhook_message(ctx, new_message, "yotsuba", "https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png", "# such magnificence").await;
                    new_message
                        .react(&ctx.http, emoji_react("fabseman_willbeatu"))
                        .await?;
                }
                "riny" => {
                    new_message
                        .channel_id
                        .say(&ctx.http, "we hate rin-rin")
                        .await?;
                    webhook_message(ctx, new_message, "yotsuba", "https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png", "fr, useless rice cooker").await;
                }
                "rin_willbeatu" | "<@1014524859532980255>" => {
                    new_message
                        .react(&ctx.http, emoji_react("fabseman_willbeatu"))
                        .await?;
                }
                "rinynm" | "rinymn" => {
                    webhook_message(ctx, new_message, "yotsuba", "https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png", "she should be banned fr <:wicked:1174093566017028116>").await;
                }
                "star platinum" => {
                    webhook_message(ctx, new_message, "yotsuba", "https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png", "ZAA WARUDOOOOO").await;
                }
                "xsensei" => {
                    webhook_message(ctx, new_message, "yotsuba", "https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png", "we hate sensei").await;
                }
                _ => {}
            }
            let settings_row = sqlx::query(
                "SELECT dead_chat_rate, dead_chat_channel FROM guild_settings WHERE guild_id = ?",
            )
            .bind(new_message.guild_id.map(|id| id.to_string()))
            .fetch_optional(&data.db)
            .await?;
            let (dead_chat_rate, dead_chat_channel) = match settings_row {
                Some(row) => (row.get("dead_chat_rate"), row.get("dead_chat_channel")),
                None => (60, 1069629692937764937),
            };
            if new_message.channel_id == ChannelId::new(1069629692937764937) {
                let mut timestamp_lock = LAST_MESSAGE_TIMESTAMP.lock().await;
                *timestamp_lock = Some(Instant::now());
            }
            let last_timestamp = *LAST_MESSAGE_TIMESTAMP.lock().await;
            if let Some(last_timestamp) = last_timestamp {
                let elapsed_time = Instant::now().duration_since(last_timestamp);
                if elapsed_time >= Duration::from_secs(dead_chat_rate * 60) {
                    let new_last_timestamp = last_timestamp + Duration::from_secs(dead_chat_rate);
                    let mut timestamp_lock = LAST_MESSAGE_TIMESTAMP.lock().await;
                    *timestamp_lock = Some(new_last_timestamp);
                    let channel_id = ChannelId::new(dead_chat_channel);
                    dead_chat(ctx, channel_id).await?;
                }
            }
        }
        FullEvent::GuildScheduledEventCreate { event } => {
            let channel = ChannelId::new(1069629692937764937);
            let _ = channel
                .send_message(
                    &ctx.http,
                    CreateMessage::default().embed(
                        CreateEmbed::new()
                            .title(&event.name)
                            .field("Creator", event.creator_id.unwrap().to_string(), false)
                            .field("Start time: ", event.start_time.to_string(), true)
                            .field("Channel id: ", event.channel_id.unwrap().to_string(), true)
                            .field("Description: ", event.description.as_ref().unwrap(), false),
                    ),
                )
                .await?;
        }
        _ => {}
    }
    Ok(())
}
