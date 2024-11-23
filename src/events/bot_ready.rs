use crate::config::{
    settings::{EmojiReactions, GuildSettings, UserSettings, WordReactions, WordTracking},
    types::{Data, Error, GuildData},
};

use anyhow::Context;
use dashmap::DashMap;
use poise::{
    builtins::register_globally,
    serenity_prelude::{Context as SContext, GuildId, Ready, UserId},
    FrameworkContext,
};
use sqlx::query_as;
use tokio::join;
use tracing::info;

pub async fn handle_ready(
    ctx: &SContext,
    data_about_bot: &Ready,
    framework_context: FrameworkContext<'_, Data, Error>,
) -> Result<(), Error> {
    let mut tx = framework_context
        .user_data()
        .db
        .begin()
        .await
        .context("Failed to acquire savepoint")?;
    let guild_settings = query_as!(GuildSettings, "SELECT * FROM guild_settings")
        .fetch_all(&mut *tx)
        .await?;
    let user_settings = query_as!(UserSettings, "SELECT * FROM user_settings")
        .fetch_all(&mut *tx)
        .await?;
    let word_reactions = query_as!(WordReactions, "SELECT * FROM guild_word_reaction")
        .fetch_all(&mut *tx)
        .await?;
    let word_tracking = query_as!(WordTracking, "SELECT * FROM guild_word_tracking")
        .fetch_all(&mut *tx)
        .await?;
    let emoji_reactions = query_as!(EmojiReactions, "SELECT * FROM guild_emoji_reaction")
        .fetch_all(&mut *tx)
        .await?;
    tx.commit()
        .await
        .context("Failed to commit sql-transaction")?;

    let grouped_word_reactions: DashMap<i64, Vec<WordReactions>> = DashMap::default();
    let grouped_word_tracking: DashMap<i64, Vec<WordTracking>> = DashMap::default();
    let grouped_emoji_reactions: DashMap<i64, Vec<EmojiReactions>> = DashMap::default();

    for reaction in word_reactions {
        grouped_word_reactions
            .entry(reaction.guild_id)
            .or_default()
            .push(reaction);
    }

    for tracking in word_tracking {
        grouped_word_tracking
            .entry(tracking.guild_id)
            .or_default()
            .push(tracking);
    }

    for emoji in emoji_reactions {
        grouped_emoji_reactions
            .entry(emoji.guild_id)
            .or_default()
            .push(emoji);
    }

    join!(
        async {
            for settings in guild_settings {
                let guild_id = GuildId::new(
                    u64::try_from(settings.guild_id).expect("Guild-id out of bounds for u64"),
                );
                let settings_guild_id = settings.guild_id;
                let guild_data = GuildData {
                    settings,
                    word_reactions: grouped_word_reactions
                        .remove(&settings_guild_id)
                        .unwrap_or_default()
                        .1,
                    word_tracking: grouped_word_tracking
                        .remove(&settings_guild_id)
                        .unwrap_or_default()
                        .1,
                    emoji_reactions: grouped_emoji_reactions
                        .remove(&settings_guild_id)
                        .unwrap_or_default()
                        .1,
                };
                framework_context
                    .user_data()
                    .guild_data
                    .insert(guild_id, guild_data);
            }
        },
        async {
            for settings in user_settings {
                let guild_id = GuildId::new(
                    u64::try_from(settings.guild_id).expect("Guild-id out of bounds for u64"),
                );
                let user_id = UserId::new(
                    u64::try_from(settings.user_id).expect("User-id out of bounds for u64"),
                );
                framework_context
                    .user_data()
                    .user_settings
                    .entry(guild_id)
                    .or_default()
                    .insert(user_id, settings);
            }
        }
    );

    let user_count = match ctx.http.get_current_application_info().await {
        Ok(info) => info.approximate_user_install_count.unwrap_or(0),
        Err(_) => 0,
    };
    info!(
        "Logged in as {} in {} server(s) and installed for {user_count} user(s)",
        data_about_bot.user.name,
        data_about_bot.guilds.len(),
    );

    register_globally(
        &framework_context.serenity_context.http,
        &framework_context.options().commands,
    )
    .await?;

    Ok(())
}
