use crate::{
    config::{
        constants::{COLOUR_RED, FONTS, QUOTE_HEIGHT, QUOTE_WIDTH},
        types::{Error, SContext, HTTP_CLIENT},
    },
    utils::{ai::ai_response_simple, image::quote_image},
};

use ab_glyph::FontArc;
use anyhow::Context;
use dashmap::DashSet;
use image::{load_from_memory, ImageBuffer, ImageError, ImageFormat::WebP, Rgba, RgbaImage};
use poise::{
    serenity_prelude::{
        nonmax::NonMaxU16, ButtonStyle, Channel, ChannelId, ComponentInteractionCollector,
        ComponentInteractionDataKind, CreateActionRow, CreateAllowedMentions, CreateAttachment,
        CreateButton, CreateEmbed, CreateInteractionResponse, CreateMessage, CreateSelectMenu,
        CreateSelectMenuKind, CreateSelectMenuOption, EditChannel, EditMessage, Member, MessageId,
        UserId,
    },
    CreateReply,
};
use sqlx::query;
use std::{io::Cursor, time::Duration};
use tokio::time::{sleep, timeout};

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
    #[description = "Command to get help with"] command: Option<String>,
) -> Result<(), Error> {
    ctx.say("help").await?;
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
            ctx.reply(
                "Ugh, fine. It's nice to see you again, I suppose... 
                for now, don't get any ideas thinking this means I actually like you or anything",
            )
            .await?;
        }
    }
    Ok(())
}

pub struct ImageInfo {
    avatar_image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    author_name: String,
    content: String,
    current: RgbaImage,
    is_bw: bool,
    is_reverse: bool,
    is_light: bool,
    is_gradient: bool,
    content_font: FontArc,
    author_font: FontArc,
}

impl ImageInfo {
    pub async fn new(
        avatar_image: ImageBuffer<Rgba<u8>, Vec<u8>>,
        author_name: String,
        content: String,
    ) -> Self {
        let content_font = FontArc::try_from_slice(FONTS[0].1).unwrap();
        let author_font = FontArc::try_from_slice(FONTS[1].1).unwrap();
        let image = quote_image(
            &avatar_image,
            &author_name,
            &content,
            &author_font,
            &content_font,
            false,
            false,
            false,
            false,
        )
        .await;
        Self {
            avatar_image,
            author_name,
            content,
            current: image,
            is_bw: false,
            is_reverse: false,
            is_light: false,
            is_gradient: false,
            author_font,
            content_font,
        }
    }

    pub async fn toggle_bw(&mut self) {
        self.is_bw = !self.is_bw;
        self.image_gen().await;
    }

    pub async fn toggle_reverse(&mut self) {
        self.is_reverse = !self.is_reverse;
        self.image_gen().await;
    }

    pub async fn toggle_light(&mut self) {
        self.is_light = !self.is_light;
        self.image_gen().await;
    }

    pub async fn toggle_gradient(&mut self) {
        self.is_gradient = !self.is_gradient;
        self.image_gen().await;
    }

    pub fn write_to_webp(&self, buffer: &mut Vec<u8>) -> Result<(), ImageError> {
        buffer.clear();
        let mut cursor = Cursor::new(buffer);
        self.current.write_to(&mut cursor, WebP)
    }

    pub async fn new_font(&mut self, new_font: FontArc) {
        self.content_font = new_font;
        self.image_gen().await;
    }

    pub async fn image_gen(&mut self) {
        self.current = quote_image(
            &self.avatar_image,
            &self.author_name,
            &self.content,
            &self.author_font,
            &self.content_font,
            self.is_reverse,
            self.is_light,
            self.is_bw,
            self.is_gradient,
        )
        .await;
    }
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

