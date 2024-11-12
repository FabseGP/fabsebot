use crate::{
    config::{
        constants::COLOUR_RED,
        types::{Error, SContext, HTTP_CLIENT},
    },
    utils::{ai::ai_response_simple, image::quote_image},
};

use anyhow::Context;
use dashmap::DashSet;
use image::load_from_memory;
use poise::{
    builtins::{self, pretty_help},
    serenity_prelude::{
        nonmax::NonMaxU16, ButtonStyle, Channel, ChannelId, ComponentInteractionCollector,
        CreateActionRow, CreateAllowedMentions, CreateAttachment, CreateButton, CreateEmbed,
        CreateInteractionResponse, CreateMessage, EditChannel, EditMessage, Member, MessageId,
        UserId,
    },
    CreateReply,
};
use sqlx::query;
use std::{path::Path, time::Duration};
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
    let mut final_embed = embed.clone();

    let ctx_id_copy = ctx.id();
    let buttons: Vec<CreateButton> = (0..options_count)
        .map(|index| {
            CreateButton::new(format!("{ctx_id_copy}_{index}"))
                .style(ButtonStyle::Primary)
                .label((index + 1).to_string())
        })
        .collect();
    let action_row = [CreateActionRow::buttons(&buttons)];

    let message = ctx
        .send(CreateReply::default().embed(embed).components(&action_row))
        .await?;

    let mut vote_counts = vec![0; options_count];
    let voted_users = DashSet::new();

    while let Some(interaction) =
        ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
            .timeout(Duration::from_secs(duration * 60))
            .filter(move |interaction| {
                interaction
                    .data
                    .custom_id
                    .starts_with(ctx_id_copy.to_string().as_str())
            })
            .await
    {
        interaction
            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
            .await?;
        if voted_users.insert(interaction.user.id) {
            if let Some(index) = interaction
                .data
                .custom_id
                .split('_')
                .nth(1)
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
                    final_embed = new_embed.clone();

                    let mut msg = interaction.message;
                    msg.edit(ctx.http(), EditMessage::default().embed(new_embed))
                        .await?;
                }
            }
        } else {
            ctx.send(
                CreateReply::default()
                    .content("bruh, you have already voted!")
                    .ephemeral(true),
            )
            .await?;
        }
    }
    message
        .edit(
            ctx,
            CreateReply::default().embed(final_embed).components(&[]),
        )
        .await?;

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
        CreateReply::default()
            .embed(
                CreateEmbed::default()
                    .title(format!("HAPPY BIRTHDAY {name}!"))
                    .thumbnail(avatar_url)
                    .image("https://media.tenor.com/GiCE3Iq3_TIAAAAC/pokemon-happy-birthday.gif")
                    .colour(COLOUR_RED),
            )
            .reply(true),
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
            .global_chats
            .remove(&guild_id)
            .unwrap_or_default();
        if let Some(mut guild_data) = ctx.data().guild_data.get_mut(&guild_id) {
            guild_data.settings.global_chat = false;
        }
        ctx.reply("Call ended...").await?;
    }
    Ok(())
}

