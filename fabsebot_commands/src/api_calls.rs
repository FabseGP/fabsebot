use std::{borrow::Cow, fmt::Write as _, time::Duration};

use anyhow::Result as AResult;
use base64::{Engine as _, engine::general_purpose};
use fabsebot_core::{
	config::{
		constants::MESSAGE_LIMIT,
		types::{AIChatMessage, Error, HTTP_CLIENT, SContext, utils_config},
	},
	errors::commands::{AIError, Base64Error},
	utils::{
		ai::{ContentPart, ai_response, ai_response_with_tools, image_content, uri_content},
		helpers::{
			UserType, banner_vec, fetch_and_parse, get_gifs, get_waifu, media_gallery,
			non_empty_string, non_empty_vec, paginate_container, reply_container, text_display,
			thumbnail_section, true_bool, url_bytes, visit_page_button,
		},
	},
};
use poise::CreateReply;
use reqwest::multipart::Form;
use serde::{Deserialize, Serialize};
use serenity::{
	all::{Attachment, Colour, CreateAttachment, CreateContainer, Member, MessageId, User},
	builder::{
		CreateActionRow, CreateComponent, CreateContainerComponent, CreateMediaGallery,
		CreateSection,
	},
	futures::StreamExt as _,
	model::id::UserId,
};
use sqlx::query_scalar;
use url::form_urlencoded::byte_serialize;

use crate::command_permissions;

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

