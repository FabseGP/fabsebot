use crate::{
    types::{Error, SContext, HTTP_CLIENT},
    utils::{ai_response_simple, quote_image},
};

use dashmap::DashSet;
use image::load_from_memory;
use poise::{
    builtins,
    serenity_prelude::{
        nonmax::NonMaxU16, ButtonStyle, Channel, ChannelId, ComponentInteractionCollector,
        CreateActionRow, CreateAttachment, CreateButton, CreateEmbed, CreateInteractionResponse,
        CreateMessage, EditChannel, EditMessage, MessageId, User, UserId,
    },
    CreateReply,
};
use sqlx::{query, query_as};
use std::{borrow::Cow, path::Path, time::Duration};
use tokio::fs::remove_file;

/// When you want to find the imposter
#[poise::command(slash_command)]
pub async fn anony_poll(
    ctx: SContext<'_>,
    #[description = "Question"] title: String,
    #[description = "Comma-separated options"] options: String,
    #[description = "Duration in minutes"] duration: u64,
) -> Result<(), Error> {
    let options_list: Vec<_> = options
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let options_count = options_list.len();
    if options_count < 2 {
        ctx.say("Bruh, 1 option ain't gonna cut it for a poll")
            .await?;
        return Ok(());
    }

    let embed = CreateEmbed::default()
        .title(title.as_str())
        .color(0xFF5733)
        .fields(options_list.iter().map(|&option| (option, "0", false)));

    let action_row = CreateActionRow::Buttons(Cow::Owned(
        (0..=options_count)
            .map(|index| {
                CreateButton::new(format!("option_{index}"))
                    .style(ButtonStyle::Primary)
                    .label((index + 1).to_string())
            })
            .collect(),
    ));

    ctx.send(
        CreateReply::default()
            .embed(embed)
            .components(&[action_row]),
    )
    .await?;

    let mut vote_counts = vec![0; options_count];
    let voted_users = DashSet::new();

    let id_borrow = ctx.id();

    while let Some(interaction) =
        ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
            .timeout(Duration::from_secs(duration * 60))
            .filter(move |interaction| {
                let id = interaction.data.custom_id.as_str();
                (0..options_count).any(|index| {
                    let expected_id = format!("{index}_{id_borrow}");
                    id == expected_id
                })
            })
            .await
    {
        let user_id = interaction.user.id;
        if voted_users.insert(user_id) {
            let choice = &interaction.data.custom_id;

            interaction
                .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                .await?;

            let index = choice.split('_').next().unwrap().parse::<usize>().unwrap();
            vote_counts[index] += 1;

            let new_embed = CreateEmbed::default().title(&title).color(0xFF5733).fields(
                options_list
                    .iter()
                    .zip(vote_counts.iter())
                    .map(|(&option, &count)| (option, count.to_string(), false)),
            );

            let mut msg = interaction.message;
            msg.edit(ctx.http(), EditMessage::default().embed(new_embed))
                .await?;
        }
    }

    Ok(())
}

/// Send a birthday wish to a user
#[poise::command(prefix_command, slash_command)]
pub async fn birthday(
    ctx: SContext<'_>,
    #[description = "User to congratulate"]
    #[rest]
    user: User,
) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => {
            return Ok(());
        }
    };
    let member = guild.member(ctx.http(), user.id).await?;
    let avatar_url = member
        .avatar_url()
        .unwrap_or_else(|| user.avatar_url().unwrap());
    let name = member.display_name();
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::default()
                .title(format!("HAPPY BIRTHDAY {name}!"))
                .thumbnail(avatar_url)
                .image("https://media.tenor.com/GiCE3Iq3_TIAAAAC/pokemon-happy-birthday.gif")
                .color(0xFF5733),
        ),
    )
    .await?;
    Ok(())
}

/// Ignore this command
#[poise::command(prefix_command, owners_only)]
pub async fn end_pgo(_: SContext<'_>) -> Result<(), Error> {
    panic!("pgo-profiling ended");

    #[expect(unreachable_code)]
    Ok(())
}

