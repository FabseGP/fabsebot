use core::fmt::{Display, Formatter, Result as FmtResult};
use std::{borrow::Cow, time::Duration};

use anyhow::Result as AResult;
use base64::{Engine as _, engine::general_purpose};
use fabsebot_core::{
	config::{
		constants::{COLOUR_GREEN, COLOUR_ORANGE},
		types::{Error, HTTP_CLIENT, SContext, utils_config},
	},
	errors::commands::{AIError, Base64Error, HTTPError, InteractionError},
	utils::{
		ai::ai_response_simple,
		helpers::{
			get_gifs, get_waifu, media_gallery, non_empty_string, non_empty_vec, send_container,
			separator, text_display, thumbnail_section, true_bool, visit_page_button,
		},
	},
};
use poise::CreateReply;
use serde::{Deserialize, Serialize};
use serenity::{
	all::{
		ButtonStyle, Colour, ComponentInteractionCollector, CreateActionRow, CreateAttachment,
		CreateButton, CreateComponent, CreateContainer, CreateContainerComponent, CreateEmbed,
		CreateInteractionResponse, EditMessage, Member, MessageFlags, MessageId, User,
	},
	futures::StreamExt as _,
};
use sqlx::query_scalar;
use tracing::warn;
use url::form_urlencoded::byte_serialize;

use crate::{command_permissions, require_guild_id};

struct State {
	next_id: String,
	prev_id: String,
	index: usize,
	len: usize,
}

impl State {
	fn new(ctx_id: u64, len: usize) -> Self {
		Self {
			next_id: format!("{ctx_id}_n"),
			prev_id: format!("{ctx_id}_p"),
			index: 0,
			len,
		}
	}
}

#[derive(Deserialize)]
struct FabseAIImage {
	#[serde(deserialize_with = "true_bool")]
	#[expect(dead_code)]
	success: bool,
	result: AIResponseImage,
}
#[derive(Deserialize)]
struct AIResponseImage {
	image: String,
}

#[derive(Serialize)]
struct ImageRequest {
	prompt: String,
}

