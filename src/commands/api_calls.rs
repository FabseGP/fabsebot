use crate::{
    config::{
        constants::{COLOUR_BLUE, COLOUR_GREEN, COLOUR_ORANGE, COLOUR_RED, COLOUR_YELLOW},
        settings::UserSettings,
        types::{Error, HTTP_CLIENT, RNG, SContext, UTILS_CONFIG},
    },
    utils::{
        ai::ai_response_simple,
        helpers::{get_gifs, get_waifu},
    },
};

use base64::{Engine as _, engine::general_purpose};
use core::fmt::{Display, Formatter, Result as FmtResult};
use poise::{
    CreateReply,
    serenity_prelude::{
        ButtonStyle, ComponentInteractionCollector, CreateActionRow, CreateAttachment,
        CreateButton, CreateEmbed, CreateInteractionResponse, EditMessage, Member, MessageId,
        futures::StreamExt as _,
    },
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use urlencoding::encode;

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

/// Did someone say AI image?
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn ai_image(
    ctx: SContext<'_>,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let rand_num = RNG.lock().await.usize(..1024);
    let request = ImageRequest {
        prompt: format!("{prompt} {rand_num}"),
    };
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let resp = HTTP_CLIENT
        .post(&utils_config.ai.image_gen)
        .bearer_auth(&utils_config.ai.token)
        .json(&request)
        .send()
        .await?;

    if let Ok(resp_parsed) = resp.json::<FabseAIImage>().await
        && resp_parsed.success
    {
        ctx.send(
            CreateReply::default()
                .reply(true)
                .attachment(CreateAttachment::bytes(
                    general_purpose::STANDARD
                        .decode(resp_parsed.result.image)
                        .unwrap(),
                    "output.png",
                )),
        )
        .await?;
    } else {
        let resp = HTTP_CLIENT
            .post(&utils_config.ai.image_gen_fallback)
            .bearer_auth(&utils_config.ai.token_fallback)
            .json(&request)
            .send()
            .await?;
        if let Ok(resp_parsed) = resp.json::<FabseAIImage>().await
            && resp_parsed.success
        {
            ctx.send(
                CreateReply::default()
                    .reply(true)
                    .attachment(CreateAttachment::bytes(
                        general_purpose::STANDARD
                            .decode(resp_parsed.result.image)
                            .unwrap(),
                        "output.png",
                    )),
            )
            .await?;
        } else {
            ctx.reply(format!("\"{prompt}\" is too dangerous to generate"))
                .await?;
        }
    }

    Ok(())
}

#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn ai_text(
    ctx: SContext<'_>,
    #[description = "AI personality, e.g. *you're an evil assistant*"] role: String,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    match ai_response_simple(&role, &prompt).await {
        Some(resp) if !resp.is_empty() => {
            let mut embed = CreateEmbed::default().title(prompt).colour(COLOUR_RED);
            let mut current_chunk = String::with_capacity(1024);
            let mut chunk_index = 0;
            for ch in resp.chars() {
                if current_chunk.len() >= 1024 {
                    let field_name = if chunk_index == 0 {
                        "Response:".to_owned()
                    } else {
                        format!("Response (cont. {}):", chunk_index + 1)
                    };
                    embed = embed.field(field_name, current_chunk, false);
                    current_chunk = String::with_capacity(1024);
                    chunk_index += 1;
                }
                current_chunk.push(ch);
            }
            if !current_chunk.is_empty() {
                let field_name = if chunk_index == 0 {
                    "Response:".to_owned()
                } else {
                    format!("Response (cont. {}):", chunk_index + 1)
                };
                embed = embed.field(field_name, current_chunk, false);
            }
            ctx.send(CreateReply::default().embed(embed)).await?;
        }
        Some(_) | None => {
            ctx.reply(format!("\"{prompt}\" is too dangerous to ask"))
                .await?;
        }
    }

    Ok(())
}