async fn fetch_and_decode_image(prompt: String) -> AResult<Vec<u8>> {
	let utils_config = utils_config();

	let form = Form::new()
		.text("prompt", prompt)
		.text("steps", "25")
		.text("width", "512")
		.text("height", "512");

	let resp_parsed: FabseAIImage = fetch_and_parse(
		HTTP_CLIENT
			.post(&utils_config.api.cloudflare_image_gen)
			.bearer_auth(&utils_config.api.cloudflare_token)
			.multipart(form)
			.send(),
	)
	.await?;

	let img_dec = general_purpose::STANDARD
		.decode(&resp_parsed.result.image)
		.map_err(Base64Error::FailedBytesDecode)?;

	Ok(img_dec)
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
	let _typing = ctx.defer_or_broadcast().await;

	match fetch_and_decode_image(prompt.clone()).await {
		Ok(bytes) => {
			ctx.send(
				CreateReply::new()
					.reply(true)
					.attachment(CreateAttachment::bytes(bytes, "output.png")),
			)
			.await?;
		}
		Err(err) => {
			ctx.reply(format!("\"{prompt}\" is too dangerous to generate"))
				.await?;
			return Err(err);
		}
	}
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
	#[description = "Prompt"] prompt: String,
	#[description = "Optional image for context (only JPEG, WEBP and AVIF are supported)"]
	attachment: Option<Attachment>,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	ctx.defer().await?;

	let user_message = if let Some(attachment) = attachment
		&& let Some(content_type) = attachment.content_type.as_deref()
		&& content_type.starts_with("image")
		&& let Ok(bytes) = url_bytes(&attachment.url).await
	{
		let mut chat_vec = Vec::with_capacity(2);
		match image_content(&mut chat_vec, &bytes) {
			Ok(()) => {
				chat_vec.push(ContentPart::Text {
					text: Cow::Owned(prompt.clone()),
				});
				AIChatMessage::user_parts(chat_vec)
			}
			Err(err) => {
				ctx.reply("Why you give me an invalid image format >:(")
					.await?;
				return Err(err);
			}
		}
	} else {
		AIChatMessage::user_text(Cow::Owned(prompt.clone()))
	};

	let mut messages = vec![AIChatMessage::system(Cow::Owned(role)), user_message];

	let resp = match ai_response_with_tools(
		&mut messages,
		ctx.guild_id().unwrap(),
		None,
		&utils_config().fabseserver.text_model_large,
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

	let mut text = String::with_capacity(prompt.len().saturating_add(resp.len()).saturating_add(8));
	write!(text, "# {prompt}\n{resp}")?;
	text.truncate(MESSAGE_LIMIT);

	let text_display = [text_display(&text)];
	let container = CreateContainer::new(&text_display).accent_colour(Colour::RED);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

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
	title: String,
	#[serde(rename = "type")]
	media_type: String,
	status: String,
	score: Option<f32>,
	popularity: Option<i32>,
	favorites: Option<i32>,
	synopsis: Option<String>,
	#[serde(deserialize_with = "non_empty_vec")]
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
struct AniMangaAired {
	#[serde(rename = "string")]
	aired_string: Option<String>,
}

#[derive(Deserialize)]
struct AniMangaGenres {
	name: String,
}

impl<T> AniManga<T> {
	fn description(&self, output: &mut String) {
		write!(
			output,
			"# {}\n**Format:** {}\n**Status:** {}\n**Genres:** ",
			self.title, self.media_type, self.status
		)
		.unwrap();

		for genre in self
			.genres
			.iter()
			.map(|g| g.name.as_str())
			.intersperse(" - ")
		{
			output.push_str(genre);
		}

		output.push('\n');

		if let Some(description) = &self.synopsis {
			writeln!(output, "**Description:**\n{description}").unwrap();
		}

		if let Some(score) = self.score {
			writeln!(output, "**Score:** {score}").unwrap();
		}

		if let Some(popularity) = self.popularity {
			writeln!(output, "**Popularity:** {popularity}").unwrap();
		}

		if let Some(favorites) = self.favorites {
			writeln!(output, "**Favorites:** {favorites}").unwrap();
		}
	}
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
	let typing = ctx.defer_or_broadcast().await;

	let json: AniMangaResponse<AnimeSpecific> = match fetch_and_parse(
		HTTP_CLIENT
			.get("https://api.tenrai.org/v1/anime")
			.query(&[("q", anime.as_str()), ("limit", "5")])
			.send(),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Not worthy of looking up").await?;
			return Err(err);
		}
	};

	drop(typing);

	paginate_container(
		ctx,
		&json.data,
		Duration::from_mins(1),
		|entry, _idx, _len| async move {
			let mut text = String::with_capacity(512);
			entry.description(&mut text);
			if let Some(episodes) = entry.specific.episodes {
				writeln!(text, "**Episodes:** {episodes}").unwrap();
			}
			if let Some(duration) = &entry.specific.duration {
				writeln!(text, "**Duration:** {duration}").unwrap();
			}
			if let Some(aired) = &entry.specific.aired.aired_string {
				writeln!(text, "**Aired:** {aired}").unwrap();
			}
			text.truncate(MESSAGE_LIMIT);
			let (text, thumbnail) = thumbnail_section(text, &entry.images.webp.image_url);
			let text_array = vec![text];
			let thumbnail_display = vec![CreateContainerComponent::Section(CreateSection::new(
				text_array, thumbnail,
			))];
			let button = visit_page_button(&entry.url);
			CreateContainer::new(thumbnail_display)
				.add_component(CreateContainerComponent::ActionRow(
					CreateActionRow::Buttons(Cow::Owned(vec![button])),
				))
				.accent_colour(Colour::ORANGE)
		},
	)
	.await?;

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
	episode: Option<f32>,
	from: f32,
	to: f32,
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
	let _typing = ctx.defer_or_broadcast().await;
	let scene: MoeResponse = match fetch_and_parse(
		HTTP_CLIENT
			.get("https://api.trace.moe/search?cutBorders&anilistInfo")
			.query(&[("url", input)])
			.send(),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Not worthy of looking up").await?;
			return Err(err);
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
		first_result.episode.unwrap_or(0.0),
		first_result.from,
		first_result.to
	);

	let text_display = [text_display(&text)];
	let media = [media_gallery(&first_result.video)];

	let container = CreateContainer::new(&text_display)
		.add_component(CreateContainerComponent::MediaGallery(
			CreateMediaGallery::new(&media),
		))
		.accent_colour(Colour::BLUE);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}

#[derive(Deserialize)]
struct EightBallResponse {
	#[serde(deserialize_with = "non_empty_string")]
	reading: String,
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
	let _typing = ctx.defer_or_broadcast().await;

	let judging: EightBallResponse = match fetch_and_parse(
		HTTP_CLIENT
			.get("https://eightballapi.com/api/biased")
			.query(&[("question", question.as_str()), ("lucky", "false")])
			.send(),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Sometimes riding a giraffe is what you need")
				.await?;
			return Err(err);
		}
	};

	let text = format!("# {question}\n{}", judging.reading);
	let text_display = [text_display(&text)];

	let container = CreateContainer::new(&text_display).accent_colour(Colour::ORANGE);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

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
	let typing = ctx.defer_or_broadcast().await;
	let gifs = get_gifs(&input).await;

	drop(typing);

	paginate_container(
		ctx,
		&gifs,
		Duration::from_mins(1),
		|entry, _idx, _len| async move {
			let text = format!("# {}", entry.1);
			let display = vec![text_display(text)];
			let image = vec![media_gallery(&*entry.0)];
			CreateContainer::new(display)
				.add_component(CreateContainerComponent::MediaGallery(
					CreateMediaGallery::new(image),
				))
				.accent_colour(Colour::ORANGE)
		},
	)
	.await?;

	Ok(())
}

