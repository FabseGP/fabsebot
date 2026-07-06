use std::{borrow::Cow, io::Cursor, sync::Arc, time::Duration};

use anyhow::{Result as AResult, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bytes::Bytes;
use fastrand::usize;
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
		CreateContainer, CreateContainerComponent, CreateMediaGallery, CreateMediaGalleryItem,
		CreateMessage, CreateSection, CreateSectionAccessory, CreateSectionComponent,
		CreateSeparator, CreateTextDisplay, CreateThumbnail, CreateUnfurledMediaItem, GuildId,
		Http, Member, Message, MessageFlags, Permissions, ReactionType, Role, User, UserId,
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
use tracing::warn;
use winnow::{
	ModalResult, Parser as _,
	ascii::digit1,
	combinator::{preceded, separated_pair},
	error::{ContextError, ErrMode},
};

use crate::{
	config::types::{EmojisMap, Error, HTTP_CLIENT, SContext, UsersMap, client_data, utils_config},
	errors::commands::HTTPError,
	log_error,
	stats::counters::METRICS,
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

pub fn channel_counter(channel_name: String) {
	counter!(
		METRICS.channel_triggers.clone(),
		"channel" => channel_name,
	)
	.increment(1);
}

pub fn thumbnail_section<'a>(
	text: impl Into<Cow<'a, str>>,
	image: impl Into<Cow<'a, str>>,
) -> CreateContainerComponent<'a> {
	CreateContainerComponent::Section(CreateSection::new(
		vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
			text,
		))],
		CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
			image,
		))),
	))
}

pub fn visit_page_button<'a>(url: impl Into<Cow<'a, str>>) -> CreateButton<'a> {
	CreateButton::new_link(url)
		.label("Visit page")
		.emoji(ReactionType::Unicode(FixedString::from_str_trunc("🌐")))
}

pub fn media_gallery<'a>(url: impl Into<Cow<'a, str>>) -> CreateContainerComponent<'a> {
	CreateContainerComponent::MediaGallery(CreateMediaGallery::new(vec![
		CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url)),
	]))
}

pub fn text_display<'a>(text: impl Into<Cow<'a, str>>) -> CreateContainerComponent<'a> {
	CreateContainerComponent::TextDisplay(CreateTextDisplay::new(text))
}

pub fn separator<'a>() -> CreateContainerComponent<'a> {
	CreateContainerComponent::Separator(CreateSeparator::new())
}

pub fn message_container<'a>(
	message_opt: Option<&Message>,
	container: CreateContainer<'a>,
) -> CreateMessage<'a> {
	let mut create_message = CreateMessage::default()
		.components(vec![CreateComponent::Container(container)])
		.flags(MessageFlags::IS_COMPONENTS_V2)
		.allowed_mentions(CreateAllowedMentions::default().replied_user(false));
	if let Some(message) = message_opt {
		create_message = create_message.reference_message(message);
	}
	create_message
}

#[must_use]
pub fn reply_container(container: CreateContainer<'_>) -> CreateReply<'_> {
	CreateReply::default()
		.components(vec![CreateComponent::Container(container)])
		.flags(MessageFlags::IS_COMPONENTS_V2)
		.reply(true)
		.allowed_mentions(CreateAllowedMentions::default().replied_user(false))
}

pub fn edit_message_container(container: CreateContainer<'_>) -> EditMessage<'_> {
	EditMessage::default()
		.components(vec![CreateComponent::Container(container)])
		.flags(MessageFlags::IS_COMPONENTS_V2)
		.content("")
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
	let urls: GifResponse = fetch_and_parse(
		HTTP_CLIENT
			.get("https://tenor.googleapis.com/v2/search")
			.query(&[
				("q", input),
				("key", utils_config().api.gif_token.as_str()),
				("contentfilter", "medium"),
				("limit", "40"),
				("media_filter", "minimal"),
			])
			.send(),
	)
	.await?;

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
			let output = format!("# Failed to fetch gifs\n{error}");
			counter!(METRICS.gifs_errors.clone()).increment(1);
			log_error(&output, ctx).await;
			vec![(
				"https://i.postimg.cc/zffntsGs/tenor.gif".to_owned(),
				"Sucks to be you".to_owned(),
			)]
		}
	}
}