#[derive(Deserialize)]
struct AniMangaResponse<T> {
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

/// Lookup anime when the other bot sucks (MAL-edition)
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn anime(
    ctx: SContext<'_>,
    #[description = "Anime to search"]
    #[rest]
    anime: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let request_url = {
        let encoded_input = &encode(&anime);
        format!("https://api.jikan.moe/v4/anime?q={encoded_input}&limit=5")
    };
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(resp) => match resp.json::<AniMangaResponse<AnimeSpecific>>().await {
            Ok(data) if !data.data.is_empty() => {
                let empty = String::new();
                let japanese_title = data.data[0]
                    .titles
                    .iter()
                    .find(|t| t.title_type == "Japanese")
                    .map_or("No japanese title available", |t| t.title.as_str());
                let mut embed = CreateEmbed::default()
                    .title(japanese_title)
                    .thumbnail(&data.data[0].images.webp.image_url)
                    .url(&data.data[0].url)
                    .colour(COLOUR_ORANGE);
                if let Some(synopsis) = &data.data[0].synopsis {
                    embed = embed.description(format!("*{synopsis}*"));
                }
                embed = embed.field("Format", &data.data[0].anime_type, true);
                embed = embed.field("Status", &data.data[0].status, true);
                if let Some(english_title) = data.data[0]
                    .titles
                    .iter()
                    .find(|t| t.title_type == "English")
                    .map(|t| &t.title)
                {
                    embed = embed.field("English title", english_title, true);
                }
                embed = embed.field("", &empty, false);
                if let Some(score) = &data.data[0].score {
                    embed = embed.field("Score", score.to_string(), true);
                }
                if let Some(popularity) = &data.data[0].popularity {
                    embed = embed.field("Popularity", popularity.to_string(), true);
                }
                if let Some(favorites) = &data.data[0].favorites {
                    embed = embed.field("Favorites", favorites.to_string(), true);
                }
                embed = embed.field("", &empty, false);
                if let Some(episodes) = &data.data[0].specific.episodes {
                    embed = embed.field("Episodes", episodes.to_string(), true);
                }
                if let Some(duration) = &data.data[0].specific.duration {
                    embed = embed.field("Duration", duration, true);
                }
                if let Some(aired) = &data.data[0].specific.aired.aired_string {
                    embed = embed.field("Aired", aired, true);
                }
                let genres_string = &data.data[0]
                    .genres
                    .iter()
                    .map(|genre| genre.name.as_str())
                    .intersperse(" - ")
                    .collect::<String>();
                embed = embed.field("Genres", genres_string, false);
                let len = data.data.len();
                if ctx.guild_id().is_some() && len > 1 {
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
                    let action_row = [CreateActionRow::buttons(&buttons[1..])];

                    let message = ctx
                        .send(
                            CreateReply::default()
                                .reply(true)
                                .embed(embed)
                                .components(&action_row),
                        )
                        .await?;

                    let ctx_id_copy = ctx.id();
                    while let Some(interaction) =
                        ComponentInteractionCollector::new(ctx.serenity_context())
                            .timeout(Duration::from_secs(60))
                            .filter(move |interaction| {
                                interaction
                                    .data
                                    .custom_id
                                    .starts_with(ctx_id_copy.to_string().as_str())
                            })
                            .await
                    {
                        interaction
                            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                            .await?;

                        if interaction.data.custom_id.ends_with('n') && state.index < state.len - 1
                        {
                            state.index += 1;
                        } else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
                            state.index -= 1;
                        }

                        let japanese_title = data.data[state.index]
                            .titles
                            .iter()
                            .find(|t| t.title_type == "Japanese")
                            .map_or("No japanese title available", |t| t.title.as_str());
                        let mut new_embed = CreateEmbed::default()
                            .title(japanese_title)
                            .thumbnail(&data.data[state.index].images.webp.image_url)
                            .url(&data.data[state.index].url)
                            .colour(COLOUR_ORANGE);

                        if let Some(synopsis) = &data.data[state.index].synopsis {
                            new_embed = new_embed.description(format!("*{synopsis}*"));
                        }
                        new_embed =
                            new_embed.field("Format", &data.data[state.index].anime_type, true);
                        new_embed = new_embed.field("Status", &data.data[state.index].status, true);
                        if let Some(english_title) = data.data[state.index]
                            .titles
                            .iter()
                            .find(|t| t.title_type == "English")
                            .map(|t| &t.title)
                        {
                            new_embed = new_embed.field("English title", english_title, true);
                        }
                        new_embed = new_embed.field("", &empty, false);
                        if let Some(score) = &data.data[state.index].score {
                            new_embed = new_embed.field("Score", score.to_string(), true);
                        }
                        if let Some(popularity) = &data.data[state.index].popularity {
                            new_embed = new_embed.field("Popularity", popularity.to_string(), true);
                        }
                        if let Some(favorites) = &data.data[state.index].favorites {
                            new_embed = new_embed.field("Favorites", favorites.to_string(), true);
                        }
                        new_embed = new_embed.field("", &empty, false);
                        if let Some(episodes) = &data.data[state.index].specific.episodes {
                            new_embed = new_embed.field("Episodes", episodes.to_string(), true);
                        }
                        if let Some(duration) = &data.data[state.index].specific.duration {
                            new_embed = new_embed.field("Duration", duration, true);
                        }
                        if let Some(aired) = &data.data[state.index].specific.aired.aired_string {
                            new_embed = new_embed.field("Aired", aired, true);
                        }
                        let genres_string = data.data[state.index]
                            .genres
                            .iter()
                            .map(|genre| genre.name.as_str())
                            .intersperse(" - ")
                            .collect::<String>();
                        new_embed = new_embed.field("Genres", genres_string, false);
                        final_embed = new_embed.clone();

                        let new_components = {
                            if state.index == 0 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[1..]))]
                            } else if state.index == len - 1 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[..1]))]
                            } else {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons))]
                            }
                        };

                        let mut msg = interaction.message;

                        msg.edit(
                            ctx.http(),
                            EditMessage::default()
                                .embed(new_embed)
                                .components(&new_components),
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
            }
            Ok(_) | Err(_) => {
                ctx.reply("Not worthy of looking up").await?;
            }
        },
        Err(_) => {
            ctx.reply("API down, get a life!").await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct MoeResponse {
    result: Vec<AnimeScene>,
}

#[derive(Deserialize)]
struct AnimeScene {
    anilist: Anilist,
    episode: Option<i32>,
    from: Option<f32>,
    to: Option<f32>,
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

/// What anime was that scene from?
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn anime_scene(
    ctx: SContext<'_>,
    #[description = "Link to anime image"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let request_url = {
        let encoded_input = encode(&input);
        format!("https://api.trace.moe/search?cutBorders&anilistInfo&url={encoded_input}")
    };
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(response) => match response.json::<MoeResponse>().await {
            Ok(scene) => {
                if let Some(first_result) = scene.result.first() {
                    if first_result.video.is_empty() {
                        ctx.reply("No matching scene found").await?;
                        return Ok(());
                    }
                    let episode_text = first_result.episode.unwrap_or(0).to_string();
                    let title = first_result
                        .anilist
                        .title
                        .english
                        .as_deref()
                        .unwrap_or("Unknown title");
                    ctx.send(
                        CreateReply::default()
                            .embed(
                                CreateEmbed::default()
                                    .title(title)
                                    .field("Episode", episode_text, true)
                                    .field(
                                        "From",
                                        first_result.from.unwrap_or(0.0).to_string(),
                                        true,
                                    )
                                    .field("To", first_result.to.unwrap_or(0.0).to_string(), true)
                                    .colour(COLOUR_BLUE),
                            )
                            .reply(true),
                    )
                    .await?;
                    ctx.reply(&first_result.video).await?;
                } else {
                    ctx.reply("No results found").await?;
                }
            }
            Err(_) => {
                ctx.reply("Failed to parse the response").await?;
            }
        },
        Err(_) => {
            ctx.reply("Oof, anime-server down!").await?;
        }
    }

    Ok(())
}