async fn ai_image_internal(ctx: &SContext<'_>, prompt: &str) -> AResult<()> {
	let _typing = ctx.defer_or_broadcast().await;
	let request = ImageRequest {
		prompt: format!("{prompt} {}", fastrand::usize(..1024)),
	};
	let utils_config = utils_config();

	let resp = match HTTP_CLIENT
		.post(&utils_config.api.cloudflare_image_gen)
		.bearer_auth(&utils_config.api.cloudflare_token)
		.json(&request)
		.send()
		.await
	{
		Ok(response) => response,
		Err(err) => {
			ctx.reply("Servers too overworked :/").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let resp_parsed = match resp.json::<FabseAIImage>().await {
		Ok(resp_parsed) => resp_parsed,
		Err(err) => {
			ctx.reply(format!("\"{prompt}\" is too dangerous to generate"))
				.await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	match general_purpose::STANDARD.decode(resp_parsed.result.image) {
		Ok(img_dec) => {
			ctx.send(
				CreateReply::default()
					.reply(true)
					.attachment(CreateAttachment::bytes(img_dec, "output.png")),
			)
			.await?;
		}
		Err(err) => {
			ctx.reply(format!("\"{prompt}\" is too dangerous to generate"))
				.await?;
			return Err(Base64Error::FailedBytesDecode(err).into());
		}
	}
	Ok(())
}

/// Did someone say AI image?
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn ai_image(
	ctx: SContext<'_>,
	#[description = "Prompt"]
	#[rest]
	prompt: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	ai_image_internal(&ctx, &prompt).await?;
	Ok(())
}

async fn ai_text_internal(ctx: &SContext<'_>, prompt: &str, role: &str) -> AResult<()> {
	ctx.defer().await?;

	let resp = match ai_response_simple(
		role,
		prompt,
		&utils_config().fabseserver.text_gen_model,
		Some(1000),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply(format!("\"{prompt}\" is too dangerous to ask"))
				.await?;
			return Err(err);
		}
	};

	let mut text = format!("# {prompt}\n{resp}");
	text.truncate(4000);

	let text_display = [text_display(&text)];

	let container = CreateContainer::new(&text_display).accent_colour(Colour::RED);

	send_container(ctx, container).await?;

	Ok(())
}

/// Make the ai generate text for you
#[poise::command(
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn ai_text(
	ctx: SContext<'_>,
	#[description = "AI personality, e.g. *you're an evil assistant*"] role: String,
	#[description = "Prompt"]
	#[rest]
	prompt: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	ai_text_internal(&ctx, &prompt, &role).await?;
	Ok(())
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct AniMangaResponse<T> {
	#[serde(deserialize_with = "non_empty_vec")]
	data: Vec<AniManga<T>>,
}

#[derive(Deserialize)]
struct AniManga<T> {
	url: String,
	images: AniMangaImageTypes,
	titles: Vec<AniMangaTitleTypes>,
	#[serde(rename = "type")]
	anime_type: String,
	status: String,
	score: Option<f32>,
	popularity: Option<i32>,
	favorites: Option<i32>,
	synopsis: Option<String>,
	genres: Vec<AniMangaGenres>,
	#[serde(flatten)]
	specific: T,
}

#[derive(Deserialize)]
struct AnimeSpecific {
	episodes: Option<i32>,
	duration: Option<String>,
	aired: AniMangaAired,
}

#[derive(Deserialize)]
struct MangaSpecific {
	chapters: Option<i32>,
	volumes: Option<i32>,
	published: AniMangaAired,
}

#[derive(Deserialize)]
struct AniMangaImageTypes {
	webp: AniMangaImageWebp,
}

#[derive(Deserialize)]
struct AniMangaImageWebp {
	image_url: String,
}

#[derive(Deserialize)]
struct AniMangaTitleTypes {
	#[serde(rename = "type")]
	title_type: String,
	title: String,
}

#[derive(Deserialize)]
struct AniMangaAired {
	#[serde(rename = "string")]
	aired_string: Option<String>,
}

#[derive(Deserialize)]
struct AniMangaGenres {
	name: String,
}

async fn anime_internal(ctx: SContext<'_>, anime: &str) -> AResult<()> {
	let typing = ctx.defer_or_broadcast().await;
	let resp = match HTTP_CLIENT
		.get("https://api.jikan.moe/v4/anime")
		.query(&[("q", anime), ("limit", "5")])
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("API down, get a life!").await?;
			return Err(HTTPError::Request(err).into());
		}
	};
	let json = match resp.json::<AniMangaResponse<AnimeSpecific>>().await {
		Ok(json) => json,
		Err(err) => {
			ctx.reply("Not worthy of looking up").await?;
			return Err(HTTPError::Request(err).into());
		}
	};
	let first_entry = json.data.first().unwrap();

	let empty = String::new();
	let mut japanese_title = first_entry
		.titles
		.iter()
		.find(|t| t.title_type == "Japanese")
		.map_or("No japanese title available", |t| t.title.as_str());
	let mut embed = CreateEmbed::default()
		.title(japanese_title)
		.image(&first_entry.images.webp.image_url)
		.url(&first_entry.url)
		.colour(COLOUR_ORANGE);

	if let Some(synopsis) = &first_entry.synopsis {
		embed = embed.description(format!("*{synopsis}*"));
	}

	embed = embed.field("Format", &first_entry.anime_type, true);
	embed = embed.field("Status", &first_entry.status, true);

	if let Some(english_title) = first_entry
		.titles
		.iter()
		.find(|t| t.title_type == "English")
		.map(|t| t.title.as_str())
	{
		embed = embed.field("English title", english_title, true);
	}
	embed = embed.field("", &empty, false);
	if let Some(score) = first_entry.score {
		embed = embed.field("Score", score.to_string(), true);
	}
	if let Some(popularity) = first_entry.popularity {
		embed = embed.field("Popularity", popularity.to_string(), true);
	}
	if let Some(favorites) = first_entry.favorites {
		embed = embed.field("Favorites", favorites.to_string(), true);
	}
	embed = embed.field("", &empty, false);
	if let Some(episodes) = first_entry.specific.episodes {
		embed = embed.field("Episodes", episodes.to_string(), true);
	}
	if let Some(duration) = &first_entry.specific.duration {
		embed = embed.field("Duration", duration, true);
	}
	if let Some(aired) = &first_entry.specific.aired.aired_string {
		embed = embed.field("Aired", aired, true);
	}
	let mut genres_string = first_entry
		.genres
		.iter()
		.map(|genre| genre.name.as_str())
		.intersperse(" - ")
		.collect::<String>();
	embed = embed.field("Genres", genres_string, false);
	let len = json.data.len();
	if ctx.guild_id().is_some() && len > 1 {
		drop(typing);
		let mut state = State::new(ctx.id(), len);
		let mut final_embed = embed.clone();
		let buttons = [
			CreateButton::new(&state.prev_id)
				.style(ButtonStyle::Primary)
				.label("⬅️"),
			CreateButton::new(&state.next_id)
				.style(ButtonStyle::Primary)
				.label("➡️"),
		];
		let mut action_row = [CreateComponent::ActionRow(CreateActionRow::buttons(
			&buttons[1..],
		))];

		let message = ctx
			.send(
				CreateReply::default()
					.reply(true)
					.embed(embed)
					.components(&action_row),
			)
			.await?;

		let ctx_id_str = ctx.id().to_string();
		let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
			.timeout(Duration::from_mins(1))
			.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
			.stream();

		while let Some(interaction) = collector_stream.next().await {
			interaction
				.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
				.await?;

			if interaction.data.custom_id.ends_with('n')
				&& state.index < state.len.saturating_sub(1)
			{
				state.index = state.index.saturating_add(1);
			} else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
				state.index = state.index.saturating_sub(1);
			}

			let Some(current_entry) = json.data.get(state.index) else {
				warn!("Invalid anime index: {}", state.index);
				continue;
			};

			japanese_title = current_entry
				.titles
				.iter()
				.find(|t| t.title_type == "Japanese")
				.map_or("No japanese title available", |t| t.title.as_str());
			embed = CreateEmbed::default()
				.title(japanese_title)
				.thumbnail(&current_entry.images.webp.image_url)
				.url(&current_entry.url)
				.colour(COLOUR_ORANGE);

			if let Some(synopsis) = &current_entry.synopsis {
				embed = embed.description(format!("*{synopsis}*"));
			}
			embed = embed.field("Format", &current_entry.anime_type, true);
			embed = embed.field("Status", &current_entry.status, true);
			if let Some(english_title) = current_entry
				.titles
				.iter()
				.find(|t| t.title_type == "English")
				.map(|t| &t.title)
			{
				embed = embed.field("English title", english_title, true);
			}
			embed = embed.field("", &empty, false);
			if let Some(score) = current_entry.score {
				embed = embed.field("Score", score.to_string(), true);
			}
			if let Some(popularity) = current_entry.popularity {
				embed = embed.field("Popularity", popularity.to_string(), true);
			}
			if let Some(favorites) = current_entry.favorites {
				embed = embed.field("Favorites", favorites.to_string(), true);
			}
			embed = embed.field("", &empty, false);
			if let Some(episodes) = current_entry.specific.episodes {
				embed = embed.field("Episodes", episodes.to_string(), true);
			}
			if let Some(duration) = &current_entry.specific.duration {
				embed = embed.field("Duration", duration, true);
			}
			if let Some(aired) = &current_entry.specific.aired.aired_string {
				embed = embed.field("Aired", aired, true);
			}
			genres_string = current_entry
				.genres
				.iter()
				.map(|genre| genre.name.as_str())
				.intersperse(" - ")
				.collect::<String>();
			embed = embed.field("Genres", genres_string, false);
			final_embed = embed.clone();

			action_row = [CreateComponent::ActionRow(CreateActionRow::Buttons({
				if state.index == 0 {
					Cow::Borrowed(&buttons[1..])
				} else if state.index == len.saturating_sub(1) {
					Cow::Borrowed(&buttons[..1])
				} else {
					Cow::Borrowed(&buttons)
				}
			}))];

			let mut msg = interaction.message;

			msg.edit(
				ctx.http(),
				EditMessage::default().embed(embed).components(&action_row),
			)
			.await?;
		}
		message
			.edit(
				ctx,
				CreateReply::default()
					.reply(true)
					.embed(final_embed)
					.components(&[]),
			)
			.await?;
	} else {
		ctx.send(CreateReply::default().reply(true).embed(embed))
			.await?;
	}
	Ok(())
}

/// Lookup anime (MAL-edition)
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn anime(
	ctx: SContext<'_>,
	#[description = "Anime to search"]
	#[rest]
	anime: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	anime_internal(ctx, &anime).await?;
	Ok(())
}

#[derive(Deserialize)]
struct MoeResponse {
	#[serde(deserialize_with = "non_empty_vec")]
	result: Vec<AnimeScene>,
}

#[derive(Deserialize)]
struct AnimeScene {
	anilist: Anilist,
	episode: Option<i32>,
	from: Option<f32>,
	to: Option<f32>,
	#[serde(deserialize_with = "non_empty_string")]
	video: String,
}

#[derive(Deserialize)]
struct Anilist {
	title: AnimeTitle,
}

#[derive(Deserialize)]
struct AnimeTitle {
	english: Option<String>,
}

impl Display for AnimeTitle {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		match &self.english {
			Some(english_title) => write!(f, "{english_title}"),
			None => write!(f, "Unknown Title"),
		}
	}
}