const ROASTS: &[&str] = &[
	"your life",
	"you're not funny",
	"you",
	"get a life bitch",
	"I don't like you",
	"you smell",
	"look at the mirror",
];

#[derive(Deserialize)]
struct JokeResponse {
	#[serde(deserialize_with = "non_empty_string")]
	joke: String,
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
	let request_url =
		"https://api.humorapi.com/jokes/random?api-key=48c239c85f804a0387251d9b3587fa2c";

	match fetch_and_parse::<JokeResponse>(HTTP_CLIENT.get(request_url).send()).await {
		Ok(data) => {
			ctx.reply(&data.joke).await?;
		}
		Err(err) => {
			let index = fastrand::usize(..ROASTS.len());
			ctx.reply(*ROASTS.get(index).unwrap()).await?;
			return Err(err);
		}
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

	let typing = ctx.defer_or_broadcast().await;

	let json: AniMangaResponse<MangaSpecific> = match fetch_and_parse(
		HTTP_CLIENT
			.get("https://api.tenrai.org/v1/manga")
			.query(&[("q", manga.as_str()), ("limit", "5")])
			.send(),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Not worthy of looking up").await?;
			return Err(err);
		}
	};

	drop(typing);

	paginate_container(
		ctx,
		&json.data,
		Duration::from_mins(1),
		|entry, _idx, _len| async move {
			let mut text = String::with_capacity(512);
			entry.description(&mut text);
			if let Some(chapters) = entry.specific.chapters {
				writeln!(text, "**Chapters:** {chapters}").unwrap();
			}
			if let Some(volumes) = &entry.specific.volumes {
				writeln!(text, "**Volumes:** {volumes}").unwrap();
			}
			if let Some(published) = &entry.specific.published.aired_string {
				writeln!(text, "**Published:** {published}").unwrap();
			}
			text.truncate(MESSAGE_LIMIT);
			let (text, thumbnail) = thumbnail_section(text, &entry.images.webp.image_url);
			let text_array = vec![text];
			let thumbnail_display = vec![CreateContainerComponent::Section(CreateSection::new(
				text_array, thumbnail,
			))];
			let button = visit_page_button(&entry.url);
			CreateContainer::new(thumbnail_display)
				.add_component(CreateContainerComponent::ActionRow(
					CreateActionRow::Buttons(Cow::Owned(vec![button])),
				))
				.accent_colour(Colour::ORANGE)
		},
	)
	.await?;

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
	let request_url = {
		let encoded_left: String = byte_serialize(top_left.as_bytes()).collect();
		let encoded_right: String = byte_serialize(top_right.as_bytes()).collect();
		let encoded_bottom: String = byte_serialize(bottom.as_bytes()).collect();
		format!("https://api.memegen.link/images/exit/{encoded_left}/{encoded_right}/{encoded_bottom}.png")
	};
	ctx.reply(request_url).await?;

	Ok(())
}

