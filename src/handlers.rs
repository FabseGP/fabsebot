use crate::types::{ChatMessage, Data, Error};
use crate::utils::{
    ai_response, ai_response_local, embed_builder, emoji_id, get_waifu, random_number, spoiler_message,
    webhook_message,
};

use poise::serenity_prelude::{self as serenity, Colour, CreateAttachment, FullEvent};
use serenity::{
    builder::{CreateMessage, EditProfile},
    futures::StreamExt,
    gateway::ActivityData,
    model::{channel::ReactionType, id::ChannelId, user::OnlineStatus, Timestamp},
};
use sqlx::query;
use std::collections::HashMap;

pub async fn event_handler(
    framework: poise::FrameworkContext<'_, Data, Error>,
    event: &FullEvent,
) -> Result<(), Error> {
    let data = framework.user_data();
    let ctx = framework.serenity_context;
    match event {
        FullEvent::Ready { data_about_bot } => {
            println!("Logged in as {}", data_about_bot.user.name);
            let activity = ActivityData::listening("You Could Be Mine");
            let avatar = CreateAttachment::url(
                &ctx.http,
                "https://media1.tenor.com/m/029KypcoTxQAAAAC/sleep-pokemon.gif",
                "psyduck_avatar.gif",
            )
            .await?;
            let banner = CreateAttachment::url(&ctx.http, "https://i.postimg.cc/RFWkBJfs/2024-08-2012-50-17online-video-cutter-com-ezgif-com-optimize.gif", "fabsebot_banner.gif")
                .await?;

            ctx.set_presence(Some(activity), OnlineStatus::Online);
            ctx.http
                .edit_profile(
                    &EditProfile::new()
                        .avatar(&avatar)
                        .banner(&banner)
                        .username("fabsebot"),
                )
                .await?;
        }
        FullEvent::Message { new_message } => {
            if !new_message.author.bot() {
                let content = new_message.content.to_lowercase();
                let guild_id: u64 = new_message.guild_id.unwrap().into();
                query!("INSERT IGNORE INTO guilds (guild_id) VALUES (?)", guild_id,)
                    .execute(&mut *data.db.acquire().await?)
                    .await?;
                query!(
                    "INSERT INTO message_count (guild_id, user_name, messages) VALUES (?, ?, 1)
                    ON DUPLICATE KEY UPDATE messages = messages + 1",
                    guild_id,
                    new_message.author.name.to_string()
                )
                .execute(&mut *data.db.acquire().await?)
                .await
                .unwrap();
                if let Ok(record) = query!(
                    "SELECT spoiler_channel FROM guild_settings WHERE guild_id = ?",
                    guild_id
                )
                .fetch_one(&mut *data.db.acquire().await?)
                .await
                {
                    if let Some(channel) = record.spoiler_channel {
                        let spoiler_channel = ChannelId::new(channel);
                        if new_message.channel_id == spoiler_channel {
                            spoiler_message(ctx, new_message, &new_message.content).await;
                        }
                    }
                }
                if let Ok(record) = query!(
                    "SELECT dead_chat_channel, dead_chat_rate FROM guild_settings WHERE guild_id = ?",
                    guild_id
                )
                .fetch_one(&mut *data.db.acquire().await?)
                .await
                {
                    if let Some(channel) = record.dead_chat_channel {
                        let dead_chat_channel = ChannelId::new(channel);
                        let last_message_time = dead_chat_channel
                            .messages_iter(&ctx)
                            .boxed()
                            .next()
                            .await
                            .unwrap()
                            .unwrap()
                            .timestamp.timestamp();
                        let current_time = Timestamp::now().timestamp();
                        if current_time - last_message_time > record.dead_chat_rate.unwrap() as i64 * 60 {    
                            let dead_chat_gifs = [
                                "https://media1.tenor.com/m/k6k3vCBIYlYAAAAC/dead-chat.gif",
                                "https://media1.tenor.com/m/t_DmbWvjTKMAAAAd/dead-chat-discord.gif",
                                "https://media1.tenor.com/m/8JHVRggIIl4AAAAd/hello-chat-dead-chat.gif",
                                "https://media1.tenor.com/m/BDJsAenz_SUAAAAd/chat-dead-chat.gif",
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
                            dead_chat_channel.say(&ctx.http, dead_chat_gifs[random_number(dead_chat_gifs.len())]).await?;
                        }
                    }
                }
                let words_to_count: Vec<String> =
                    query!("SELECT word FROM words_count WHERE guild_id = ?", guild_id)
                        .fetch_all(&mut *data.db.acquire().await?)
                        .await
                        .unwrap()
                        .iter()
                        .map(|row| row.word.clone())
                        .collect();
                for word in words_to_count.iter() {
                    if content.contains(word) {
                        query!(
                            "INSERT INTO words_count (word, guild_id, count) VALUES (?, ?, 1)
                            ON DUPLICATE KEY UPDATE count = count + 1",
                            word,
                            guild_id
                        )
                        .execute(&mut *data.db.acquire().await?)
                        .await
                        .unwrap();
                    }
                }
                if let Some(topic) = new_message.guild_channel(&ctx.http).await.unwrap().topic {
                    if topic.contains("ai-chat") {
                        let typing = new_message.channel_id.start_typing(ctx.http.clone());
                        let mut conversations = data.conversations.lock().await;
                        let guild_conversations =
                            conversations.entry(guild_id).or_insert_with(HashMap::new);
                        if content == "clear" {
                            guild_conversations.remove(&new_message.channel_id.into());
                            new_message
                                .reply(&ctx.http, "Conversation cleared!")
                                .await?;
                        } else {
                            let history = guild_conversations
                                .entry(new_message.channel_id.into())
                                .or_insert_with(Vec::new);
                            history.push(ChatMessage {
                                role: "user".to_string(),
                                content: new_message.content.to_string(),
                            });
                            match ai_response(history.clone()).await {
                                Ok(response) => {
                                    history.push(ChatMessage {
                                        role: "assistant".to_string(),
                                        content: response.clone(),
                                    });
                                    new_message.channel_id.say(&ctx.http, response).await?;
                                },
                                Err(_) => {
                                    let error_msg = "Sorry, I had to forget our convo, too boring!";
                                    new_message.channel_id.say(&ctx.http, error_msg).await?;
                                    history.clear();
                                    history.push(ChatMessage {
                                        role: "assistant".to_string(),
                                        content: error_msg.to_string(),
                                    });
                                }
                            }
                        }
                        typing.stop();
                    }
                }
                if new_message.mentions_me(&ctx.http).await.unwrap() {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "why ping me bitch, go get a life!",
                                "https://media.tenor.com/HNshDeQoEKsAAAAd/psyduck-hit-smash.gif",
                                Colour(0x00b0f4),
                            )),
                        )
                        .await?;
                } else if content.contains("<@1014524859532980255>") && !content.contains("!user") {
                    let fabse_life_gifs = [
                        "https://media1.tenor.com/m/hcjOU7y8RgMAAAAd/pokemon-psyduck.gif",
                        "https://media1.tenor.com/m/z0ZTwNfJJDAAAAAC/psyduck-psyduck-x.gif",
                        "https://media1.tenor.com/m/7lgxLiGtCX4AAAAC/psyduck-psyduck-x.gif",
                        "https://media1.tenor.com/m/yhO7PxBKUVoAAAAC/pokemon-hole.gif",
                        "https://media1.tenor.com/m/t--85A1qznIAAAAd/pupuce-cat.gif",
                        "https://media1.tenor.com/m/rdkYJPdWkyAAAAAC/psychokwak-psyduck.gif",
                        "https://media1.tenor.com/m/w5m9Sh-s4igAAAAC/psychokwak-psyduck.gif",
                    ];
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "fabseman is out to open source life",
                                fabse_life_gifs[random_number(fabse_life_gifs.len())],
                                Colour(0xf8e45c),
                            )),
                        )
                        .await?; /*
                                 let fabse_travel_gifs = [
                                     "https://media1.tenor.com/m/-OS17IIpcL0AAAAC/psyduck-pokemon.gif"
                                 ];
                                 new_message
                                     .channel_id
                                     .send_message(
                                         &ctx.http,
                                         CreateMessage::default().embed(embed_builder(
                                             "fabseman is out to buy a volcano in iceland",
                                             fabse_travel_gifs[random_number(fabse_travel_gifs.len())],
                                             Colour(0xf8e45c),
                                         )),
                                     )
                                     .await?; */
                } else if content.contains("<@409113157550997515>")
                    && !content.contains("!user_misuse")
                {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "haiiii ^_^ hi!! hiiiii<3 haii :3 meow",
                                "https://i.imgur.com/lJV82uz.gif",
                                Colour(0x00b0f4),
                            )),
                        )
                        .await?;
                } else if content.contains("<@999604056072929321>")
                    && !content.contains("!user_misuse")
                {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "Glare this cute waifu while handsome ayaan_luffy replies your message",
                                &get_waifu().await,
                                Colour(0x00b0f4),
                            )),
                        )
                        .await?;
                } else if content.contains("<@1110757956775051294>")
                    && !content.contains("!user_misuse")
                {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "kachooow",
                                "https://media1.tenor.com/m/gL0ZoZuJdAkAAAAd/omgtakumi-ae86comeon.gif",
                                Colour(0x00b0f4),
                            )),
                        )
                        .await?;
                } else if content.contains("<@701838215757299772>")
                    && !content.contains("!user_misuse")
                {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder("don't be harsh on me",
                                "https://media1.tenor.com/m/JYSs-svHAaMAAAAC/sunglasses-men-in-black.gif",
                                Colour(0x00b0f4),
                            ))
                        )
                        .await?;
                } else if content.contains("<@749949941975089213>")
                    && !content.contains("!user_misuse")
                {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                            "not expired",
                            "https://media1.tenor.com/m/wmmJSYZqcPIAAAAC/lets-get-this-bread-praise-the-loaf.gif",
                            Colour(0x00b0f4),
                        )))
                        .await?;
                } else if content.contains("<@287809220210851851>")
                    && !content.contains("!user_misuse")
                {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "It's me, hi",
                                "https://media1.tenor.com/m/9298nZYrUfcAAAAC/hi.gif",
                                Colour(0x00b0f4),
                            )),
                        )
                        .await?;
                } else if content == "fabse" || content == "fabseman" {
                    webhook_message(
                        ctx,
                        new_message,
                        "yotsuba",
                        "https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png",
                        "# such magnificence",
                    )
                    .await;
                    let emoji =  emoji_id(ctx, new_message.guild_id.unwrap(), "fabseman_willbeatu")
                        .await;
                    if let Ok(emoji) = emoji {
                        new_message
                            .react(
                                &ctx.http,
                                ReactionType::try_from(emoji).unwrap()
                            )
                            .await?;
                    }
                } else if content == "star_platinum" {
                    webhook_message(ctx, new_message, "yotsuba", "https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png", "ZAA WARUDOOOOO").await;
                } else if content == "floppaganda" {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().content("https://i.imgur.com/Pys97pb.png"),
                        )
                        .await?;
                } else if content == "floppa" {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default()
                                .content("https://libreddit.bus-hit.me/img/3bpsrhciju091.jpg"),
                        )
                        .await?;
                } else if content.contains("furina") {
                    let furina_gifs = [
                        "https://media1.tenor.com/m/-DdP7PTL6r8AAAAC/furina-focalors.gif",
                        "https://media1.tenor.com/m/gARaejr6ODIAAAAd/furina-focalors.gif",
                        "https://media1.tenor.com/m/_H_syqWiknsAAAAd/focalors-genshin-impact.gif",
                    ];
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "your queen has arrived",
                                furina_gifs[random_number(furina_gifs.len())],
                                Colour(0xf8e45c),
                            )),
                        )
                        .await?;
                } else if content.contains("kafka") {
                    let kafka_gifs = [
                        "https://media1.tenor.com/m/Hse9P_W_A3UAAAAC/kafka-hsr-live-reaction-kafka.gif",
                        "https://media1.tenor.com/m/Z-qCHXJsDwoAAAAC/kafka.gif",
                        "https://media1.tenor.com/m/6RXMiM9te7AAAAAC/kafka-honkai-star-rail.gif",
                        "https://media1.tenor.com/m/QDXaFgSJMAcAAAAd/kafka-kafka-honkai.gif",
                        "https://media1.tenor.com/m/zDDaAU3TX38AAAAC/kafka-honkai.gif",
                        "https://media1.tenor.com/m/dy9TUjKaq4MAAAAC/kafka-honkai-star-rail.gif",
                        "https://media1.tenor.com/m/Fsyz6klrIqUAAAAd/kafka-honkai-star-rail.gif",
                        "https://media1.tenor.com/m/aDWOgEh1GycAAAAd/kafka-honkai.gif",
                        "https://media1.tenor.com/m/C1Y9XD8U7XMAAAAC/kafka-hsr.gif",
                        "https://media1.tenor.com/m/_RiBHVVH-wIAAAAC/kafka-kafka-pat.gif",
                    ];
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "your queen has arrived",
                                kafka_gifs[random_number(kafka_gifs.len())],
                                Colour(0xf8e45c),
                            )),
                        )
                        .await?;
                } else if content.contains("kinich") {
                    let kinich_gifs = [
                        "https://media1.tenor.com/m/GAA5_YmbClkAAAAC/natlan-dendro-boy.gif",
                        "https://media1.tenor.com/m/qcdZ04vXqEIAAAAC/natlan-guy-kinich.gif",
                        "https://media1.tenor.com/m/mJC2SsAcQB8AAAAd/dendro-natlan.gif",
                    ];
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                                "pls destroy lily's oven",
                                kinich_gifs[random_number(kinich_gifs.len())],
                                Colour(0xf8e45c),
                            )),
                        )
                        .await?;
                } else if content.contains("fabse") {
                    let emoji =  emoji_id(ctx, new_message.guild_id.unwrap(), "fabseman_willbeatu")
                        .await;
                    if let Ok(emoji) = emoji {
                        new_message
                            .react(
                                &ctx.http,
                                ReactionType::try_from(emoji).unwrap()
                            )
                            .await?;
                    }
                } else if content.contains("kurukuru_seseren") {
                    let count = content.matches("kurukuru_seseren").count();
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
            }
        }
        FullEvent::MessageDelete {
            channel_id,
            guild_id,
            deleted_message_id,
        } => {
            let message_author_id = ctx
                .cache
                .message(*channel_id, *deleted_message_id)
                .map(|msg| msg.author.id);
            if let Some(author_id) = message_author_id {
                if author_id == 1146382254927523861 {
                    let guild_id = guild_id.unwrap();
                    let guild = ctx.http.get_guild(guild_id).await.unwrap();
                    let audit = guild
                        .audit_logs(
                            &ctx.http,
                            Some(serenity::model::guild::audit_log::Action::Message(
                                serenity::model::guild::audit_log::MessageAction::Delete,
                            )),
                            None,
                            None,
                            None,
                        )
                        .await
                        .unwrap();
                    if let Some(entry) = audit.entries.first() {
                        if let Some(user_id) = entry.user_id {
                            let evil_person = ctx.http.get_user(user_id).await.unwrap();
                            let admin_perms = ctx
                                .http
                                .get_member(guild_id, user_id)
                                .await
                                .unwrap()
                                .permissions(&ctx.cache)
                                .unwrap()
                                .administrator();
                            if evil_person.id
                                != ctx.http.get_guild(guild_id).await.unwrap().owner_id
                                && !admin_perms
                            {
                                let name = evil_person
                                    .nick_in(&ctx.http, guild_id)
                                    .await
                                    .unwrap_or(evil_person.name.to_string());
                                channel_id
                                    .send_message(
                                        &ctx.http,
                                        CreateMessage::default().content(format!(
                                            "bruh, {} deleted my message, sending it again",
                                            name
                                        )),
                                    )
                                    .await?;
                                let deleted_content = ctx
                                    .cache
                                    .message(*channel_id, *deleted_message_id)
                                    .unwrap()
                                    .clone();
                                if !deleted_content.embeds.is_empty() {
                                    channel_id
                                        .send_message(
                                            &ctx.http,
                                            CreateMessage::default()
                                                .content(deleted_content.content)
                                                .embed(deleted_content.embeds[0].clone().into()),
                                        )
                                        .await?;
                                } else {
                                    channel_id
                                        .send_message(
                                            &ctx.http,
                                            CreateMessage::default()
                                                .content(deleted_content.content),
                                        )
                                        .await?;
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}