async fn anime_scene_internal(ctx: &SContext<'_>, input: &str) -> AResult<()> {
	let _typing = ctx.defer_or_broadcast().await;
	let response = match HTTP_CLIENT
		.get("https://api.trace.moe/search?cutBorders&anilistInfo")
		.query(&[("url", input)])
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Oof, anime-server down!").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let scene = match response.json::<MoeResponse>().await {
		Ok(json) => json,
		Err(err) => {
			ctx.reply("Not worthy of looking up").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let first_result = scene.result.first().unwrap();

	let title = first_result
		.anilist
		.title
		.english
		.as_deref()
		.unwrap_or("Unknown title");

	let text = format!(
		"# {title}\n**Episode:** {}\n**From:** {}\n**To:**: {}",
		first_result.episode.unwrap_or(0),
		first_result.from.unwrap_or(0.0),
		first_result.to.unwrap_or(0.0)
	);

	let text_display = [text_display(&text)];
	let media = media_gallery(&first_result.video);

	let container = CreateContainer::new(&text_display)
		.add_component(media)
		.accent_colour(Colour::BLUE);

	send_container(ctx, container).await?;

	Ok(())
}

/// What anime was that scene from?
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn anime_scene(
	ctx: SContext<'_>,
	#[description = "Link to anime image"]
	#[rest]
	input: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	anime_scene_internal(&ctx, &input).await?;
	Ok(())
}

#[derive(Deserialize)]
struct EightBallResponse {
	#[serde(deserialize_with = "non_empty_string")]
	reading: String,
}

async fn eightball_internal(ctx: &SContext<'_>, question: &str) -> AResult<()> {
	let _typing = ctx.defer_or_broadcast().await;

	let request = match HTTP_CLIENT
		.get("https://eightballapi.com/api/biased")
		.query(&[("question", question), ("lucky", "false")])
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("I don't feel like answering...").await?;

			return Err(HTTPError::Request(err).into());
		}
	};

	let judging = match request.json::<EightBallResponse>().await {
		Ok(data) => data,
		Err(err) => {
			ctx.reply("Sometimes riding a giraffe is what you need")
				.await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let text = format!("# {question}\n{}", judging.reading);

	let text_display = [text_display(&text)];

	let container = CreateContainer::new(&text_display).accent_colour(Colour::ORANGE);

	send_container(ctx, container).await?;

	Ok(())
}

/// When you need a wise opinion
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn eightball(
	ctx: SContext<'_>,
	#[description = "Your question"]
	#[rest]
	question: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	eightball_internal(&ctx, &question).await?;
	Ok(())
}

async fn gif_internal(ctx: SContext<'_>, input: &str) -> AResult<()> {
	let typing = ctx.defer_or_broadcast().await;
	let gifs = get_gifs(ctx.serenity_context(), input).await;
	let mut embed = CreateEmbed::default().colour(COLOUR_ORANGE);
	let len = gifs.len();
	if ctx.guild_id().is_some() && len > 1 {
		drop(typing);
		if let Some(gif) = gifs.first() {
			embed = embed.image(gif.0.as_ref()).title(gif.1.as_ref());
		}
		let mut state = State::new(ctx.id(), len);
		let mut final_embed = embed.clone();
		let buttons = [
			CreateButton::new(&state.prev_id)
				.style(ButtonStyle::Primary)
				.label("⬅️"),
			CreateButton::new(&state.next_id)
				.style(ButtonStyle::Primary)
				.label("➡️"),
		];
		let mut action_row = [CreateComponent::ActionRow(CreateActionRow::buttons(
			&buttons[1..],
		))];

		let message = ctx
			.send(
				CreateReply::default()
					.reply(true)
					.embed(embed)
					.components(&action_row),
			)
			.await?;

		let ctx_id_str = ctx.id().to_string();
		let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
			.timeout(Duration::from_mins(1))
			.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
			.stream();

		while let Some(interaction) = collector_stream.next().await {
			interaction
				.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
				.await?;

			if interaction.data.custom_id.ends_with('n')
				&& state.index < state.len.saturating_sub(1)
			{
				state.index = state.index.saturating_add(1);
			} else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
				state.index = state.index.saturating_sub(1);
			}

			embed = CreateEmbed::default().colour(COLOUR_ORANGE);
			if let Some(gif) = gifs.get(state.index) {
				embed = embed.image(gif.0.as_ref()).title(gif.1.as_ref());
			}
			final_embed = embed.clone();

			action_row = [CreateComponent::ActionRow(CreateActionRow::Buttons({
				if state.index == 0 {
					Cow::Borrowed(&buttons[1..])
				} else if state.index == len.saturating_sub(1) {
					Cow::Borrowed(&buttons[..1])
				} else {
					Cow::Borrowed(&buttons)
				}
			}))];

			let mut msg = interaction.message;

			msg.edit(
				ctx.http(),
				EditMessage::default().embed(embed).components(&action_row),
			)
			.await?;
		}
		message
			.edit(
				ctx,
				CreateReply::default()
					.reply(true)
					.embed(final_embed)
					.components(&[]),
			)
			.await?;
	} else {
		let index = fastrand::usize(..len);
		if let Some(gif) = gifs.get(index) {
			embed = embed.image(gif.0.as_ref()).title(gif.1.as_ref());
		}
		ctx.send(CreateReply::default().reply(true).embed(embed))
			.await?;
	}

	Ok(())
}

