use crate::{
    commands::music::get_configured_handler,
    config::types::{AIChatContext, AIChatMessage, AIChatStatic, Error, HTTP_CLIENT, UTILS_CONFIG},
    utils::helpers::discord_message_link,
};

use bytes::Bytes;
use poise::serenity_prelude::{
    self as serenity, GenericChannelId, GuildId, Http, Message, MessageId, Timestamp,
};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use songbird::{Call, input::Input};
use std::{collections::HashSet, fmt::Write, sync::Arc};
use tokio::sync::Mutex;
use urlencoding::encode;
use winnow::Parser;

pub async fn ai_chatbot(
    ctx: &serenity::Context,
    message: &Message,
    chatbot_role: String,
    chatbot_internet_search: Option<bool>,
    chatbot_temperature: Option<f32>,
    chatbot_top_p: Option<f32>,
    chatbot_top_k: Option<i32>,
    chatbot_repetition_penalty: Option<f32>,
    chatbot_frequency_penalty: Option<f32>,
    chatbot_presence_penalty: Option<f32>,
    guild_id: GuildId,
    conversations: &Arc<Mutex<AIChatContext>>,
    voice_handle: Option<Arc<Mutex<Call>>>,
) -> Result<(), Error> {
    if message.content.eq_ignore_ascii_case("clear") {
        {
            let mut convo_lock = conversations.lock().await;
            convo_lock.messages.clear();
            convo_lock.messages.shrink_to_fit();
            convo_lock.static_info = AIChatStatic::default();
        }
        message.reply(&ctx.http, "Conversation cleared!").await?;
        return Ok(());
    } else if !message.content.starts_with('#') {
        let typing = message
            .channel_id
            .start_typing(Arc::<Http>::clone(&ctx.http));
        let author_name = message.author.name.as_str();
        let author_id_u64 = message.author.id.get();
        {
            let (static_set, known_user, same_bot_role) = {
                let convo_lock = conversations.lock().await;
                (
                    convo_lock.static_info.is_set,
                    convo_lock.static_info.users.contains_key(&author_id_u64),
                    convo_lock.static_info.chatbot_role == chatbot_role,
                )
            };
            if static_set {
                if !known_user
                    && let Ok(author_member) = guild_id.member(&ctx.http, message.author.id).await
                    && let Some(author_roles) = author_member.roles(&ctx.cache)
                {
                    let roles_joined = author_roles
                        .iter()
                        .map(|role| role.name.as_str())
                        .intersperse(", ")
                        .collect::<String>();
                    let pfp_desc = match HTTP_CLIENT
                        .get(
                            author_member
                                .avatar_url()
                                .unwrap_or_else(|| message.author.static_face()),
                        )
                        .send()
                        .await
                    {
                        Ok(pfp) => (ai_image_desc(&pfp.bytes().await?, None).await)
                            .map_or_else(|| "Unable to describe".to_owned(), |desc| desc),
                        Err(_) => "Unable to describe".to_owned(),
                    };
                    let author_name_guild = author_member.display_name();
                    let author_joined_guild = author_member.joined_at.unwrap_or_default();
                    conversations.lock().await.static_info.users.insert(author_id_u64, format!(
                        "\n{author_name}'s pfp can be described as: {pfp_desc} and {author_name} has the following roles: {roles_joined}. Their nickname in the current guild is {author_name_guild} which they joined on this date {author_joined_guild}"
                    ));
                }
                if !same_bot_role {
                    let mut convo_lock = conversations.lock().await;
                    convo_lock.static_info.chatbot_role = chatbot_role;
                }
            } else {
                {
                    let mut convo_lock = conversations.lock().await;
                    convo_lock.static_info.chatbot_role = chatbot_role;
                    if let Some(guild) = message.guild(&ctx.cache) {
                        convo_lock.static_info.guild_desc = format!(
                            "\nThe guild you're currently talking in is named {} with this description {}, have {} members and have {} channels with these names {}. {}",
                            guild.name,
                            guild
                                .description
                                .as_ref()
                                .map_or("not known", |guild_desc| guild_desc),
                            guild.member_count,
                            guild.channels.len(),
                            guild
                                .channels
                                .iter()
                                .map(|c| c.base.name.as_str())
                                .collect::<String>(),
                            if message.author.id == guild.owner_id {
                                "You're also talking to this guild's owner"
                            } else {
                                "But you're not talking to this guild's owner"
                            }
                        );
                    }
                }
                if let Ok(author_member) = guild_id.member(&ctx.http, message.author.id).await
                    && let Some(author_roles) = author_member.roles(&ctx.cache)
                {
                    let roles_joined = author_roles
                        .iter()
                        .map(|role| role.name.as_str())
                        .intersperse(", ")
                        .collect::<String>();
                    let pfp_desc = match HTTP_CLIENT
                        .get(
                            author_member
                                .avatar_url()
                                .unwrap_or_else(|| message.author.static_face()),
                        )
                        .send()
                        .await
                    {
                        Ok(pfp) => (ai_image_desc(&pfp.bytes().await?, None).await)
                            .map_or_else(|| "Unable to describe".to_owned(), |desc| desc),
                        Err(_) => "Unable to describe".to_owned(),
                    };
                    let author_name_guild = author_member.display_name();
                    let author_joined_guild = author_member.joined_at.unwrap_or_default();
                    conversations.lock().await.static_info.users.insert(author_id_u64, format!(
                        "\n{author_name}'s pfp can be described as: {pfp_desc} and {author_name} has the following roles: {roles_joined}. Their nickname in the current guild is {author_name_guild} which they joined on this date {author_joined_guild}"
                    ));
                }
            }
        }
        let mut system_content = String::new();
        if let Some(reply) = &message.referenced_message {
            let ref_name = reply.author.display_name();
            write!(
                system_content,
                "\n{author_name} replied to a message sent by: {ref_name} and had this content: {}",
                reply.content
            )?;
        }
        if !message.mentions.is_empty() {
            write!(
                system_content,
                "\n{} user(s) were mentioned:",
                message.mentions.len()
            )?;
            for target in &message.mentions {
                if !conversations
                    .lock()
                    .await
                    .static_info
                    .users
                    .contains_key(&target.id.get())
                    && let Ok(target_member) = guild_id.member(&ctx.http, target.id).await
                {
                    let target_roles = target_member.roles(&ctx.cache).map_or_else(
                        || "No roles found".to_owned(),
                        |roles| {
                            roles
                                .iter()
                                .map(|role| role.name.as_str())
                                .intersperse(", ")
                                .collect::<String>()
                        },
                    );
                    let pfp_desc = match HTTP_CLIENT
                        .get(target.avatar_url().unwrap_or_else(|| target.static_face()))
                        .send()
                        .await
                    {
                        Ok(pfp) => (ai_image_desc(&pfp.bytes().await?, None).await)
                            .map_or_else(|| "Unable to describe".to_owned(), |desc| desc),
                        Err(_) => "Unable to describe".to_owned(),
                    };
                    let target_name = target_member.display_name();
                    let target_global_name = target.name.as_str();
                    let target_joined_guild = target_member.joined_at.unwrap_or_default();
                    let target_desc = format!(
                        "\n{target_name} was mentioned (global name is {target_global_name}). Roles: {target_roles}. Profile picture: {pfp_desc}. Joined this guild at this date: {target_joined_guild}"
                    );
                    write!(system_content, "{}", target_desc.as_str())?;
                    conversations
                        .lock()
                        .await
                        .static_info
                        .users
                        .insert(target.id.get(), target_desc);
                } else {
                    write!(
                        system_content,
                        "{}",
                        conversations
                            .lock()
                            .await
                            .static_info
                            .users
                            .get(&target.id.get())
                            .unwrap()
                    )?;
                }
            }
        }
        if !message.attachments.is_empty() {
            write!(
                system_content,
                "\n{} attachment(s) were sent:",
                message.attachments.len()
            )?;
            for attachment in &message.attachments {
                if let Some(content_type) = attachment.content_type.as_deref()
                    && content_type.starts_with("image")
                {
                    if let Some(desc) =
                        ai_image_desc(&attachment.download().await?, Some(&message.content)).await
                    {
                        write!(system_content, "\n{desc}")?;
                    }
                }
            }
        }
        if let Ok(link) = discord_message_link.parse_next(&mut message.content.as_str()) {
            let guild_id = GuildId::new(link.guild_id);
            if let Ok(ref_channel) = GenericChannelId::new(link.channel_id)
                .to_channel(&ctx.http, Some(guild_id))
                .await
            {
                let (guild_name, ref_msg) = (
                    guild_id
                        .name(&ctx.cache)
                        .unwrap_or_else(|| "unknown".to_owned()),
                    ref_channel
                        .id()
                        .message(&ctx.http, MessageId::new(link.message_id))
                        .await,
                );
                match ref_msg {
                    Ok(linked_message) => {
                        let link_author = linked_message.author.display_name();
                        let link_content = linked_message.content;
                        write!(
                            system_content,
                            "\n{author_name} linked to a message sent in: {guild_name}, sent by: {link_author} and had this content: {link_content}"
                        )?;
                    }
                    Err(_) => {
                        write!(
                            system_content,
                            "\n{author_name} linked to a message in non-accessible guild"
                        )?;
                    }
                }
            }
        }
        let internet_search_opt = if let Some(internet_search) = chatbot_internet_search
            && internet_search
        {
            let utils_config = UTILS_CONFIG
                .get()
                .expect("UTILS_CONFIG must be set during initialization");
            if let Ok(resp) = HTTP_CLIENT
                .get(format!(
                    "{}/search?q={}&categories=general",
                    utils_config.fabseserver.search,
                    encode(&message.content)
                ))
                .send()
                .await
                && let Ok(resp_text) = resp.text().await
            {
                let parsed_page = Html::parse_document(&resp_text);
                let snippet_selector = Selector::parse("article.result-default p.content")
                    .expect("Failed to parse search results");
                Some(
                    parsed_page
                        .select(&snippet_selector)
                        .fold(String::with_capacity(2048), |mut acc, element| {
                            element.text().for_each(|text| acc.push_str(text));
                            acc.push(' ');
                            acc
                        })
                        .trim_end()
                        .to_string(),
                )
            } else {
                None
            }
        } else {
            None
        };
        let response_opt = {
            let convo_copy = {
                let content_safe = message.content_safe(&ctx.cache);
                let mut convo_history = conversations.lock().await;
                if convo_history
                    .messages
                    .iter()
                    .any(|message| message.role.is_user())
                {
                    system_content.push_str("\nCurrent users in the conversation");
                    let mut is_first = true;
                    let mut seen_users: HashSet<&str, _> = HashSet::new();
                    for message in &convo_history.messages {
                        if message.role.is_user() {
                            if let Some(user) = message.content.split(':').next().map(str::trim) {
                                if seen_users.insert(user) {
                                    if !is_first {
                                        system_content.push('\n');
                                    }
                                    system_content.push_str(user);
                                    is_first = false;
                                }
                            }
                        }
                    }
                }
                let bot_context = format!(
                    "{}{}{}Currently the date and time in UTC-timezone is{}\nScraping DuckDuckGo for the user's message gives this: {}\n{}",
                    convo_history.static_info.chatbot_role,
                    convo_history.static_info.guild_desc,
                    convo_history.static_info.users.get(&author_id_u64).unwrap(),
                    Timestamp::now(),
                    internet_search_opt.map_or_else(
                        || "Nothing scraped from the internet".to_string(),
                        |internet_search| internet_search
                    ),
                    system_content
                );
                if let Some(system_message) = convo_history
                    .messages
                    .iter_mut()
                    .find(|msg| msg.role.is_system())
                {
                    system_message.content = bot_context;
                } else {
                    let system_msg = AIChatMessage::system(bot_context);
                    convo_history.messages.push(system_msg);
                }
                convo_history.static_info.is_set = true;
                convo_history.messages.push(AIChatMessage::user(format!(
                    "User: {author_name}: {content_safe}"
                )));
                convo_history.messages.clone()
            };
            ai_response(&convo_copy).await
        };

        if let Some(response) = response_opt {
            if response.len() >= 2000 {
                let mut start = 0;
                while start < response.len() {
                    let end = response[start..]
                        .char_indices()
                        .take_while(|(i, _)| *i < 2000)
                        .last()
                        .map_or(response.len(), |(i, c)| start + i + c.len_utf8());
                    message.reply(&ctx.http, &response[start..end]).await?;
                    start = end;
                }
            } else {
                message.reply(&ctx.http, response.as_str()).await?;
            }
            if let Some(handler_lock) = voice_handle {
                if let Some(bytes) = ai_voice(&response).await {
                    get_configured_handler(&handler_lock)
                        .await
                        .enqueue_input(Input::from(bytes))
                        .await;
                }
            }
            let mut convo_lock = conversations.lock().await;
            convo_lock.messages.push(AIChatMessage::assistant(response));
        } else {
            let error_msg = "Sorry, I had to forget our convo, too boring!";
            {
                let mut convo_lock = conversations.lock().await;
                convo_lock.messages.clear();
                convo_lock.messages.shrink_to_fit();
                convo_lock.static_info = AIChatStatic::default();
            }
            message.reply(&ctx.http, error_msg).await?;
        }

        typing.stop();
    }
    Ok(())
}

