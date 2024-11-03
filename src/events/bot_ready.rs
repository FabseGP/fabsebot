use crate::types::{Data, Error};

use poise::{
    builtins,
    serenity_prelude::{Context as SContext, Ready},
    FrameworkContext,
};
use tracing::info;

pub async fn handle_ready(
    ctx: &SContext,
    data_about_bot: &Ready,
    framework_context: FrameworkContext<'_, Data, Error>,
) -> Result<(), Error> {
    let user_count = match ctx.http.get_current_application_info().await {
        Ok(info) => info.approximate_user_install_count.unwrap_or(0),
        Err(_) => 0,
    };
    info!(
        "Logged in as {} in {} server(s) and installed for {user_count} user(s)",
        data_about_bot.user.name,
        data_about_bot.guilds.len(),
    );
    builtins::register_globally(
        &framework_context.serenity_context.http,
        &framework_context.options().commands,
    )
    .await?;
    Ok(())
}
