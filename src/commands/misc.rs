use crate::{
    types::{Error, SContext, HTTP_CLIENT},
    utils::{ai_response_simple, quote_image},
};

use image::load_from_memory;
use poise::{
    builtins,
    serenity_prelude::{
        nonmax::NonMaxU16, ButtonStyle, Channel, ChannelId, ComponentInteractionCollector,
        CreateActionRow, CreateAttachment, CreateButton, CreateEmbed, CreateInteractionResponse,
        CreateMessage, EditChannel, EditMessage, User, UserId,
    },
    CreateReply,
};
use sqlx::{query, query_as};
use std::{collections::HashSet, path::Path, process, time::Duration};
use tokio::fs::remove_file;

/// When you want to find the imposter
#[poise::command(slash_command)]
pub async fn anony_poll(
    ctx: SContext<'_>,
    #[description = "Question"] title: String,
    #[description = "Comma-separated options"] options: String,
    #[description = "Duration in minutes"] duration: u64,
) -> Result<(), Error> {
    let options_list: Vec<String> = options
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if options_list.len() < 2 {
        ctx.say("Bruh, 1 option ain't gonna cut it for a poll")
            .await?;
        return Ok(());
    }
    let mut embed = CreateEmbed::default().title(title.as_str()).color(0xFF5733);
    let mut buttons: Vec<CreateButton> = Vec::new();
    let mut vote_counts: Vec<u32> = vec![0; options_list.len()];
    let mut voted_users: HashSet<UserId> = HashSet::new();
    for (index, option) in options_list.iter().enumerate() {
        embed = embed.field(option, "0", false);
        buttons.push(
            CreateButton::new(format!("{}_{}", index, ctx.id()))
                .style(ButtonStyle::Primary)
                .label(index.to_string()),
        );
    }
    let components = vec![CreateActionRow::Buttons(buttons)];
    ctx.send(CreateReply::default().embed(embed).components(components))
        .await?;

    let id_borrow = ctx.id();
    let options_count = options_list.len();

    while let Some(interaction) =
        ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
            .timeout(Duration::from_secs(duration * 60))
            .filter(move |interaction| {
                let id = interaction.data.custom_id.as_str();
                (0..options_count).any(|index| {
                    let expected_id = format!("{}_{}", index, id_borrow);
                    id == expected_id
                })
            })
            .await
    {
        let user_id = interaction.user.id;
        if voted_users.contains(&user_id) {
            continue;
        }
        let choice = &interaction.data.custom_id;

        interaction
            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
            .await?;

        let index = choice.split('_').next().unwrap().parse::<usize>().unwrap();
        vote_counts[index] += 1;
        voted_users.insert(user_id);

        let mut new_embed = CreateEmbed::default().title(title.as_str()).color(0xFF5733);
        for (i, option) in options_list.iter().enumerate() {
            new_embed = new_embed.field(option, vote_counts[i].to_string(), false);
        }

        let mut msg = interaction.message;
        msg.edit(ctx.http(), EditMessage::default().embed(new_embed))
            .await?;
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
    let avatar_url = member.avatar_url().unwrap_or(user.avatar_url().unwrap());
    let name = member.display_name();
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::default()
                .title(format!("HAPPY BIRTHDAY {}!", name))
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
    process::exit(0);

    #[allow(unreachable_code)]
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
    user_id: u64,
    message_count: u64,
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
    let thumbnail = match guild.banner_url() {
        Some(banner) => banner,
        None => match guild.icon_url() {
            Some(icon) => icon,
            None => "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned(),
        },
    };
    let mut users = query_as!(
        UserCount,
        "SELECT message_count, user_id FROM user_settings WHERE guild_id = ?",
        u64::from(guild.id)
    )
    .fetch_all(&mut *ctx.data().db.acquire().await?)
    .await?;
    users.sort_by(|a, b| b.message_count.cmp(&a.message_count));
    let mut embed = CreateEmbed::default()
        .title("Server leaderboard of sent messages")
        .thumbnail(thumbnail)
        .color(0xFF5733);
    for user in users.into_iter() {
        if let Ok(target) = ctx.http().get_user(user.user_id.into()).await {
            let user_name = target.display_name().to_owned();
            embed = embed.field(user_name, user.message_count.to_string(), false);
        }
    }

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Oh it's you
#[poise::command(prefix_command, slash_command)]
pub async fn ohitsyou(ctx: SContext<'_>) -> Result<(), Error> {
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
    let msg = ctx
        .channel_id()
        .message(&ctx.http(), ctx.id().into())
        .await?;
    let reply = match msg.referenced_message {
        Some(ref_msg) => ref_msg,
        None => {
            ctx.reply("Bruh, reply to a message").await?;
            return Ok(());
        }
    };

    let message_url = reply.link();
    let content = reply.content;
    match reply.webhook_id {
        Some(_) => {
            let avatar_image = {
                let avatar_url = reply.author.avatar_url().unwrap();
                let avatar_bytes = HTTP_CLIENT
                    .get(&avatar_url)
                    .send()
                    .await
                    .unwrap()
                    .bytes()
                    .await
                    .unwrap();
                load_from_memory(&avatar_bytes).unwrap().to_rgba8()
            };
            let name = reply.author.display_name();
            quote_image(&avatar_image, name, &content)
                .await
                .save("quote.webp")
                .unwrap();
        }
        None => {
            let guild = match ctx.guild() {
                Some(guild) => guild.clone(),
                None => {
                    return Ok(());
                }
            };
            let member = guild.member(ctx.http(), reply.author.id).await?;
            let avatar_image = {
                let avatar_url = member
                    .avatar_url()
                    .unwrap_or(reply.author.avatar_url().unwrap());
                let avatar_bytes = HTTP_CLIENT
                    .get(&avatar_url)
                    .send()
                    .await
                    .unwrap()
                    .bytes()
                    .await
                    .unwrap();
                load_from_memory(&avatar_bytes).unwrap().to_rgba8()
            };
            let name = member.display_name();
            quote_image(&avatar_image, name, &content)
                .await
                .save("quote.webp")
                .unwrap();
        }
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
            "SELECT quotes_channel FROM guild_settings WHERE guild_id = ?",
            guild_id.get()
        )
        .fetch_one(&mut *ctx.data().db.acquire().await?)
        .await
        {
            if let Some(channel) = record.quotes_channel {
                let quote_channel = ChannelId::new(channel);
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
            .content(format!(
                "{} is ratelimited for {} seconds",
                channel, duration
            ))
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
                r#"
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
```"#,
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
            "SELECT word, count FROM words_count WHERE guild_id = ?",
            guild_id.get()
        )
        .fetch_one(&mut *ctx.data().db.acquire().await?)
        .await
        {
            ctx.reply(format!(
                "{} was counted {} times, I'm not sure if that's a good thing or not tho",
                record.word, record.count
            ))
            .await?;
        } else {
            ctx.reply("hmm, no words were counted... peace?").await?;
        }
    }
    Ok(())
}