#[derive(Serialize)]
struct SimpleMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ImageDesc<'a> {
    messages: [SimpleMessage<'a>; 2],
    model: &'a str,
    image: &'a [u8],
}

#[derive(Deserialize)]
struct AIReponse {
    choices: Vec<AIText>,
}

#[derive(Deserialize)]
struct AIText {
    message: AIMessage,
}

#[derive(Deserialize)]
struct AIMessage {
    content: String,
}

pub async fn ai_image_desc(content: &[u8], user_context: Option<&str>) -> Option<String> {
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let request = ImageDesc {
        messages: [
            SimpleMessage {
                role: "system",
                content: "Generate a detailed caption for this image",
            },
            SimpleMessage {
                role: "user",
                content: user_context.map_or("What is in this image?", |context| context),
            },
        ],
        model: &utils_config.fabseserver.image_to_text_model,
        image: content,
    };
    let resp = HTTP_CLIENT
        .post(&utils_config.fabseserver.llm_host_text)
        .json(&request)
        .send()
        .await
        .ok()?;

    resp.json::<AIReponse>()
        .await
        .ok()
        .map(|output| output.choices[0].message.content.clone())
}

#[derive(Serialize)]
struct SimpleAIRequest<'a> {
    messages: [SimpleMessage<'a>; 2],
    model: &'a str,
}