#[derive(Deserialize)]
struct EightBallResponse {
    reading: String,
}

/// When you need a wise opinion
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn eightball(
    ctx: SContext<'_>,
    #[description = "Your question"]
    #[rest]
    question: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let request_url = {
        let encoded_input = encode(&question);
        format!("https://eightballapi.com/api/biased?question={encoded_input}&lucky=false")
    };
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(request) => match request.json::<EightBallResponse>().await {
            Ok(judging) if !judging.reading.is_empty() => {
                ctx.send(
                    CreateReply::default()
                        .embed(
                            CreateEmbed::default()
                                .title(question)
                                .colour(COLOUR_ORANGE)
                                .field("", &judging.reading, true),
                        )
                        .reply(true),
                )
                .await?;
            }
            Ok(_) | Err(_) => {
                ctx.reply("Sometimes riding a giraffe is what you need")
                    .await?;
            }
        },
        Err(_) => {
            ctx.reply("Sometimes riding a giraffe is what you need")
                .await?;
        }
    }
    Ok(())
}

/// Gifing
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn gif(
    ctx: SContext<'_>,
    #[description = "Search gif"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let urls = get_gifs(&input).await;
    let embed = CreateEmbed::default()
        .title(input.as_str())
        .image(urls[0].clone())
        .colour(COLOUR_ORANGE);
    let len = urls.len();
    if ctx.guild_id().is_some() && len > 1 {
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
        let action_row = [CreateActionRow::buttons(&buttons[1..])];

        let message = ctx
            .send(
                CreateReply::default()
                    .reply(true)
                    .embed(embed)
                    .components(&action_row),
            )
            .await?;

        let ctx_id_copy = ctx.id();
        while let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
            .timeout(Duration::from_secs(60))
            .filter(move |interaction| {
                interaction
                    .data
                    .custom_id
                    .starts_with(ctx_id_copy.to_string().as_str())
            })
            .await
        {
            interaction
                .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                .await?;

            if interaction.data.custom_id.ends_with('n') && state.index < state.len - 1 {
                state.index += 1;
            } else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
                state.index -= 1;
            }

            let new_embed = CreateEmbed::default()
                .title(input.as_str())
                .image(urls[state.index].clone())
                .colour(COLOUR_ORANGE);
            final_embed = new_embed.clone();

            let new_components = {
                if state.index == 0 {
                    [CreateActionRow::Buttons(Cow::Borrowed(&buttons[1..]))]
                } else if state.index == len - 1 {
                    [CreateActionRow::Buttons(Cow::Borrowed(&buttons[..1]))]
                } else {
                    [CreateActionRow::Buttons(Cow::Borrowed(&buttons))]
                }
            };

            let mut msg = interaction.message;

            msg.edit(
                ctx.http(),
                EditMessage::default()
                    .embed(new_embed)
                    .components(&new_components),
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

#[derive(Deserialize)]
struct JokeResponse {
    joke: String,
}

/// When your life isn't fun anymore
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn joke(ctx: SContext<'_>) -> Result<(), Error> {
    let request_url =
        "https://api.humorapi.com/jokes/random?api-key=48c239c85f804a0387251d9b3587fa2c";
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(request) => match request.json::<JokeResponse>().await {
            Ok(data) if !data.joke.is_empty() => {
                ctx.reply(&data.joke).await?;
            }
            Ok(_) | Err(_) => {
                let roasts = [
                    "your life",
                    "you're not funny",
                    "you",
                    "get a life bitch",
                    "I don't like you",
                    "you smell",
                ];
                let index = RNG.lock().await.usize(..roasts.len());
                if let Some(roast) = roasts.get(index).copied() {
                    ctx.reply(roast).await?;
                }
            }
        },
        Err(_) => {
            ctx.reply("no jokes now").await?;
        }
    }
    Ok(())
}

/// Lookup manga when the other bot sucks (MAL-edition)
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn manga(
    ctx: SContext<'_>,
    #[description = "Manga to search"]
    #[rest]
    manga: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let request_url = {
        let encoded_input = &encode(&manga);
        format!("https://api.jikan.moe/v4/manga?q={encoded_input}&limit=5")
    };
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(resp) => match resp.json::<AniMangaResponse<MangaSpecific>>().await {
            Ok(data) if !data.data.is_empty() => {
                let empty = String::new();
                let japanese_title = data.data[0]
                    .titles
                    .iter()
                    .find(|t| t.title_type == "Japanese")
                    .map_or("No japanese title available", |t| t.title.as_str());
                let mut embed = CreateEmbed::default()
                    .title(japanese_title)
                    .thumbnail(&data.data[0].images.webp.image_url)
                    .url(&data.data[0].url)
                    .colour(COLOUR_ORANGE);
                if let Some(synopsis) = &data.data[0].synopsis {
                    embed = embed.description(format!("*{synopsis}*"));
                }
                embed = embed.field("Format", &data.data[0].anime_type, true);
                embed = embed.field("Status", &data.data[0].status, true);
                if let Some(english_title) = data.data[0]
                    .titles
                    .iter()
                    .find(|t| t.title_type == "English")
                    .map(|t| &t.title)
                {
                    embed = embed.field("English title", english_title, true);
                }
                embed = embed.field("", &empty, false);
                if let Some(score) = &data.data[0].score {
                    embed = embed.field("Score", score.to_string(), true);
                }
                if let Some(popularity) = &data.data[0].popularity {
                    embed = embed.field("Popularity", popularity.to_string(), true);
                }
                if let Some(favorites) = &data.data[0].favorites {
                    embed = embed.field("Favorites", favorites.to_string(), true);
                }
                embed = embed.field("", &empty, false);
                if let Some(chapters) = &data.data[0].specific.chapters {
                    embed = embed.field("Chapters", chapters.to_string(), true);
                }
                if let Some(volumes) = &data.data[0].specific.volumes {
                    embed = embed.field("Volumes", volumes.to_string(), true);
                }
                if let Some(published) = &data.data[0].specific.published.aired_string {
                    embed = embed.field("Published", published, true);
                }
                let genres_string = &data.data[0]
                    .genres
                    .iter()
                    .map(|genre| genre.name.as_str())
                    .intersperse(" - ")
                    .collect::<String>();
                embed = embed.field("Genres", genres_string, false);
                let len = data.data.len();
                if ctx.guild_id().is_some() && len > 1 {
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
                    let action_row = [CreateActionRow::buttons(&buttons[1..])];

                    let message = ctx
                        .send(
                            CreateReply::default()
                                .reply(true)
                                .embed(embed)
                                .components(&action_row),
                        )
                        .await?;

                    let ctx_id_copy = ctx.id();
                    while let Some(interaction) =
                        ComponentInteractionCollector::new(ctx.serenity_context())
                            .timeout(Duration::from_secs(60))
                            .filter(move |interaction| {
                                interaction
                                    .data
                                    .custom_id
                                    .starts_with(ctx_id_copy.to_string().as_str())
                            })
                            .await
                    {
                        interaction
                            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                            .await?;

                        if interaction.data.custom_id.ends_with('n') && state.index < state.len - 1
                        {
                            state.index += 1;
                        } else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
                            state.index -= 1;
                        }

                        let japanese_title = data.data[state.index]
                            .titles
                            .iter()
                            .find(|t| t.title_type == "Japanese")
                            .map_or("No japanese title available", |t| t.title.as_str());
                        let mut new_embed = CreateEmbed::default()
                            .title(japanese_title)
                            .thumbnail(&data.data[state.index].images.webp.image_url)
                            .url(&data.data[state.index].url)
                            .colour(COLOUR_ORANGE);

                        if let Some(synopsis) = &data.data[state.index].synopsis {
                            new_embed = new_embed.description(format!("*{synopsis}*"));
                        }
                        new_embed =
                            new_embed.field("Format", &data.data[state.index].anime_type, true);
                        new_embed = new_embed.field("Status", &data.data[state.index].status, true);
                        if let Some(english_title) = data.data[state.index]
                            .titles
                            .iter()
                            .find(|t| t.title_type == "English")
                            .map(|t| &t.title)
                        {
                            new_embed = new_embed.field("English title", english_title, true);
                        }
                        new_embed = new_embed.field("", &empty, false);
                        if let Some(score) = &data.data[state.index].score {
                            new_embed = new_embed.field("Score", score.to_string(), true);
                        }
                        if let Some(popularity) = &data.data[state.index].popularity {
                            new_embed = new_embed.field("Popularity", popularity.to_string(), true);
                        }
                        if let Some(favorites) = &data.data[state.index].favorites {
                            new_embed = new_embed.field("Favorites", favorites.to_string(), true);
                        }
                        new_embed = new_embed.field("", &empty, false);
                        if let Some(chapters) = &data.data[state.index].specific.chapters {
                            new_embed = new_embed.field("Chapters", chapters.to_string(), true);
                        }
                        if let Some(volumes) = &data.data[state.index].specific.volumes {
                            new_embed = new_embed.field("Volumes", volumes.to_string(), true);
                        }
                        if let Some(published) =
                            &data.data[state.index].specific.published.aired_string
                        {
                            new_embed = new_embed.field("Published", published, true);
                        }
                        let genres_string = data.data[state.index]
                            .genres
                            .iter()
                            .map(|genre| genre.name.as_str())
                            .intersperse(" - ")
                            .collect::<String>();
                        new_embed = new_embed.field("Genres", genres_string, false);
                        final_embed = new_embed.clone();

                        let new_components = {
                            if state.index == 0 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[1..]))]
                            } else if state.index == len - 1 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[..1]))]
                            } else {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons))]
                            }
                        };

                        let mut msg = interaction.message;

                        msg.edit(
                            ctx.http(),
                            EditMessage::default()
                                .embed(new_embed)
                                .components(&new_components),
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
            }
            Ok(_) | Err(_) => {
                ctx.reply("Not worthy of looking up").await?;
            }
        },
        Err(_) => {
            ctx.reply("API down, get a life!").await?;
        }
    }
    Ok(())
}