/// When you need some help
#[poise::command(prefix_command, slash_command)]
pub async fn help(
    ctx: SContext<'_>,
    #[description = "Command to show help about"]
    #[autocomplete = "builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    builtins::pretty_help(
        ctx,
        command.as_deref(),
        builtins::PrettyHelpConfiguration {
            extra_text_at_bottom: "Courtesy of Fabseman Inc.",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

struct UserCount {
    user_id: i64,
    message_count: i64,
}

/// Leaderboard of lifeless ppl
#[poise::command(prefix_command, slash_command)]
pub async fn leaderboard(ctx: SContext<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => {
            return Ok(());
        }
    };
    let thumbnail = guild.banner_url().unwrap_or_else(|| {
        guild
            .icon_url()
            .unwrap_or_else(|| "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned())
    });
    let users = query_as!(
        UserCount,
        "SELECT message_count, user_id FROM user_settings WHERE guild_id = $1
        ORDER BY message_count DESC LIMIT 25",
        i64::from(guild.id)
    )
    .fetch_all(&mut *ctx.data().db.acquire().await?)
    .await?;

    let mut embed = CreateEmbed::default()
        .title(format!("Top {} users by message count", users.len()))
        .thumbnail(thumbnail)
        .color(0xFF5733);

    for (index, user) in users.iter().enumerate() {
        if let Ok(target) = ctx
            .http()
            .get_user(UserId::new(
                u64::try_from(user.user_id).expect("user id out of bounds for u64"),
            ))
            .await
        {
            let rank = index + 1;
            let user_name = target.display_name();
            embed = embed.field(
                format!("#{rank} {user_name}"),
                user.message_count.to_string(),
                false,
            );
        }
    }

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Oh it's you
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn ohitsyou(ctx: SContext<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let resp = ai_response_simple(
        "you're a tsundere",
        "generate a one-line love-hate greeting",
    )
    .await?;
    if !resp.is_empty() {
        ctx.reply(resp).await?;
    } else {
        ctx.reply("Ugh, fine. It's nice to see you again, I suppose... for now, don't get any ideas thinking this means I actually like you or anything").await?;
    }
    Ok(())
}

/// When your memory is not enough
#[poise::command(prefix_command, slash_command)]
pub async fn quote(ctx: SContext<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let msg = ctx
        .channel_id()
        .message(&ctx.http(), MessageId::new(ctx.id()))
        .await?;
    let Some(reply) = msg.referenced_message else {
        ctx.reply("Bruh, reply to a message").await?;
        return Ok(());
    };
    let message_url = reply.link();
    let content = reply.content;
    if reply.webhook_id.is_some() {
        let avatar_image = match reply.author.avatar_url() {
            Some(avatar_url) => match HTTP_CLIENT.get(&avatar_url).send().await {
                Ok(resp) => match resp.bytes().await {
                    Ok(avatar_bytes) => match load_from_memory(&avatar_bytes) {
                        Ok(mem_bytes) => mem_bytes.to_rgba8(),
                        Err(_) => {
                            return Ok(());
                        }
                    },
                    Err(_) => {
                        return Ok(());
                    }
                },
                Err(_) => return Ok(()),
            },
            None => {
                return Ok(());
            }
        };
        let name = reply.author.display_name();
        quote_image(&avatar_image, name, &content)
            .await
            .save("quote.webp")
            .unwrap();
    } else {
        let guild = match ctx.guild() {
            Some(guild) => guild.clone(),
            None => {
                return Ok(());
            }
        };
        let member = guild.member(ctx.http(), reply.author.id).await?;
        let avatar_image = {
            let avatar_url = match member.avatar_url() {
                Some(url) => url,
                None => match reply.author.avatar_url() {
                    Some(url) => url,
                    None => {
                        return Ok(());
                    }
                },
            };
            match HTTP_CLIENT.get(&avatar_url).send().await {
                Ok(resp) => match resp.bytes().await {
                    Ok(avatar_bytes) => match load_from_memory(&avatar_bytes) {
                        Ok(mem_bytes) => mem_bytes.to_rgba8(),
                        Err(_) => {
                            return Ok(());
                        }
                    },
                    Err(_) => {
                        return Ok(());
                    }
                },
                Err(_) => return Ok(()),
            }
        };
        let name = member.display_name();
        quote_image(&avatar_image, name, &content)
            .await
            .save("quote.webp")
            .unwrap();
    };
    let paths = [CreateAttachment::path("quote.webp").await?];
    ctx.channel_id()
        .send_files(
            ctx.http(),
            paths.clone(),
            CreateMessage::default().content(&message_url),
        )
        .await?;

    if let Some(guild_id) = ctx.guild_id() {
        if let Ok(record) = query!(
            "SELECT quotes_channel FROM guild_settings WHERE guild_id = $1",
            i64::from(guild_id),
        )
        .fetch_one(&mut *ctx.data().db.acquire().await?)
        .await
        {
            if let Some(channel) = record.quotes_channel {
                let quote_channel = ChannelId::new(
                    u64::try_from(channel).expect("channel id out of bounds for u64"),
                );
                quote_channel
                    .send_files(
                        ctx.http(),
                        paths,
                        CreateMessage::default().content(message_url),
                    )
                    .await?;
            }
        }
    }
    remove_file(Path::new("quote.webp")).await?;
    Ok(())
}

/// When your users are yapping
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn slow_mode(
    ctx: SContext<'_>,
    #[description = "Channel to rate limit"] channel: Channel,
    #[description = "Duration of rate limit in seconds"] duration: NonMaxU16,
) -> Result<(), Error> {
    let settings = EditChannel::default().rate_limit_per_user(duration);
    channel.id().edit(ctx.http(), settings).await?;
    ctx.send(
        CreateReply::default()
            .content(format!("{channel} is ratelimited for {duration} seconds"))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Do you dare?
#[poise::command(slash_command, prefix_command)]
pub async fn troll(ctx: SContext<'_>) -> Result<(), Error> {
    ctx.send(
        CreateReply::default()
            .content(
                "
```
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⣤⣤⡴⠶⠶⠶⠶⠶⠶⠶⠶⢶⣦⣤⣤⣀⣀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣤⡴⠶⠛⠋⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠉⠉⠙⠛⠷⠶⢦⣤⣀⡀
⠀⠀⠀⠀⠀⠀⠀⣠⠞⠉⠀⠀⠀⢀⠀⠀⠒⠀⠀⠀⠀⠀⠒⠒⠐⠒⢒⡀⠈⠀⠀⠀⠀⡀⠒⠀⢀⠀⠀⠀⠈⠛⣦⡀
⠀⠀⠀⠀⠀⢀⣾⠋⠀⠀⢀⠀⢊⠥⢐⠈⠁⠀⠀⠀⢀⠀⠀⠉⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠈⢑⠠⢉⠂⢀⠀⠀⠈⢷⡄
⠀⠀⠀⠀⠀⣼⠃⠀⠀⠀⠀⠀⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⣀⣀⠀⠈⠀⠁⠀⠀⠀⠀⠈⢷⡀
⠀⠀⠀⣠⣾⠃⠀⠀⠀⠀⠀⠀⣠⠶⠛⣉⣩⣽⣟⠳⢶⣄⠀⠀⠀⠀⠀⠀⣠⡶⠛⣻⣿⣯⣉⣙⠳⣆⠀⠀⠀⠀⠀⠀⠈⣷⣄
⠀⣠⠞⠋⠀⢁⣤⣤⣤⣌⡁⠀⠛⠛⠉⣉⡍⠙⠛⠶⣤⠿⠀⢸⠀⠀⡇⠀⠻⠶⠞⠛⠉⠩⣍⡉⠉⠋⠀⣈⣤⡤⠤⢤⣄⠀⠈⠳⣄
⢰⡏⠀⠀⣴⠋⠀⢀⣆⠉⠛⠓⠒⠖⠚⠋⠀⠀⠀⠀⠀⠀⠀⡾⠀⠀⢻⠀⠀⠀⠀⠀⠀⠀⠈⠛⠒⠒⠛⠛⠉⣰⣆⠀⠈⢷⡀⠀⠘⡇
⢸⡇⠀⠀⣧⢠⡴⣿⠉⠛⢶⣤⣀⡀⠀⠠⠤⠤⠄⣶⠒⠂⠀⠀⠀⠀⢀⣀⣘⠛⣷⠀⠀⠀⠀⠀⢀⣠⣴⠶⠛⠉⣿⠷⠤⣸⠃⠀⢀⡟
⠈⢷⡀⠄⠘⠀⠀⠸⣷⣄⡀⠈⣿⠛⠻⠶⢤⣄⣀⡻⠆⠋⠉⠉⠀⠀⠉⠉⠉⠐⣛⣀⣤⡴⠶⠛⠋⣿⠀⣀⣠⣾⠇⠀⠀⠋⠠⢁⡾⠃
⠀⠀⠙⢶⡀⠀⠀⠀⠘⢿⡙⠻⣿⣷⣤⣀⡀⠀⣿⠛⠛⠳⠶⠦⣴⠶⠶⠶⠛⠛⠋⢿⡀⣀⣠⣤⣾⣿⠟⢉⡿⠃⠀⠀⠀⢀⡾⠋
⠀⠀⠀⠈⢻⡄⠀⠀⠀⠈⠻⣤⣼⠉⠙⠻⠿⣿⣿⣤⣤⣤⣀⣀⣿⡀⣀⣀⣠⣤⣶⣾⣿⠿⠛⠋⠁⢿⣴⠟⠁⠀⠀⠀⢠⡟⠁
⠀⠀⠀⠀⠀⢷⡄⠀⠀⠀⠀⠙⠿⣦⡀⠀⠀⣼⠃⠉⠉⠛⠛⠛⣿⡟⠛⠛⠛⠉⠉⠉⢿⡀⠀⣀⣴⠟⠋⠀⠀⠀⠀⢠⡾
⠀⠀⠀⠀⠀⠀⠙⢦⣀⠀⣀⠀⠀⡈⠛⠷⣾⣇⣀⠀⠀⠀⠀⠀⢸⡇⠀⠀⠀⠀⢀⣀⣼⡷⠾⠋⢁⠀⢀⡀⠀⣀⡴⠋
⠀⠀⠀⠀⠀⠀⠀⠀⠙⠳⣦⣉⠒⠬⣑⠂⢄⡉⠙⠛⠛⠶⠶⠶⠾⠷⠶⠚⠛⠛⠛⠉⣁⠤⢐⡨⠤⢒⣩⡴⠞⠋
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠛⠶⣤⣉⠀⠂⠥⠀⠀⠤⠀⠀⠀⠀⠀⠤⠄⠀⠠⠌⠂⢈⣡⡴⠖⠋⠉
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠛⠶⣤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⡴⠞⠋⠁
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠛⠳⠶⠶⠶⠶⠶⠖⠛⠋⠁
```",
            )
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Hmm, I wonder how pure we are
#[poise::command(prefix_command, slash_command)]
pub async fn word_count(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        if let Ok(record) = query!(
            "SELECT word, count FROM words_count WHERE guild_id = $1",
            i64::from(guild_id),
        )
        .fetch_one(&mut *ctx.data().db.acquire().await?)
        .await
        {
            let word = record.word;
            let word_count = record.count;
            ctx.reply(format!(
                "{word} was counted {word_count} times, I'm not sure if that's a good thing or not tho"
            ))
            .await?;
        } else {
            ctx.reply("hmm, no words were counted... peace?").await?;
        }
    }
    Ok(())
}
