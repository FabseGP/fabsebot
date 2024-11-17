use crate::config::{
    constants::{DISCORD_CHANNEL_PREFIX, FALLBACK_GIF, FALLBACK_WAIFU},
    types::{HTTP_CLIENT, UTILS_CONFIG},
};

use serde::Deserialize;
use std::borrow::Cow;
use urlencoding::encode;
use winnow::{
    ascii::digit1,
    combinator::{delimited, preceded, separated_pair},
    token::take_till,
    PResult, Parser,
};

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

pub struct DiscordEmoji {
    pub emoji_name: String,
    pub emoji_id: u64,
}

fn discord_id(input: &mut &str) -> PResult<u64> {
    digit1.parse_to().parse_next(input)
}

fn emoji_name(input: &mut &str) -> PResult<String> {
    take_till(0.., |c| c == ':')
        .map(ToString::to_string)
        .parse_next(input)
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

pub fn discord_emoji(input: &mut &str) -> PResult<DiscordEmoji> {
    let (name, id) =
        delimited("<:", separated_pair(emoji_name, ':', discord_id), ">").parse_next(input)?;

    Ok(DiscordEmoji {
        emoji_name: name,
        emoji_id: id,
    })
}