/// Gifing
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn gif(
	ctx: SContext<'_>,
	#[description = "Search gif"]
	#[rest]
	input: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	gif_internal(ctx, &input).await?;
	Ok(())
}

const ROASTS: &[&str] = &[
	"your life",
	"you're not funny",
	"you",
	"get a life bitch",
	"I don't like you",
	"you smell",
];

#[derive(Deserialize)]
struct JokeResponse {
	#[serde(deserialize_with = "non_empty_string")]
	joke: String,
}

async fn joke_internal(ctx: &SContext<'_>) -> AResult<()> {
	let request_url =
		"https://api.humorapi.com/jokes/random?api-key=48c239c85f804a0387251d9b3587fa2c";

	let request = match HTTP_CLIENT.get(request_url).send().await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Look at the mirror").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	match request.json::<JokeResponse>().await {
		Ok(data) => {
			ctx.reply(&data.joke).await?;
		}
		Err(err) => {
			let index = fastrand::usize(..ROASTS.len());
			if let Some(roast) = ROASTS.get(index) {
				ctx.reply(*roast).await?;
			} else {
				ctx.reply("No jokes now").await?;
			}
			return Err(HTTPError::Request(err).into());
		}
	}
	Ok(())
}

/// When your life isn't fun anymore
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn joke(ctx: SContext<'_>) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	joke_internal(&ctx).await?;
	Ok(())
}

