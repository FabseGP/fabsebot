use crate::types::{Error, SContext, HTTP_CLIENT};
use core::fmt::{Display, Formatter, Result as FmtResult};
use poise::{serenity_prelude::CreateEmbed, CreateReply};
use serde::Deserialize;
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
    let encoded_input = encode(&input);
    let request_url =
        format!("https://api.trace.moe/search?cutBorders&anilistInfo&url={encoded_input}");
    
    let response = HTTP_CLIENT.get(request_url).send().await?;
    
    match response.json::<MoeResponse>().await {
        Ok(scene) => {
            if let Some(first_result) = scene.result.first() {
                if first_result.video.is_empty() {
                    ctx.send(CreateReply::default().content("No matching scene found"))
                        .await?;
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
                    CreateReply::default().embed(
                        CreateEmbed::default()
                            .title(title)
                            .field("Episode", episode_text, true)
                            .field("From", first_result.from.unwrap_or(0.0).to_string(), true)
                            .field("To", first_result.to.unwrap_or(0.0).to_string(), true)
                            .color(0x57e389),
                    ),
                )
                .await?;
                
                ctx.send(CreateReply::default().content(&first_result.video))
                    .await?;
            } else {
                ctx.send(CreateReply::default().content("No results found"))
                    .await?;
            }
        }
        Err(_) => {
            ctx.send(CreateReply::default().content("Failed to parse the response"))
                .await?;
        }
    }
    
    Ok(())
}
