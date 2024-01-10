use crate::types::{Data, Error};
use crate::utils::{
    dead_chat, embed_builder, emoji_react, random_number, spoiler_message, webhook_message,
};

use poise::serenity_prelude as serenity;
use poise::serenity_prelude::Colour;
use poise::Event;
use serenity::model::{
    application::interaction::{Interaction, InteractionResponseType},
    channel::{Channel, Message, ReactionType},
    gateway::Activity,
    prelude::{ChannelId, GuildId},
    user::OnlineStatus,
};
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
    event: &Event<'_>,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    _data: &Data,
) -> Result<(), Error> {
    match event {
        Event::Ready { data_about_bot } => {
            println!("Logged in as {}", data_about_bot.user.name);
            let activity = Activity::listening("I can't stop this feeling");
            let status = OnlineStatus::Online;
            ctx.set_presence(Some(activity), status).await;
        }
        Event::Message { new_message } => {
            let content = new_message.content.to_lowercase();
            if new_message.author.bot {
            } else if new_message.guild_id == Some(GuildId(1069629692216365077))
                || new_message.guild_id == Some(GuildId(1103723321683611698))
            {
                if new_message.channel_id != 1136989445992751264
                    && new_message.channel_id != 1136997211025199166
                    && new_message.channel_id != 1136989514653519924
                {
                    if (new_message.channel_id == 1103729117184139325)
                        && (new_message.author.id == 1014524859532980255
                            || new_message.author.id == 538731291970109471)
                        && !new_message.attachments.is_empty()
                    {
                        spoiler_message(ctx, new_message, new_message.content.to_string()).await;
                    } else if content.contains(&ctx.cache.current_user_id().to_string()) {
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
                        "rin_willbeatu" => {
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
                    if new_message.channel_id == ChannelId(1103728998372102154) {
                        let mut timestamp_lock = LAST_MESSAGE_TIMESTAMP.lock().await;
                        *timestamp_lock = Some(Instant::now());
                    }
                }
                let last_timestamp;
                {
                    let timestamp_lock = LAST_MESSAGE_TIMESTAMP.lock().await;
                    last_timestamp = *timestamp_lock;
                }
                if let Some(last_timestamp) = last_timestamp {
                    let current_timestamp = Instant::now();
                    let elapsed_time = current_timestamp.duration_since(last_timestamp);
                    if elapsed_time >= Duration::from_secs(3600) {
                        let new_last_timestamp = last_timestamp + Duration::from_secs(3600);
                        let mut timestamp_lock = LAST_MESSAGE_TIMESTAMP.lock().await;
                        *timestamp_lock = Some(new_last_timestamp);
                        let channel_id = ChannelId(1103728998372102154);
                        dead_chat(ctx, channel_id).await?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}
