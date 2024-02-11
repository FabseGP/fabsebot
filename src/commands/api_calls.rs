use crate::types::{Context, Error};
use crate::utils::random_number;

use poise::serenity_prelude::CreateEmbed;
use poise::CreateReply;
use reqwest::{multipart, Client};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::model::Timestamp;
use std::{
    fs::{remove_file, File},
    io::copy,
    path::Path,
};
use urlencoding::encode;

const QUERY: &str = "
query ($id: Int) { # Define which variables will be used in the query (id)
  Media (id: $id, type: ANIME) { # Insert our variables into the query arguments (id) (type: ANIME is hard-coded in the query)
    id
    title {
      romaji
      english
      native
    }
  }
}
";

/// When the other bot sucks
#[poise::command(slash_command, prefix_command)]
pub async fn anilist_anime(
    ctx: Context<'_>,
    #[description = "Anime to search"]
    #[rest]
    anime: String,
) -> Result<(), Error> {
    let client = Client::new();
    let json = json!({"query": QUERY, "variables": {"id": 15125}});
    let resp = client
        .post("https://graphql.anilist.co/")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(json.to_string())
        .send()
        .await
        .unwrap()
        .text();
    let result: serde_json::Value = serde_json::from_str(&resp.await.unwrap()).unwrap();
    ctx.send(CreateReply::default().embed(
        CreateEmbed::new().title("Anime").color(0x33d17a).field(
            "Output:",
            &result.to_string(),
            false,
        ),
    ))
    .await?;
    println!("{:#}", result);
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
        ctx.send(CreateReply::default().content(&data.activity))
            .await?;
    } else {
        ctx.send(CreateReply::default().content("don't be bored or kill yourself"))
            .await?;
    }
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
    let request = reqwest::get(request_url).await?;
    let judging: EightBallResponse = request.json().await.expect("Error while parsing json");
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
    let request = reqwest::get(request_url).await?;
    let urls: GifResponse = request.json().await.expect("Error while parsing json");
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
struct CrapResponse {
    data: ImgData,
}
#[derive(Deserialize, Debug, Serialize)]
struct ImgData {
    url: String,
}

