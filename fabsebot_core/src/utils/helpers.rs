use std::{
	borrow::Cow,
	io::Cursor,
	sync::{Arc, RwLock, atomic::AtomicBool},
	time::Duration,
};

use anyhow::{Error as AError, Result as AResult, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bytes::Bytes;
use fabsebot_db::{
	guild::{fetch_guild_prefix, insert_guild_settings},
	user::insert_user_settings,
};
use image::{ImageFormat, guess_format, load_from_memory};
use metrics::counter;
use poise::{CreateReply, serenity_prelude::Channel};
use reqwest::{Response, Result as RResult};
use serde::{
	Deserialize, Deserializer,
	de::{DeserializeOwned, Error as _},
};
use serenity::{
	all::{
		Context, CreateActionRow, CreateAllowedMentions, CreateButton, CreateComponent,
		CreateContainer, CreateContainerComponent, CreateMediaGalleryItem, CreateMessage,
		CreateSectionAccessory, CreateSectionComponent, CreateSeparator, CreateTextDisplay,
		CreateThumbnail, CreateUnfurledMediaItem, GuildId, Member, MessageFlags, Permissions,
		ReactionType, User, UserId,
	},
	builder::{CreateInteractionResponse, EditMessage},
	collector::ComponentInteractionCollector,
	futures::{StreamExt as _, channel::mpsc::TrySendError},
	gateway::ShardRunnerMessage,
	model::{
		application::ButtonStyle,
		guild::Emoji,
		id::{EmojiId, ShardId},
	},
	small_fixed_array::FixedString,
};
use tokio::{
	spawn,
	sync::{mpsc, watch},
};
use tracing::warn;
use winnow::{
	ModalResult, Parser as _,
	ascii::digit1,
	combinator::{preceded, separated_pair},
	error::{ContextError, ErrMode},
};

use crate::{
	config::{
		constants::DEFAULT_PREFIX,
		types::{
			Data, EmojisMap, Error, GuildCache, HTTP_CLIENT, MusicData, SContext, client_data,
			utils_config,
		},
	},
	errors::commands::HTTPError,
	log_error,
	stats::counters::METRICS,
	utils::{
		ai::{ContentPart, ai_task, uri_content},
		voice::{ConnectionStatus, TrackSignal, music_task},
	},
};

const DISCORD_CHANNEL_DEFAULT_PREFIX: &str = "https://discord.com/channels/";
const DISCORD_CHANNEL_PTB_PREFIX: &str = "https://ptb.discord.com/channels/";
const DISCORD_CHANNEL_CANARY_PREFIX: &str = "https://canary.discord.com/channels/";

pub async fn correct_permissions(
	ctx: &SContext<'_>,
	guild_id: GuildId,
	required_permissions: Permissions,
) -> AResult<()> {
	let Some(Some(channel)) = ctx.channel().await.map(Channel::guild) else {
		let msg = "Couldn't fetch channel :/";
		ctx.reply(msg).await?;
		bail!(msg);
	};

	let bot_member = match guild_id.member(ctx.http(), ctx.framework().bot_id()).await {
		Ok(member) => member,
		Err(err) => {
			let msg = "Couldn't fetch bot member :/";
			ctx.reply(msg).await?;
			bail!("{msg}: {err}");
		}
	};

	let bot_permissions = ctx
		.guild()
		.unwrap()
		.user_permissions_in(&channel, &bot_member);

	if !bot_permissions.contains(required_permissions) {
		let missing_permissions = (!bot_permissions) & required_permissions;
		let msg = format!("I'm missing these required permissions: **{missing_permissions}**");
		ctx.reply(&msg).await?;
		bail!("{msg}");
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

pub fn channel_counter(channel_name: &'static str) {
	counter!(
		METRICS.channel_triggers.as_str(),
		"channel" => channel_name,
	)
	.increment(1);
}

pub fn thumbnail_section<'a>(
	text: impl Into<Cow<'a, str>>,
	image: impl Into<Cow<'a, str>>,
) -> (CreateSectionComponent<'a>, CreateSectionAccessory<'a>) {
	(
		CreateSectionComponent::TextDisplay(CreateTextDisplay::new(text)),
		CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
			image,
		))),
	)
}

pub fn visit_page_button<'a>(url: impl Into<Cow<'a, str>>) -> CreateButton<'a> {
	CreateButton::new_link(url)
		.label("Visit page")
		.emoji(ReactionType::Unicode(FixedString::from_str_trunc("🌐")))
}

