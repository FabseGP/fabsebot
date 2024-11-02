use crate::{
    consts::{COLOUR_RED, TSUNDERE_FALLBACK},
    types::{Error, SContext, HTTP_CLIENT},
    utils::{ai_response_simple, quote_image},
};

use anyhow::Context;
use dashmap::DashSet;
use image::load_from_memory;
use poise::{
    builtins,
    serenity_prelude::{
        nonmax::NonMaxU16, ButtonStyle, Channel, ChannelId, ComponentInteractionCollector,
        CreateActionRow, CreateAttachment, CreateButton, CreateEmbed, CreateInteractionResponse,
        CreateMessage, EditChannel, EditMessage, Member, MessageId, UserId,
    },
    CreateReply,
};
use sqlx::{query, query_as};
use std::{borrow::Cow, path::Path, time::Duration};
use tokio::{
    fs::remove_file,
    time::{sleep, timeout},
};

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
    if options_count < 1 {
        ctx.say("Bruh, no options ain't gonna cut it for a poll!")
            .await?;
        return Ok(());
    }

    let embed = CreateEmbed::default()
        .title(title.as_str())
        .colour(COLOUR_RED)
        .fields(options_list.iter().map(|&option| (option, "0", false)));

    let ctx_id = ctx.id();
    let action_row = CreateActionRow::Buttons(Cow::Owned(
        (0..options_count)
            .map(|index| {
                CreateButton::new(format!("option_{index}_{ctx_id}"))
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

    while let Some(interaction) =
        ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
            .timeout(Duration::from_secs(duration * 60))
            .filter(move |interaction| {
                let id = interaction.data.custom_id.as_str();
                (0..options_count).any(|index| {
                    let expected_id = format!("option_{index}_{ctx_id}");
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

            if let Some(index) = choice
                .strip_prefix("option_")
                .and_then(|s| s.split('_').next())
                .and_then(|s| s.parse::<usize>().ok())
            {
                if index < options_count {
                    vote_counts[index] += 1;

                    let new_embed = CreateEmbed::default()
                        .title(&title)
                        .colour(COLOUR_RED)
                        .fields(
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
        }
    }

    Ok(())
}

/// Send a birthday wish to a member
#[poise::command(prefix_command, slash_command)]
pub async fn birthday(
    ctx: SContext<'_>,
    #[description = "Member to congratulate"]
    #[rest]
    member: Member,
) -> Result<(), Error> {
    let avatar_url = member.avatar_url().unwrap_or_else(|| {
        member.user.avatar_url().unwrap_or_else(|| {
            member
                .user
                .avatar_url()
                .unwrap_or_else(|| member.user.default_avatar_url())
        })
    });
    let name = member.display_name();
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::default()
                .title(format!("HAPPY BIRTHDAY {name}!"))
                .thumbnail(avatar_url)
                .image("https://media.tenor.com/GiCE3Iq3_TIAAAAC/pokemon-happy-birthday.gif")
                .colour(COLOUR_RED),
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

/// When you're not lonely anymore
#[poise::command(prefix_command, slash_command)]
pub async fn global_chat_end(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        query!(
            "INSERT INTO guild_settings (guild_id, global_chat)
            VALUES ($1, FALSE)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                global_chat = FALSE",
            i64::from(guild_id),
        )
        .execute(&mut *ctx.data().db.acquire().await?)
        .await?;
        ctx.data()
            .global_chat_last
            .remove(&guild_id)
            .unwrap_or_default();
        ctx.reply("Call ended...").await?;
    }
    Ok(())
}

/// When you're lonely and need someone to chat with
#[poise::command(prefix_command, slash_command)]
pub async fn global_chat_start(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let guild_id_i64 = i64::from(guild_id);
        let mut tx = ctx.data().db.begin().await?;
        query!(
            "INSERT INTO guild_settings (guild_id, global_chat, global_chat_channel)
            VALUES ($1, TRUE, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                global_chat = TRUE,
                global_chat_channel = $2",
            guild_id_i64,
            i64::from(ctx.channel_id()),
        )
        .execute(&mut *tx)
        .await?;
        let message = ctx.reply("Calling...").await?;
        let result = timeout(Duration::from_secs(60), async {
            loop {
                let other_calls = query!(
                    "SELECT EXISTS(
                        SELECT 1 FROM guild_settings 
                        WHERE guild_id != $1 AND global_chat = TRUE
                    ) AS HAS_CALL",
                    guild_id_i64
                )
                .fetch_optional(&mut *tx)
                .await?;
                if other_calls.is_some() {
                    return Ok::<_, Error>(true);
                }
                sleep(Duration::from_secs(5)).await;
            }
        })
        .await;
        let found_call = result.unwrap_or(Ok(false))?;
        if found_call {
            message
                .edit(
                    ctx,
                    CreateReply::default().content("Connected to global call!"),
                )
                .await?;
        } else {
            query!(
                "UPDATE guild_settings SET global_chat = FALSE WHERE guild_id = $1",
                guild_id_i64
            )
            .execute(&mut *tx)
            .await?;
            message
                .edit(
                    ctx,
                    CreateReply::default().content("No one joined the call within 1 minute 😢"),
                )
                .await?;
        }
        tx.commit()
            .await
            .context("Failed to commit sql-transaction")?;
    }
    Ok(())
}

/// When you need some help
#[poise::command(prefix_command, slash_command)]
pub async fn help(
    ctx: SContext<'_>,
    #[description = "Command to show help about"]
    #[autocomplete = "builtins::autocomplete_command"]
    #[rest]
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
    if let Some(guild_id) = ctx.guild_id() {
        let thumbnail = match ctx.guild() {
            Some(guild) => guild.banner_url().unwrap_or_else(|| {
                guild
                    .icon_url()
                    .unwrap_or_else(|| "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned())
            }),
            None => {
                return Ok(());
            }
        };
        ctx.defer().await?;
        let users = query_as!(
            UserCount,
            "SELECT message_count, user_id FROM user_settings WHERE guild_id = $1
            ORDER BY message_count 
            DESC LIMIT 25",
            i64::from(guild_id)
        )
        .fetch_all(&mut *ctx.data().db.acquire().await?)
        .await?;

        let mut embed = CreateEmbed::default()
            .title(format!("Top {} users by message count", users.len()))
            .thumbnail(thumbnail)
            .colour(COLOUR_RED);

        for (index, user) in users.iter().enumerate() {
            if let Ok(target) = guild_id
                .member(
                    &ctx.http(),
                    UserId::new(
                        u64::try_from(user.user_id).expect("user id out of bounds for u64"),
                    ),
                )
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
    }
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
    match ai_response_simple(
        "you're a tsundere",
        "generate a one-line love-hate greeting",
    )
    .await
    {
        Some(resp) => {
            ctx.reply(resp).await?;
        }
        None => {
            ctx.reply(TSUNDERE_FALLBACK).await?;
        }
    }
    Ok(())
}

/// When your memory is not enough
#[poise::command(prefix_command)]
pub async fn quote(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let msg = ctx
            .channel_id()
            .message(&ctx.http(), MessageId::new(ctx.id()))
            .await?;

        let Some(reply) = msg.referenced_message else {
            ctx.reply("Bruh, reply to a message").await?;
            return Ok(());
        };

        ctx.defer().await?;
        let message_url = reply.link();
        let content = reply.content;
        let quote_path = Path::new("quote.webp");

        let (avatar_image, name) = if reply.webhook_id.is_some() {
            let avatar_url = reply.author.avatar_url().unwrap_or_else(|| {
                reply
                    .author
                    .static_avatar_url()
                    .unwrap_or_else(|| reply.author.default_avatar_url())
            });
            let Ok(resp) = HTTP_CLIENT.get(&avatar_url).send().await else {
                return Ok(());
            };
            let Ok(avatar_bytes) = resp.bytes().await else {
                return Ok(());
            };
            let Ok(mem_bytes) = load_from_memory(&avatar_bytes) else {
                return Ok(());
            };
            (mem_bytes.to_rgba8(), reply.author.name.into_string())
        } else {
            let member = guild_id.member(&ctx.http(), reply.author.id).await?;
            let avatar_url = member.avatar_url().unwrap_or_else(|| {
                member
                    .user
                    .static_avatar_url()
                    .unwrap_or_else(|| member.user.default_avatar_url())
            });
            let Ok(resp) = HTTP_CLIENT.get(&avatar_url).send().await else {
                return Ok(());
            };
            let Ok(avatar_bytes) = resp.bytes().await else {
                return Ok(());
            };
            let Ok(mem_bytes) = load_from_memory(&avatar_bytes) else {
                return Ok(());
            };
            (mem_bytes.to_rgba8(), member.user.name.into_string())
        };

        quote_image(&avatar_image, &name, &content)
            .await
            .save(quote_path)
            .unwrap();

        let attachment = CreateAttachment::path(quote_path).await?;

        ctx.channel_id()
            .send_files(
                ctx.http(),
                [attachment.clone()],
                CreateMessage::default().content(&message_url),
            )
            .await?;

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
                        [attachment],
                        CreateMessage::default().content(message_url),
                    )
                    .await?;
            }
        }

        remove_file(quote_path).await?;
    }
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

/// Count of tracked words
#[poise::command(prefix_command, slash_command)]
pub async fn word_count(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let thumbnail = match ctx.guild() {
            Some(guild) => guild.banner_url().unwrap_or_else(|| {
                guild
                    .icon_url()
                    .unwrap_or_else(|| "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned())
            }),
            None => {
                return Ok(());
            }
        };
        let words = query!(
            "SELECT word, count FROM guild_word_tracking WHERE guild_id = $1
            ORDER BY count
            DESC LIMIT 25",
            i64::from(guild_id),
        )
        .fetch_all(&mut *ctx.data().db.acquire().await?)
        .await?;
        let mut embed = CreateEmbed::default()
            .title(format!("Top {} word tracked by count", words.len()))
            .thumbnail(thumbnail)
            .colour(COLOUR_RED);
        for (index, word) in words.iter().enumerate() {
            let rank = index + 1;
            embed = embed.field(
                format!("#{rank} {}", word.word),
                word.count.to_string(),
                false,
            );
        }
        ctx.send(CreateReply::default().embed(embed)).await?;
    }
    Ok(())
}