async fn manga_internal(ctx: SContext<'_>, manga: &str) -> AResult<()> {
	let typing = ctx.defer_or_broadcast().await;

	let resp = match HTTP_CLIENT
		.get("https://api.jikan.moe/v4/manga")
		.query(&[("manga", manga), ("limit", "5")])
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("API down, get a life!").await?;
			return Err(HTTPError::Request(err).into());
		}
	};
	let data = match resp.json::<AniMangaResponse<MangaSpecific>>().await {
		Ok(json) => json,
		Err(err) => {
			ctx.reply("Not worthy of looking up").await?;
			return Err(HTTPError::Request(err).into());
		}
	};
	let first_entry = data.data.first().unwrap();

	let empty = String::new();
	let mut japanese_title = first_entry
		.titles
		.iter()
		.find(|t| t.title_type == "Japanese")
		.map_or("No japanese title available", |t| t.title.as_str());
	let mut embed = CreateEmbed::default()
		.title(japanese_title)
		.thumbnail(&first_entry.images.webp.image_url)
		.url(&first_entry.url)
		.colour(COLOUR_ORANGE);
	if let Some(synopsis) = &first_entry.synopsis {
		embed = embed.description(format!("*{synopsis}*"));
	}
	embed = embed.field("Format", &first_entry.anime_type, true);
	embed = embed.field("Status", &first_entry.status, true);
	if let Some(english_title) = first_entry
		.titles
		.iter()
		.find(|t| t.title_type == "English")
		.map(|t| &t.title)
	{
		embed = embed.field("English title", english_title, true);
	}
	embed = embed.field("", &empty, false);
	if let Some(score) = first_entry.score {
		embed = embed.field("Score", score.to_string(), true);
	}
	if let Some(popularity) = first_entry.popularity {
		embed = embed.field("Popularity", popularity.to_string(), true);
	}
	if let Some(favorites) = first_entry.favorites {
		embed = embed.field("Favorites", favorites.to_string(), true);
	}
	embed = embed.field("", &empty, false);
	if let Some(chapters) = first_entry.specific.chapters {
		embed = embed.field("Chapters", chapters.to_string(), true);
	}
	if let Some(volumes) = first_entry.specific.volumes {
		embed = embed.field("Volumes", volumes.to_string(), true);
	}
	if let Some(published) = &first_entry.specific.published.aired_string {
		embed = embed.field("Published", published, true);
	}
	let mut genres_string = first_entry
		.genres
		.iter()
		.map(|genre| genre.name.as_str())
		.intersperse(" - ")
		.collect::<String>();
	embed = embed.field("Genres", genres_string, false);
	let len = data.data.len();
	if ctx.guild_id().is_some() && len > 1 {
		drop(typing);
		let mut state = State::new(ctx.id(), len);
		let mut final_embed = embed.clone();
		let buttons = [
			CreateButton::new(&state.prev_id)
				.style(ButtonStyle::Primary)
				.label("⬅️"),
			CreateButton::new(&state.next_id)
				.style(ButtonStyle::Primary)
				.label("➡️"),
		];
		let mut action_row = [CreateComponent::ActionRow(CreateActionRow::buttons(
			&buttons[1..],
		))];

		let message = ctx
			.send(
				CreateReply::default()
					.reply(true)
					.embed(embed)
					.components(&action_row),
			)
			.await?;

		let ctx_id_str = ctx.id().to_string();

		let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
			.timeout(Duration::from_mins(1))
			.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
			.stream();

		while let Some(interaction) = collector_stream.next().await {
			interaction
				.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
				.await?;

			if interaction.data.custom_id.ends_with('n')
				&& state.index < state.len.saturating_sub(1)
			{
				state.index = state.index.saturating_add(1);
			} else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
				state.index = state.index.saturating_sub(1);
			}

			let Some(current_entry) = data.data.get(state.index) else {
				warn!("Invalid manga index: {}", state.index);
				continue;
			};

			japanese_title = current_entry
				.titles
				.iter()
				.find(|t| t.title_type == "Japanese")
				.map_or("No japanese title available", |t| t.title.as_str());
			embed = CreateEmbed::default()
				.title(japanese_title)
				.thumbnail(&current_entry.images.webp.image_url)
				.url(&current_entry.url)
				.colour(COLOUR_ORANGE);

			if let Some(synopsis) = &current_entry.synopsis {
				embed = embed.description(format!("*{synopsis}*"));
			}
			embed = embed.field("Format", &current_entry.anime_type, true);
			embed = embed.field("Status", &current_entry.status, true);
			if let Some(english_title) = current_entry
				.titles
				.iter()
				.find(|t| t.title_type == "English")
				.map(|t| &t.title)
			{
				embed = embed.field("English title", english_title, true);
			}
			embed = embed.field("", &empty, false);
			if let Some(score) = current_entry.score {
				embed = embed.field("Score", score.to_string(), true);
			}
			if let Some(popularity) = current_entry.popularity {
				embed = embed.field("Popularity", popularity.to_string(), true);
			}
			if let Some(favorites) = current_entry.favorites {
				embed = embed.field("Favorites", favorites.to_string(), true);
			}
			embed = embed.field("", &empty, false);
			if let Some(chapters) = current_entry.specific.chapters {
				embed = embed.field("Chapters", chapters.to_string(), true);
			}
			if let Some(volumes) = current_entry.specific.volumes {
				embed = embed.field("Volumes", volumes.to_string(), true);
			}
			if let Some(published) = &current_entry.specific.published.aired_string {
				embed = embed.field("Published", published, true);
			}
			genres_string = current_entry
				.genres
				.iter()
				.map(|genre| genre.name.as_str())
				.intersperse(" - ")
				.collect::<String>();
			embed = embed.field("Genres", genres_string, false);
			final_embed = embed.clone();

			action_row = [CreateComponent::ActionRow(CreateActionRow::Buttons({
				if state.index == 0 {
					Cow::Borrowed(&buttons[1..])
				} else if state.index == len.saturating_sub(1) {
					Cow::Borrowed(&buttons[..1])
				} else {
					Cow::Borrowed(&buttons)
				}
			}))];

			let mut msg = interaction.message;

			msg.edit(
				ctx.http(),
				EditMessage::default().embed(embed).components(&action_row),
			)
			.await?;
		}
		message
			.edit(
				ctx,
				CreateReply::default()
					.reply(true)
					.embed(final_embed)
					.components(&[]),
			)
			.await?;
	} else {
		ctx.send(CreateReply::default().reply(true).embed(embed))
			.await?;
	}

	Ok(())
}

/// Lookup manga (MAL-edition)
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn manga(
	ctx: SContext<'_>,
	#[description = "Manga to search"]
	#[rest]
	manga: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	manga_internal(ctx, &manga).await?;
	Ok(())
}

async fn memegen_internal(
	ctx: &SContext<'_>,
	top_left: &str,
	top_right: &str,
	bottom: &str,
) -> AResult<()> {
	let request_url = {
		let encoded_left: String = byte_serialize(top_left.as_bytes()).collect();
		let encoded_right: String = byte_serialize(top_right.as_bytes()).collect();
		let encoded_bottom: String = byte_serialize(bottom.as_bytes()).collect();
		format!("https://api.memegen.link/images/exit/{encoded_left}/{encoded_right}/{encoded_bottom}.png")
	};
	ctx.reply(request_url).await?;
	Ok(())
}

