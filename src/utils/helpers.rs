use crate::config::types::{Error, HTTP_CLIENT, UTILS_CONFIG};

use anyhow::anyhow;
use poise::serenity_prelude::{self as serenity, GuildId};
use serde::Deserialize;
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
        .find_map(|e| {
            if e.name == emoji_name {
                Some(e.to_string())
            } else {
                None
            }
        })
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

pub async fn get_gifs(input: &str) -> Vec<String> {
    let request_url = {
        let encoded_input = encode(input);
        format!(
            "https://tenor.googleapis.com/v2/search?q={encoded_input}&key={}&contentfilter=medium&limit=40",
            UTILS_CONFIG.get().expect("UTILS_CONFIG must be set during initialization").api.tenor_token,
        )
    };
    let Ok(response) = HTTP_CLIENT.get(request_url).send().await else {
        return vec!["https://i.postimg.cc/zffntsGs/tenor.gif".to_owned()];
    };
    response.json::<GifResponse>().await.ok().map_or_else(
        || vec!["https://i.postimg.cc/zffntsGs/tenor.gif".to_owned()],
        |urls| {
            urls.results
                .into_iter()
                .filter_map(|result| result.media_formats.gif.map(|media| media.url))
                .collect()
        },
    )
}

#[derive(Deserialize)]
struct WaifuResponse {
    images: Vec<WaifuData>,
}
#[derive(Deserialize)]
struct WaifuData {
    url: String,
}

pub async fn get_waifu() -> String {
    let Ok(response) = HTTP_CLIENT
        .get("https://api.waifu.im/search?height=>=2000&is_nsfw=false")
        .send()
        .await
    else {
        return "https://c.tenor.com/CosM_E8-RQUAAAAC/tenor.gif".to_owned();
    };
    response
        .json::<WaifuResponse>()
        .await
        .ok()
        .and_then(|urls| urls.images.into_iter().next().map(|img| img.url))
        .unwrap_or_else(|| "https://c.tenor.com/CosM_E8-RQUAAAAC/tenor.gif".to_owned())
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
    let prefix_parser = "https://discord.com/channels/";
    let (guild_id, (channel_id, message_id)) = preceded(
        prefix_parser,
        separated_pair(discord_id, '/', separated_pair(discord_id, '/', discord_id)),
    )
    .parse_next(input)?;
    Ok(DiscordMessageLink {
        guild_id,
        channel_id,
        message_id,
    })
}
