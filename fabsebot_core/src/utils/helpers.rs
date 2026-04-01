use std::{borrow::Cow, sync::Arc};

use anyhow::{Result as AResult, bail};
use fastrand::usize;
use metrics::counter;
use mini_moka::sync::Cache;
use poise::{CreateReply, serenity_prelude::Channel};
use serde::{Deserialize, Deserializer, de::Error as _};
use serenity::{
	all::{
		Context, CreateActionRow, CreateAllowedMentions, CreateButton, CreateComponent,
		CreateContainer, CreateContainerComponent, CreateMediaGallery, CreateMediaGalleryItem,
		CreateMessage, CreateSection, CreateSectionAccessory, CreateSectionComponent,
		CreateSeparator, CreateTextDisplay, CreateThumbnail, CreateUnfurledMediaItem, Emoji,
		EmojiId, GenericChannelId, GuildId, Message, MessageFlags, MessageId, Permissions,
		ReactionType,
	},
	small_fixed_array::FixedString,
};
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
		types::{Data, HTTP_CLIENT, SContext, utils_config},
	},
	log_error,
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

pub async fn correct_permissions(
	ctx: &SContext<'_>,
	guild_id: GuildId,
	required_permissions: Permissions,
) -> AResult<()> {
	let Some(Some(channel)) = ctx.channel().await.map(Channel::guild) else {
		ctx.reply("Couldn't fetch channel :/").await?;
		bail!("Failed to fetch channel");
	};

	let bot_member = match guild_id.member(ctx.http(), ctx.framework().bot_id()).await {
		Ok(member) => member,
		Err(err) => {
			ctx.reply("Couldn't fetch bot member :/").await?;
			bail!("Failed to fetch bot member: {err}");
		}
	};

	let bot_permissions = ctx
		.guild()
		.unwrap()
		.user_permissions_in(&channel, &bot_member);

	if !bot_permissions.contains(required_permissions) {
		let missing_permissions = (!bot_permissions) & required_permissions;
		ctx.reply(format!(
			"I'm missing these required permissions in this channel: {missing_permissions}"
		))
		.await?;

		bail!("Bot doesn't have required permissions: {missing_permissions}");
	}

	Ok(())
}

pub fn non_empty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
	D: Deserializer<'de>,
{
	let s = String::deserialize(deserializer)?;
	if s.trim().is_empty() {
		return Err(D::Error::custom("field cannot be empty"));
	}
	Ok(s)
}

pub fn non_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
	D: Deserializer<'de>,
	T: Deserialize<'de>,
{
	let vec = Vec::<T>::deserialize(deserializer)?;
	if vec.is_empty() {
		return Err(D::Error::custom("field cannot be empty"));
	}
	Ok(vec)
}

pub fn true_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
	D: Deserializer<'de>,
{
	let boolean = bool::deserialize(deserializer)?;
	if !boolean {
		return Err(D::Error::custom("field cannot be false"));
	}
	Ok(boolean)
}

pub fn channel_counter(channel_name: String) {
	counter!(
		METRICS.channel_triggers.clone(),
		"channel" => channel_name,
	)
	.increment(1);
}

pub fn thumbnail_section<'a>(text: &'a str, image: &'a str) -> CreateContainerComponent<'a> {
	CreateContainerComponent::Section(CreateSection::new(
		vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
			text,
		))],
		CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
			image,
		))),
	))
}

pub fn visit_page_button(url: &str) -> CreateContainerComponent<'_> {
	CreateContainerComponent::ActionRow(CreateActionRow::Buttons(Cow::Owned(vec![
		CreateButton::new_link(url)
			.label("Visit page")
			.emoji(ReactionType::Unicode(FixedString::from_str_trunc("🌐"))),
	])))
}

pub fn media_gallery(url: &str) -> CreateContainerComponent<'_> {
	CreateContainerComponent::MediaGallery(CreateMediaGallery::new(vec![
		CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url)),
	]))
}

pub fn text_display(text: &str) -> CreateContainerComponent<'_> {
	CreateContainerComponent::TextDisplay(CreateTextDisplay::new(text))
}

pub fn separator<'a>() -> CreateContainerComponent<'a> {
	CreateContainerComponent::Separator(CreateSeparator::new())
}