/// When there aren't enough memes
#[poise::command(
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn memegen(
	ctx: SContext<'_>,
	#[description = "Top-left text"] top_left: String,
	#[description = "Top-right text"] top_right: String,
	#[description = "Bottom text"] bottom: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	memegen_internal(&ctx, &top_left, &top_right, &bottom).await?;
	Ok(())
}

async fn roast_internal(ctx: &SContext<'_>, description: &str, name: &str) -> AResult<()> {
	let role = "you're an evil ai assistant that excels at roasting ppl, especially weebs. no \
	            mercy shown. the prompt will contain information of your target";

	let resp = match ai_response_simple(
		role,
		description,
		&utils_config().fabseserver.text_gen_model,
		Some(1000),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply(format!("{name}'s life is already roasted"))
				.await?;
			return Err(AIError::UnexpectedResponse(err).into());
		}
	};

	let mut text = format!("# Roasting {name}\n{resp}");
	text.truncate(4000);

	let text_display = [text_display(&text)];

	let container = CreateContainer::new(&text_display).accent_colour(Colour::RED);

	send_container(ctx, container).await?;

	Ok(())
}

/// When someone offended you
#[poise::command(
	slash_command,
	install_context = "User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn roast_user(
	ctx: SContext<'_>,
	#[description = "Target"] user: User,
) -> Result<(), Error> {
	let _typing = ctx.defer_or_broadcast().await;
	let avatar_url = user
		.avatar_url()
		.unwrap_or_else(|| user.default_avatar_url());
	let banner_url = (ctx.http().get_user(user.id).await).map_or_else(
		|_| "user has no banner".to_owned(),
		|user| {
			user.banner_url()
				.unwrap_or_else(|| "user has no banner".to_owned())
		},
	);
	let name = user.display_name();
	let account_date = user.id.created_at();

	let description =
		format!("name:{name},avatar:{avatar_url},banner:{banner_url},acc_create:{account_date}");

	roast_internal(&ctx, &description, name).await?;

	Ok(())
}

/// When someone offended you
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | \
	                            READ_MESSAGE_HISTORY"
)]
pub async fn roast(
	ctx: SContext<'_>,
	#[description = "Target"] member: Member,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let _typing = ctx.defer_or_broadcast().await;
	let avatar_url = member.avatar_url().unwrap_or_else(|| {
		member
			.user
			.avatar_url()
			.unwrap_or_else(|| member.user.default_avatar_url())
	});
	let banner_url = (ctx.http().get_user(member.user.id).await).map_or_else(
		|_| "user has no banner".to_owned(),
		|user| {
			user.banner_url()
				.unwrap_or_else(|| "user has no banner".to_owned())
		},
	);
	let roles = member.roles(ctx.cache()).map_or_else(
		|| "no roles".to_owned(),
		|member_roles| {
			member_roles
				.iter()
				.map(|role| role.name.as_str())
				.intersperse(", ")
				.collect()
		},
	);
	let name = member.display_name();
	let account_date = member.user.id.created_at();
	let join_date = member.joined_at.unwrap_or_default();

	let message_count = query_scalar!(
		r#"
		SELECT message_count
		FROM user_settings
		WHERE guild_id = $1
		AND user_id = $2
		"#,
		guild_id.get().cast_signed(),
		ctx.author().id.get().cast_signed()
	)
	.fetch_one(&mut *ctx.data().db.acquire().await?)
	.await?;

	let mut messages = ctx.channel_id().messages_iter(&ctx).boxed();

	let messages_string = {
		let mut result = String::new();
		let mut result_count: u32 = 0;
		let mut missing_match_count: u32 = 0;

		while let Some(message_result) = messages.next().await {
			if let Ok(message) = message_result {
				if message.author.id == member.user.id {
					let index = result_count.saturating_add(1);
					if result_count > 0 {
						result.push(',');
					}
					result.push_str(&index.to_string());
					result.push(':');
					result.push_str(&message.content);
					result_count = result_count.saturating_add(1);
				} else {
					missing_match_count = missing_match_count.saturating_add(1);
				}
			} else {
				break;
			}
			if result_count >= 25 || missing_match_count >= 100 {
				break;
			}
		}

		result
	};

	let description = format!(
		"name:{name},avatar:{avatar_url},banner:{banner_url},roles:{roles},acc_create:\
		 {account_date},joined_svr:{join_date},msg_count:{message_count},last_msgs:\
		 {messages_string}"
	);

	roast_internal(&ctx, &description, name).await?;

	Ok(())
}

#[derive(Deserialize)]
struct FabseTranslate {
	alternatives: Vec<String>,
	#[serde(rename(deserialize = "detectedLanguage"))]
	detected_language: FabseLanguage,
	#[serde(
		rename(deserialize = "translatedText"),
		deserialize_with = "non_empty_string"
	)]
	translated_text: String,
}

#[derive(Deserialize)]
struct FabseLanguage {
	confidence: f64,
	language: String,
}

#[derive(Serialize)]
struct TranslateRequest<'a> {
	q: &'a str,
	source: &'a str,
	target: &'a str,
	alternatives: u8,
}

