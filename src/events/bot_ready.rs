use crate::types::Error;

use anyhow::Context;
use poise::serenity_prelude::{
    self as serenity, ActivityData, CreateAttachment, EditProfile, OnlineStatus, Ready,
};

pub async fn handle_ready(ctx: &serenity::Context, data_about_bot: &Ready) -> Result<(), Error> {
    tracing::info!("Logged in as {}", data_about_bot.user.name);
    let activity = ActivityData::listening("You Could Be Mine");
    let avatar = CreateAttachment::url(
        &ctx.http,
        "https://media1.tenor.com/m/029KypcoTxQAAAAC/sleep-pokemon.gif",
        "psyduck_avatar.gif",
    )
    .await?;
    let banner = CreateAttachment::url(
            &ctx.http,
            "https://i.postimg.cc/RFWkBJfs/2024-08-2012-50-17online-video-cutter-com-ezgif-com-optimize.gif",
            "fabsebot_banner.gif"
        )
        .await?;
    ctx.set_presence(Some(activity), OnlineStatus::Online);
    ctx.http
        .edit_profile(
            &EditProfile::new()
                .avatar(&avatar)
                .banner(&banner)
                .username("fabsebot"),
        )
        .await
        .context("Failed to edit bot profile")?;
    Ok(())
}
