use crate::{
    config::types::{Data, Error},
    events::{
        bot_ready::handle_ready, guild_create::handle_guild_create,
        http_ratelimit::handle_ratelimit, message_sent::handle_message,
    },
};
use anyhow::Result;
use poise::{serenity_prelude::FullEvent, FrameworkContext, FrameworkError, PartialContext};
use std::borrow::Cow;
use tracing::error;

pub async fn on_error(error: FrameworkError<'_, Data, Error>) -> Result<()> {
    match error {
        FrameworkError::Command { error, ctx, .. } => {
            error!("Error in command `{}`: {:?}", ctx.command().name, error);
        }
        FrameworkError::DynamicPrefix { error, .. } => {
            error!("Error in dynamic_prefix: {:?}", error);
        }
        _ => {}
    }
    Ok(())
}

pub async fn dynamic_prefix(
    ctx: PartialContext<'_, Data, Error>,
) -> anyhow::Result<Option<Cow<'static, str>>> {
    let prefix = ctx.guild_id.map_or(Cow::Borrowed("!"), |id| {
        if let Some(guild_data) = ctx.framework.user_data().guild_data.get(&id) {
            guild_data
                .settings
                .prefix
                .clone()
                .map_or(Cow::Borrowed("!"), Cow::Owned)
        } else {
            Cow::Borrowed("!")
        }
    });

    Ok(Some(prefix))
}

pub async fn event_handler(
    framework: FrameworkContext<'_, Data, Error>,
    event: &FullEvent,
) -> Result<(), Error> {
    let data = framework.user_data();
    let ctx = framework.serenity_context;

    match event {
        FullEvent::Ready { data_about_bot } => handle_ready(ctx, data_about_bot, framework).await?,
        FullEvent::Message { new_message } => handle_message(ctx, data, new_message).await?,
        FullEvent::GuildCreate { guild, is_new } => {
            handle_guild_create(data, guild, is_new.as_ref()).await?;
        }
        FullEvent::Ratelimit { data } => handle_ratelimit(data).await?,
        _ => {}
    }

    Ok(())
}
