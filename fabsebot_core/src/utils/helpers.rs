use std::{borrow::Cow, sync::Arc};

use metrics::counter;
use mini_moka::sync::Cache;
use serde::Deserialize;
use serenity::all::{Context as SContext, Emoji, EmojiId, GenericChannelId, GuildId, MessageId};
use songbird::{
	Call,
	driver::Bitrate,
	input::{AudioStream, AuxMetadata, Input, LiveInput, YoutubeDl},
	tracks::Track,
};
use symphonia::core::io::MediaSource;
use tokio::sync::{Mutex, MutexGuard};
use tracing::error;
use url::Url;
use uuid::Uuid;
use winnow::{
	ModalResult, Parser as _,
	ascii::digit1,
	combinator::{delimited, preceded, separated_pair},
	error::{ContextError, ErrMode},
	token::take_till,
};

use crate::{
	config::{
		constants::{FALLBACK_GIF, FALLBACK_GIF_TITLE, FALLBACK_WAIFU},
		types::{Data, HTTP_CLIENT, utils_config},
	},
	errors::commands::{EmojiError, HTTPError},
	stats::counters::METRICS,
};

#[macro_export]
macro_rules! log_errors {
    ($($result:expr),+ $(,)?) => {
        $(
            if let Err(err) = $result {
                error!("{err}");
            }
        )+
    };
}

const DISCORD_CHANNEL_DEFAULT_PREFIX: &str = "https://discord.com/channels/";
const DISCORD_CHANNEL_PTB_PREFIX: &str = "https://ptb.discord.com/channels/";
const DISCORD_CHANNEL_CANARY_PREFIX: &str = "https://canary.discord.com/channels/";

pub fn channel_counter(channel_name: String) {
	counter!(
		METRICS.channel_triggers.clone(),
		"channel" => channel_name,
	)
	.increment(1);
}

pub async fn get_configured_songbird_handler(
	handler_lock: &Arc<Mutex<Call>>,
) -> MutexGuard<'_, Call> {
	let mut handler = handler_lock.lock().await;
	handler.set_bitrate(Bitrate::Max);
	handler
}

/*
struct YoutubeUrl(String);

impl YoutubeUrl {
	fn new(url: String) -> Option<Self> {
		Url::parse(&url).map_or(None, |parsed_url| {
			parsed_url
				.domain()
				.filter(|d| {
					*d == "youtube.com"
						|| *d == "www.youtube.com"
						|| *d == "youtu.be"
						|| *d == "m.youtube.com"
				})
				.map(|_| Self(url))
		})
	}
}
*/

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

pub async fn queue_song(
	metadata: AuxMetadata,
	audio: AudioStream<Box<dyn MediaSource>>,
	source: YoutubeDl<'static>,
	handler_lock: Arc<Mutex<Call>>,
	guild_id: GuildId,
	data: Arc<Data>,
	msg_id: MessageId,
	channel_id: GenericChannelId,
	author_name: &str,
) {
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
	track_metadata.1.insert(
		guild_id,
		(format!("Added by {author_name}"), msg_id, channel_id),
	);

	data.track_metadata.insert(uuid, Arc::new(track_metadata));

	get_configured_songbird_handler(&handler_lock)
		.await
		.enqueue(Track::new_with_uuid(
			Input::Live(LiveInput::Raw(audio), Some(Box::new(source))),
			uuid,
		))
		.await;
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

async fn fetch_gifs_internal(
	input: &str,
) -> Result<Vec<(Cow<'static, str>, Cow<'static, str>)>, HTTPError> {
	let response = HTTP_CLIENT
		.get("https://tenor.googleapis.com/v2/search")
		.query(&[
			("q", input),
			("key", utils_config().api.gif_token.as_str()),
			("contentfilter", "medium"),
			("limit", "40"),
			("media_filter", "minimal"),
		])
		.send()
		.await?;

	let urls = response.json::<GifResponse>().await?;

	Ok(urls
		.results
		.into_iter()
		.filter_map(|result| {
			result.media_formats.gif.map(|media| {
				(
					Cow::Owned(media.url),
					Cow::Owned(result.content_description),
				)
			})
		})
		.collect())
}

pub async fn get_gifs(input: &str) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
	match fetch_gifs_internal(input).await {
		Ok(gifs) => gifs,
		Err(err) => {
			error!("{}", err);
			vec![(
				Cow::Borrowed(FALLBACK_GIF),
				Cow::Borrowed(FALLBACK_GIF_TITLE),
			)]
		}
	}
}

