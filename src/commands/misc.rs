use crate::types::{Context, Error};
use crate::utils::{ai_response_simple, quote_image};

use image::load_from_memory;
use poise::serenity_prelude::{
    self as serenity, ChannelId, CreateAttachment, CreateEmbed, CreateMessage, EditChannel,
};
use poise::CreateReply;
use serenity::{
    model::{channel::Channel, Timestamp},
    nonmax::NonMaxU16,
};
use sqlx::{query, query_as};
use std::{path::Path, process, sync::Arc};
use tokio::fs::remove_file;

/// Send a birthday wish to a user
#[poise::command(prefix_command, slash_command)]
pub async fn birthday(
    ctx: Context<'_>,
    #[description = "User to congratulate"]
    #[rest]
    user: serenity::User,
) -> Result<(), Error> {
    let member = ctx
        .http()
        .get_member(ctx.guild_id().unwrap(), user.id)
        .await?;
    let avatar_url = member.avatar_url().unwrap_or(user.avatar_url().unwrap());
    let name = member.display_name();
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .title(format!("HAPPY BIRTHDAY {}!", name))
                .thumbnail(avatar_url)
                .image("https://media.tenor.com/GiCE3Iq3_TIAAAAC/pokemon-happy-birthday.gif")
                .color(0xFF5733)
                .timestamp(Timestamp::now()),
        ),
    )
    .await?;
    Ok(())
}

/// Ignore this command
#[poise::command(prefix_command, owners_only)]
pub async fn end_pgo(_: Context<'_>) -> Result<(), Error> {
    process::exit(0);

    #[allow(unreachable_code)]
    Ok(())
}

/// When you need some help
#[poise::command(prefix_command, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::pretty_help(
        ctx,
        command.as_deref(),
        poise::builtins::PrettyHelpConfiguration {
            extra_text_at_bottom: "Courtesy of Fabseman Inc.",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

struct User {
    user_id: u64,
    message_count: u64,
}

/// Leaderboard of lifeless ppl
#[poise::command(prefix_command, slash_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(g) => Arc::new(g.clone()),
        None => {
            return Ok(());
        }
    };

    let thumbnail = if let Some(banner) = guild.banner.clone() {
        banner.to_string()
    } else if let Some(icon_hash) = &guild.icon {
        format!(
            "https://cdn.discordapp.com/icons/{}/{}.png",
            guild.id, icon_hash
        )
    } else {
        "https://external-content.duckduckgo.com/iu/?u=http%3A%2F%2Fvignette1.wikia.nocookie.net%2Fpokemon%2Fimages%2Fe%2Fe2%2F054Psyduck_Pokemon_Mystery_Dungeon_Red_and_Blue_Rescue_Teams.png%2Frevision%2Flatest%3Fcb%3D20150106002458&f=1&nofb=1&ipt=b7e9fef392b547546f7aded0dbc11449fe38587bfc507022a8f103995eaf8dd0&ipo=images".to_string()
    };
    let mut users = query_as!(
        User,
        "SELECT message_count, user_id FROM user_settings WHERE guild_id = ?",
        u64::from(ctx.guild_id().unwrap())
    )
    .fetch_all(&mut *ctx.data().db.acquire().await?)
    .await?;
    users.sort_by(|a, b| b.message_count.cmp(&a.message_count));
    let mut embed = CreateEmbed::new()
        .title("Server leaderboard of sent messages")
        .thumbnail(thumbnail)
        .color(0xFF5733);
    for user in users.into_iter() {
        let target = ctx.http().get_user(user.user_id.into()).await.unwrap();
        let user_name = target.display_name().to_string();
        embed = embed.field(user_name, user.message_count.to_string(), false);
    }

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Oh it's you
#[poise::command(prefix_command, slash_command)]
pub async fn ohitsyou(ctx: Context<'_>) -> Result<(), Error> {
    let resp = ai_response_simple(
        "you're a tsundere".to_string(),
        "generate a one-line love-hate greeting".to_string(),
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
pub async fn quote(ctx: Context<'_>) -> Result<(), Error> {
    let reply = match ctx
        .channel_id()
        .message(&ctx.http(), ctx.id().into())
        .await?
    {
        msg if msg.referenced_message.is_some() => msg.referenced_message.unwrap(),
        _ => {
            ctx.reply("bruh, reply to a message").await?;
            return Ok(());
        }
    };

    let message_url = reply.link();
    let content = reply.content.to_string();
    if reply.webhook_id.is_none() {
        let member = ctx
            .http()
            .get_member(ctx.guild_id().unwrap(), reply.author.id)
            .await?;
        let avatar_image = {
            let avatar_url = member
                .avatar_url()
                .unwrap_or(reply.author.avatar_url().unwrap());
            let avatar_bytes = reqwest::get(&avatar_url)
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
    } else {
        let avatar_image = {
            let avatar_url = reply.author.avatar_url().unwrap();
            let avatar_bytes = reqwest::get(&avatar_url)
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
    let paths = [CreateAttachment::path("quote.webp").await?];
    ctx.channel_id()
        .send_files(
            ctx.http(),
            paths.clone(),
            CreateMessage::new().content(&message_url),
        )
        .await?;

    if let Ok(record) = query!(
        "SELECT quotes_channel FROM guild_settings WHERE guild_id = ?",
        ctx.guild_id().unwrap().get()
    )
    .fetch_one(&mut *ctx.data().db.acquire().await?)
    .await
    {
        if let Some(channel) = record.quotes_channel {
            let quote_channel = ChannelId::new(channel);
            quote_channel
                .send_files(ctx.http(), paths, CreateMessage::new().content(message_url))
                .await?;
        }
    }
    remove_file(Path::new("quote.webp")).await?;
    Ok(())
}

/// Hmm, I wonder how pure we are
#[poise::command(prefix_command, slash_command)]
pub async fn pure_count(ctx: Context<'_>) -> Result<(), Error> {
    let id: u64 = ctx.guild_id().unwrap().into();
    if let Ok(record) = query!(
        "SELECT count FROM words_count WHERE word = ? AND guild_id = ?",
        "nigga",
        id
    )
    .fetch_one(&mut *ctx.data().db.acquire().await?)
    .await
    {
        ctx.reply(format!(
            "oof, {} n-words counted, martin luther king jr. would be disappointed fr",
            record.count
        ))
        .await?;
    } else {
        ctx.reply("hmm, 0 n-word counted... yeet must be gone")
            .await?;
    }
    Ok(())
}

/// When your users are yapping
#[poise::command(slash_command)]
pub async fn slow_mode(
    ctx: Context<'_>,
    #[description = "Channel to rate limit"] channel: Channel,
    #[description = "Duration of rate limit in seconds"] duration: NonMaxU16,
) -> Result<(), Error> {
    let admin_perms = ctx
        .author_member()
        .await
        .unwrap()
        .permissions
        .unwrap()
        .administrator();
    if ctx.author().id == ctx.partial_guild().await.unwrap().owner_id
        || admin_perms
        || ctx.author().id == 1014524859532980255
    {
        let settings = EditChannel::new().rate_limit_per_user(duration);
        channel.id().edit(ctx.http(), settings).await?;
        ctx.send(
            CreateReply::default()
                .content(format!("channel is ratelimited for {} seconds", duration))
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("hush, you're not permitted to use this command")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}

/// Do you dare?
#[poise::command(slash_command, prefix_command)]
pub async fn troll(ctx: Context<'_>) -> Result<(), Error> {
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