pub async fn send_container(ctx: &SContext<'_>, container: CreateContainer<'_>) -> AResult<()> {
	ctx.send(
		CreateReply::default()
			.components(vec![CreateComponent::Container(container)])
			.flags(MessageFlags::IS_COMPONENTS_V2)
			.reply(true)
			.allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
	)
	.await?;

	Ok(())
}

pub async fn event_container(
	ctx: &Context,
	message: &Message,
	container: CreateContainer<'_>,
) -> AResult<()> {
	message
		.channel_id
		.send_message(
			&ctx.http,
			CreateMessage::default()
				.components(vec![CreateComponent::Container(container)])
				.flags(MessageFlags::IS_COMPONENTS_V2)
				.reference_message(message)
				.allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
		)
		.await?;

	Ok(())
}

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
	#[serde(deserialize_with = "non_empty_vec")]
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

async fn fetch_gifs_internal(input: &str) -> AResult<Vec<(String, String)>> {
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
			result
				.media_formats
				.gif
				.map(|media| (media.url, result.content_description))
		})
		.collect())
}

pub async fn get_gifs(ctx: &Context, input: &str) -> Vec<(String, String)> {
	match fetch_gifs_internal(input).await {
		Ok(gifs) => gifs,
		Err(error) => {
			log_error(
				"# Failed to fetch gifs",
				error.to_string(),
				ctx,
				METRICS.gifs_errors.clone(),
			)
			.await;
			vec![(FALLBACK_GIF.to_owned(), FALLBACK_GIF_TITLE.to_owned())]
		}
	}
}

pub async fn get_gif(ctx: &Context, input: &str) -> String {
	let gifs = get_gifs(ctx, input).await;
	let index = usize(..gifs.len());
	gifs.into_iter().nth(index).map(|g| g.0).unwrap()
}

#[derive(Deserialize)]
struct LyricsResponse {
	#[serde(
		rename(deserialize = "plainLyrics"),
		deserialize_with = "non_empty_string"
	)]
	plain_lyrics: String,
}

async fn get_lyrics_internal(artist_name: &str, track_name: &str) -> AResult<String> {
	let response = HTTP_CLIENT
		.get("https://lrclib.net/api/get")
		.query(&[("artist_name", artist_name), ("track_name", track_name)])
		.send()
		.await?;

	let json = response.json::<LyricsResponse>().await?;

	Ok(json.plain_lyrics)
}

pub async fn get_lyrics(ctx: &Context, artist_name: &str, track_name: &str) -> Option<String> {
	match get_lyrics_internal(artist_name, track_name).await {
		Ok(lyrics) => Some(lyrics),
		Err(error) => {
			log_error(
				"# Failed to fetch lyrics",
				error.to_string(),
				ctx,
				METRICS.lyrics_errors.clone(),
			)
			.await;
			None
		}
	}
}

#[derive(Deserialize)]
struct WaifuResponse {
	#[serde(deserialize_with = "non_empty_vec")]
	items: Vec<WaifuImage>,
}
#[derive(Deserialize)]
struct WaifuImage {
	url: String,
}

async fn fetch_waifu_internal() -> AResult<String> {
	let response = HTTP_CLIENT
		.get("https://api.waifu.im/images?IsNsfw=False")
		.send()
		.await?;
	let waifu_response = response.json::<WaifuResponse>().await?;

	Ok(waifu_response.items.into_iter().next().unwrap().url)
}

pub async fn get_waifu(ctx: &Context) -> String {
	match fetch_waifu_internal().await {
		Ok(waifu) => waifu,
		Err(error) => {
			log_error(
				"# Failed to fetch waifu",
				error.to_string(),
				ctx,
				METRICS.waifu_errors.clone(),
			)
			.await;
			FALLBACK_WAIFU.to_owned()
		}
	}
}

pub async fn send_emoji(
	ctx: &Context,
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

async fn get_app_emoji(ctx: &Context, target_emoji: u64) -> AResult<(String, EmojiId, bool)> {
	let app_emojis = ctx.get_application_emojis().await?;

	let Some(app_emoji) = app_emojis
		.iter()
		.find(|emoji| emoji.id.get() == target_emoji)
	else {
		bail!("Missing emoji");
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
