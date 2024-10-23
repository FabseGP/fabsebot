use crate::types::{Error, SContext, HTTP_CLIENT};

use poise::{serenity_prelude::CreateEmbed, CreateReply};
use serde::Deserialize;
use std::fmt::{Display, Formatter};
use urlencoding::encode;

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
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.english {
            Some(english_title) => write!(f, "{english_title}"),
            None => write!(f, "Bruh"),
        }
    }
}

/// What anime was that scene from?
#[poise::command(prefix_command, slash_command)]
pub async fn anime_scene(
    ctx: SContext<'_>,
    #[description = "Link to anime image"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    let encoded_input = encode(&input);
    let request_url =
        format!("https://api.trace.moe/search?cutBorders&anilistInfo&url={encoded_input}");
    let request = HTTP_CLIENT.get(request_url).send().await?;
    let scene: Option<MoeResponse> = request.json().await?;
    if let Some(payload) = scene {
        if payload.result[0].video.is_empty() {
            ctx.send(
                CreateReply::default()
                    .content("Why are you hallucinating, that scene never happened"),
            )
            .await?;
        } else {
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::default()
                        .title(payload.result[0].anilist.title.to_string())
                        .field(
                            "Episode",
                            payload.result[0].episode.unwrap().to_string(),
                            true,
                        )
                        .field("From", payload.result[0].from.unwrap().to_string(), true)
                        .field("To", payload.result[0].to.unwrap().to_string(), true)
                        .color(0x57e389),
                ),
            )
            .await?;
            ctx.send(CreateReply::default().content(&payload.result[0].video))
                .await?;
        }
    }
    Ok(())
}