async fn translate_internal(
	ctx: SContext<'_>,
	target: Option<String>,
	sentence: Option<String>,
) -> AResult<()> {
	let typing = ctx.defer_or_broadcast().await;
	let content = if let Some(query) = sentence {
		query
	} else if ctx.guild_id().is_some() {
		let msg = ctx
			.channel_id()
			.message(&ctx.http(), MessageId::new(ctx.id()))
			.await?;
		if let Some(ref_msg) = msg.referenced_message {
			ref_msg.content.into_string()
		} else {
			ctx.reply("Bruh, give me smth to translate").await?;
			return Err(InteractionError::EmptyMessage.into());
		}
	} else {
		ctx.reply("Bruh, give me smth to translate").await?;
		return Err(InteractionError::EmptyMessage.into());
	};
	let target_lang = target.map_or_else(
		|| "en".to_owned(),
		|mut lang| {
			lang.make_ascii_lowercase();
			lang
		},
	);
	let request = TranslateRequest {
		q: &content,
		source: "auto",
		target: &target_lang,
		alternatives: 3,
	};
	let translate_server = utils_config().fabseserver.translate.as_str();

	let response = match HTTP_CLIENT
		.post(translate_server)
		.json(&request)
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Too dangerous to translate").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let data = match response.json::<FabseTranslate>().await {
		Ok(json) => json,
		Err(err) => {
			ctx.reply("Too dangerous to translate").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let mut embed = CreateEmbed::default()
		.title(format!(
			"Translation from {} to {target_lang} with {}% confidence",
			data.detected_language.language, data.detected_language.confidence
		))
		.colour(COLOUR_GREEN)
		.field("Original:", &content, false)
		.field("Translation:", &data.translated_text, false);
	let len = data.alternatives.len();
	if ctx.guild_id().is_some() && len > 1 {
		drop(typing);
		let mut state = State::new(ctx.id(), len);
		let mut final_embed = embed.clone();
		let buttons = [
			CreateButton::new(&state.prev_id)
				.style(ButtonStyle::Primary)
				.label("⬅️"),
			CreateButton::new(&state.next_id)
				.style(ButtonStyle::Primary)
				.label("➡️"),
		];
		let mut action_row = [CreateComponent::ActionRow(CreateActionRow::buttons(
			&buttons[1..],
		))];

		let message = ctx
			.send(
				CreateReply::default()
					.reply(true)
					.embed(embed)
					.components(&action_row),
			)
			.await?;

		let ctx_id_str = ctx.id().to_string();

		let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
			.timeout(Duration::from_mins(1))
			.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
			.stream();

		while let Some(interaction) = collector_stream.next().await {
			interaction
				.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
				.await?;

			if interaction.data.custom_id.ends_with('n')
				&& state.index < state.len.saturating_sub(1)
			{
				state.index = state.index.saturating_add(1);
			} else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
				state.index = state.index.saturating_sub(1);
			}

			embed = CreateEmbed::default()
				.title(format!(
					"Translation from {} to {target_lang} with {}% confidence",
					data.detected_language.language, data.detected_language.confidence
				))
				.colour(COLOUR_GREEN)
				.field("Original:", &content, false)
				.field(
					"Translation:",
					if state.index == 0 {
						&data.translated_text
					} else if let Some(alternative) =
						data.alternatives.get(state.index.saturating_sub(1))
					{
						alternative
					} else {
						"rip"
					},
					false,
				);
			final_embed = embed.clone();

			action_row = [CreateComponent::ActionRow(CreateActionRow::Buttons({
				if state.index == 0 {
					Cow::Borrowed(&buttons[1..])
				} else if state.index == len.saturating_sub(1) {
					Cow::Borrowed(&buttons[..1])
				} else {
					Cow::Borrowed(&buttons)
				}
			}))];

			let mut msg = interaction.message;

			msg.edit(
				ctx.http(),
				EditMessage::default().embed(embed).components(&action_row),
			)
			.await?;
		}
		message
			.edit(
				ctx,
				CreateReply::default()
					.reply(true)
					.embed(final_embed)
					.components(&[]),
			)
			.await?;
	} else {
		ctx.send(CreateReply::default().reply(true).embed(embed))
			.await?;
	}

	Ok(())
}

/// When you stumble on some ancient sayings
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn translate(
	ctx: SContext<'_>,
	#[description = "Language to be translated to, e.g. en"] target: Option<String>,
	#[description = "What should be translated"] sentence: Option<String>,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	translate_internal(ctx, target, sentence).await?;
	Ok(())
}

#[derive(Deserialize)]
struct UrbanResponse {
	#[serde(deserialize_with = "non_empty_vec")]
	list: Vec<UrbanDict>,
}
#[derive(Deserialize)]
struct UrbanDict {
	definition: String,
	example: String,
	word: String,
}

