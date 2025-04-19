use crate::{
    config::types::{Data, Error},
    events::{
        bot_ready::handle_ready, guild_create::handle_guild_create,
        message_delete::handle_message_delete, message_sent::handle_message,
    },
};
use anyhow::Result as AResult;
use poise::{FrameworkError, PartialContext, serenity_prelude::FullEvent};
use serenity::prelude::{Context, EventHandler as SEventHandler};
use std::borrow::Cow;
use tracing::{error, warn};

pub async fn on_error(error: FrameworkError<'_, Data, Error>) -> AResult<()> {
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
) -> AResult<Option<Cow<'static, str>>> {
    let prefix = if let Some(id) = ctx.guild_id {
        ctx.framework
            .user_data()
            .guild_data
            .lock()
            .await
            .get(&id)
            .map_or(Cow::Borrowed("!"), |guild_data| {
                guild_data
                    .settings
                    .prefix
                    .clone()
                    .map_or(Cow::Borrowed("!"), Cow::Owned)
            })
    } else {
        Cow::Borrowed("!")
    };

    Ok(Some(prefix))
}

pub struct EventHandler;

#[serenity::async_trait]
impl SEventHandler for EventHandler {
    async fn dispatch(&self, ctx: &Context, event: &FullEvent) {
        match event {
            FullEvent::Ready { data_about_bot, .. } => {
                if let Err(error) = handle_ready(ctx, data_about_bot).await {
                    warn!("Error handling connection to Discord: {error}");
                }
            }
            FullEvent::Message { new_message, .. } => {
                if let Err(error) = handle_message(ctx, new_message).await {
                    warn!("Error handling sent message: {error}");
                }
            }
            FullEvent::GuildCreate { guild, is_new, .. } => {
                if let Err(error) = handle_guild_create(ctx.data(), guild, is_new.as_ref()).await {
                    warn!("Error handling newly created guild: {error}");
                }
            }
            FullEvent::MessageDelete {
                channel_id,
                deleted_message_id,
                guild_id,
                ..
            } => {
                if let Err(error) =
                    handle_message_delete(ctx, *channel_id, *guild_id, *deleted_message_id).await
                {
                    warn!("Error handling deleted message: {error}");
                }
            }
            _ => {}
        }
    }
}
