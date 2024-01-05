use crate::types::{
    Context,
    Error,
};
use crate::utils::{
    random_number,
};

use serde::{Deserialize, Serialize};
use urlencoding::encode;

#[derive(Deserialize, Debug, Serialize)]
struct BallResponse {
    reading: String,
}

/// When you need some activities
#[poise::command(slash_command, prefix_command)]
pub async fn eightball(ctx: Context<'_>, #[description = "Your question"]#[rest] question: String) -> Result<(), Error> {
    let encoded_input = encode(&question);
    let request_url = format!("https://eightballapi.com/api/biased?question={query}&lucky=false",
        query = encoded_input);
    let request = reqwest::get(request_url).await?;
    let judging: BallResponse = request.json().await.expect("Error while parsing json");
    if !judging.reading.is_empty() {
        ctx.send(|m| m.content(&judging.reading)).await?;
    }
    else {
        ctx.send(|m| m.content("sometimes riding a giraffe is what you need")).await?;
    }
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct BoredResponse {
    activity: String,
}

/// When you need some activities
#[poise::command(slash_command, prefix_command)]
pub async fn bored(ctx: Context<'_>) -> Result<(), Error> {
    let request_url = "https://www.boredapi.com/api/activity";
    let request = reqwest::get(request_url).await?;
    let data: BoredResponse = request.json().await.expect("Error while parsing json");
    if !data.activity.is_empty() {
        ctx.send(|m| m.content(&data.activity)).await?;
    }
    else {
        ctx.send(|m| m.content("don't be bored or kill yourself")).await?;
    }
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct GifResponse {
    results: Vec<GifUrl>,
}
#[derive(Deserialize, Debug, Serialize)]
struct GifUrl {
    url: String,
}

/// Gifing
#[poise::command(slash_command, prefix_command)]
pub async fn gif(ctx: Context<'_>, #[description = "Search gif"]#[rest] input: String) -> Result<(), Error> {
    let encoded_input = encode(&input);
    let request_url = format!("https://tenor.googleapis.com/v2/search?q={search}&key={key}&contentfilter=medium&limit=30",
        search = encoded_input,
        key = "AIzaSyD-XwC-iyuRIZQrcaBzCuTLfAvEGh3DPwo");
    let request = reqwest::get(request_url).await?;
    let urls: GifResponse = request.json().await.expect("Error while parsing json");
    if !urls.results.is_empty() {
        ctx.send(|m| m.content(&urls.results[random_number(30)].url)).await?;
    }
    else {
        ctx.send(|m| m.content("life is not gifing")).await?;
    }
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct RomanianResponse {
    FileInfo: Gokapi,
    HotlinkUrl: String,
}
#[derive(Deserialize, Debug, Serialize)]
struct Gokapi {
    HotlinkId: String,
}

/// Romanian file upload
#[poise::command(slash_command, prefix_command)]
pub async fn gokapi(ctx: Context<'_>, #[description = "Upload image"]#[rest] image: String) -> Result<(), Error> {
    let api_key = "gkHjiOgQboVvt1ngYPbdw15OXuaZaF";
    let attachment_url = image;
    let request_url = format!("https://192.168.0.200/api/files/add",
        key = api_key,
        url = attachment_url);
    let request = reqwest::get(request_url).await?;
    let upload: CrapResponse = request.json().await.expect("Error while parsing json");
    if !upload.FileInfo.HotlinkId.is_empty() {
        ctx.send(|m| m.content(format!("image url: {}", &upload.HotlinkUrl/&upload.FileInfo.HotlinkId))).await?;
    }
    else {
        ctx.send(|m| m.content("typical romanian slacking off")).await?;
    }
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct CrapResponse {
    data: ImgBB,
}
#[derive(Deserialize, Debug, Serialize)]
struct ImgBB {
    url: String,
}

/// Proprietary image upload
#[poise::command(slash_command, prefix_command)]
pub async fn imgbb(ctx: Context<'_>, #[description = "User"] user: String, #[description = "Upload image"]#[rest] image: String) -> Result<(), Error> {
    let mut api_key = "test";
    if user == "rinynm" {
      api_key = "93b076cd514f50ae220e7502a24b7690";
    }
    let attachment_url = image;
    let request_url = format!("https://api.imgbb.com/1/upload?key={key}&image={url}",
        key = api_key,
        url = attachment_url);
    let request = reqwest::get(request_url).await?;
    let upload: CrapResponse = request.json().await.expect("Error while parsing json");
    if !upload.data.url.is_empty() {
        ctx.send(|m| m.content(format!("image url: {}", &upload.data.url))).await?;
    }
    else {
        ctx.send(|m| m.content("your image is too perfect for imgbb")).await?;
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
    let request_url = "https://api.humorapi.com/jokes/random?api-key=48c239c85f804a0387251d9b3587fa2c";
    let request = reqwest::get(request_url).await?;
    let data: JokeResponse = request.json().await.expect("Error while parsing json");
    if !data.joke.is_empty() {
        ctx.send(|m| m.content(&data.joke)).await?;
    }
    else {
        let roasts = ["your life", "you're not funny", "you", "get a life bitch", "I don't like you", "you smell"];
        ctx.send(|m| m.content(roasts[random_number(roasts.len())])).await?;
    }
    Ok(())
}

/// When there aren't not enough memes
#[poise::command(slash_command, prefix_command)]
pub async fn memegen(ctx: Context<'_>, #[description = "Top-left text"] top_left: String, #[description = "Top-right text"] top_right: String, #[description = "Bottom text"] bottom: String) -> Result<(), Error> {
    let encoded_topl = encode(&top_left);
    let encoded_topr = encode(&top_right);
    let encoded_bottom = encode(&bottom);
    let request_url = format!("https://api.memegen.link/images/exit/{left}/{right}/{bottom}.png",
        left = encoded_topl, 
        right = encoded_topr, 
        bottom = encoded_bottom);
    ctx.send(|m| m.content(request_url)).await?;
    Ok(())
}

#[derive(Deserialize, Debug, Serialize)]
struct TranslateResponse {
    engine: String,
    #[serde(rename = "translated-text")]
    translated_text: String,
    source_language: String,
    target_language: String,
}

/// When you stumble on some ancient sayings
#[poise::command(slash_command, prefix_command)]
pub async fn translate(ctx: Context<'_>, #[description = "Language to be translated from"] source: String, #[description = "Language to be translated to"] target: String, #[description = "What should be translated"]#[rest] sentence: String) -> Result<(), Error> {
    let encoded_input = encode(&sentence);
    let request_url = format!("https://translate.projectsegfau.lt/api/translate?engine=all&from={s}&to={t}&text={m}",
        s = source,
        t = target,
        m = encoded_input);
    let request = reqwest::Client::new().get(&request_url).header("accept", "application/json");
    let response = request.send().await?;
    let data: Vec<TranslateResponse> = response.json().await.expect("Error while parsing json");
    if !data[2].source_language.is_empty() {
        ctx.send(|e| {
            e.embed(|t| {
                t.title(format!("Translation from {} to {}", source, target))
                    .color(0x33d17a)
                    .field("Original:", sentence, false)
                    .field("Translation:", &data[2].translated_text, false)
            })
        }).await?;
    }
    else {
        ctx.send(|m| m.content("invalid syntax, pls follow this template: '!translate da,en text', here translating from danish to english")).await?;
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
pub async fn urban(ctx: Context<'_>, #[description = "Word(s) to lookup"]#[rest] input: String) -> Result<(), Error> {
    let encoded_input = encode(&input);
    let request_url = format!("https://api.urbandictionary.com/v0/define?term={search}",
        search = encoded_input);
    let request = reqwest::get(request_url).await?;
    let data: UrbanResponse = request.json().await.expect("Error while parsing json");
    if !data.list.is_empty() {
        ctx.send(|e| {
            e.embed(|u| {
                u.title(&data.list[0].word)
                    .color(0xEFFF00)
                    .field("Definition:", &data.list[0].definition.replace("[", "").replace("]", ""), false)
                    .field("Example:", &data.list[0].example.replace("[", "").replace("]", ""), false)
            })
        }).await?;
    }
    else {
        ctx.send(|m| m.content(format!("like you, {} don't exist", input))).await?;
    }
    Ok(())
}