#[derive(Deserialize)]
struct LyricsResponse {
	#[serde(rename(deserialize = "plainLyrics"))]
	plain_lyrics: String,
}

pub async fn get_lyrics(artist_name: &str, track_name: &str) -> Result<String, HTTPError> {
	let response = HTTP_CLIENT
		.get("https://lrclib.net/api/get")
		.query(&[("artist_name", artist_name), ("track_name", track_name)])
		.send()
		.await?;

	let json = response.json::<LyricsResponse>().await?;

	Ok(json.plain_lyrics)
}

#[derive(Deserialize)]
struct WaifuResponse {
	images: [WaifuImage; 1],
}
#[derive(Deserialize)]
struct WaifuImage {
	url: String,
}

async fn fetch_waifu_internal() -> Result<Option<String>, HTTPError> {
	let response = HTTP_CLIENT
		.get("https://api.waifu.im/search?height=>=2000&is_nsfw=false")
		.send()
		.await?;
	let waifu_response = response.json::<WaifuResponse>().await?;

	Ok(waifu_response.images.into_iter().next().map(|w| w.url))
}

pub async fn get_waifu() -> Cow<'static, str> {
	match fetch_waifu_internal().await {
		Ok(waifu_opt) => waifu_opt.map_or(Cow::Borrowed(FALLBACK_WAIFU), Cow::Owned),
		Err(err) => {
			error!("{}", err);
			Cow::Borrowed(FALLBACK_WAIFU)
		}
	}
}

pub async fn send_emoji(
	ctx: &SContext,
	content: &str,
	emojis: &Cache<u64, Arc<Emoji>>,
	target_emoji: u64,
) -> Option<String> {
	let (emoji_name, emoji_id, is_animated) = if let Some(app_emoji) = emojis.get(&target_emoji) {
		(
			app_emoji.name.to_string(),
			app_emoji.id,
			app_emoji.animated(),
		)
	} else {
		match get_app_emoji(ctx, target_emoji).await {
			Ok(emoji) => emoji,
			Err(err) => {
				error!("{}", err);
				return None;
			}
		}
	};
	let emoji_string = format!(
		"<{}:{}:{}>",
		if is_animated { "a" } else { "" },
		emoji_name,
		emoji_id
	);
	let count = content.matches(&emoji_name).count();
	Some(emoji_string.repeat(count))
}

async fn get_app_emoji(
	ctx: &SContext,
	target_emoji: u64,
) -> Result<(String, EmojiId, bool), EmojiError> {
	let app_emojis = ctx.get_application_emojis().await?;

	let Some(app_emoji) = app_emojis
		.iter()
		.find(|emoji| emoji.id.get() == target_emoji)
	else {
		return Err(EmojiError::MissingEmoji);
	};

	Ok((
		app_emoji.name.to_string(),
		app_emoji.id,
		app_emoji.animated(),
	))
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
		.map(ToOwned::to_owned)
		.parse_next(input)
}

pub fn discord_message_link(input: &mut &str) -> ModalResult<DiscordMessageLink> {
	let channel_prefix = if let Some(index) = input.find(DISCORD_CHANNEL_DEFAULT_PREFIX) {
		*input = &input[index..];
		DISCORD_CHANNEL_DEFAULT_PREFIX
	} else if let Some(index) = input.find(DISCORD_CHANNEL_CANARY_PREFIX) {
		*input = &input[index..];
		DISCORD_CHANNEL_CANARY_PREFIX
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