/// When there aren't enough memes
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn memegen(
    ctx: SContext<'_>,
    #[description = "Top-left text"] top_left: String,
    #[description = "Top-right text"] top_right: String,
    #[description = "Bottom text"] bottom: String,
) -> Result<(), Error> {
    let request_url = {
        let encoded_left = encode(&top_left);
        let encoded_right = encode(&top_right);
        let encoded_bottom = encode(&bottom);
        format!(
            "https://api.memegen.link/images/exit/{encoded_left}/{encoded_right}/{encoded_bottom}.png"
        )
    };
    ctx.reply(request_url).await?;
    Ok(())
}

/// When someone offended you
#[poise::command(prefix_command, slash_command)]
pub async fn roast(
    ctx: SContext<'_>,
    #[description = "Target"]
    #[rest]
    member: Member,
) -> Result<(), Error> {
    if let Some(guild_id) = ctx.guild_id() {
        ctx.defer().await?;
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
                    .map_or_else(|| "user has no banner".to_owned(), |banner| banner)
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
        let message_count = {
            let ctx_data = ctx.data();
            let user_settings_lock = ctx_data.user_settings.lock().await;
            let mut user_settings_opt = user_settings_lock.get(&guild_id);
            match user_settings_opt {
                Some(settings) => settings
                    .get(&member.user.id)
                    .map_or(0, |count| count.message_count),
                _ => {
                    let mut modified_settings =
                        user_settings_opt.get_or_insert_default().as_ref().clone();
                    modified_settings.insert(
                        member.user.id,
                        UserSettings {
                            guild_id: i64::from(guild_id),
                            user_id: i64::from(member.user.id),
                            ..Default::default()
                        },
                    );
                    user_settings_lock.insert(guild_id, Arc::new(modified_settings.clone()));
                    0
                }
            }
        };
        let mut messages = ctx.channel_id().messages_iter(&ctx).boxed();

        let messages_string = {
            let mut result = String::new();
            let mut result_count = 0;
            let mut missing_match_count = 0;

            while let Some(message_result) = messages.next().await {
                match message_result {
                    Ok(message) => {
                        if message.author.id == member.user.id {
                            let index = result_count + 1;
                            if result_count > 0 {
                                result.push(',');
                            }
                            result.push_str(&index.to_string());
                            result.push(':');
                            result.push_str(&message.content);
                            result_count += 1;
                        } else {
                            missing_match_count += 1;
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
                if result_count >= 25 || missing_match_count >= 100 {
                    break;
                }
            }

            result
        };

        let description = format!(
            "name:{name},avatar:{avatar_url},banner:{banner_url},roles:{roles},acc_create:{account_date},joined_svr:{join_date},msg_count:{message_count},last_msgs:{messages_string}"
        );
        let role = "you're an evil ai assistant that excels at roasting ppl, especially weebs. no mercy shown. the prompt will contain information of your target";
        match ai_response_simple(role, &description).await {
            Some(resp) if !resp.is_empty() => {
                let mut embed = CreateEmbed::default()
                    .title(format!("Roasting {name}"))
                    .colour(COLOUR_RED);
                let mut current_chunk = String::with_capacity(1024);
                let mut chunk_index = 0;
                for ch in resp.chars() {
                    if current_chunk.len() >= 1024 {
                        let field_name = if chunk_index == 0 {
                            "Response:".to_owned()
                        } else {
                            format!("Response (cont. {}):", chunk_index + 1)
                        };
                        embed = embed.field(field_name, current_chunk, false);
                        current_chunk = String::with_capacity(1024);
                        chunk_index += 1;
                    }
                    current_chunk.push(ch);
                }
                if !current_chunk.is_empty() {
                    let field_name = if chunk_index == 0 {
                        "Response:".to_owned()
                    } else {
                        format!("Response (cont. {}):", chunk_index + 1)
                    };
                    embed = embed.field(field_name, current_chunk, false);
                }
                ctx.send(CreateReply::default().reply(true).embed(embed))
                    .await?;
            }
            Some(_) | None => {
                ctx.reply(format!("{name}'s life is already roasted"))
                    .await?;
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct FabseTranslate {
    alternatives: Vec<String>,
    #[serde(rename = "detectedLanguage")]
    detected_language: FabseLanguage,
    #[serde(rename = "translatedText")]
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
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn translate(
    ctx: SContext<'_>,
    #[description = "Language to be translated to, e.g. en"] target: Option<String>,
    #[description = "What should be translated"]
    #[rest]
    sentence: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let content = match ctx.guild_id() {
        Some(_) => {
            let msg = ctx
                .channel_id()
                .message(&ctx.http(), MessageId::new(ctx.id()))
                .await?;
            match msg.referenced_message {
                Some(ref_msg) => ref_msg.content.to_string(),
                None => {
                    if let Some(query) = sentence {
                        query
                    } else {
                        ctx.reply("Bruh, give me smth to translate").await?;
                        return Ok(());
                    }
                }
            }
        }
        None => {
            if let Some(query) = sentence {
                query
            } else {
                ctx.reply("Bruh, give me smth to translate").await?;
                return Ok(());
            }
        }
    };
    let target_lang = target.map_or_else(|| "en".to_owned(), |lang| lang.to_lowercase());
    let request = TranslateRequest {
        q: &content,
        source: "auto",
        target: &target_lang,
        alternatives: 3,
    };
    match HTTP_CLIENT
        .post(
            &UTILS_CONFIG
                .get()
                .expect("UTILS_CONFIG must be set during initialization")
                .ai
                .translate,
        )
        .json(&request)
        .send()
        .await
    {
        Ok(response) => match response.json::<FabseTranslate>().await {
            Ok(data) if !data.translated_text.is_empty() => {
                let embed = CreateEmbed::default()
                    .title(format!(
                        "Translation from {} to {target_lang} with {}% confidence",
                        data.detected_language.language, data.detected_language.confidence
                    ))
                    .colour(COLOUR_GREEN)
                    .field("Original:", &content, false)
                    .field("Translation:", &data.translated_text, false);
                let len = data.alternatives.len();
                if ctx.guild_id().is_some() && len > 1 {
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
                    let action_row = [CreateActionRow::buttons(&buttons[1..])];

                    let message = ctx
                        .send(
                            CreateReply::default()
                                .reply(true)
                                .embed(embed)
                                .components(&action_row),
                        )
                        .await?;

                    let ctx_id_copy = ctx.id();
                    while let Some(interaction) =
                        ComponentInteractionCollector::new(ctx.serenity_context())
                            .timeout(Duration::from_secs(60))
                            .filter(move |interaction| {
                                interaction
                                    .data
                                    .custom_id
                                    .starts_with(ctx_id_copy.to_string().as_str())
                            })
                            .await
                    {
                        interaction
                            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                            .await?;

                        if interaction.data.custom_id.ends_with('n') && state.index < state.len - 1
                        {
                            state.index += 1;
                        } else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
                            state.index -= 1;
                        }

                        let new_embed = CreateEmbed::default()
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
                                } else {
                                    &data.alternatives[state.index - 1]
                                },
                                false,
                            );
                        final_embed = new_embed.clone();

                        let new_components = {
                            if state.index == 0 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[1..]))]
                            } else if state.index == len - 1 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[..1]))]
                            } else {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons))]
                            }
                        };

                        let mut msg = interaction.message;

                        msg.edit(
                            ctx.http(),
                            EditMessage::default()
                                .embed(new_embed)
                                .components(&new_components),
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
            }
            Ok(_) | Err(_) => {
                ctx.reply("Too dangerous to translate").await?;
            }
        },
        Err(_) => {
            ctx.reply("Too dangerous to translate").await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct UrbanResponse {
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
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn urban(
    ctx: SContext<'_>,
    #[description = "Word(s) to lookup"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let request_url = {
        let encoded_input = encode(&input);
        format!("https://api.urbandictionary.com/v0/define?term={encoded_input}")
    };
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(response) => match response.json::<UrbanResponse>().await {
            Ok(data) if !data.list.is_empty() => {
                let mut embed = CreateEmbed::default()
                    .title(&data.list[0].word)
                    .colour(COLOUR_YELLOW);
                let mut current_chunk = String::with_capacity(1024);
                let mut chunk_index = 0;
                for ch in data.list[0].definition.replace(['[', ']'], "").chars() {
                    if current_chunk.len() >= 1024 {
                        let field_name = if chunk_index == 0 {
                            "Definition:".to_owned()
                        } else {
                            format!("Response (cont. {}):", chunk_index + 1)
                        };
                        embed = embed.field(field_name, current_chunk, false);
                        current_chunk = String::with_capacity(1024);
                        chunk_index += 1;
                    }
                    current_chunk.push(ch);
                }
                if !current_chunk.is_empty() {
                    let field_name = if chunk_index == 0 {
                        "Definition:".to_owned()
                    } else {
                        format!("Response (cont. {}):", chunk_index + 1)
                    };
                    embed = embed.field(field_name, current_chunk, false);
                }

                embed = embed.field(
                    "Example:".to_owned(),
                    data.list[0].example.replace(['[', ']'], ""),
                    false,
                );

                let len = data.list.len();
                if ctx.guild_id().is_some() && len > 1 {
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
                    let action_row = [CreateActionRow::buttons(&buttons[1..])];

                    let message = ctx
                        .send(
                            CreateReply::default()
                                .reply(true)
                                .embed(embed)
                                .components(&action_row),
                        )
                        .await?;

                    let ctx_id_copy = ctx.id();
                    while let Some(interaction) =
                        ComponentInteractionCollector::new(ctx.serenity_context())
                            .timeout(Duration::from_secs(60))
                            .filter(move |interaction| {
                                interaction
                                    .data
                                    .custom_id
                                    .starts_with(ctx_id_copy.to_string().as_str())
                            })
                            .await
                    {
                        interaction
                            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                            .await?;

                        if interaction.data.custom_id.ends_with('n') && state.index < state.len - 1
                        {
                            state.index += 1;
                        } else if interaction.data.custom_id.ends_with('p') && state.index > 0 {
                            state.index -= 1;
                        }

                        let mut new_embed = CreateEmbed::default()
                            .title(&data.list[state.index].word)
                            .colour(COLOUR_YELLOW);
                        let mut current_chunk = String::with_capacity(1024);
                        let mut chunk_index = 0;
                        for ch in data.list[state.index]
                            .definition
                            .replace(['[', ']'], "")
                            .chars()
                        {
                            if current_chunk.len() >= 1024 {
                                let field_name = if chunk_index == 0 {
                                    "Definition:".to_owned()
                                } else {
                                    format!("Response (cont. {}):", chunk_index + 1)
                                };
                                new_embed = new_embed.field(field_name, current_chunk, false);
                                current_chunk = String::with_capacity(1024);
                                chunk_index += 1;
                            }
                            current_chunk.push(ch);
                        }
                        if !current_chunk.is_empty() {
                            let field_name = if chunk_index == 0 {
                                "Definition:".to_owned()
                            } else {
                                format!("Response (cont. {}):", chunk_index + 1)
                            };
                            new_embed = new_embed.field(field_name, current_chunk, false);
                        }

                        new_embed = new_embed.field(
                            "Example:".to_owned(),
                            data.list[state.index].example.replace(['[', ']'], ""),
                            false,
                        );
                        final_embed = new_embed.clone();

                        let new_components = {
                            if state.index == 0 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[1..]))]
                            } else if state.index == len - 1 {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons[..1]))]
                            } else {
                                [CreateActionRow::Buttons(Cow::Borrowed(&buttons))]
                            }
                        };

                        let mut msg = interaction.message;

                        msg.edit(
                            ctx.http(),
                            EditMessage::default()
                                .embed(new_embed)
                                .components(&new_components),
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
            }
            Ok(_) | Err(_) => {
                ctx.reply(format!("**Like you, {input} don't exist**"))
                    .await?;
            }
        },
        Err(_) => {
            ctx.reply(format!("**Like you, {input} don't exist**"))
                .await?;
        }
    }
    Ok(())
}