pub async fn get_gif(ctx: &Context, input: &str) -> String {
	let gifs = get_gifs(ctx, input).await;
	let index = usize(..gifs.len());
	gifs.into_iter().nth(index).map(|g| g.0).unwrap()
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

async fn get_lyrics_internal(track_name: &str, artist_name: &str) -> AResult<String> {
	let json: LyricsResponse = fetch_and_parse(
		HTTP_CLIENT
			.get("https://lrclib.net/api/get")
			.query(&[("track_name", track_name), ("artist_name", artist_name)])
			.send(),
	)
	.await?;

	Ok(json.0.plain_lyrics)
}

pub async fn get_lyrics(ctx: &Context, track_name: &str, artist_name: &str) -> Option<String> {
	match get_lyrics_internal(track_name, artist_name).await {
		Ok(lyrics) => Some(lyrics),
		Err(error) => {
			let output = format!("# Failed to fetch lyrics\n{error}");
			counter!(METRICS.lyrics_errors.clone()).increment(1);
			log_error(&output, ctx).await;
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
	let waifu_response: WaifuResponse = fetch_and_parse(
		HTTP_CLIENT
			.get("https://api.waifu.im/images?IsNsfw=False")
			.send(),
	)
	.await?;

	Ok(waifu_response.items.into_iter().next().unwrap().url)
}

pub async fn get_waifu(ctx: &Context) -> String {
	match fetch_waifu_internal().await {
		Ok(waifu) => waifu,
		Err(error) => {
			let output = format!("# Failed to fetch waifu\n{error}");
			counter!(METRICS.waifu_errors.clone()).increment(1);
			log_error(&output, ctx).await;
			"https://c.tenor.com/CosM_E8-RQUAAAAC/tenor.gif".to_owned()
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

pub async fn get_user(http: &Http, users: &UsersMap, user_id: UserId) -> AResult<Arc<User>> {
	let user = if let Some(user) = users.get(&user_id) {
		user
	} else {
		match http.get_user(user_id).await {
			Ok(user) => {
				let arc_user = Arc::new(user);
				users.insert(user_id, arc_user.clone());
				arc_user
			}
			Err(err) => {
				bail!("Failed to fetch emoji: {err}");
			}
		}
	};
	Ok(user)
}

pub async fn get_emoji(
	ctx: &Context,
	emojis: &EmojisMap,
	emoji_id: EmojiId,
) -> AResult<Arc<Emoji>> {
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
				bail!("Failed to fetch user: {err}");
			}
		}
	};
	Ok(emoji)
}

pub async fn user_banner(http: &Http, users: &UsersMap, user_id: UserId) -> Option<String> {
	match get_user(http, users, user_id).await {
		Ok(user) => user.banner_url(),
		Err(err) => {
			warn!("{err}");
			None
		}
	}
}

#[must_use]
pub fn user_roles_joined(roles: &[Role]) -> String {
	roles
		.iter()
		.map(|role| role.name.as_str())
		.intersperse(", ")
		.collect::<String>()
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
		ctx.send(reply_container(container)).await?;
		return Ok(());
	}

	let ctx_id = ctx.id();

	let build_page = |container: CreateContainer<'a>, index: usize| -> CreateContainer<'a> {
		let buttons = vec![
			CreateButton::new(format!("{ctx_id}_p"))
				.style(ButtonStyle::Primary)
				.label("⬅️"),
			CreateButton::new(format!("{ctx_id}_n"))
				.style(ButtonStyle::Primary)
				.label("➡️"),
		];

		let active_buttons = if index == 0 {
			buttons.get(1..).unwrap().to_vec()
		} else if index >= len.saturating_sub(1) {
			buttons.get(..1).unwrap().to_vec()
		} else {
			buttons
		};

		let action_row = CreateContainerComponent::ActionRow(CreateActionRow::Buttons(Cow::Owned(
			active_buttons,
		)));

		container
			.add_component(separator())
			.add_component(action_row)
	};

	let initial_container = render(items.first().unwrap(), 0, len).await;
	let message = ctx
		.send(reply_container(build_page(initial_container, 0)))
		.await?;

	let ctx_id_str = ctx_id.to_string();
	let mut index: usize = 0;
	let mut stream = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(timeout)
		.filter(move |i| i.data.custom_id.starts_with(&ctx_id_str))
		.stream();

	while let Some(interaction) = stream.next().await {
		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;

		let prev = index;
		if interaction.data.custom_id.ends_with('n') && index < len.saturating_sub(1) {
			index = index.saturating_add(1);
		} else if interaction.data.custom_id.ends_with('p') && index > 0 {
			index = index.saturating_sub(1);
		}

		if index == prev {
			continue;
		}

		let Some(item) = items.get(index) else {
			continue;
		};
		let container = render(item, index, len).await;

		let mut msg = interaction.message.clone();
		msg.edit(
			ctx.http(),
			edit_message_container(build_page(container, index)),
		)
		.await?;
	}

	let final_container = render(items.get(index).unwrap(), index, len).await;

	message.edit(ctx, reply_container(final_container)).await?;

	Ok(())
}

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
