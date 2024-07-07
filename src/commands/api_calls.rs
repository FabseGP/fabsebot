use crate::types::{Context, Error};
use crate::utils::random_number;

use poise::serenity_prelude::{CreateAttachment, CreateEmbed};
use poise::CreateReply;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::futures::StreamExt;
use sqlx::Row;

use urlencoding::encode;

/// Did someone say AI image?
#[poise::command(slash_command, prefix_command)]
pub async fn ai_image(
    ctx: Context<'_>,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let encoded_input = encode(&prompt);
    let client = &ctx.data().req_client;
    let resp = client
        .post("https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/bytedance/stable-diffusion-xl-lightning")
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")       
        .json(&json!({ "prompt": encoded_input }))
        .send()
        .await?;
    let image_data = resp.bytes().await?.to_vec();
    if !image_data.is_empty() {
        let file = CreateAttachment::bytes(image_data, "output.png");
        ctx.send(CreateReply::default().attachment(file)).await?;
    } else {
        ctx.send(CreateReply::default().content(format!("\"{}\" is too dangerous to ask", prompt)))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct FabseAIText {
    result: AiResponseText,
}
#[derive(Deserialize, Serialize)]
struct AiResponseText {
    response: String,
}

/// Did someone say AI text?
#[poise::command(slash_command, prefix_command)]
pub async fn ai_text(
    ctx: Context<'_>,
    #[description = "AI personality, e.g. *you're an evil assistant*"] role: String,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let encoded_role = encode(&role);
    let encoded_input = encode(&prompt);
    let client = &ctx.data().req_client;
    let resp = client
        .post("https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/meta/llama-3-8b-instruct")
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")       
        .json(&json!({        
            "messages": [
                { "role": "system", "content": encoded_role },
                { "role": "user", "content": encoded_input }
            ]
        }))
        .send()
        .await?;
    let output: FabseAIText = resp.json().await?;
    if !output.result.response.is_empty() {
        let response_chars: Vec<char> = output.result.response.chars().collect();
        let chunks = response_chars.chunks(1024);
        let mut embed = CreateEmbed::default();
        embed = embed.title(&prompt).color(0xFF7800);
        for (i, chunk) in chunks.enumerate() {
            let chunk_str: String = chunk.iter().collect();
            let field_name = if i == 0 {
                "Response:".to_string()
            } else {
                format!("Response (cont. {})", i + 1)
            };
            embed = embed.field(field_name, chunk_str, false);
        }
        ctx.send(CreateReply::default().embed(embed)).await?;
    } else {
        ctx.send(CreateReply::default().content(format!("\"{}\" is too dangerous to ask", prompt)))
            .await?;
    }
    Ok(())
}

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

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
struct GifResponse {
    results: Vec<GifData>,
}
#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
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

#[poise::command(slash_command, prefix_command)]
pub async fn roast(
    ctx: Context<'_>,
    #[description = "Target"] user: poise::serenity_prelude::User,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().unwrap();
    let guild_roles = {
        let guild = ctx.partial_guild().await.unwrap();
        guild.roles.clone()
    };
    let member = ctx.http().get_member(guild_id, user.id).await?;
    let avatar_url = member.avatar_url().unwrap_or(user.avatar_url().unwrap());
    let banner_url = ctx.http().get_user(user.id).await.unwrap().banner_url().unwrap_or("user has no banner".to_string());
    let roles: Vec<String> = member.roles.iter()
        .filter_map(|role_id| guild_roles.get(role_id))
        .map(|role| role.name.clone().to_string())
        .collect();
    let name = member.nick.unwrap_or(user.name.clone());
    let account_date = user.created_at();
    let join_date = member.joined_at.unwrap();
    let message_count = {
        let id: u64 = guild_id.into();
        let mut conn = ctx.data().db.acquire().await?;
        let result = sqlx::query(
        "SELECT messages FROM message_count WHERE guild_id = ? AND user_name = ?",
        )
        .bind(id)
        .bind(user.name.to_string())
        .fetch_one(&mut *conn)
        .await;
        let result_filtered: Option<u64> = match result {
            Ok(row) => Some(row.try_get("messages").unwrap()),
            Err(_) => None,
        };
        result_filtered.map_or("unknown message count".to_string(), |count| count.to_string())
    };
    let mut messages = ctx.channel_id().messages_iter(&ctx).boxed();

    let messages_string = {
        let mut collected_messages = Vec::new();
        let mut count = 0;

        while let Some(message_result) = messages.next().await {
            if let Ok(message) = message_result {
                if message.author.id == user.id {
                    let formatted_message = format!("message {}: {}", count + 1, message.content);
                    collected_messages.push(formatted_message);
                    count += 1;
                    
                }
            } else {
                break;
            }
            if count >= 40 {
                break;
            }
        }
  
        collected_messages.join(", ")
    };
    
    let description = format!("name: {}, avatar: {}, banner: {}, roles: {}, account creation date: {}, joined server date: {}, message count in server: {}, last 10 messages: {}", name, avatar_url, banner_url, roles.join(", "), account_date, join_date, message_count, messages_string);
    let encoded_input = encode(&description);

    let client = &ctx.data().req_client;
    let resp = client
        .post("https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/meta/llama-3-8b-instruct")
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")
        .json(&json!({
            "messages": [
                { "role": "system", "content": "you're an evil ai assistant that excels at roasting ppl, especially weebs, and you show no mercy. every prompt will contain some information of your target" },
                { "role": "user", "content": encoded_input }
            ]
        }))
        .send()
        .await?;
    let output: FabseAIText = resp.json().await?;

    if !output.result.response.is_empty() {
        let response_chars: Vec<char> = output.result.response.chars().collect();
        let chunks = response_chars.chunks(1024);
        let mut embed = CreateEmbed::default();
        embed = embed.title(format!("Roasting {}", name)).color(0xFF7800);
        for (i, chunk) in chunks.enumerate() {
            let chunk_str: String = chunk.iter().collect();
            let field_name = if i == 0 {
                "Response:".to_string()
            } else {
                format!("Response (cont. {})", i + 1)
            };
            embed = embed.field(field_name, chunk_str, false);
        }
        ctx.send(CreateReply::default().embed(embed)).await?;
    } else {
        ctx.send(CreateReply::default().content(format!("{}'s life is already roasted", name)))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
struct UrbanResponse {
    list: Vec<UrbanDict>,
}
#[derive(Deserialize, Serialize)]
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
        let response_chars: Vec<char> = data.list[0].definition.chars().collect();
        let chunks = response_chars.chunks(1024);
        let mut embed = CreateEmbed::default();
        embed = embed.title(&data.list[0].word).color(0xEFFF00);
        for (i, chunk) in chunks.enumerate() {
            let chunk_str: String = chunk.iter().collect();
            let field_name = if i == 0 {
                "Definition:".to_string()
            } else {
                format!("Response (cont. {})", i + 1)
            };
            embed = embed.field(field_name, chunk_str, false);
        }
        embed = embed.field(
            "Example:",
            data.list[0].example.replace(['[', ']'], ""),
            false,
        );
        ctx.send(CreateReply::default().embed(embed)).await?;
    } else {
        ctx.send(CreateReply::default().content(format!("like you, {} don't exist", input)))
            .await?;
    }
    Ok(())
}
