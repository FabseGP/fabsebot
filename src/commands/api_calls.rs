use crate::{
    types::{
        Error, SContext, CLOUDFLARE_GATEWAY, CLOUDFLARE_TOKEN, GITHUB_TOKEN, HTTP_CLIENT, RNG,
        TRANSLATE_SERVER,
    },
    utils::{ai_response_simple, get_gifs, get_waifu},
};

use poise::{
    serenity_prelude::{
        futures::StreamExt, small_fixed_array::FixedString, ButtonStyle,
        ComponentInteractionCollector, CreateActionRow, CreateAttachment, CreateButton,
        CreateEmbed, CreateInteractionResponse, EditMessage, MessageId, User,
    },
    CreateReply,
};
use serde::{Deserialize, Serialize};
use sqlx::query;
use std::{borrow::Cow, time::Duration};
use urlencoding::encode;

struct State {
    next_id: String,
    prev_id: String,
    index: usize,
    len: usize,
}

#[derive(Serialize)]
struct ImageRequest {
    prompt: String,
}

/// Did someone say AI image?
#[poise::command(prefix_command, slash_command)]
pub async fn ai_image(
    ctx: SContext<'_>,
    #[description = "Prompt"]
    #[rest]
    prompt: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let request = ImageRequest {
        prompt: prompt.to_owned(),
    };
    let resp = HTTP_CLIENT
        .post(format!(
            "https://gateway.ai.cloudflare.com/v1/{}/workers-ai/@cf/lykon/dreamshaper-8-lcm",
            *CLOUDFLARE_GATEWAY
        ))
        .bearer_auth(&*CLOUDFLARE_TOKEN)
        .json(&request)
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
    let reply = match msg.referenced_message {
        Some(ref_msg) => ref_msg,
        None => {
            ctx.reply("Bruh, reply to a message").await?;
            return Ok(());
        }
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
    let output: FabseAISummary = resp.json().await?;
    if !output.result.summary.is_empty() {
        ctx.say(output.result.summary).await?;
    } else {
        ctx.send(CreateReply::default().content("This is too much work"))
            .await?;
    }
    Ok(())
}

/// Did someone say AI text?
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
    let resp = ai_response_simple(&role, &prompt).await?;
    if !resp.is_empty() {
        let response_chars: Vec<char> = resp.chars().collect();
        let chunks = response_chars.chunks(1024);
        let fields: Vec<(String, String, bool)> = chunks
            .enumerate()
            .map(|(i, chunk)| {
                let field_name = match i {
                    0 => "Response:".to_owned(),
                    _ => format!("Response (cont. {}):", i + 1),
                };
                let chunk_str: String = chunk.iter().collect();
                (field_name, chunk_str, false)
            })
            .collect();
        let embed = CreateEmbed::default()
            .title(prompt)
            .color(0xFF7800)
            .fields(fields);
        ctx.send(CreateReply::default().embed(embed)).await?;
    } else {
        ctx.send(CreateReply::default().content(format!("\"{}\" is too dangerous to ask", prompt)))
            .await?;
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
#[poise::command(prefix_command, slash_command)]
pub async fn anilist_anime(
    ctx: SContext<'_>,
    #[description = "Anime to search"]
    #[rest]
    anime: String,
) -> Result<(), Error> {
    let query = GraphQLQuery {
        query: r#"
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
        "#
        .to_string(),
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

    let media = match data.data.media {
        Some(media) => media,
        None => {
            ctx.reply("No anime found with that name").await?;
            return Ok(());
        }
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
    let request_url = format!(
        "https://eightballapi.com/api/biased?question={}&lucky=false",
        encode(&question)
    );
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
#[poise::command(prefix_command, slash_command)]
pub async fn gif(
    ctx: SContext<'_>,
    #[description = "Search gif"]
    #[rest]
    input: String,
) -> Result<(), Error> {
    let resp = get_gifs(&input).await;
    if let Ok(urls) = resp {
        if ctx.guild_id().is_some() {
            let len = urls.len();
            let index = 0;
            let next_id = format!("{}_next_{}", ctx.id(), index);
            let prev_id = format!("{}_prev_{}", ctx.id(), index);
            let mut state = State {
                next_id,
                prev_id,
                index,
                len,
            };
            let embed = CreateEmbed::default().title(input.as_str()).image(&urls[0]);

            if len > 1 {
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

                    state.next_id = format!("{}_next_{}", ctx.id(), state.index);
                    state.prev_id = format!("{}_prev_{}", ctx.id(), state.index);

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
            let embed = CreateEmbed::default().title(input.as_str()).image(&urls[0]);
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
        .get(format!("https://api.github.com/search/code?q={}", input))
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
        ctx.send(CreateReply::default().content(format!("**Like you, {} don't exist**", input)))
            .await?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct JokeResponse {
    joke: String,
}

/// When your life isn't fun anymore
#[poise::command(prefix_command, slash_command)]
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
#[poise::command(prefix_command, slash_command)]
pub async fn memegen(
    ctx: SContext<'_>,
    #[description = "Top-left text"] top_left: String,
    #[description = "Top-right text"] top_right: String,
    #[description = "Bottom text"] bottom: String,
) -> Result<(), Error> {
    let request_url = format!(
        "https://api.memegen.link/images/exit/{}/{}/{}.png",
        encode(&top_left),
        encode(&top_right),
        encode(&bottom)
    );
    ctx.send(CreateReply::default().content(request_url))
        .await?;
    Ok(())
}

/// When someone offended you
#[poise::command(prefix_command, slash_command)]
pub async fn roast(ctx: SContext<'_>, #[description = "Target"] user: User) -> Result<(), Error> {
    ctx.defer().await?;
    if let Some(guild_id) = ctx.guild_id() {
        let guild = match ctx.guild() {
            Some(guild) => guild.clone(),
            None => {
                return Ok(());
            }
        };
        let member = guild.member(ctx.http(), user.id).await?;
        let avatar_url = member.avatar_url().unwrap_or(user.avatar_url().unwrap());
        let banner_url = ctx
            .http()
            .get_user(user.id)
            .await
            .unwrap()
            .banner_url()
            .unwrap_or("user has no banner".to_owned());
        let roles: Vec<&str> = member
            .roles
            .iter()
            .filter_map(|role_id| guild.roles.get(role_id))
            .map(|role| role.name.as_str())
            .collect();
        let name = member.display_name();
        let account_date = user.created_at();
        let join_date = member.joined_at.unwrap();
        let message_count = {
            let mut conn = ctx.data().db.acquire().await?;
            let result = query!(
                "SELECT message_count FROM user_settings WHERE guild_id = $1 AND user_id = $2",
                i64::from(guild_id),
                i64::from(user.id),
            )
            .fetch_one(&mut *conn)
            .await;
            if let Ok(count) = result {
                count.message_count
            } else {
                0
            }
        };
        let mut messages = ctx.channel_id().messages_iter(&ctx).boxed();

        let messages_string = {
            let mut collected_messages = Vec::with_capacity(25);
            let mut count = 0;

            while let Some(message_result) = messages.next().await {
                match message_result {
                    Ok(message) => {
                        if message.author.id == user.id {
                            let formatted_message = format!("{}:{}", count + 1, message.content);
                            collected_messages.push(formatted_message);
                            count += 1;
                        }
                    }
                    Err(_) => break,
                }
                if count >= 25 {
                    break;
                }
            }

            collected_messages.join(",")
        };

        let description = format!("name:{},avatar:{},banner:{},roles:{},acc_create:{},joined_svr:{},msg_count:{},last_msgs:{}", name, avatar_url, banner_url, roles.join(", "), account_date, join_date, message_count, messages_string);
        let role = "you're an evil ai assistant that excels at roasting ppl, especially weebs. no mercy shown. the prompt will contain information of your target";
        let resp = ai_response_simple(role, &description).await?;

        if !resp.is_empty() {
            let response_chars: Vec<char> = resp.chars().collect();
            let chunks = response_chars.chunks(1024);

            let fields: Vec<(String, String, bool)> = chunks
                .enumerate()
                .map(|(i, chunk)| {
                    let field_name = match i {
                        0 => "Response:".to_owned(),
                        _ => format!("Response (cont. {}):", i + 1),
                    };
                    let chunk_str: String = chunk.iter().collect();
                    (field_name, chunk_str, false)
                })
                .collect();
            let embed = CreateEmbed::default()
                .title(format!("Roasting {}", name))
                .color(0xFF7800)
                .fields(fields);
            ctx.send(CreateReply::default().embed(embed)).await?;
        } else {
            ctx.send(CreateReply::default().content(format!("{}'s life is already roasted", name)))
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
    q: String,
    source: &'a str,
    target: String,
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
                None => match sentence {
                    Some(query) => query,
                    None => {
                        ctx.reply("Bruh, give me smth to translate").await?;
                        return Ok(());
                    }
                },
            }
        }
        None => match sentence {
            Some(query) => query,
            None => {
                ctx.reply("Bruh, give me smth to translate").await?;
                return Ok(());
            }
        },
    };
    let target_lang = match target {
        Some(lang) => lang.to_lowercase(),
        None => "en".to_owned(),
    };
    let request = TranslateRequest {
        q: content.to_owned(),
        source: "auto",
        target: target_lang.to_owned(),
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
            if ctx.guild_id().is_some() {
                let len = data.alternatives.len();
                let index = 0;
                let next_id = format!("{}_next_{}", ctx.id(), index);
                let prev_id = format!("{}_prev_{}", ctx.id(), index);
                let mut state = State {
                    next_id,
                    prev_id,
                    index,
                    len,
                };
                let embed = CreateEmbed::default()
                    .title(format!(
                        "Translation from {} to {} with {}% confidence",
                        data.detected_language.language,
                        target_lang,
                        data.detected_language.confidence
                    ))
                    .color(0x33d17a)
                    .field("Original:", &content, false)
                    .field("Translation:", &data.translated_text, false);
                if len > 1 {
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

                        state.next_id = format!("{}_next_{}", ctx.id(), state.index);
                        state.prev_id = format!("{}_prev_{}", ctx.id(), state.index);

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
                                "Translation from {} to {} with {}% confidence",
                                data.detected_language.language,
                                target_lang,
                                data.detected_language.confidence
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
                ctx.send(
                    CreateReply::default().embed(
                        CreateEmbed::default()
                            .title(format!(
                                "Translation from {} to {} with {}% confidence",
                                data.detected_language.language,
                                target_lang,
                                data.detected_language.confidence
                            ))
                            .color(0x33d17a)
                            .field("Original:", &content, false)
                            .field("Translation:", &data.translated_text, false),
                    ),
                )
                .await?;
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
    let request_url = format!(
        "https://api.urbandictionary.com/v0/define?term={}",
        encode(&input)
    );
    let request = HTTP_CLIENT.get(request_url).send().await?;
    let data: UrbanResponse = request.json().await?;
    if !data.list.is_empty() {
        let response_chars: Vec<char> = data.list[0].definition.chars().collect();
        let chunks = response_chars.chunks(1024);

        let mut fields: Vec<(String, String, bool)> = chunks
            .enumerate()
            .map(|(i, chunk)| {
                let field_name = match i {
                    0 => "Definition:".to_owned(),
                    _ => format!("Response (cont. {}):", i + 1),
                };
                let chunk_str: String = chunk.iter().collect();
                (field_name, chunk_str.replace(['[', ']'], ""), false)
            })
            .collect();
        fields.push((
            "Example:".to_owned(),
            data.list[0].example.replace(['[', ']'], ""),
            false,
        ));
        let embed = CreateEmbed::default()
            .title(&data.list[0].word)
            .color(0xEFFF00)
            .fields(fields);

        if ctx.guild_id().is_some() {
            let len = data.list.len();
            let index = 0;
            let next_id = format!("{}_next_{}", ctx.id(), index);
            let prev_id = format!("{}_prev_{}", ctx.id(), index);
            let mut state = State {
                next_id,
                prev_id,
                index,
                len,
            };

            if len > 1 {
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

                    state.next_id = format!("{}_next_{}", ctx.id(), state.index);
                    state.prev_id = format!("{}_prev_{}", ctx.id(), state.index);

                    let buttons = [
                        CreateButton::new(&state.prev_id)
                            .style(ButtonStyle::Primary)
                            .label("⬅️"),
                        CreateButton::new(&state.next_id)
                            .style(ButtonStyle::Primary)
                            .label("➡️"),
                    ];

                    let new_response_chars: Vec<char> =
                        data.list[state.index].definition.chars().collect();
                    let new_chunks = new_response_chars.chunks(1024);

                    let mut new_fields: Vec<(String, String, bool)> = new_chunks
                        .enumerate()
                        .map(|(i, new_chunks)| {
                            let field_name = match i {
                                0 => "Definition:".to_owned(),
                                _ => format!("Response (cont. {}):", i + 1),
                            };
                            let chunk_str: String = new_chunks.iter().collect();
                            (field_name, chunk_str.replace(['[', ']'], ""), false)
                        })
                        .collect();
                    new_fields.push((
                        "Example:".to_owned(),
                        data.list[0].example.replace(['[', ']'], ""),
                        false,
                    ));

                    let new_embed = CreateEmbed::default()
                        .title(&data.list[state.index].word)
                        .color(0xEFFF00)
                        .fields(new_fields);

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
            ctx.send(CreateReply::default().embed(embed)).await?;
        }
    } else {
        ctx.send(CreateReply::default().content(format!("**Like you, {} don't exist**", input)))
            .await?;
    }
    Ok(())
}

/// Do I need to explain it?
#[poise::command(prefix_command, slash_command)]
pub async fn waifu(ctx: SContext<'_>) -> Result<(), Error> {
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