pub async fn ai_response_simple(role: &str, prompt: &str, model: &str) -> Option<String> {
    let request = SimpleAIRequest {
        messages: [
            SimpleMessage {
                role: "system",
                content: role,
            },
            SimpleMessage {
                role: "user",
                content: prompt,
            },
        ],
        model,
    };
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let resp = HTTP_CLIENT
        .post(&utils_config.fabseserver.llm_host_text)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<AIReponse>()
        .await
        .ok()
        .map(|output| output.choices[0].message.content.clone())
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    messages: &'a [AIChatMessage],
    model: &'a str,
}

pub async fn ai_response(content: &[AIChatMessage]) -> Option<String> {
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let request = ChatRequest {
        messages: content,
        model: &utils_config.fabseserver.text_gen_model,
    };
    let resp = HTTP_CLIENT
        .post(&utils_config.fabseserver.llm_host_text)
        .json(&request)
        .send()
        .await
        .ok()?;

    resp.json::<AIReponse>()
        .await
        .ok()
        .map(|output| output.choices[0].message.content.clone())
}

#[derive(Serialize)]
struct AIVoiceRequest<'a> {
    input: &'a str,
    voice: &'a str,
    model: &'a str,
    response_format: &'a str,
    return_timestamps: bool,
    stream: bool,
    speed: f32,
    normalization_options: NormalizationOptions,
}

#[derive(Serialize)]
struct NormalizationOptions {
    unit_normalization: bool,
}

pub async fn ai_voice(prompt: &str) -> Option<Bytes> {
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let request = AIVoiceRequest {
        input: &prompt.replace('\'', ""),
        model: &utils_config.fabseserver.text_to_speech_model,
        voice: "af_heart",
        response_format: "wav",
        return_timestamps: false,
        stream: false,
        speed: 1.25,
        normalization_options: NormalizationOptions {
            unit_normalization: true,
        },
    };
    let resp = HTTP_CLIENT
        .post(&utils_config.fabseserver.llm_host_tts)
        .json(&request)
        .send()
        .await
        .ok()?;

    resp.bytes().await.ok()
}