pub fn media_gallery<'a>(url: impl Into<Cow<'a, str>>) -> CreateMediaGalleryItem<'a> {
	CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url))
}

pub fn text_display<'a>(text: impl Into<Cow<'a, str>>) -> CreateContainerComponent<'a> {
	CreateContainerComponent::TextDisplay(CreateTextDisplay::new(text))
}

pub fn separator<'a>() -> CreateContainerComponent<'a> {
	CreateContainerComponent::Separator(CreateSeparator::new())
}

pub fn default_mentions() -> CreateAllowedMentions<'static> {
	CreateAllowedMentions::new().replied_user(false)
}

pub fn message_container<'a>(component: &'a [CreateComponent<'a>]) -> CreateMessage<'a> {
	CreateMessage::new()
		.components(component)
		.flags(MessageFlags::IS_COMPONENTS_V2)
		.allowed_mentions(default_mentions())
}

pub fn silent_message(content: &str) -> CreateMessage<'_> {
	CreateMessage::new()
		.content(content)
		.allowed_mentions(CreateAllowedMentions::new().replied_user(false))
}

#[must_use]
pub fn reply_container<'a>(
	component: impl Into<Cow<'a, [CreateComponent<'a>]>>,
) -> CreateReply<'a> {
	CreateReply::new()
		.components(component)
		.flags(MessageFlags::IS_COMPONENTS_V2)
		.reply(true)
		.allowed_mentions(default_mentions())
}

pub fn edit_message_container<'a>(
	component: impl Into<Cow<'a, [CreateComponent<'a>]>>,
) -> EditMessage<'a> {
	EditMessage::new()
		.components(component)
		.flags(MessageFlags::IS_COMPONENTS_V2)
		.allowed_mentions(default_mentions())
		.content("")
}

#[derive(Deserialize)]
struct GifResponse {
	data: GifData,
}

#[derive(Deserialize)]
struct GifData {
	#[serde(deserialize_with = "non_empty_vec")]
	data: Vec<GifResult>,
}

#[derive(Deserialize)]
struct GifResult {
	title: String,
	file: MediaQuality,
}

#[derive(Deserialize)]
struct MediaQuality {
	hd: MediaFormat,
}

#[derive(Deserialize)]
struct MediaFormat {
	webp: MediaUrl,
}

#[derive(Deserialize)]
struct MediaUrl {
	url: String,
}

const FALLBACK_GIF: &str = "https://i.postimg.cc/zffntsGs/tenor.gif";

async fn fetch_gifs_internal(input: &str, page_size: &str) -> AResult<GifResponse> {
	fetch_and_parse(
		HTTP_CLIENT
			.get(utils_config().api.gif_url.as_str())
			.query(&[
				("per_page", page_size),
				("q", input),
				("content_filter", "medium"),
				("format_filter", "webp"),
			])
			.send(),
	)
	.await
}

async fn gif_error(error: AError) {
	let output = format!("# Failed to fetch gifs\n{error}");
	counter!(METRICS.gifs_errors.as_str()).increment(1);
	log_error(output).await;
}

pub async fn get_gifs(input: &str) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
	match fetch_gifs_internal(input, "40").await {
		Ok(gifs) => gifs
			.data
			.data
			.into_iter()
			.map(|result| {
				(
					Cow::Owned(result.file.hd.webp.url),
					Cow::Owned(result.title),
				)
			})
			.collect(),
		Err(error) => {
			gif_error(error).await;
			vec![(
				Cow::Borrowed(FALLBACK_GIF),
				Cow::Borrowed("Sucks to be you"),
			)]
		}
	}
}

pub async fn get_gif(input: &str) -> Cow<'static, str> {
	match fetch_gifs_internal(input, "1").await {
		Ok(gifs) => gifs
			.data
			.data
			.into_iter()
			.next()
			.map(|result| Cow::Owned(result.file.hd.webp.url))
			.unwrap(),
		Err(error) => {
			gif_error(error).await;
			Cow::Borrowed(FALLBACK_GIF)
		}
	}
}

#[derive(Deserialize)]
struct LyricsResponse(pub LyricsEntry);

#[derive(Deserialize)]
struct LyricsEntry {
	#[serde(
		rename(deserialize = "plainLyrics"),
		deserialize_with = "non_empty_string"
	)]
	plain_lyrics: String,
}