/// When you're lonely and need someone to chat with
#[poise::command(prefix_command, slash_command)]
pub async fn global_chat_start(ctx: SContext<'_>) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let guild_id_i64 = i64::from(guild_id);
        let channel_id_i64 = i64::from(ctx.channel_id());
        let mut tx = ctx.data().db.begin().await?;
        query!(
            "INSERT INTO guild_settings (guild_id, global_chat, global_chat_channel)
            VALUES ($1, TRUE, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                global_chat = TRUE,
                global_chat_channel = $2",
            guild_id_i64,
            channel_id_i64,
        )
        .execute(&mut *tx)
        .await?;
        if let Some(mut guild_data) = ctx.data().guild_data.get_mut(&guild_id) {
            guild_data.settings.global_chat = true;
            guild_data.settings.global_chat_channel = Some(channel_id_i64);
            let message = ctx.reply("Calling...").await?;
            let result = timeout(Duration::from_secs(60), async {
                loop {
                    let has_other_calls = ctx.data().guild_data.iter().any(|entry| {
                        entry.key() != &guild_id
                            && entry.value().settings.global_chat
                            && entry.value().settings.global_chat_channel.is_some()
                    });
                    if has_other_calls {
                        return Ok::<_, Error>(true);
                    }
                    sleep(Duration::from_secs(5)).await;
                }
            })
            .await;
            if result.is_ok() {
                message
                    .edit(
                        ctx,
                        CreateReply::default()
                            .reply(true)
                            .content("Connected to global call!"),
                    )
                    .await?;
            } else {
                query!(
                    "UPDATE guild_settings SET global_chat = FALSE, global_chat_channel = NULL WHERE guild_id = $1",
                    guild_id_i64
                )
                .execute(&mut *tx)
                .await?;
                guild_data.settings.global_chat = false;
                guild_data.settings.global_chat_channel = None;
                message
                    .edit(
                        ctx,
                        CreateReply::default()
                            .reply(true)
                            .content("No one joined the call within 1 minute 😢"),
                    )
                    .await?;
            }
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
    pretty_help(
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
    id: i64,
    count: i32,
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

        let mut users =
            ctx.data()
                .user_settings
                .get(&guild_id)
                .map_or_else(Vec::new, |user_settings| {
                    user_settings
                        .iter()
                        .map(|entry| UserCount {
                            id: entry.value().user_id,
                            count: entry.value().message_count,
                        })
                        .collect::<Vec<_>>()
                });

        users.sort_by(|a, b| b.count.cmp(&a.count));
        users.truncate(25);

        let mut embed = CreateEmbed::default()
            .title(format!("Top {} users by message count", users.len()))
            .thumbnail(thumbnail)
            .colour(COLOUR_RED);

        for (index, user) in users.iter().enumerate() {
            if let Ok(target) = guild_id
                .member(
                    &ctx.http(),
                    UserId::new(u64::try_from(user.id).expect("user id out of bounds for u64")),
                )
                .await
            {
                let rank = index + 1;
                let user_name = target.display_name();
                embed = embed.field(
                    format!("#{rank} {user_name}"),
                    user.count.to_string(),
                    false,
                );
            }
        }

        ctx.send(CreateReply::default().reply(true).embed(embed))
            .await?;
    }
    Ok(())
}

/// Oh it's you
#[poise::command(
    prefix_command,
    slash_command,
 /*   install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel" */
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
            ctx.reply(
                "Ugh, fine. It's nice to see you again, I suppose... 
                for now, don't get any ideas thinking this means I actually like you or anything",
            )
            .await?;
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

        let Some(ref reply) = msg.referenced_message else {
            ctx.reply("Bruh, reply to a message").await?;
            return Ok(());
        };

        ctx.defer().await?;
        let message_url = reply.link();
        let content = &reply.content;
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
            (
                mem_bytes.to_rgba8(),
                reply.author.name.clone().into_string(),
            )
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

        quote_image(&avatar_image, &name, content)
            .await
            .save(quote_path)
            .unwrap();

        let attachment = CreateAttachment::path(quote_path).await?;

        ctx.channel_id()
            .send_files(
                ctx.http(),
                [attachment.clone()],
                CreateMessage::default()
                    .reference_message(&msg)
                    .content(&message_url)
                    .allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
            )
            .await?;

        if let Some(guild_data) = ctx.data().guild_data.get(&guild_id) {
            if let Some(channel) = guild_data.settings.quotes_channel {
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
            .content(format!("{channel} is ratelimited for {duration}s"))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

struct WordCount {
    word: String,
    count: i64,
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

        let mut words = ctx
            .data()
            .guild_data
            .get(&guild_id)
            .map_or_else(Vec::new, |guild_data| {
                guild_data
                    .word_tracking
                    .iter()
                    .map(|entry| WordCount {
                        word: entry.word.clone(),
                        count: entry.count,
                    })
                    .collect::<Vec<_>>()
            });

        words.sort_by(|a, b| b.count.cmp(&a.count));
        words.truncate(25);

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
        ctx.send(CreateReply::default().reply(true).embed(embed))
            .await?;
    }
    Ok(())
}
