use crate::types::{Data, Error};

use poise::{
    builtins,
    serenity_prelude::{self as serenity, ActivityData, OnlineStatus, Ready},
    FrameworkContext,
};
use tracing::info;

pub async fn handle_ready(
    ctx: &serenity::Context,
    data_about_bot: &Ready,
    framework_context: FrameworkContext<'_, Data, Error>,
) -> Result<(), Error> {
    info!(
        "Logged in as {} in {} servers",
        data_about_bot.user.name,
        data_about_bot.guilds.len(),
    );
    let activity = ActivityData::listening("You Could Be Mine");
    ctx.set_presence(Some(activity), OnlineStatus::Online);
    builtins::register_globally(
        &framework_context.serenity_context.http,
        &framework_context.options().commands,
    )
    .await?;
    Ok(())
}
