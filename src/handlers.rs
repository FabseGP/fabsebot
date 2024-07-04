use crate::types::{Data, Error};
use crate::utils::{embed_builder, emoji_id, random_number, spoiler_message, webhook_message};
use poise::serenity_prelude::{self as serenity, Colour, CreateAttachment, FullEvent};
use serenity::{
    builder::{CreateMessage, EditProfile},
    gateway::ActivityData,
    model::{channel::ReactionType, user::OnlineStatus},
};

pub async fn event_handler(
    ctx: &serenity::Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    _data: &Data,
) -> Result<(), Error> {
    match event {
        FullEvent::Ready { data_about_bot } => {
            println!("Logged in as {}", data_about_bot.user.name);
            let activity = ActivityData::listening("You Could Be Mine");
            let avatar = CreateAttachment::url(
                &ctx.http,
                "https://media1.tenor.com/m/029KypcoTxQAAAAC/sleep-pokemon.gif",
            )
            .await?;
            let banner =
                CreateAttachment::url(&ctx.http, "https://external-content.duckduckgo.com/iu/?u=https%3A%2F%2Fs1.zerochan.net%2FFAIRY.TAIL.600.1870606.jpg&f=1&nofb=1&ipt=1a9ade7d1a4d0a2f783a15018c53faa63a7c38bc72a288d4df37e11e7f3d0e4d&ipo=images")
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
            if !new_message.author.bot {
                let content = new_message.content.to_lowercase();
                let mut conn = _data.db.acquire().await?;
                let id: u64 = new_message.guild_id.unwrap().into();
                sqlx::query(
                    "INSERT INTO message_count (guild_id, user_name, messages) VALUES (?, ?, 1)
                ON DUPLICATE KEY UPDATE messages = messages + 1"
                )
                .bind(id)
                .bind(&new_message.author.name)
                .execute(&mut *conn)
                .await
                .unwrap();
                if new_message.author.id == 1014524859532980255 &&  content == "pgo-end" {
                    std::process::exit(0);
                }
                if new_message.channel_id == 1146385698279137331 {
                    spoiler_message(ctx, new_message, &new_message.content).await;
                } 
                 else if content.contains(&ctx.cache.current_user().to_string()) {
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
                        .await?;
                } else if (content.contains("<@409113157550997515>")
                    || content == "nito"
                    || content == "denito")
                    && !content.contains("!user")
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
                } else if (content.contains("<@1110757956775051294>")
                    || content == "kato"
                    || content == "kachooow"
                    || content == "kachoow")
                    && !content.contains("!user")
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
                } else if (content.contains("<@701838215757299772>") || content == "harsh g")
                    && !content.contains("!user")
                {
                    new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder( 
                        "don't be harsh on me",
                        "https://media1.tenor.com/m/JYSs-svHAaMAAAAC/sunglasses-men-in-black.gif",
                        Colour(0x00b0f4),
                    )))
                    .await?;
                } else if (content.contains("<@749949941975089213>") || content == "bread")
                    && !content.contains("!user")
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
                } else if (content.contains("<@287809220210851851>")
                    || content == "ant1hero"
                    || content == "antihero")
                    && !content.contains("!user")
                { new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                        "It's me, hi",
                        "https://media1.tenor.com/m/9298nZYrUfcAAAAC/hi.gif",
                        Colour(0x00b0f4),
                    )))
                    .await?;
                } else if content == "sensei is here" { new_message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            CreateMessage::default().embed(embed_builder(
                        "shrugging",
                        "https://media.tenor.com/rEgYW314NQ0AAAAi/shruggers-shrug.gif",
                        Colour(0x00b0f4),
                    )))
                    .await?;
                } else if content.contains("fabseman_willbeatu") {
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
                match content.as_str() {
                    "fabse" | "fabseman" => {
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
                                    emoji_id(ctx, new_message.guild_id.unwrap(), "fabseman_willbeatu").await,
                                )
                                .unwrap(),
                            )
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
                            .react(
                                &ctx.http,
                                ReactionType::try_from(
                                    emoji_id(ctx, new_message.guild_id.unwrap(), "fabseman_willbeatu").await,
                                )
                                .unwrap(),
                            )
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
            }
        }
        _ => {}
    }
    Ok(())
}
