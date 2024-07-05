use crate::types::{Context, Error};

use poise::serenity_prelude::{self as serenity, CreateEmbed};
use poise::CreateReply;
use serenity::model::Timestamp;
use std::sync::Arc;

/// Send a birthday wish to a user
#[poise::command(slash_command, prefix_command)]
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
    let name = member.nick.unwrap_or(user.name);
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

/// When you need some help
#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration {
            extra_text_at_bottom: "Courtesy of Fabseman Inc.",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

/// Leaderboard of lifeless ppl
#[poise::command(slash_command, prefix_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(g) => Arc::new(g.clone()),
        None => {
            return Ok(());
        }
    };

    let thumbnail = if let Some(banner) = guild.banner.clone() {
        banner
    } else if let Some(icon_hash) = &guild.icon {
        format!(
            "https://cdn.discordapp.com/icons/{}/{}.png",
            guild.id, icon_hash
        )
    } else {
        "https://external-content.duckduckgo.com/iu/?u=http%3A%2F%2Fvignette1.wikia.nocookie.net%2Fpokemon%2Fimages%2Fe%2Fe2%2F054Psyduck_Pokemon_Mystery_Dungeon_Red_and_Blue_Rescue_Teams.png%2Frevision%2Flatest%3Fcb%3D20150106002458&f=1&nofb=1&ipt=b7e9fef392b547546f7aded0dbc11449fe38587bfc507022a8f103995eaf8dd0&ipo=images".to_string()
    };
    let mut conn = ctx.data().db.acquire().await?;
    let id: u64 = ctx.guild_id().unwrap().into();
    let mut users = sqlx::query_as::<_, (String, u64)>(
        "SELECT user_name, messages FROM message_count WHERE guild_id = ?",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    users.sort_by(|a, b| b.1.cmp(&a.1));
    let mut embed = CreateEmbed::new()
        .title("Server leaderboard")
        .thumbnail(thumbnail)
        .color(0xFF5733);
    for user in users.into_iter() {
        embed = embed.field(user.0, user.1.to_string(), false);
    }

    ctx.send(CreateReply::default().embed(embed)).await?;
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
