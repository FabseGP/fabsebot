use std::{borrow::Cow, sync::Arc};

use anyhow::Result as AResult;
use serde::Deserialize;
use serenity::all::{GenericChannelId, GuildId, MessageId};
use songbird::{
	Call,
	driver::Bitrate,
	input::{AudioStream, AuxMetadata, Input, LiveInput, YoutubeDl},
	tracks::Track,
};
use symphonia::core::io::MediaSource;
use tokio::sync::{Mutex, MutexGuard};
use url::Url;
use uuid::Uuid;
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
	types::{Data, HTTP_CLIENT, UTILS_CONFIG},
};

pub async fn get_configured_songbird_handler(
	handler_lock: &Arc<Mutex<Call>>,
) -> MutexGuard<'_, Call> {
	let mut handler = handler_lock.lock().await;
	handler.set_bitrate(Bitrate::Max);
	handler
}

pub async fn youtube_source(url: String) -> Option<YoutubeDl<'static>> {
	match Url::parse(&url) {
		Ok(parsed_url) => parsed_url
			.domain()
			.filter(|d| {
				*d == "youtube.com"
					|| *d == "www.youtube.com"
					|| *d == "youtu.be"
					|| *d == "m.youtube.com"
			})
			.map(|_| YoutubeDl::new(HTTP_CLIENT.clone(), url)),
		Err(_) => Some(YoutubeDl::new_search(HTTP_CLIENT.clone(), url)),
	}
}

pub struct MusicData {
	pub duration: u64,
}

pub async fn queue_song(
	metadata: AuxMetadata,
	audio: AudioStream<Box<dyn MediaSource>>,
	source: YoutubeDl<'static>,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: GuildId,
	data: Arc<Data>,
	msg_id: MessageId,
	channel_id: GenericChannelId,
	author_name: String,
) -> AResult<()> {
	let uuid = metadata
		.source_url
		.as_ref()
		.map_or_else(Uuid::new_v4, |url| {
			Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())
		});

	let mut track_metadata = data
		.track_metadata
		.get(&uuid)
		.unwrap_or_default()
		.as_ref()
		.clone();

	track_metadata.0 = metadata;
	track_metadata
		.1
		.insert(guild_id, (author_name, msg_id, channel_id));

	data.track_metadata.insert(uuid, Arc::new(track_metadata));

	get_configured_songbird_handler(&handler_lock)
		.await
		.enqueue(Track::new_with_uuid(
			Input::Live(LiveInput::Raw(audio), Some(Box::new(source))),
			uuid,
		))
		.await;

	Ok(())
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
	let key = UTILS_CONFIG
		.get()
		.map(|u| u.api.gif_token.as_str())
		.unwrap();
	if let Ok(response) = HTTP_CLIENT
		.get("https://tenor.googleapis.com/v2/search")
		.query(&[
			("q", input.as_str()),
			("key", key),
			("contentfilter", "medium"),
			("limit", "40"),
			("media_filter", "minimal"),
		])
		.send()
		.await && let Ok(urls) = response.json::<GifResponse>().await
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
	if let Ok(response) = HTTP_CLIENT
		.get("https://lrclib.net/api/get")
		.query(&[("artist_name", artist_name), ("track_name", track_name)])
		.send()
		.await && let Ok(data) = response.json::<LyricsResponse>().await
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
	pub guild: u64,
	pub channel: u64,
	pub message: u64,
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

	let (guild, (channel, message)) = preceded(
		channel_prefix,
		separated_pair(discord_id, '/', separated_pair(discord_id, '/', discord_id)),
	)
	.parse_next(input)?;
	Ok(DiscordMessageLink {
		guild,
		channel,
		message,
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
