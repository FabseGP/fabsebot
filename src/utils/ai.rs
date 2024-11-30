use crate::{
    commands::music::get_configured_handler,
    config::types::{AIChatMessage, Error, HTTP_CLIENT, UTILS_CONFIG},
    utils::helpers::discord_message_link,
};

use base64::{Engine, engine::general_purpose};
use poise::serenity_prelude::{self as serenity, ChannelId, GuildId, Http, Message, MessageId};
use serde::{Deserialize, Serialize};
use songbird::{Call, input::Input};
use std::{collections::HashSet, fmt::Write, sync::Arc};
use tokio::sync::Mutex;
use winnow::Parser;

pub async fn ai_chatbot(
    ctx: &serenity::Context,
    message: &Message,
    bot_role: String,
    guild_id: GuildId,
    conversations: &Arc<Mutex<Vec<AIChatMessage>>>,
    voice_handle: Option<Arc<Mutex<Call>>>,
) -> Result<(), Error> {
    if message.content.eq_ignore_ascii_case("clear") {
        let mut convo_lock = conversations.lock().await;
        convo_lock.clear();
        convo_lock.shrink_to_fit();
        drop(convo_lock);
        message.reply(&ctx.http, "Conversation cleared!").await?;
        return Ok(());
    } else if !message.content.starts_with('#') {
        let typing = message
            .channel_id
            .start_typing(Arc::<Http>::clone(&ctx.http));
        let author_name = message.author.name.as_str();
        let mut system_content = bot_role;
        if let Some(guild) = message.guild(&ctx.cache) {
            let owner_message = if message.author.id == guild.owner_id {
                "You're also talking to this guild's owner"
            } else {
                "But you're not talking to this guild's owner"
            };
            write!(
                system_content,
                "\nthe guild you're currently talking in is named {} and have {} members. {owner_message}",
                guild.name, guild.member_count,
            )?;
            if let Some(guild_desc) = &guild.description {
                write!(
                    system_content,
                    "\nThe guild you're currently talking in has this description: {guild_desc}"
                )?;
            }
        }
        if let Some(reply) = &message.referenced_message {
            let ref_name = reply.author.display_name();
            write!(
                system_content,
                "\n{author_name} replied to a message sent by: {ref_name} and had this content: {}",
                reply.content
            )?;
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
                Ok(pfp) => {
                    let binary_pfp = pfp.bytes().await?;
                    (ai_image_desc(&binary_pfp, None).await)
                        .map_or_else(|| "Unable to describe".to_owned(), |desc| desc)
                }
                Err(_) => "Unable to describe".to_owned(),
            };
            let author_name_guild = author_member.display_name();
            write!(
                system_content,
                "\n{author_name}'s pfp can be described as: {pfp_desc} and {author_name} has the following roles: {roles_joined}. Their nickname in the current guild is {author_name_guild}"
            )?;
        }
        if !message.mentions.is_empty() {
            write!(
                system_content,
                "\n{} user(s) were mentioned:",
                message.mentions.len()
            )?;
            for target in &message.mentions {
                if let Ok(target_member) = guild_id.member(&ctx.http, target.id).await {
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
                        Ok(pfp) => {
                            let binary_pfp = pfp.bytes().await?;
                            (ai_image_desc(&binary_pfp, None).await)
                                .map_or_else(|| "Unable to describe".to_owned(), |desc| desc)
                        }
                        Err(_) => "Unable to describe".to_owned(),
                    };
                    let target_name = target_member.display_name();
                    write!(
                        system_content,
                        "\n{target_name} was mentioned. Roles: {target_roles}. Profile picture: {pfp_desc}"
                    )?;
                }
            }
        }
        if !message.attachments.is_empty() {
            write!(
                system_content,
                "\n{} image(s) were sent:",
                message.attachments.len()
            )?;
            for attachment in &message.attachments {
                if let Some(content_type) = attachment.content_type.as_deref() {
                    if content_type.starts_with("image") {
                        let file = attachment.download().await?;
                        if let Some(desc) = ai_image_desc(&file, Some(&message.content)).await {
                            write!(system_content, "\n{desc}")?;
                        }
                    }
                }
            }
        }
        if let Ok(link) = discord_message_link.parse_next(&mut message.content.as_str()) {
            let guild_id = GuildId::new(link.guild_id);
            let channel_id = ChannelId::new(link.channel_id);
            let message_id = MessageId::new(link.message_id);
            if let Ok(ref_channel) = channel_id.to_guild_channel(&ctx.http, Some(guild_id)).await {
                let (guild_name, ref_msg) = (
                    guild_id
                        .name(&ctx.cache)
                        .unwrap_or_else(|| "unknown".to_owned()),
                    ref_channel.id.message(&ctx.http, message_id).await,
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
        let response_opt = {
            let convo_copy = {
                let mut convo_history = conversations.lock().await;
                if convo_history.iter().any(|message| message.role.is_user()) {
                    system_content.push_str("\nCurrent users in the conversation");
                    let mut is_first = true;
                    let mut seen_users: HashSet<&str, _> = HashSet::new();
                    for message in convo_history.iter() {
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

                let system_message = convo_history.iter_mut().find(|msg| msg.role.is_system());

                match system_message {
                    Some(system_message) => {
                        system_message.content = system_content;
                    }
                    None => {
                        convo_history.push(AIChatMessage::system(system_content));
                    }
                }
                let content_safe = message.content_safe(&ctx.cache);
                convo_history.push(AIChatMessage::user(format!(
                    "User: {author_name}: {content_safe}"
                )));
                convo_history.clone()
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
            convo_lock.push(AIChatMessage::assistant(response));
        } else {
            let error_msg = "Sorry, I had to forget our convo, too boring!";
            {
                let mut convo_lock = conversations.lock().await;
                convo_lock.clear();
                convo_lock.push(AIChatMessage::assistant(error_msg.to_string()));
            }
            message.reply(&ctx.http, error_msg).await?;
        }
        typing.stop();
    }
    Ok(())
}

#[derive(Deserialize)]
struct FabseAIText {
    result: AIResponseText,
}
#[derive(Deserialize)]
struct AIResponseText {
    response: String,
}

#[derive(Serialize)]
struct SimpleMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ImageDesc<'a> {
    messages: [SimpleMessage<'a>; 2],
    image: &'a [u8],
}

pub async fn ai_image_desc(content: &[u8], user_context: Option<&str>) -> Option<String> {
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
        image: content,
    };
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let resp = HTTP_CLIENT
        .post(&utils_config.ai.image_desc)
        .bearer_auth(&utils_config.ai.token)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIText>()
        .await
        .ok()
        .map(|output| output.result.response)
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    messages: &'a [AIChatMessage],
    max_tokens: i32,
    temperature: f32,
    top_p: f32,
    top_k: i32,
    repetition_penalty: f32,
    frequency_penalty: f32,
    presence_penalty: f32,
}

pub async fn ai_response(content: &[AIChatMessage]) -> Option<String> {
    let request = ChatRequest {
        messages: content,
        max_tokens: 2048,
        temperature: 1.1,
        top_p: 0.9,
        top_k: 45,
        repetition_penalty: 1.0,
        frequency_penalty: 0.8,
        presence_penalty: 0.8,
    };
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let resp = HTTP_CLIENT
        .post(&utils_config.ai.text_gen)
        .bearer_auth(&utils_config.ai.token)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIText>()
        .await
        .ok()
        .map(|output| output.result.response)
}

#[derive(Deserialize)]
struct FallbackAIResponse {
    choices: Vec<FallbackAIChoice>,
}

#[derive(Deserialize)]
struct FallbackAIChoice {
    message: FallbackAIText,
}

#[derive(Deserialize)]
struct FallbackAIText {
    content: String,
}

#[derive(Serialize)]
struct FallbackAIRequest<'a> {
    model: &'static str,
    stream: bool,
    messages: &'a [AIChatMessage],
}

pub async fn ai_response_fallback(messages: &[AIChatMessage]) -> Option<String> {
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let request = FallbackAIRequest {
        model: &utils_config.ai.text_model,
        stream: false,
        messages,
    };
    let resp = HTTP_CLIENT
        .post(&utils_config.ai.fallback_provider)
        .bearer_auth(&utils_config.ai.token_fallback)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FallbackAIResponse>()
        .await
        .ok()
        .map(|output| output.choices[0].message.content.clone())
}

#[derive(Serialize)]
struct SimpleAIRequest<'a> {
    messages: [SimpleMessage<'a>; 2],
    max_tokens: i32,
}

pub async fn ai_response_simple(role: &str, prompt: &str) -> Option<String> {
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
        max_tokens: 512,
    };
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let resp = HTTP_CLIENT
        .post(&utils_config.ai.text_gen)
        .bearer_auth(&utils_config.ai.token)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIText>()
        .await
        .ok()
        .map(|output| output.result.response)
}

#[derive(Serialize)]
struct AIVoiceRequest<'a> {
    prompt: &'a str,
    lang: &'a str,
}

#[derive(Deserialize)]
struct FabseAIVoice {
    result: AIResponseVoice,
}

#[derive(Deserialize)]
struct AIResponseVoice {
    audio: String,
}

pub async fn ai_voice(prompt: &str) -> Option<Vec<u8>> {
    let request = AIVoiceRequest {
        prompt: &prompt.replace('\'', ""),
        lang: "en",
    };
    let utils_config = UTILS_CONFIG
        .get()
        .expect("UTILS_CONFIG must be set during initialization");
    let resp = HTTP_CLIENT
        .post(&utils_config.ai.tts)
        .bearer_auth(&utils_config.ai.token)
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<FabseAIVoice>()
        .await
        .ok()
        .and_then(|output| general_purpose::STANDARD.decode(output.result.audio).ok())
}