/// Do I need to explain it?
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn waifu(ctx: SContext<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    ctx.reply(get_waifu().await).await?;
    Ok(())
}

#[derive(Deserialize)]
struct WikiResponse {
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
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn wiki(
    ctx: SContext<'_>,
    #[description = "Topic to lookup"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let request_url = {
        let encoded_input = encode(&input);
        format!("https://en.wikipedia.org/api/rest_v1/page/summary/{encoded_input}")
    };
    match HTTP_CLIENT.get(request_url).send().await {
        Ok(request) => {
            match request
                .json::<WikiResponse>()
                .await
                .ok()
                .filter(|output| !output.title.is_empty())
            {
                Some(data) => {
                    let mut embed = CreateEmbed::default()
                        .title(data.title)
                        .description(data.extract)
                        .url(data.content_urls.desktop.page)
                        .colour(COLOUR_GREEN);
                    if let Some(image) = data.originalimage {
                        embed = embed.image(image.source);
                    }
                    ctx.send(CreateReply::default().reply(true).embed(embed))
                        .await?;
                }
                None => {
                    ctx.reply(format!("**Like you, {input} don't exist**"))
                        .await?;
                }
            }
        }
        Err(_) => {
            ctx.reply(format!("**Like you, {input} don't exist**"))
                .await?;
        }
    }
    Ok(())
}
