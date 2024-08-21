use crate::types::{Data, Error};
use crate::utils::{
    embed_builder, emoji_id, get_waifu, random_number, spoiler_message, webhook_message,
};
use poise::serenity_prelude::{self as serenity, Colour, CreateAttachment, FullEvent};
use regex::Regex;
use serenity::{
    builder::{CreateMessage, EditProfile},
    gateway::ActivityData,
    model::{channel::ReactionType, id::ChannelId, user::OnlineStatus},
};
use sqlx::query;

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
                let id: u64 = new_message.guild_id.unwrap().into();
                query!(
                    "INSERT INTO message_count (guild_id, user_name, messages) VALUES (?, ?, 1)
                    ON DUPLICATE KEY UPDATE messages = messages + 1",
                    id,
                    new_message.author.name.to_string()
                )
                .execute(&mut *data.db.acquire().await?)
                .await
                .unwrap();
                if let Ok(record) = query!(
                    "SELECT spoiler_channel FROM guild_settings WHERE guild_id = ?",
                    id
                )
                .fetch_one(&mut *data.db.acquire().await?)
                .await
                {
                    let spoiler_channel = ChannelId::new(record.spoiler_channel);
                    if new_message.channel_id == spoiler_channel {
                        spoiler_message(ctx, new_message, &new_message.content).await;
                    }
                }
                if content.contains("nigga") {
                    if new_message.author.id == 538731291970109471 {
                        let re = Regex::new(r"(?i)nigg?a").unwrap();
                        let new_content =
                            re.replace_all(new_message.content.as_str(), "beautiful person");
                        webhook_message(
                            ctx,
                            new_message,
                            new_message
                                .author_nick(&ctx.http)
                                .await
                                .unwrap_or(new_message.author.name.to_string())
                                .as_str(),
                            new_message.author.avatar_url().unwrap().as_str(),
                            &new_content,
                        )
                        .await;
                        new_message.delete(&ctx.http, Some("pure")).await?;
                    }
                    query!(
                        "INSERT INTO words_count (word, guild_id, count) VALUES (?, ?, 1)
                        ON DUPLICATE KEY UPDATE count = count + 1",
                        "nigga",
                        id
                    )
                    .execute(&mut *data.db.acquire().await?)
                    .await
                    .unwrap();
                }
                if content.contains(&ctx.cache.current_user().to_string()) {
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
                    new_message
                        .react(
                            &ctx.http,
                            ReactionType::try_from(
                                emoji_id(ctx, new_message.guild_id.unwrap(), "fabseman_willbeatu")
                                    .await,
                            )
                            .unwrap(),
                        )
                        .await?;
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
                    new_message
                        .react(
                            &ctx.http,
                            ReactionType::try_from(
                                emoji_id(ctx, new_message.guild_id.unwrap(), "fabseman_willbeatu")
                                    .await
                                    .as_str(),
                            )
                            .unwrap(),
                        )
                        .await?;
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
            if ctx
                .cache
                .message(*channel_id, *deleted_message_id)
                .unwrap()
                .author
                .id
                == 1146382254927523861
            {
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
                            .permissions
                            .unwrap()
                            .administrator();
                        if evil_person.id != ctx.http.get_guild(guild_id).await.unwrap().owner_id
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
                                        CreateMessage::default().content(deleted_content.content),
                                    )
                                    .await?;
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
