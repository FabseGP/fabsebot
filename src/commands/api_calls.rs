use crate::types::{Context, Error};
use crate::utils::{get_gif, random_number};

use poise::serenity_prelude::{
    ButtonStyle, ComponentInteractionCollector, CreateActionRow, CreateAttachment, CreateButton,
    CreateEmbed, CreateInteractionResponse, EditMessage,
};
use poise::CreateReply;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::futures::StreamExt;
use sqlx::{query, Row};
use std::time::Duration;
use urlencoding::encode;

#[derive(Deserialize, Serialize)]
struct EventResponse {
    event_id: String,
}

/// Anime image
#[poise::command(prefix_command, slash_command)]
pub async fn ai_anime(
    ctx: Context<'_>,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let url = "https://cagliostrolab-animagine-xl-3-1.hf.space/call/run";
    let client = &ctx.data().req_client;
    let request_body = json!({
        "data": [
            prompt,
            "",
            random_number(2147483647),
            2048,
            2048,
            7,
            28,
            "Euler a",
            "1024 x 1024",
            "(None)",
            "Heavy v3.1",
            false,
            0,
            1,
            true,
        ]
    });
    let resp = client.post(url).json(&request_body).send().await?;
    let output: EventResponse = resp.json().await?;
    if !output.event_id.is_empty() {
        let status_url = format!("{}/{}", url, output.event_id);
        let path_regex = Regex::new(r#""path":\s*"(.*?)""#).unwrap();
        loop {
            let status_resp = client.get(&status_url).send().await?;
            let status_text = status_resp.text().await?;
            if status_text.contains("event: complete") {
                if let Some(captures) = path_regex.captures(&status_text) {
                    if let Some(path) = captures.get(1) {
                        let image_url = format!(
                            "https://cagliostrolab-animagine-xl-3-1.hf.space/file={}",
                            path.as_str()
                        );
                        let image_data = client.get(&image_url).send().await?;
                        let image_data = image_data.bytes().await?.to_vec();
                        let file = CreateAttachment::bytes(image_data, "output.png");
                        ctx.send(CreateReply::default().attachment(file)).await?;
                        break;
                    }
                }
            }
        }
    } else {
        ctx.send(
            CreateReply::default().content(format!("\"{}\" is too dangerous to generate", prompt)),
        )
        .await?;
    }
    Ok(())
}

/// Did someone say AI image?
#[poise::command(prefix_command, slash_command)]
pub async fn ai_image(
    ctx: Context<'_>,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let client = &ctx.data().req_client;
    let models = [
        "https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/lykon/dreamshaper-8-lcm", 
        "https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/bytedance/stable-diffusion-xl-lightning"
    ];
    let resp = client
        .post(models[random_number(models.len())])
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")
        .json(&json!({ "prompt": prompt }))
        .send()
        .await?;
    let image_data = resp.bytes().await?.to_vec();
    if !image_data.is_empty() {
        let file = CreateAttachment::bytes(image_data, "output.png");
        ctx.send(CreateReply::default().attachment(file)).await?;
    } else {
        ctx.send(
            CreateReply::default().content(format!("\"{}\" is too dangerous to generate", prompt)),
        )
        .await?;
    }
    Ok(())
}

/// Midjourney image
#[poise::command(prefix_command, slash_command)]
pub async fn ai_midjourney(
    ctx: Context<'_>,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let url = "https://mukaist-Midjourney.hf.space/call/run";
    let client = &ctx.data().req_client;
    let request_body = json!({
        "data": [
            prompt,
            "(deformed iris, deformed pupils, semi-realistic, cgi, 3d, render, sketch, cartoon, drawing, anime:1.4), text, close up, cropped, out of frame, worst quality, low quality, jpeg artifacts, ugly, duplicate, morbid, mutilated, extra fingers, mutated hands, poorly drawn hands, poorly drawn face, mutation, deformed, blurry, dehydrated, bad anatomy, bad proportions, extra limbs, cloned face, disfigured, gross proportions, malformed limbs, missing arms, missing legs, extra arms, extra legs, fused fingers, too many fingers, long neck)",
            true,
            "2560 x 1440",
            0,
            2048,
            2048,
            6,
            true
        ]
    });
    let resp = client.post(url).json(&request_body).send().await?;
    let output: EventResponse = resp.json().await?;
    if !output.event_id.is_empty() {
        let status_url = format!("{}/{}", url, output.event_id);
        let path_regex = Regex::new(r#""path":\s*"(.*?)""#).unwrap();
        loop {
            let status_resp = client.get(&status_url).send().await?;
            let status_text = status_resp.text().await?;
            if status_text.contains("event: complete") {
                if let Some(captures) = path_regex.captures(&status_text) {
                    if let Some(path) = captures.get(1) {
                        let image_url =
                            format!("https://mukaist-midjourney.hf.space/file={}", path.as_str());
                        let image_data = client.get(&image_url).send().await?;
                        let image_data = image_data.bytes().await?.to_vec();
                        let file = CreateAttachment::bytes(image_data, "output.png");
                        ctx.send(CreateReply::default().attachment(file)).await?;
                        break;
                    }
                }
            }
        }
    } else {
        ctx.send(
            CreateReply::default().content(format!("\"{}\" is too dangerous to generate", prompt)),
        )
        .await?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct FabseAISummary {
    result: AIResponseSummary,
}
#[derive(Deserialize, Serialize)]
struct AIResponseSummary {
    summary: String,
}

/// Did someone say AI summarize?
#[poise::command(prefix_command, slash_command)]
pub async fn ai_summarize(
    ctx: Context<'_>,
    #[description = "Maximum length of summary in words"] length: u64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let reply = match ctx
        .channel_id()
        .message(&ctx.http(), ctx.id().into())
        .await?
    {
        msg if msg.referenced_message.is_some() => msg.referenced_message.unwrap(),
        _ => {
            ctx.say("bruh, reply to a message").await?;
            return Ok(());
        }
    };
    let client = &ctx.data().req_client;
    let resp = client
        .post("https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/facebook/bart-large-cnn")
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")       
        .json(&json!({"input_text": reply.content.to_string(),
            "max_length": length 
        }))
        .send()
        .await?;
    let output: FabseAISummary = resp.json().await?;
    if !output.result.summary.is_empty() {
        ctx.say(output.result.summary).await?;
    } else {
        ctx.send(CreateReply::default().content("this is too much work"))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct FabseAIText {
    result: AIResponseText,
}
#[derive(Deserialize, Serialize)]
struct AIResponseText {
    response: String,
}

/// Did someone say AI text?
#[poise::command(prefix_command, slash_command)]
pub async fn ai_text(
    ctx: Context<'_>,
    #[description = "AI personality, e.g. *you're an evil assistant*"] role: String,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let client = &ctx.data().req_client;
    let resp = client
        .post("https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/meta/llama-3-8b-instruct")
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")       
        .json(&json!({"messages": [
                { "role": "system", "content": role },
                { "role": "user", "content": prompt }
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
#[poise::command(prefix_command, slash_command)]
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
#[poise::command(prefix_command, slash_command)]
pub async fn eightball(
    ctx: Context<'_>,
    #[description = "Your question"]
    #[rest]
    question: String,
) -> Result<(), Error> {
    let request_url = format!(
        "https://eightballapi.com/api/biased?question={query}&lucky=false",
        query = encode(&question)
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

/// Gifing
#[poise::command(prefix_command, slash_command)]
pub async fn gif(
    ctx: Context<'_>,
    #[description = "Search gif"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    let url = get_gif(input).await;
    ctx.send(CreateReply::default().content(url)).await?;
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct JokeResponse {
    joke: String,
}

/// When your life isn't fun anymore
#[poise::command(prefix_command, slash_command)]
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
#[poise::command(prefix_command, slash_command)]
pub async fn memegen(
    ctx: Context<'_>,
    #[description = "Top-left text"] top_left: String,
    #[description = "Top-right text"] top_right: String,
    #[description = "Bottom text"] bottom: String,
) -> Result<(), Error> {
    let request_url = format!(
        "https://api.memegen.link/images/exit/{left}/{right}/{bottom}.png",
        left = encode(&top_left),
        right = encode(&top_right),
        bottom = encode(&bottom)
    );
    ctx.send(CreateReply::default().content(request_url))
        .await?;
    Ok(())
}

/// When someone offended you
#[poise::command(prefix_command, slash_command)]
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
    let banner_url = ctx
        .http()
        .get_user(user.id)
        .await
        .unwrap()
        .banner_url()
        .unwrap_or("user has no banner".to_string());
    let roles: Vec<String> = member
        .roles
        .iter()
        .filter_map(|role_id| guild_roles.get(role_id))
        .map(|role| role.name.clone().to_string())
        .collect();
    let name = member.nick.unwrap_or(user.name.clone());
    let account_date = user.created_at();
    let join_date = member.joined_at.unwrap();
    let message_count = {
        let id: u64 = guild_id.into();
        let mut conn = ctx.data().db.acquire().await?;
        let result =
            query("SELECT messages FROM message_count WHERE guild_id = ? AND user_name = ?")
                .bind(id)
                .bind(user.name.to_string())
                .fetch_one(&mut *conn)
                .await;
        let result_filtered: Option<u64> = match result {
            Ok(row) => Some(row.try_get("messages").unwrap()),
            Err(_) => None,
        };
        result_filtered.map_or("unknown message count".to_string(), |count| {
            count.to_string()
        })
    };
    let mut messages = ctx.channel_id().messages_iter(&ctx).boxed();

    let messages_string = {
        let mut collected_messages = Vec::new();
        let mut count = 0;

        while let Some(message_result) = messages.next().await {
            if let Ok(message) = message_result {
                if message.author.id == user.id {
                    let formatted_message = format!("{}:{}", count + 1, message.content);
                    collected_messages.push(formatted_message);
                    count += 1;
                }
            } else {
                break;
            }
            if count >= 25 {
                break;
            }
        }

        collected_messages.join(",")
    };

    let description = format!("name:{},avatar:{},banner:{},roles:{},acc_create:{},joined_svr:{},msg_count:{},last_msgs:{}", name, avatar_url, banner_url, roles.join(","), account_date, join_date, message_count, messages_string);

    let client = &ctx.data().req_client;
    let resp = client
        .post("https://gateway.ai.cloudflare.com/v1/dbc36a22e79dd7acf1ed94aa596bb44e/fabsebot/workers-ai/@cf/meta/llama-3-8b-instruct")
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")
        .json(&json!({
            "messages": [
                { "role": "system", "content": "you're an evil ai assistant that excels at roasting ppl, especially weebs. no mercy shown. the prompt will contain information of your target" },
                { "role": "user", "content": description }
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
struct FabseTranslate {
    #[serde(rename = "detectedLanguage")]
    detected_language: FabseLanguage,
    #[serde(rename = "translatedText")]
    translated_text: String,
}

#[derive(Deserialize, Serialize)]
struct FabseLanguage {
    confidence: f64,
    language: String,
}

/// When you stumble on some ancient sayings
#[poise::command(prefix_command, slash_command)]
pub async fn translate(
    ctx: Context<'_>,
    #[description = "Language to be translated to, e.g. en"] target: Option<String>,
    #[description = "What should be translated"]
    #[rest]
    sentence: Option<String>,
) -> Result<(), Error> {
    let content = match ctx
        .channel_id()
        .message(&ctx.http(), ctx.id().into())
        .await?
    {
        msg if msg.referenced_message.is_some() => {
            msg.referenced_message.unwrap().content.to_string()
        }
        _ => {
            if let Some(query) = sentence {
                query
            } else {
                ctx.say("bruh, give me smth to translate").await?;
                return Ok(());
            }
        }
    };
    let target_lang = if let Some(language) = target {
        language
    } else {
        "en".to_string()
    };
    let form_data = json!({
        "q": content,
        "source": "auto",
        "target": target_lang
    });
    let client = &ctx.data().req_client;
    let response = client
        .post("https://translate.fabseman.space/translate")
        .bearer_auth("5UDCidIPqJWWrUZKQPLAncYPYBd6zHH1IJBTLh2r")
        .json(&form_data)
        .send()
        .await?;

    if response.status().is_success() {
        let data: FabseTranslate = response.json().await?;
        if !data.translated_text.is_empty() {
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title(format!(
                            "Translation from {} to {} with {}% confidence",
                            data.detected_language.language,
                            target_lang,
                            data.detected_language.confidence
                        ))
                        .color(0x33d17a)
                        .field("Original:", content, false)
                        .field("Translation:", &data.translated_text, false),
                ),
            )
            .await?;
        } else {
            ctx.send(CreateReply::default().content("too dangerous to translate"))
                .await?;
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

struct UrbanState {
    next_id: String,
    prev_id: String,
    index: usize,
    len: usize,
}

/// The holy moly urbandictionary
#[poise::command(prefix_command, slash_command)]
pub async fn urban(
    ctx: Context<'_>,
    #[description = "Word(s) to lookup"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    ctx.defer().await.unwrap();
    let request_url = format!(
        "https://api.urbandictionary.com/v0/define?term={search}",
        search = encode(&input)
    );
    let client = &ctx.data().req_client;
    let request = client.get(request_url).send().await?;
    let data: UrbanResponse = request.json().await.unwrap();
    if !data.list.is_empty() {
        let len = data.list.len();
        let index = 0;
        let next_id = format!("{}_next_{}", ctx.id(), index);
        let prev_id = format!("{}_prev_{}", ctx.id(), index);
        let mut state = UrbanState {
            next_id: next_id.clone(),
            prev_id: prev_id.clone(),
            index,
            len,
        };
        let next_button = CreateActionRow::Buttons(vec![CreateButton::new(&state.next_id)
            .style(ButtonStyle::Primary)
            .label("➡️")]);
        let components = if len > 1 { vec![next_button] } else { vec![] };
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
            embed = embed.field(field_name, chunk_str.replace(['[', ']'], ""), false);
        }
        embed = embed.field(
            "Example:",
            data.list[0].example.replace(['[', ']'], ""),
            false,
        );

        ctx.send(CreateReply::default().embed(embed).components(components))
            .await?;

        if len > 1 {
            while let Some(interaction) =
                ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                    .timeout(Duration::from_secs(600))
                    .filter(move |interaction| {
                        let next_id_clone = state.next_id.clone();
                        let prev_id_clone = state.prev_id.clone();
                        let id = interaction.data.custom_id.as_str();
                        id == next_id_clone.as_str() || id == prev_id_clone.as_str()
                    })
                    .await
            {
                let choice = &interaction.data.custom_id.as_str();

                interaction
                    .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                    .await?;

                if choice.contains("next") && state.index < state.len - 1 {
                    state.index += 1;
                } else if choice.contains("prev") && state.index > 0 {
                    state.index -= 1;
                }

                state.next_id = format!("{}_next_{}", ctx.id(), state.index);
                state.prev_id = format!("{}_prev_{}", ctx.id(), state.index);

                let next_button = CreateButton::new(&state.next_id)
                    .style(ButtonStyle::Primary)
                    .label("➡️");

                let prev_button = CreateButton::new(&state.prev_id)
                    .style(ButtonStyle::Primary)
                    .label("⬅️");

                let new_response_chars: Vec<char> =
                    data.list[state.index].definition.chars().collect();
                let new_chunks = new_response_chars.chunks(1024);

                let mut new_embed = CreateEmbed::default();
                new_embed = new_embed
                    .title(&data.list[state.index].word)
                    .color(0xEFFF00);

                for (i, chunk) in new_chunks.enumerate() {
                    let chunk_str: String = chunk.iter().collect();
                    let field_name = if i == 0 {
                        "Definition:".to_string()
                    } else {
                        format!("Response (cont. {})", i + 1)
                    };
                    new_embed =
                        new_embed.field(field_name, chunk_str.replace(['[', ']'], ""), false);
                }

                new_embed = new_embed.field(
                    "Example:",
                    data.list[state.index].example.replace(['[', ']'], ""),
                    false,
                );

                let new_components = if state.index == 0 {
                    vec![CreateActionRow::Buttons(vec![next_button])]
                } else if state.index == len - 1 {
                    vec![CreateActionRow::Buttons(vec![prev_button])]
                } else {
                    vec![CreateActionRow::Buttons(vec![prev_button, next_button])]
                };

                let mut msg = interaction.message.clone();

                msg.edit(
                    ctx.http(),
                    EditMessage::new()
                        .embed(new_embed)
                        .components(new_components),
                )
                .await?;
            }
        }
    } else {
        ctx.send(CreateReply::default().content(format!("**like you, {} don't exist**", input)))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct WaifuResponse {
    images: Vec<WaifuData>,
}
#[derive(Deserialize, Serialize)]
struct WaifuData {
    url: String,
}

/// Do I need to explain it?
#[poise::command(prefix_command, slash_command)]
pub async fn waifu(ctx: Context<'_>) -> Result<(), Error> {
    let request_url = "https://api.waifu.im/search?height=>=2000&is_nsfw=false";
    let client = &ctx.data().req_client;
    let request = client.get(request_url).send().await?;
    let url: WaifuResponse = request.json().await.unwrap();
    if !url.images[0].url.is_empty() {
        ctx.send(CreateReply::default().content(&url.images[0].url))
            .await?;
    } else {
        ctx.send(CreateReply::default().content("life is not waifuing"))
            .await?;
    }
    Ok(())
}