async fn roast_internal(
	ctx: &SContext<'_>,
	user_message: AIChatMessage,
	id: UserId,
) -> AResult<()> {
	let role = Cow::Borrowed(
		"you're an evil ai assistant that excels at roasting ppl. no mercy shown. the prompt will \
		 contain information of your target",
	);

	let messages = [AIChatMessage::system(role), user_message];

	let resp = match ai_response(&messages, &utils_config().fabseserver.text_model_small).await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply(format!("<@{id}>'s life is already roasted"))
				.await?;
			return Err(AIError::UnexpectedResponse(err).into());
		}
	};

	let mut text = String::with_capacity(resp.len().saturating_add(64));
	write!(text, "# Roasting <@{id}>\n{resp}")?;
	text.truncate(MESSAGE_LIMIT);

	let text_display = [text_display(&text)];

	let container = CreateContainer::new(&text_display).accent_colour(Colour::RED);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

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

	let mut chat_vec = banner_vec(&ctx, UserType::User(&user)).await?;
	uri_content(&user.face(), &mut chat_vec).await?;

	let name = &user.name;
	let account_date = user.id.created_at();

	let description = format!("name:{name},acc_create:{account_date}");

	chat_vec.push(ContentPart::Text {
		text: Cow::Owned(description),
	});

	roast_internal(&ctx, AIChatMessage::user_parts(chat_vec), user.id).await?;

	Ok(())
}

