use crate::types::{Context, Error};

use poise::serenity_prelude::CreateEmbed;
use poise::CreateReply;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use urlencoding::encode;

#[derive(Deserialize, Serialize)]
struct MoeResponse {
    result: Vec<AnimeScene>,
}
#[derive(Deserialize, Serialize)]
struct AnimeScene {
    anilist: Anilist,
    episode: Option<i32>,
    from: Option<f32>,
    to: Option<f32>,
    video: String,
}
#[derive(Deserialize, Serialize)]
struct Anilist {
    title: AnimeTitle,
}
#[derive(Deserialize, Serialize)]
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
#[poise::command(prefix_command, slash_command)]
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
    let client = &ctx.data().req_client;
    let request = client.get(request_url).send().await?;
    let scene: MoeResponse = request.json().await.unwrap();
    if !scene.result[0].video.is_empty() {
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::new()
                    .title(scene.result[0].anilist.title.to_string())
                    .field(
                        "Episode",
                        scene.result[0].episode.unwrap().to_string(),
                        true,
                    )
                    .field("From", scene.result[0].from.unwrap().to_string(), true)
                    .field("To", scene.result[0].to.unwrap().to_string(), true)
                    .color(0x57e389),
            ),
        )
        .await?;
        ctx.send(CreateReply::default().content(&scene.result[0].video))
            .await?;
    } else {
        ctx.send(
            CreateReply::default().content("why are you hallucinating, that scene never happened"),
        )
        .await?;
    }
    Ok(())
}
