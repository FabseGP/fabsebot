use crate::types::{Context, Error};

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use urlencoding::encode;

#[derive(Deserialize, Debug, Serialize)]
struct MoeResponse {
    result: Vec<AnimeScene>,
}
#[derive(Deserialize, Debug, Serialize)]
struct AnimeScene {
    anilist: Anilist,
    episode: Option<i32>,
    from: Option<f32>,
    to: Option<f32>,
    video: String,
}
#[derive(Deserialize, Debug, Serialize)]
struct Anilist {
    title: AnimeTitle,
}
#[derive(Deserialize, Debug, Serialize)]
struct AnimeTitle {
    english: Option<String>,
}

impl Display for AnimeTitle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(english_title) = &self.english {
            write!(f, "{}", english_title)
        } else {
            write!(f, "bruh")
        }
    }
}

/// What anime was that scene from?
#[poise::command(slash_command, prefix_command)]
pub async fn anime_scene(
    ctx: Context<'_>,
    #[description = "Link to anime image"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    let encoded_input = encode(&input);
    let request_url = format!(
        "https://api.trace.moe/search?cutBorders&anilistInfo&url={input}",
        input = encoded_input
    );
    let request = reqwest::get(request_url).await?;
    let scene: MoeResponse = request.json().await.expect("Error while parsing json");
    if !scene.result.is_empty() {
        ctx.send(|e| {
            e.embed(|a| {
                a.title(&scene.result[0].anilist.title)
                    .field("Episode", scene.result[0].episode.unwrap_or_default(), true)
                    .field("From", scene.result[0].from.unwrap_or_default(), true)
                    .field("To", scene.result[0].to.unwrap_or_default(), true)
                    .color(0x57e389)
            })
        })
        .await?;
        ctx.send(|m| m.content(&scene.result[0].video)).await?;
    } else {
        ctx.send(|m| m.content("why are you hallucinating, that scene never happened"))
            .await?;
    }
    Ok(())
}