async fn urban_internal(ctx: SContext<'_>, input: &str) -> AResult<()> {
	let typing = ctx.defer_or_broadcast().await;

	let response = match HTTP_CLIENT
		.get("https://api.urbandictionary.com/v0/define")
		.query(&[("term", input)])
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply(format!("**Like you, {input} don't exist**"))
				.await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let data = match response.json::<UrbanResponse>().await {
		Ok(json) => json,
		Err(err) => {
			ctx.reply(format!("**Like you, {input} don't exist**"))
				.await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let first_entry = data.list.first().unwrap();

	let mut text = format!(
		"# {}\n**Definition:**\n{}\n\n**Example:**\n{}",
		first_entry.word,
		first_entry.definition.replace(['[', ']'], ""),
		first_entry.example.replace(['[', ']'], "")
	);
	text.truncate(4000);

	let display = [text_display(&text)];

	let container = CreateContainer::new(&display).accent_colour(Colour::ROHRKATZE_BLUE);

	let len = data.list.len();
	if ctx.guild_id().is_some() && len > 1 {
		drop(typing);
		let mut state = State::new(ctx.id(), len);
		let buttons = [
			CreateButton::new(&state.prev_id)
				.style(ButtonStyle::Primary)
				.label("⬅️"),
			CreateButton::new(&state.next_id)
				.style(ButtonStyle::Primary)
				.label("➡️"),
		];
		let action_row =
			CreateContainerComponent::ActionRow(CreateActionRow::buttons(&buttons[1..]));

		let updated_container = container
			.add_component(separator())
			.add_component(action_row);

		let message = ctx
			.send(
				CreateReply::default()
					.reply(true)
					.flags(MessageFlags::IS_COMPONENTS_V2)
					.components(&[CreateComponent::Container(updated_container)]),
			)
			.await?;

		let ctx_id_str = ctx.id().to_string();

		let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
			.timeout(Duration::from_mins(5))
			.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
			.stream();

		while let Some(interaction) = collector_stream.next().await {
			interaction
				.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
				.await?;

			if interaction.data.custom_id.ends_with('n')
				&& state.index < state.len.saturating_sub(1)
			{
				state.index = state.index.saturating_add(1);
			} else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
				state.index = state.index.saturating_sub(1);
			}

			let (current_word, current_definition, current_example) = data
				.list
				.get(state.index)
				.map(|c| {
					(
						&c.word,
						c.definition.replace(['[', ']'], ""),
						c.example.replace(['[', ']'], ""),
					)
				})
				.unwrap();

			text = format!(
				"# {current_word}\n**Definition:**\n{current_definition}
				\n**Example:**\n{current_example}",
			);
			text.truncate(4000);

			let display = [text_display(&text)];

			let action_row = CreateContainerComponent::ActionRow(CreateActionRow::Buttons({
				if state.index == 0 {
					Cow::Borrowed(&buttons[1..])
				} else if state.index == len.saturating_sub(1) {
					Cow::Borrowed(&buttons[..1])
				} else {
					Cow::Borrowed(&buttons)
				}
			}));

			let updated_container = CreateContainer::new(&display)
				.add_component(separator())
				.add_component(action_row)
				.accent_colour(Colour::ROHRKATZE_BLUE);

			let mut msg = interaction.message;

			msg.edit(
				ctx.http(),
				EditMessage::default()
					.components(&[CreateComponent::Container(updated_container)])
					.flags(MessageFlags::IS_COMPONENTS_V2),
			)
			.await?;
		}
		let display = [text_display(&text)];

		let final_container = CreateContainer::new(&display).accent_colour(Colour::ROHRKATZE_BLUE);

		message
			.edit(
				ctx,
				CreateReply::default()
					.reply(true)
					.components(&[CreateComponent::Container(final_container)]),
			)
			.await?;
	} else {
		ctx.send(
			CreateReply::default()
				.reply(true)
				.components(&[CreateComponent::Container(container)])
				.flags(MessageFlags::IS_COMPONENTS_V2),
		)
		.await?;
	}

	Ok(())
}

/// The holy moly urbandictionary
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn urban(
	ctx: SContext<'_>,
	#[description = "Word(s) to lookup"]
	#[rest]
	input: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	urban_internal(ctx, &input).await?;
	Ok(())
}

async fn waifu_internal(ctx: &SContext<'_>) -> AResult<()> {
	let _typing = ctx.defer_or_broadcast().await;
	ctx.reply(get_waifu(ctx.serenity_context()).await).await?;
	Ok(())
}

/// Do I need to explain it?
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn waifu(ctx: SContext<'_>) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	waifu_internal(&ctx).await?;
	Ok(())
}

#[derive(Deserialize)]
struct WikiResponse {
	#[serde(deserialize_with = "non_empty_string")]
	title: String,
	extract: String,
	originalimage: Option<WikiImage>,
	content_urls: WikiType,
}

#[derive(Deserialize)]
struct WikiImage {
	source: String,
}

#[derive(Deserialize)]
struct WikiType {
	desktop: WikiUrl,
}

#[derive(Deserialize)]
struct WikiUrl {
	page: String,
}

async fn wiki_internal(ctx: &SContext<'_>, input: &str) -> AResult<()> {
	let _typing = ctx.defer_or_broadcast().await;
	let request_url = {
		let encoded_input: String = byte_serialize(input.as_bytes()).collect();
		format!("https://en.wikipedia.org/api/rest_v1/page/summary/{encoded_input}")
	};

	let request = match HTTP_CLIENT.get(request_url).send().await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Wikipedia not responding :/").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let data = match request.json::<WikiResponse>().await {
		Ok(data) => data,
		Err(err) => {
			ctx.reply(format!("**Like you, {input} don't exist**"))
				.await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let button = visit_page_button(&data.content_urls.desktop.page);

	let text = format!("# {}\n{}", data.title, data.extract);

	let image = data.originalimage.map_or_else(|| "https://upload.wikimedia.org/wikipedia/en/thumb/8/80/Wikipedia-logo-v2.svg/3840px-Wikipedia-logo-v2.svg.png".to_owned(), |i| i.source);

	let thumbnail_section = [thumbnail_section(&text, &image)];

	let container = CreateContainer::new(&thumbnail_section)
		.add_component(button)
		.accent_colour(Colour::DARK_GOLD);

	send_container(ctx, container).await?;

	Ok(())
}

/// The holy moly... wikipedia?
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn wiki(
	ctx: SContext<'_>,
	#[description = "Topic to lookup"]
	#[rest]
	input: String,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	wiki_internal(&ctx, &input).await?;
	Ok(())
}