/// Proprietary image upload
#[poise::command(slash_command, prefix_command)]
pub async fn imgbb(
    ctx: Context<'_>,
    #[description = "User"] user: String,
    #[description = "Upload image"]
    #[rest]
    image: String,
) -> Result<(), Error> {
    let api_key = if user == "rinynm" || user == "rinymn" {
        "93b076cd514f50ae220e7502a24b7690"
    } else {
        "1d9386cec5ba63fa4ae740888656aee7"
    };
    let request_url = format!(
        "https://api.imgbb.com/1/upload?key={key}&image={url}",
        key = api_key,
        url = image
    );
    let request = reqwest::get(request_url).await?;
    let upload: CrapResponse = request.json().await.expect("Error while parsing json");
    if !upload.data.url.is_empty() {
        ctx.send(CreateReply::default().content(format!("image url: {}", &upload.data.url)))
            .await?;
    } else {
        ctx.send(CreateReply::default().content("your image is too perfect for imgbb"))
            .await?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ImgurResponse {
    data: ImgurData,
}

#[derive(Debug, Deserialize)]
struct ImgurData {
    link: String,
}

/// Proprietary image upload V2
#[poise::command(slash_command, prefix_command)]
pub async fn imgur(
    ctx: Context<'_>,
    #[description = "User"] user: String,
    #[description = "Upload image"]
    #[rest]
    image: String,
) -> Result<(), Error> {
    let token = if user == "rinynm" || user == "rinymn" {
        "93b076cd514f50ae220e7502a24b7690"
    } else {
        "350a80711beb8d2d7ba99f1af635718af6fe4c50"
    };
    let album_id = if user == "rinynm" || user == "rinymn" {
        "93b076cd514f50ae220e7502a24b7690"
    } else {
        "9VJs4pi"
    };
    let url = "https://api.imgur.com/3/image";
    let client = Client::new();
    let request_url = client
        .post(url)
        .header("Authorization", format!("Bearer {}", token))
        .form(&[("image", image), ("album", album_id.to_string())])
        .send()
        .await;
    //    if request_url.status().is_success() {
    let imgur_response: ImgurResponse = request_url?.json().await?;
    let imgur_link = imgur_response.data.link;
    ctx.send(CreateReply::default().content(format!("image url: {}", imgur_link)))
        .await?;
    //  } else {
    //    ctx.send(|m| m.content("imgur has broken up with you"))
    //         .await?;
    //  }
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
    let request = reqwest::get(request_url).await?;
    let data: JokeResponse = request.json().await.expect("Error while parsing json");
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

/// When there aren't not enough memes
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

#[derive(Debug, Deserialize)]
struct RomanianAuth {
    data: PicsurAuth,
}

#[derive(Debug, Deserialize)]
struct PicsurAuth {
    jwt_token: String,
}

#[derive(Debug, Deserialize)]
struct RomanianUpload {
    data: PicsurUpload,
}

#[derive(Debug, Deserialize)]
struct PicsurUpload {
    id: String,
}

/// Romanian image upload, beware of your metal!
#[poise::command(slash_command, prefix_command)]
pub async fn picsur(
    ctx: Context<'_>,
    #[description = "Upload image"]
    #[rest]
    image: String,
) -> Result<(), Error> {
    let username = "eventteam";
    let password = "JwC8y#V6%o9Fm4q#2tTegkX252RvoV";
    let auth_url = "https://fileshare.benzone.work/api/user/login";
    let client = Client::new();
    let request_auth = client
        .post(auth_url)
        .header("Content-Type", "application/json")
        .body(
            json!({
               "username": username,
               "password": password
            })
            .to_string(),
        )
        .send()
        .await;
    //    if request_auth.status().is_success() {
    let upload_url = "https://fileshare.benzone.work/api/image/upload";
    let romanian_auth: RomanianAuth = request_auth?.json().await?;
    let token = romanian_auth.data.jwt_token;
    let target = reqwest::get(&image).await?;
    let download = target.bytes().await?;
    let filename = image.split('/').last().unwrap_or("downloaded_file.txt");
    let path = Path::new(".").join(filename);
    let mut file = File::create(&path)?;
    copy(&mut download.as_ref(), &mut file)?;
    let file = File::open(&path)?;
    let response_upload = client
        .post(upload_url)
        .header("Authorization", format!("Bearer {}", token))
        //  .multipart(multipart::Form::new().part(
        //    "image",
        //      multipart::Part::reader(file).file_name(format!("upload_{}.png", Timestamp::now())),
        //   ))
        .send()
        .await;
    // if response_upload.status().is_success() {
    let romanian_upload: RomanianUpload = response_upload?.json().await?;
    let image_url = format!(
        "https://fileshare.benzone.work/i/{}",
        romanian_upload.data.id
    );
    ctx.send(CreateReply::default().content(format!("image url: {}", image_url)))
        .await?;
    // } else {
    //     ctx.send(|m| m.content("romania have ceased to exist"))
    //         .await?;
    // }
    remove_file(&path)?;
    //  } else {
    //      ctx.send(|m| m.content("these dammed romanians are keeping us out!"))
    //          .await?;
    //  }
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
pub async fn translate(
    ctx: Context<'_>,
    #[description = "Language to be translated from"] source: String,
    #[description = "Language to be translated to"] target: String,
    #[description = "What should be translated"]
    #[rest]
    sentence: String,
) -> Result<(), Error> {
    let encoded_input = encode(&sentence);
    let request_url = format!(
        "https://translate.projectsegfau.lt/api/translate?engine=all&from={s}&to={t}&text={m}",
        s = source,
        t = target,
        m = encoded_input
    );
    let request = reqwest::Client::new()
        .get(&request_url)
        .header("accept", "application/json")
        .send()
        .await?;
    let data: Vec<TranslateResponse> = request.json().await.expect("Error while parsing json");
    if !data[2].source_language.is_empty() {
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::new()
                    .title(format!("Translation from {} to {}", source, target))
                    .color(0x33d17a)
                    .field("Original:", sentence, false)
                    .field("Translation:", &data[2].translated_text, false),
            ),
        )
        .await?;
    } else {
        ctx.send(CreateReply::default().content("invalid syntax, pls follow this template: '!translate da,en text', here translating from danish to english")).await?;
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
    let request = reqwest::get(request_url).await?;
    let data: UrbanResponse = request.json().await.expect("Error while parsing json");
    if !data.list.is_empty() {
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::new()
                    .title(&data.list[0].word)
                    .color(0xEFFF00)
                    .field(
                        "Definition:",
                        &data.list[0].definition.replace(['[', ']'], ""),
                        false,
                    )
                    .field(
                        "Example:",
                        &data.list[0].example.replace(['[', ']'], ""),
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
