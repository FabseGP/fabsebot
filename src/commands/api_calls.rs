use crate::{
    types::{
        Error, SContext, CLOUDFLARE_GATEWAY, CLOUDFLARE_TOKEN, GITHUB_TOKEN, HTTP_CLIENT, RNG,
        TRANSLATE_SERVER,
    },
    utils::{ai_response_simple, get_gifs, get_waifu},
};

use base64::{engine::general_purpose, Engine as _};
use poise::{
    serenity_prelude::{
        futures::StreamExt as _, small_fixed_array::FixedString, ButtonStyle,
        ComponentInteractionCollector, CreateActionRow, CreateAttachment, CreateButton,
        CreateEmbed, CreateInteractionResponse, EditMessage, Member, MessageId,
    },
    CreateReply,
};
use serde::{Deserialize, Serialize};
use sqlx::query;
use std::{borrow::Cow, iter, time::Duration};
use urlencoding::encode;

struct State {
    next_id: String,
    prev_id: String,
    index: usize,
    len: usize,
}

#[derive(Deserialize)]
struct FabseAIImage {
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
    let rand_num = RNG.lock().await.usize(0..1024);
    let request = ImageRequest {
        prompt: format!("{prompt} {rand_num}"),
    };
    let resp = HTTP_CLIENT
        .post(format!(
            "https://gateway.ai.cloudflare.com/v1/{}/workers-ai/@cf/black-forest-labs/flux-1-schnell",
            *CLOUDFLARE_GATEWAY
        ))
        .bearer_auth(&*CLOUDFLARE_TOKEN)
        .json(&request)
        .send()
        .await?;
    match resp
        .json::<FabseAIImage>()
        .await
        .ok()
        .filter(|output| !output.result.image.is_empty())
        .and_then(|output| general_purpose::STANDARD.decode(output.result.image).ok())
    {
        Some(image_bytes) => {
            ctx.send(
                CreateReply::default()
                    .attachment(CreateAttachment::bytes(image_bytes, "output.png")),
            )
            .await?;
        }
        None => {
            ctx.send(
                CreateReply::default()
                    .content(format!("\"{prompt}\" is too dangerous to generate")),
            )
            .await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct FabseAISummary {
    result: AIResponseSummary,
}
#[derive(Deserialize)]
struct AIResponseSummary {
    summary: String,
}

#[derive(Serialize)]
struct SummarizeRequest {
    input_text: FixedString<u16>,
    length: u64,
}

/// Did someone say AI summarize?
#[poise::command(prefix_command, slash_command)]
pub async fn ai_summarize(
    ctx: SContext<'_>,
    #[description = "Maximum length of summary in words"] length: u64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let msg = ctx
        .channel_id()
        .message(&ctx.http(), MessageId::from(ctx.id()))
        .await?;
    let Some(reply) = msg.referenced_message else {
        ctx.reply("Bruh, reply to a message").await?;
        return Ok(());
    };
    let request = SummarizeRequest {
        input_text: reply.content,
        length,
    };
    let resp = HTTP_CLIENT
        .post(format!(
            "https://gateway.ai.cloudflare.com/v1/{}/workers-ai/@cf/facebook/bart-large-cnn",
            *CLOUDFLARE_GATEWAY
        ))
        .bearer_auth(&*CLOUDFLARE_TOKEN)
        .json(&request)
        .send()
        .await?;
    match resp.json::<FabseAISummary>().await {
        Ok(output) if !output.result.summary.is_empty() => {
            ctx.say(output.result.summary).await?;
        }
        _ => {
            ctx.send(CreateReply::default().content("This is too much work"))
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
        Ok(resp) if !resp.is_empty() => {
            let mut embed = CreateEmbed::default().title(prompt).color(0xFF7800);
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
        Ok(_) | Err(_) => {
            ctx.send(
                CreateReply::default().content(format!("\"{prompt}\" is too dangerous to ask")),
            )
            .await?;
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct GraphQLQuery {
    query: String,
    variables: AnimeVariables,
}

#[derive(Serialize)]
struct AnimeVariables {
    search: String,
}

#[derive(Deserialize)]
struct AnimeResponse {
    data: AnimeData,
}

#[derive(Deserialize)]
struct AnimeData {
    #[serde(rename = "Media")]
    media: Option<Media>,
}

#[derive(Deserialize)]
struct Media {
    id: i32,
    title: AnimeTitle,
}

#[derive(Deserialize)]
struct AnimeTitle {
    romaji: Option<String>,
}

/// When the other bot sucks
#[poise::command(
    prefix_command,
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn anilist_anime(
    ctx: SContext<'_>,
    #[description = "Anime to search"]
    #[rest]
    anime: String,
) -> Result<(), Error> {
    let query = GraphQLQuery {
        query: "
            query ($search: String) {
                Media(search: $search, type: ANIME) {
                    id
                    title {
                        romaji
                        english
                        native
                    }
                }
            }
        "
        .to_owned(),
        variables: AnimeVariables { search: anime },
    };

    let resp = HTTP_CLIENT
        .post("https://graphql.anilist.co/")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&query)
        .send()
        .await?;

    let data: AnimeResponse = resp.json().await?;

    let Some(media) = data.data.media else {
        ctx.reply("No anime found with that name").await?;
        return Ok(());
    };

    let title = media.title.romaji.unwrap_or_default();

    let embed = CreateEmbed::default()
        .title("Anime")
        .field("ID", media.id.to_string(), false)
        .field("Title", title, false)
        .color(0x33d17a);

    ctx.send(CreateReply::default().embed(embed)).await?;
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
    let encoded_input = encode(&question);
    let request_url =
        format!("https://eightballapi.com/api/biased?question={encoded_input}&lucky=false");
    let request = HTTP_CLIENT.get(request_url).send().await?;
    let judging: EightBallResponse = request.json().await?;
    if !judging.reading.is_empty() {
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::default()
                    .title(question)
                    .color(0x33d17a)
                    .field("", &judging.reading, true),
            ),
        )
        .await?;
    } else {
        ctx.send(CreateReply::default().content("Sometimes riding a giraffe is what you need"))
            .await?;
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
    let resp = get_gifs(&input).await;
    if let Ok(urls) = resp {
        ctx.defer().await?;
        let embed = CreateEmbed::default().title(input.as_str()).image(&urls[0]);
        let len = urls.len();
        if ctx.guild_id().is_some() && len > 1 {
            let index = 0;
            let ctx_id = ctx.id();
            let next_id = format!("{ctx_id}_next_{index}");
            let prev_id = format!("{ctx_id}_prev_{index}");
            let mut state = State {
                next_id,
                prev_id,
                index,
                len,
            };

            let next_button = [CreateButton::new(&state.next_id)
                .style(ButtonStyle::Primary)
                .label("➡️")];
            ctx.send(
                CreateReply::default()
                    .embed(embed)
                    .components(&[CreateActionRow::buttons(&next_button)]),
            )
            .await?;
            while let Some(interaction) =
                ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                    .timeout(Duration::from_secs(600))
                    .filter(move |interaction| {
                        let id = interaction.data.custom_id.as_str();
                        id == state.next_id.as_str() || id == state.prev_id.as_str()
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

                let state_index = state.index;
                state.next_id = format!("{ctx_id}_next_{state_index}");
                state.prev_id = format!("{ctx_id}_prev_{state_index}");

                let buttons = [
                    CreateButton::new(&state.prev_id)
                        .style(ButtonStyle::Primary)
                        .label("⬅️"),
                    CreateButton::new(&state.next_id)
                        .style(ButtonStyle::Primary)
                        .label("➡️"),
                ];

                let new_embed = CreateEmbed::default()
                    .title(input.as_str())
                    .image(&urls[state.index]);

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
        } else {
            ctx.send(CreateReply::default().embed(embed)).await?;
        }
    } else {
        ctx.send(CreateReply::default().content("Life is not gifing"))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct GithubResponse {
    items: Vec<GithubSearch>,
}
#[derive(Deserialize)]
struct GithubSearch {
    url: String,
}

/// When you need open source in your life
#[poise::command(prefix_command, slash_command)]
pub async fn github_search(
    ctx: SContext<'_>,
    #[description = "Search query"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    let request = HTTP_CLIENT
        .get(format!("https://api.github.com/search/code?q={input}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "fabseman")
        .bearer_auth(&*GITHUB_TOKEN)
        .send()
        .await?;
    let data: GithubResponse = request.json().await?;
    if !data.items.is_empty() {
        ctx.say(data.items[0].url.as_str()).await?;
    } else {
        ctx.send(CreateReply::default().content(format!("**Like you, {input} don't exist**")))
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
    let request = HTTP_CLIENT.get(request_url).send().await?;
    let data: JokeResponse = request.json().await?;
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
        ctx.send(CreateReply::default().content(roasts[RNG.lock().await.usize(..roasts.len())]))
            .await?;
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
    let encoded_left = encode(&top_left);
    let encoded_right = encode(&top_right);
    let encoded_bottom = encode(&bottom);
    let request_url = format!(
        "https://api.memegen.link/images/exit/{encoded_left}/{encoded_right}/{encoded_bottom}.png"
    );
    ctx.send(CreateReply::default().content(request_url))
        .await?;
    Ok(())
}

/// When someone offended you
#[poise::command(prefix_command, slash_command)]
pub async fn roast(
    ctx: SContext<'_>,
    #[description = "Target"] member: Member,
) -> Result<(), Error> {
    ctx.defer().await?;
    if let Some(guild_id) = ctx.guild_id() {
        let avatar_url = member
            .avatar_url()
            .unwrap_or_else(|| member.user.avatar_url().unwrap());
        let banner_url = (ctx.http().get_user(member.user.id).await).map_or_else(
            |_| "user has no banner".to_owned(),
            |user| {
                user.banner_url()
                    .map_or_else(|| "user has no banner".to_owned(), |banner| banner)
            },
        );
        let roles = {
            if let Some(guild) = ctx.guild() {
                let mut roles_iter = member
                    .roles
                    .iter()
                    .filter_map(|role_id| guild.roles.get(role_id))
                    .map(|role| role.name.as_str());
                roles_iter.next().map_or_else(
                    || "no roles".to_string(),
                    |first_role| {
                        iter::once(first_role)
                            .chain(roles_iter)
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                )
            } else {
                "no roles".to_string()
            }
        };
        let name = member.display_name();
        let account_date = member.user.created_at();
        let join_date = member.joined_at.unwrap();
        let message_count = {
            let mut conn = ctx.data().db.acquire().await?;
            let result = query!(
                "SELECT message_count FROM user_settings WHERE guild_id = $1 AND user_id = $2",
                i64::from(guild_id),
                i64::from(member.user.id),
            )
            .fetch_one(&mut *conn)
            .await;
            result.map_or(0, |count| count.message_count)
        };
        let mut messages = ctx.channel_id().messages_iter(&ctx).boxed();

        let messages_string = {
            let mut result = String::new();
            let mut count = 0;

            while let Some(message_result) = messages.next().await {
                match message_result {
                    Ok(message) => {
                        if message.author.id == member.user.id {
                            let index = count + 1;
                            if count > 0 {
                                result.push(',');
                            }
                            result.push_str(&index.to_string());
                            result.push(':');
                            result.push_str(&message.content);

                            count += 1;
                        }
                    }
                    Err(_) => break,
                }
                if count >= 25 {
                    break;
                }
            }

            result
        };

        let description = format!("name:{name},avatar:{avatar_url},banner:{banner_url},roles:{roles},acc_create:{account_date},joined_svr:{join_date},msg_count:{message_count},last_msgs:{messages_string}");
        let role = "you're an evil ai assistant that excels at roasting ppl, especially weebs. no mercy shown. the prompt will contain information of your target";
        let resp = ai_response_simple(role, &description).await?;

        if !resp.is_empty() {
            let mut embed = CreateEmbed::default()
                .title(format!("Roasting {name}"))
                .color(0xFF7800);
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
        } else {
            ctx.send(CreateReply::default().content(format!("{name}'s life is already roasted")))
                .await?;
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
                Some(ref_msg) => ref_msg.content.into_string(),
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
    let response = HTTP_CLIENT
        .post(&*TRANSLATE_SERVER)
        .json(&request)
        .send()
        .await?;

    if response.status().is_success() {
        let data: FabseTranslate = response.json().await?;
        if !data.translated_text.is_empty() {
            let embed = CreateEmbed::default()
                .title(format!(
                    "Translation from {} to {target_lang} with {}% confidence",
                    data.detected_language.language, data.detected_language.confidence
                ))
                .color(0x33d17a)
                .field("Original:", &content, false)
                .field("Translation:", &data.translated_text, false);
            let len = data.alternatives.len();
            if ctx.guild_id().is_some() && len > 1 {
                let index = 0;
                let ctx_id = ctx.id();
                let next_id = format!("{ctx_id}_next_{index}");
                let prev_id = format!("{ctx_id}_prev_{index}");
                let mut state = State {
                    next_id,
                    prev_id,
                    index,
                    len,
                };
                let next_button = [CreateButton::new(&state.next_id)
                    .style(ButtonStyle::Primary)
                    .label("➡️")];
                ctx.send(
                    CreateReply::default()
                        .embed(embed)
                        .components(&[CreateActionRow::buttons(&next_button)]),
                )
                .await?;
                while let Some(interaction) =
                    ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                        .timeout(Duration::from_secs(600))
                        .filter(move |interaction| {
                            let id = interaction.data.custom_id.as_str();
                            id == state.next_id.as_str() || id == state.prev_id.as_str()
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

                    let state_index = state.index;
                    state.next_id = format!("{ctx_id}_next_{state_index}");
                    state.prev_id = format!("{ctx_id}_prev_{state_index}");

                    let buttons = [
                        CreateButton::new(&state.prev_id)
                            .style(ButtonStyle::Primary)
                            .label("⬅️"),
                        CreateButton::new(&state.next_id)
                            .style(ButtonStyle::Primary)
                            .label("➡️"),
                    ];

                    let new_embed = CreateEmbed::default()
                        .title(format!(
                            "Translation from {} to {target_lang} with {}% confidence",
                            data.detected_language.language, data.detected_language.confidence
                        ))
                        .color(0x33d17a)
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
            } else {
                ctx.send(CreateReply::default().embed(embed)).await?;
            }
        } else {
            ctx.send(CreateReply::default().content("Too dangerous to translate"))
                .await?;
        }
    } else {
        ctx.send(CreateReply::default().content("My translator is currently busy, pls standby"))
            .await?;
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
    let encoded_input = encode(&input);
    let request_url = format!("https://api.urbandictionary.com/v0/define?term={encoded_input}");
    let request = HTTP_CLIENT.get(request_url).send().await?;
    let data: UrbanResponse = request.json().await?;
    if !data.list.is_empty() {
        let mut embed = CreateEmbed::default()
            .title(&data.list[0].word)
            .color(0xEFFF00);
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
            let index = 0;
            let ctx_id = ctx.id();
            let next_id = format!("{ctx_id}_next_{index}");
            let prev_id = format!("{ctx_id}_prev_{index}");
            let mut state = State {
                next_id,
                prev_id,
                index,
                len,
            };

            let next_button = [CreateButton::new(&state.next_id)
                .style(ButtonStyle::Primary)
                .label("➡️")];
            ctx.send(
                CreateReply::default()
                    .embed(embed)
                    .components(&[CreateActionRow::buttons(&next_button)]),
            )
            .await?;
            while let Some(interaction) =
                ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                    .timeout(Duration::from_secs(600))
                    .filter(move |interaction| {
                        let id = interaction.data.custom_id.as_str();
                        id == state.next_id.as_str() || id == state.prev_id.as_str()
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

                let state_index = state.index;
                state.next_id = format!("{ctx_id}_next_{state_index}");
                state.prev_id = format!("{ctx_id}_prev_{state_index}");

                let buttons = [
                    CreateButton::new(&state.prev_id)
                        .style(ButtonStyle::Primary)
                        .label("⬅️"),
                    CreateButton::new(&state.next_id)
                        .style(ButtonStyle::Primary)
                        .label("➡️"),
                ];

                let mut new_embed = CreateEmbed::default()
                    .title(&data.list[state.index].word)
                    .color(0xEFFF00);
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
                    data.list[state_index].example.replace(['[', ']'], ""),
                    false,
                );

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
        } else {
            ctx.send(CreateReply::default().embed(embed)).await?;
        }
    } else {
        ctx.send(CreateReply::default().content(format!("**Like you, {input} don't exist**")))
            .await?;
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
    let resp = get_waifu().await;
    match resp {
        Ok(url) => {
            ctx.send(CreateReply::default().content(url)).await?;
        }
        Err(_) => {
            ctx.send(CreateReply::default().content("life is not waifuing"))
                .await?;
        }
    }
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
    let encoded_input = encode(&input);
    let request_url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{encoded_input}");
    let request = HTTP_CLIENT.get(request_url).send().await?;
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
                .color(0xFFBE6F);
            if let Some(image) = data.originalimage {
                embed = embed.image(image.source);
            }
            ctx.send(CreateReply::default().embed(embed)).await?;
        }
        None => {
            ctx.send(CreateReply::default().content(format!("**Like you, {input} don't exist**")))
                .await?;
        }
    }
    Ok(())
}
