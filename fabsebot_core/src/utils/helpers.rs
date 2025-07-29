use std::{borrow::Cow, sync::Arc};

use serde::Deserialize;
use songbird::{Call, driver::Bitrate};
use tokio::sync::{Mutex, MutexGuard};
use urlencoding::encode;
use winnow::{
	ModalResult, Parser as _,
	ascii::digit1,
	combinator::{delimited, preceded, separated_pair},
	error::{ContextError, ErrMode},
	token::take_till,
};

use crate::config::{
	constants::{
		DISCORD_CHANNEL_CANARY_PREFIX, DISCORD_CHANNEL_DEFAULT_PREFIX, DISCORD_CHANNEL_PTB_PREFIX,
		FALLBACK_GIF, FALLBACK_WAIFU,
	},
	types::{HTTP_CLIENT, UTILS_CONFIG},
};

pub async fn get_configured_songbird_handler(
	handler_lock: &Arc<Mutex<Call>>,
) -> MutexGuard<'_, Call> {
	let mut handler = handler_lock.lock().await;
	handler.set_bitrate(Bitrate::Max);
	handler
}

#[derive(Deserialize)]
struct GifResponse {
	results: Vec<GifResult>,
}

#[derive(Deserialize)]
struct GifResult {
	media_formats: MediaFormat,
	content_description: String,
}

#[derive(Deserialize)]
struct MediaFormat {
	gif: Option<GifObject>,
}

#[derive(Deserialize)]
struct GifObject {
	url: String,
}

pub async fn get_gifs(input: String) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
	let request_url = {
		let encoded_input = encode(&input);
		format!(
            "https://tenor.googleapis.com/v2/search?q={encoded_input}&key={}&contentfilter=medium&limit=40&media_filter=minimal",
            UTILS_CONFIG
                .get()
                .map(|u| u.api.tenor_token.as_str())
                .unwrap_or_default(),
        )
	};
	if let Ok(response) = HTTP_CLIENT.get(request_url).send().await
		&& let Ok(urls) = response.json::<GifResponse>().await
	{
		urls.results
			.into_iter()
			.filter_map(|result| {
				result.media_formats.gif.map(|media| {
					(
						Cow::Owned(media.url),
						Cow::Owned(result.content_description),
					)
				})
			})
			.collect()
	} else {
		vec![(Cow::Borrowed(FALLBACK_GIF), Cow::Owned(input))]
	}
}

#[derive(Deserialize)]
struct LyricsResponse {
	#[serde(rename(deserialize = "plainLyrics"))]
	plain_lyrics: String,
}

pub async fn get_lyrics(artist_name: &str, track_name: &str) -> Option<String> {
	let request_url = {
		let encoded_artist = encode(artist_name);
		let encoded_track = encode(track_name);
		format!(
			"https://lrclib.net/api/get?artist_name={encoded_artist}&track_name={encoded_track}"
		)
	};

	if let Ok(response) = HTTP_CLIENT.get(request_url).send().await
		&& let Ok(data) = response.json::<LyricsResponse>().await
	{
		Some(data.plain_lyrics)
	} else {
		None
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
	if let Ok(response) = HTTP_CLIENT
		.get("https://api.waifu.im/search?height=>=2000&is_nsfw=false")
		.send()
		.await && let Ok(waifu_response) = response.json::<WaifuResponse>().await
		&& let Some(image) = waifu_response.images.into_iter().next()
	{
		return Cow::Owned(image.url);
	}

	Cow::Borrowed(FALLBACK_WAIFU)
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

fn discord_id(input: &mut &str) -> ModalResult<u64> {
	digit1.parse_to().parse_next(input)
}

fn emoji_name(input: &mut &str) -> ModalResult<String> {
	take_till(0.., |c| c == ':')
		.map(ToString::to_string)
		.parse_next(input)
}

pub fn discord_message_link(input: &mut &str) -> ModalResult<DiscordMessageLink> {
	let channel_prefix = if let Some(index) = input.find(DISCORD_CHANNEL_DEFAULT_PREFIX) {
		*input = &input[index..];
		DISCORD_CHANNEL_DEFAULT_PREFIX
	} else if let Some(index) = input.find(DISCORD_CHANNEL_CANARY_PREFIX) {
		*input = &input[index..];
		DISCORD_CHANNEL_DEFAULT_PREFIX
	} else if let Some(index) = input.find(DISCORD_CHANNEL_PTB_PREFIX) {
		*input = &input[index..];
		DISCORD_CHANNEL_PTB_PREFIX
	} else {
		return Err(ErrMode::Cut(ContextError::new()));
	};

	let (guild_id, (channel_id, message_id)) = preceded(
		channel_prefix,
		separated_pair(discord_id, '/', separated_pair(discord_id, '/', discord_id)),
	)
	.parse_next(input)?;
	Ok(DiscordMessageLink {
		guild_id,
		channel_id,
		message_id,
	})
}

pub fn discord_emoji(input: &mut &str) -> ModalResult<DiscordEmoji> {
	let (name, id) =
		delimited("<:", separated_pair(emoji_name, ':', discord_id), ">").parse_next(input)?;

	Ok(DiscordEmoji {
		emoji_name: name,
		emoji_id: id,
	})
}
