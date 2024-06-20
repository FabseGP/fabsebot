use crate::types::{Context, Error};
use crate::utils::random_number;

use poise::serenity_prelude::CreateEmbed;
use poise::CreateReply;
use serde::{Deserialize, Serialize};

use urlencoding::encode;

/// When the other bot sucks
#[poise::command(slash_command, prefix_command)]
pub async fn anilist_anime(
    ctx: Context<'_>,
    #[description = "Anime to search"]
    #[rest]
    anime: String,
) -> Result<(), Error> {
    let client = &ctx.data().req_client;
    let query = format!(
        r#"{{
        "query": "query ($search: String) {{
            Media (id: $id, type: ANIME) {{
                id
                title {{
                    romaji
                    english
                    native
                }}
            }}
        }}",
        "variables": {{
            "search": "{}"
        }}
    }}"#,
        anime
    );
    let resp = client
        .post("https://graphql.anilist.co/")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(query)
        .send()
        .await?;
    let data: serde_json::Value = resp.json().await?;
    println!("{:#}", data);
    let anime_data = &data["data"]["Media"];

    if anime_data.is_null() {
        ctx.say("No anime found with that name.").await?;
        return Ok(());
    }

    let id = anime_data["id"].as_u64().unwrap();
    let title = anime_data["title"]["romaji"].as_str().unwrap_or("Unknown");

    let embed = CreateEmbed::default()
        .title("Anime")
        .field("ID", id.to_string(), false)
        .field("Title (Romaji)", title.to_string(), false)
        .color(0x33d17a);

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct EightBallResponse {
    reading: String,
}

/// When you need a wise opinion
#[poise::command(slash_command, prefix_command)]
pub async fn eightball(
    ctx: Context<'_>,
    #[description = "Your question"]
    #[rest]
    question: String,
) -> Result<(), Error> {
    let encoded_input = encode(&question);
    let request_url = format!(
        "https://eightballapi.com/api/biased?question={query}&lucky=false",
        query = encoded_input
    );
    let client = &ctx.data().req_client;
    let request = client.get(request_url).send().await?;
    let judging: EightBallResponse = request.json().await?;
    if !judging.reading.is_empty() {
        ctx.send(
            CreateReply::default().embed(CreateEmbed::new().title(question).color(0x33d17a).field(
                "",
                &judging.reading,
                true,
            )),
        )
        .await?;
    } else {
        ctx.send(CreateReply::default().content("sometimes riding a giraffe is what you need"))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct GifResponse {
    results: Vec<GifData>,
}
#[derive(Deserialize, Debug, Serialize)]
struct GifData {
    url: String,
}

/// Gifing
#[poise::command(slash_command, prefix_command)]
pub async fn gif(
    ctx: Context<'_>,
    #[description = "Search gif"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    let encoded_input = encode(&input);
    let request_url = format!(
        "https://tenor.googleapis.com/v2/search?q={search}&key={key}&contentfilter=medium&limit=30",
        search = encoded_input,
        key = "AIzaSyD-XwC-iyuRIZQrcaBzCuTLfAvEGh3DPwo"
    );
    let client = &ctx.data().req_client;
    let request = client.get(request_url).send().await?;
    let urls: GifResponse = request.json().await.unwrap();
    if !urls.results.is_empty() {
        ctx.send(CreateReply::default().content(&urls.results[random_number(30)].url))
            .await?;
    } else {
        ctx.send(CreateReply::default().content("life is not gifing"))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct JokeResponse {
    joke: String,
}

/// When your life isn't fun anymore
#[poise::command(slash_command, prefix_command)]
pub async fn joke(ctx: Context<'_>) -> Result<(), Error> {
    let request_url =
        "https://api.humorapi.com/jokes/random?api-key=48c239c85f804a0387251d9b3587fa2c";
    let client = &ctx.data().req_client;
    let request = client.get(request_url).send().await?;
    let data: JokeResponse = request.json().await.unwrap();
    if !data.joke.is_empty() {
        ctx.send(CreateReply::default().content(&data.joke)).await?;
    } else {
        let roasts = [
            "your life",
            "you're not funny",
            "you",
            "get a life bitch",
            "I don't like you",
            "you smell",
        ];
        ctx.send(CreateReply::default().content(roasts[random_number(roasts.len())]))
            .await?;
    }
    Ok(())
}

/// When there aren't enough memes
#[poise::command(slash_command, prefix_command)]
pub async fn memegen(
    ctx: Context<'_>,
    #[description = "Top-left text"] top_left: String,
    #[description = "Top-right text"] top_right: String,
    #[description = "Bottom text"] bottom: String,
) -> Result<(), Error> {
    let encoded_topl = encode(&top_left);
    let encoded_topr = encode(&top_right);
    let encoded_bottom = encode(&bottom);
    let request_url = format!(
        "https://api.memegen.link/images/exit/{left}/{right}/{bottom}.png",
        left = encoded_topl,
        right = encoded_topr,
        bottom = encoded_bottom
    );
    ctx.send(CreateReply::default().content(request_url))
        .await?;
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct TranslateResponse {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

/// When you stumble on some ancient sayings
#[poise::command(slash_command, prefix_command)]
pub async fn translate(
    ctx: Context<'_>,
    #[description = "Language to be translated from"] source: String,
    #[description = "Language to be translated to"] target: String,
    #[description = "What should be translated"]
    #[rest]
    sentence: String,
) -> Result<(), Error> {
    let encoded_input = encode(&sentence);
    let form_data = format!("q={}&source={}&target={}", encoded_input, source, target);

    let response = ctx
        .data()
        .req_client
        .post("https://translate.lotigara.ru/translate")
        .header("accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_data)
        .send()
        .await?;

    if response.status().is_success() {
        let data: TranslateResponse = response.json().await?;
        if !data.translated_text.is_empty() {
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title(format!("Translation from {} to {}", source, target))
                        .color(0x33d17a)
                        .field("Original:", sentence, false)
                        .field("Translation:", &data.translated_text, false),
                ),
            )
            .await?;
        } else {
            ctx.send(CreateReply::default().content("invalid syntax, pls follow this template: '!translate da,en text', here translating from danish to english")).await?;
        }
    } else {
        ctx.send(CreateReply::default().content("my translator is currently busy, pls standby"))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct UrbanResponse {
    list: Vec<UrbanDict>,
}
#[derive(Deserialize, Debug, Serialize)]
struct UrbanDict {
    definition: String,
    example: String,
    word: String,
}

/// The holy moly urbandictionary
#[poise::command(slash_command, prefix_command)]
pub async fn urban(
    ctx: Context<'_>,
    #[description = "Word(s) to lookup"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    let encoded_input = encode(&input);
    let request_url = format!(
        "https://api.urbandictionary.com/v0/define?term={search}",
        search = encoded_input
    );
    let client = &ctx.data().req_client;
    let request = client.get(request_url).send().await?;
    let data: UrbanResponse = request.json().await.unwrap();
    if !data.list.is_empty() {
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::new()
                    .title(&data.list[0].word)
                    .color(0xEFFF00)
                    .field(
                        "Definition:",
                        data.list[0].definition.replace(['[', ']'], ""),
                        false,
                    )
                    .field(
                        "Example:",
                        data.list[0].example.replace(['[', ']'], ""),
                        false,
                    ),
            ),
        )
        .await?;
    } else {
        ctx.send(CreateReply::default().content(format!("like you, {} don't exist", input)))
            .await?;
    }
    Ok(())
}