pub async fn get_lyrics(track_name: &str, artist_name: &str) -> Cow<'static, str> {
	match fetch_and_parse::<LyricsResponse>(
		HTTP_CLIENT
			.get("https://lrclib.net/api/get")
			.query(&[("track_name", track_name), ("artist_name", artist_name)])
			.send(),
	)
	.await
	{
		Ok(payload) => Cow::Owned(payload.0.plain_lyrics),
		Err(error) => {
			let output = format!("# Failed to fetch lyrics\n{error}");
			counter!(METRICS.lyrics_errors.as_str()).increment(1);
			log_error(output).await;
			Cow::Borrowed("Not fount :(")
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

pub async fn get_waifu() -> Cow<'static, str> {
	match fetch_and_parse::<WaifuResponse>(
		HTTP_CLIENT
			.get("https://api.waifu.im/images?IsNsfw=False")
			.send(),
	)
	.await
	{
		Ok(payload) => Cow::Owned(payload.items.into_iter().next().unwrap().url),
		Err(error) => {
			let output = format!("# Failed to fetch waifu\n{error}");
			counter!(METRICS.waifu_errors.as_str()).increment(1);
			log_error(output).await;
			Cow::Borrowed("https://c.tenor.com/CosM_E8-RQUAAAAC/tenor.gif")
		}
	}
}

pub struct DiscordMessageLink {
	pub guild: u64,
	pub channel: u64,
	pub message: u64,
}

fn discord_id(input: &mut &str) -> ModalResult<u64> {
	digit1.parse_to().parse_next(input)
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

#[must_use]
pub fn member_pfp(member: &Member) -> String {
	member.avatar_url().unwrap_or_else(|| {
		member
			.user
			.avatar_url()
			.unwrap_or_else(|| member.user.default_avatar_url())
	})
}

#[must_use]
pub fn user_pfp(user: &User) -> String {
	user.avatar_url()
		.unwrap_or_else(|| user.default_avatar_url())
}

pub async fn get_emoji(ctx: &Context, emojis: &EmojisMap, emoji_id: EmojiId) -> Option<Arc<Emoji>> {
	let emoji = if let Some(emoji) = emojis.get(&emoji_id) {
		emoji
	} else {
		match ctx.get_application_emoji(emoji_id).await {
			Ok(emoji) => {
				let arc_emoji = Arc::new(emoji);
				emojis.insert(emoji_id, arc_emoji.clone());
				arc_emoji
			}
			Err(err) => {
				warn!("Failed to fetch emoji: {err}");
				return None;
			}
		}
	};
	Some(emoji)
}

pub async fn banner_vec(ctx: &SContext<'_>, user_id: UserId) -> AResult<Vec<ContentPart>> {
	let chat_vec = if let Ok(user) = ctx.http().get_user(user_id).await
		&& let Some(banner) = user.banner_url()
	{
		let mut vec = Vec::with_capacity(3);
		uri_content(&banner, &mut vec).await?;
		vec
	} else {
		Vec::with_capacity(2)
	};
	Ok(chat_vec)
}

pub fn image_uri(content: &[u8], format: Option<&str>) -> AResult<String> {
	let mime_type = if let Some(format) = format {
		format
	} else {
		guess_format(content)?.to_mime_type()
	};
	let base64_image = BASE64.encode(content);

	let data_uri = format!("data:{mime_type};base64,{base64_image}");

	Ok(data_uri)
}

pub fn encode_image(content: &[u8]) -> AResult<Vec<u8>> {
	let img = load_from_memory(content)?;
	let mut img_bytes = Vec::with_capacity(content.len());
	img.write_to(&mut Cursor::new(&mut img_bytes), ImageFormat::Jpeg)?;
	Ok(img_bytes)
}

pub async fn fetch_and_parse<T>(
	request: impl Future<Output = RResult<Response>>,
) -> Result<T, Error>
where
	T: DeserializeOwned,
{
	let response = match request.await {
		Ok(resp) => match resp.error_for_status() {
			Ok(data) => data,
			Err(err) => {
				return Err(HTTPError::Request(err).into());
			}
		},
		Err(err) => {
			return Err(HTTPError::Request(err).into());
		}
	};

	match response.json::<T>().await {
		Ok(json) => Ok(json),
		Err(err) => Err(HTTPError::Parsing(err).into()),
	}
}

pub async fn paginate_container<'a, T, F, Fut>(
	ctx: SContext<'a>,
	items: &'a [T],
	timeout: Duration,
	mut render: F,
) -> AResult<()>
where
	T: Sync,
	F: FnMut(&'a T, usize, usize) -> Fut,
	Fut: Future<Output = CreateContainer<'a>>,
{
	let len = items.len();

	if len == 1 || ctx.guild_id().is_none() {
		let container = render(items.first().unwrap(), 0, len).await;
		ctx.send(reply_container(vec![CreateComponent::Container(container)]))
			.await?;
		return Ok(());
	}

	let buttons = [
		CreateButton::new("prev")
			.style(ButtonStyle::Primary)
			.label("⬅️"),
		CreateButton::new("next")
			.style(ButtonStyle::Primary)
			.label("➡️"),
	];

	let build_page = |container: CreateContainer<'a>, index: usize| -> CreateComponent<'a> {
		let active_buttons = if index == 0 {
			buttons.get(1..).unwrap().to_vec()
		} else if index >= len.saturating_sub(1) {
			buttons.get(..1).unwrap().to_vec()
		} else {
			buttons.to_vec()
		};

		let action_row = CreateContainerComponent::ActionRow(CreateActionRow::Buttons(Cow::Owned(
			active_buttons,
		)));

		let container = container
			.add_component(separator())
			.add_component(action_row);

		CreateComponent::Container(container)
	};

	let initial_container = render(items.first().unwrap(), 0, len).await;
	let message = ctx
		.send(reply_container(vec![build_page(initial_container, 0)]))
		.await?;

	let mut index: usize = 0;
	let mut stream = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(timeout)
		.message_id(message.message().await?.id)
		.stream();

	while let Some(interaction) = stream.next().await {
		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;

		if interaction.data.custom_id == "next" && index < len.saturating_sub(1) {
			index = index.saturating_add(1);
		} else if interaction.data.custom_id == "prev" && index > 0 {
			index = index.saturating_sub(1);
		} else {
			continue;
		}

		let item = items.get(index).unwrap();
		let container = render(item, index, len).await;

		let mut msg = interaction.message.clone();
		msg.edit(
			ctx.http(),
			edit_message_container(vec![build_page(container, index)]),
		)
		.await?;
	}

	let final_container = render(items.get(index).unwrap(), index, len).await;
	message
		.edit(
			ctx,
			reply_container(vec![CreateComponent::Container(final_container)]),
		)
		.await?;

	Ok(())
}

#[expect(dead_code)]
fn shard_restart(shard_id: ShardId) -> Result<(), Box<TrySendError<ShardRunnerMessage>>> {
	if let Some(shard_runner) = client_data().runners.get(&shard_id) {
		shard_runner
			.tx
			.unbounded_send(ShardRunnerMessage::Restart)?;
	}
	Ok(())
}

pub async fn url_bytes(url: &str) -> AResult<Bytes> {
	let data = HTTP_CLIENT.get(url).send().await?;
	let bytes = data.bytes().await?;

	Ok(bytes)
}

pub async fn guild_cache(
	bot_data: &Data,
	guild_id: GuildId,
	user_id_opt: Option<i64>,
	ctx: &Context,
) -> AResult<Arc<GuildCache>> {
	if let Some(cache) = bot_data.guilds.get(&guild_id) {
		return Ok(cache);
	}

	let _guard = bot_data.guild_cache_lock.lock().await;

	if let Some(cache) = bot_data.guilds.get(&guild_id) {
		return Ok(cache);
	}

	let guild_id_i64 = i64::from(guild_id);
	insert_guild_settings(guild_id_i64, &bot_data.db).await?;
	if let Some(user_id_i64) = user_id_opt {
		insert_user_settings(guild_id_i64, user_id_i64, &bot_data.db).await?;
	}

	let ai_channel = mpsc::channel(20);
	let music_channel = mpsc::channel(2);
	let (music_signal_tx, _music_signal_rx) = watch::channel::<TrackSignal>(TrackSignal::Idle);
	let (music_status_tx, _music_status_rx) =
		watch::channel::<ConnectionStatus>(ConnectionStatus::Disconnected);
	let prefix = fetch_guild_prefix(guild_id_i64, &bot_data.db)
		.await?
		.map_or(Cow::Borrowed(DEFAULT_PREFIX), Cow::Owned);
	let cache = Arc::new(GuildCache {
		ai_queue: ai_channel.0,
		music_data: MusicData {
			queue: music_channel.0,
			global: AtomicBool::new(false),
			track_signals: music_signal_tx,
			connection_signals: music_status_tx,
		},
		prefix: RwLock::new(prefix),
	});

	let ctx_clone = ctx.clone();

	spawn(async move { ai_task(ai_channel.1).await });
	spawn(async move { music_task(music_channel.1, guild_id, ctx_clone).await });

	bot_data.guilds.insert(guild_id, cache.clone());

	Ok(cache)
}
