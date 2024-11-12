use crate::config::{
    constants::{DISCORD_CHANNEL_PREFIX, FALLBACK_GIF, FALLBACK_WAIFU},
    types::{Error, HTTP_CLIENT, UTILS_CONFIG},
};

use anyhow::anyhow;
use poise::serenity_prelude::{self as serenity, GuildId};
use serde::Deserialize;
use std::{borrow::Cow, string::ToString};
use urlencoding::encode;
use winnow::{
    ascii::digit1,
    combinator::{preceded, separated_pair},
    PResult, Parser,
};

pub async fn emoji_id(
    ctx: &serenity::Context,
    guild_id: GuildId,
    emoji_name: &str,
) -> Result<String, Error> {
    let guild_emojis = guild_id.emojis(&ctx.http).await?;
    guild_emojis
        .iter()
        .find(|e| e.name.as_str() == emoji_name)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("Emoji not found"))
}

#[derive(Deserialize)]
struct GifResponse {
    results: Vec<GifResult>,
}

#[derive(Deserialize)]
struct GifResult {
    media_formats: MediaFormat,
}

#[derive(Deserialize)]
struct MediaFormat {
    gif: Option<GifObject>,
}

#[derive(Deserialize)]
struct GifObject {
    url: String,
}

pub async fn get_gifs(input: &str) -> Vec<Cow<'static, str>> {
    let request_url = {
        let encoded_input = encode(input);
        format!(
            "https://tenor.googleapis.com/v2/search?q={encoded_input}&key={}&contentfilter=medium&limit=40",
            UTILS_CONFIG.get().expect("UTILS_CONFIG must be set during initialization").api.tenor_token,
        )
    };
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(response) => match response.json::<GifResponse>().await {
            Ok(urls) => urls
                .results
                .into_iter()
                .filter_map(|result| result.media_formats.gif.map(|media| Cow::Owned(media.url)))
                .collect(),
            Err(_) => vec![Cow::Borrowed(FALLBACK_GIF)],
        },
        Err(_) => vec![Cow::Borrowed(FALLBACK_GIF)],
    }
}

#[derive(Deserialize)]
struct WaifuResponse {
    images: [WaifuImage; 1],
}
#[derive(Deserialize)]
struct WaifuImage {
    url: String,
}

pub async fn get_waifu() -> Cow<'static, str> {
    match HTTP_CLIENT
        .get("https://api.waifu.im/search?height=>=2000&is_nsfw=false")
        .send()
        .await
    {
        Ok(response) => response
            .json::<WaifuResponse>()
            .await
            .map(|resp| Cow::Owned(resp.images[0].url.clone()))
            .unwrap_or(Cow::Borrowed(FALLBACK_WAIFU)),
        Err(_) => Cow::Borrowed(FALLBACK_WAIFU),
    }
}

pub struct DiscordMessageLink {
    pub guild_id: u64,
    pub channel_id: u64,
    pub message_id: u64,
}

fn discord_id(input: &mut &str) -> PResult<u64> {
    digit1.parse_to().parse_next(input)
}

pub fn discord_message_link(input: &mut &str) -> PResult<DiscordMessageLink> {
    let (guild_id, (channel_id, message_id)) = preceded(
        DISCORD_CHANNEL_PREFIX,
        separated_pair(discord_id, '/', separated_pair(discord_id, '/', discord_id)),
    )
    .parse_next(input)?;
    Ok(DiscordMessageLink {
        guild_id,
        channel_id,
        message_id,
    })
}