/// When someone offended you
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | \
	                            READ_MESSAGE_HISTORY"
)]
pub async fn roast(
	ctx: SContext<'_>,
	#[description = "Target"] member: Member,
) -> Result<(), Error> {
	let avatar_url = member.face();
	let _typing = ctx.defer_or_broadcast().await;

	let message_count = query_scalar!(
		r#"
		SELECT message_count
		FROM user_settings
		WHERE guild_id = $1
			AND user_id = $2
		"#,
		i64::from(ctx.guild_id().unwrap()),
		i64::from(ctx.author().id)
	)
	.fetch_one(&ctx.data().db)
	.await?;

	let mut chat_vec = banner_vec(&ctx, UserType::Member(&member)).await?;
	uri_content(&avatar_url, &mut chat_vec).await?;

	let mut description = String::with_capacity(2048);

	let name = member.display_name();

	let is_owner = if ctx.guild().unwrap().owner_id == member.user.id {
		""
	} else {
		"not "
	};

	write!(
		description,
		"name:{name}({}owner of this server),msg_count:{message_count},acc_create:{},last_msgs:",
		is_owner,
		member.user.id.created_at(),
	)?;

	let mut messages = ctx.channel_id().messages_iter(&ctx).boxed();

	let mut result_count: u32 = 0;
	let mut missing_match_count: u32 = 0;

	while let Some(message_result) = messages.next().await {
		if let Ok(message) = message_result {
			if message.author.id == member.user.id {
				write!(description, "{result_count}:{},", message.content)?;
				result_count = result_count.saturating_add(1);
			} else {
				missing_match_count = missing_match_count.saturating_add(1);
			}
		} else {
			break;
		}
		if result_count >= 100 || missing_match_count >= 100 {
			break;
		}
	}

	if let Some(joined_at) = member.joined_at {
		write!(description, "joined_svr:{joined_at},")?;
	}

	if let Some(roles) = member.roles(ctx.cache()) {
		description.push_str("roles:");
		for role in roles.iter().map(|r| r.name.as_str()).intersperse(", ") {
			description.push_str(role);
		}
	}

	chat_vec.push(ContentPart::Text {
		text: Cow::Owned(description),
	});

	roast_internal(&ctx, AIChatMessage::user_parts(chat_vec), member.user.id).await?;

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
			return Ok(());
		}
	} else {
		ctx.reply("Bruh, give me smth to translate").await?;
		return Ok(());
	};

	let target_lang = target.map_or(Cow::Borrowed("en"), |mut lang| {
		lang.make_ascii_lowercase();
		Cow::Owned(lang)
	});
	let request = TranslateRequest {
		q: &content,
		source: "auto",
		target: &target_lang,
		alternatives: 3,
	};
	let translate_server = utils_config().fabseserver.translate.as_str();

	let data: FabseTranslate =
		match fetch_and_parse(HTTP_CLIENT.post(translate_server).json(&request).send()).await {
			Ok(resp) => resp,
			Err(err) => {
				ctx.reply("Too dangerous to translate").await?;
				return Err(err);
			}
		};

	drop(typing);

	let target_lang_clone = &target_lang;
	let content_clone = &content;
	let language_clone = &data.detected_language.language;
	let translation_clone = &data.translated_text;

	paginate_container(
		ctx,
		&data.alternatives,
		Duration::from_mins(1),
		|entry, idx, _len| async move {
			let translation = if idx == 0 { translation_clone } else { entry };
			let mut text = String::with_capacity(
				translation
					.len()
					.saturating_add(content_clone.len())
					.saturating_add(target_lang_clone.len())
					.saturating_add(language_clone.len())
					.saturating_add(64),
			);
			write!(
				text,
				"# Translation from {} to {} with {}% \
				 confidence\n**Original:**\n{}\n**Translation:**\n{}",
				target_lang_clone,
				language_clone,
				data.detected_language.confidence,
				content_clone,
				translation
			)
			.unwrap();
			text.truncate(MESSAGE_LIMIT);
			let display = vec![text_display(text)];
			CreateContainer::new(display).accent_colour(Colour::DARK_GREEN)
		},
	)
	.await?;

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
	let typing = ctx.defer_or_broadcast().await;

	let data: UrbanResponse = match fetch_and_parse(
		HTTP_CLIENT
			.get("https://api.urbandictionary.com/v0/define")
			.query(&[("term", &input)])
			.send(),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply(format!("**Like you, {input} don't exist**"))
				.await?;
			return Err(err);
		}
	};

	drop(typing);

	paginate_container(
		ctx,
		&data.list,
		Duration::from_mins(5),
		|entry, _idx, _len| async move {
			let definition_clean = entry.definition.replace(['[', ']'], "");
			let example_clean = entry.example.replace(['[', ']'], "");
			let mut text = String::with_capacity(
				entry
					.word
					.len()
					.saturating_add(definition_clean.len())
					.saturating_add(example_clean.len())
					.saturating_add(32),
			);
			write!(
				text,
				"# {}\n**Definition:**\n{}\n\n**Example:**\n{}",
				entry.word,
				entry.definition.replace(['[', ']'], ""),
				entry.example.replace(['[', ']'], "")
			)
			.unwrap();
			text.truncate(MESSAGE_LIMIT);
			let display = vec![text_display(text)];
			CreateContainer::new(display).accent_colour(Colour::ROHRKATZE_BLUE)
		},
	)
	.await?;

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
	let _typing = ctx.defer_or_broadcast().await;
	ctx.reply(get_waifu().await).await?;

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
	let _typing = ctx.defer_or_broadcast().await;
	let request_url = {
		let encoded_input: String = byte_serialize(input.as_bytes()).collect();
		format!("https://en.wikipedia.org/api/rest_v1/page/summary/{encoded_input}")
	};

	let data: WikiResponse = match fetch_and_parse(HTTP_CLIENT.get(request_url).send()).await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply(format!("**Like you, {input} don't exist**"))
				.await?;
			return Err(err);
		}
	};

	let button = [visit_page_button(&data.content_urls.desktop.page)];
	let text = format!("# {}\n{}", data.title, data.extract);
	let image = data.originalimage.as_ref().map_or_else(|| "https://upload.wikimedia.org/wikipedia/en/thumb/8/80/Wikipedia-logo-v2.svg/3840px-Wikipedia-logo-v2.svg.png", |i| i.source.as_str());

	let (text, thumbnail) = thumbnail_section(text, image);
	let text_array = [text];
	let thumbnail_display = [CreateContainerComponent::Section(CreateSection::new(
		&text_array,
		thumbnail,
	))];

	let container = CreateContainer::new(&thumbnail_display)
		.add_component(CreateContainerComponent::ActionRow(
			CreateActionRow::Buttons(Cow::Borrowed(&button)),
		))
		.accent_colour(Colour::DARK_GOLD);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}