        let mut image_handle = {
            let (avatar_image, author_name) = if reply.webhook_id.is_some() {
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
                (mem_bytes.to_rgba8(), format!("- {}", reply.author.name))
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
                (mem_bytes.to_rgba8(), format!("- {}", member.user.name))
            };

            ImageInfo::new(avatar_image, author_name, reply.content.to_string()).await
        };
        let mut buffer =
            Vec::with_capacity(usize::try_from(QUOTE_HEIGHT * QUOTE_WIDTH).unwrap_or(0));
        image_handle.write_to_webp(&mut buffer)?;
        let message_url = reply.link();
        let attachment = CreateAttachment::bytes(buffer.clone(), "quote.webp");
        let buttons = [
            CreateButton::new(format!("{}_bw", ctx.id()))
                .style(ButtonStyle::Primary)
                .label("🎨"),
            CreateButton::new(format!("{}_reverse", ctx.id()))
                .style(ButtonStyle::Primary)
                .label("🪞"),
            CreateButton::new(format!("{}_light", ctx.id()))
                .style(ButtonStyle::Primary)
                .label("🔆"),
            CreateButton::new(format!("{}_gradient", ctx.id()))
                .style(ButtonStyle::Primary)
                .label("🌫️"),
        ];
        let mut font_select: Vec<CreateSelectMenuOption> = Vec::with_capacity(FONTS.len());

        for font in FONTS {
            font_select.push(CreateSelectMenuOption::new(font.0, font.0));
        }

        let font_menu = CreateSelectMenu::new(
            format!("{}_font_option", ctx.id()),
            CreateSelectMenuKind::String {
                options: font_select.into(),
            },
        )
        .placeholder("Font")
        .min_values(1)
        .max_values(1);
        let action_row = [CreateActionRow::buttons(&buttons)];
        let mut message = ctx
            .channel_id()
            .send_message(
                ctx.http(),
                CreateMessage::default()
                    .add_file(attachment.clone())
                    .reference_message(&msg)
                    .content(&message_url)
                    .components(&action_row)
                    .select_menu(font_menu)
                    .allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
            )
            .await?;
        if let Some(guild_data) = ctx.data().guild_data.get(&guild_id)
            && let Some(channel) = guild_data.settings.quotes_channel
        {
            let quote_channel =
                ChannelId::new(u64::try_from(channel).expect("channel id out of bounds for u64"));
            quote_channel
                .send_message(
                    ctx.http(),
                    CreateMessage::default()
                        .add_file(attachment.clone())
                        .content(&message_url),
                )
                .await?;
        }
        let ctx_id_copy = ctx.id();
        let mut final_attachment = attachment.clone();
        while let Some(interaction) =
            ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                .timeout(Duration::from_secs(60))
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

            let menu_choice = match &interaction.data.kind {
                ComponentInteractionDataKind::StringSelect { values } => Some(&values[0]),
                _ => None,
            };

            if let Some(font_choice) = menu_choice
                && let Some(font) = FONTS.iter().find(|font| font.0 == font_choice)
            {
                image_handle
                    .new_font(FontArc::try_from_slice(font.1).unwrap())
                    .await;
            } else if interaction.data.custom_id.ends_with("bw") {
                image_handle.toggle_bw().await;
            } else if interaction.data.custom_id.ends_with("reverse") {
                image_handle.toggle_reverse().await;
            } else if interaction.data.custom_id.ends_with("light") {
                image_handle.toggle_light().await;
            } else if interaction.data.custom_id.ends_with("gradient") {
                image_handle.toggle_gradient().await;
            }
            image_handle.write_to_webp(&mut buffer)?;
            let mut msg = interaction.message;
            final_attachment = CreateAttachment::bytes(buffer.clone(), "quote.webp");
            msg.edit(
                ctx.http(),
                EditMessage::default().new_attachment(final_attachment.clone()),
            )
            .await?;
        }
        message
            .edit(
                ctx,
                EditMessage::default()
                    .new_attachment(final_attachment)
                    .components(&[]),
            )
            .await?;
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
