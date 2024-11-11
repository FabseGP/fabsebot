use crate::{
    commands::music::get_configured_handler,
    config::types::{AIChatMessage, Error, HTTP_CLIENT, UTILS_CONFIG},
    utils::helpers::discord_message_link,
};

use base64::{engine::general_purpose, Engine};
use dashmap::{DashMap, DashSet};
use poise::serenity_prelude::{self as serenity, ChannelId, GuildId, Http, Message, MessageId};
use serde::{Deserialize, Serialize};
use songbird::{input::Input, Call};
use std::{fmt::Write, sync::Arc};
use tokio::sync::Mutex;
use winnow::Parser;

pub async fn ai_chatbot(
    ctx: &serenity::Context,
    message: &Message,
    bot_role: String,
    guild_id: GuildId,
    conversations: &Arc<DashMap<GuildId, Vec<AIChatMessage>>>,
    voice_handle: Option<Arc<Mutex<Call>>>,
) -> Result<(), Error> {
    if message.content.eq_ignore_ascii_case("clear") {
        if conversations.remove(&guild_id).is_some() {
            message.reply(&ctx.http, "Conversation cleared!").await?;
        } else {
            message.reply(&ctx.http, "Bruh, nothing to clear!").await?;
        }
        return Ok(());
    }
    if !message.content.starts_with('#') {
        let typing = message
            .channel_id
            .start_typing(Arc::<Http>::clone(&ctx.http));
        let author_name = message.author.display_name();
        let mut system_content = bot_role;
        if let Some(reply) = &message.referenced_message {
            let ref_name = reply.author.display_name();
            write!(
                system_content,
                "\n{author_name} replied to a message sent by: {ref_name} and had this content: {}",
                reply.content
            )?;
        }
        if let Ok(author_member) = guild_id.member(&ctx.http, message.author.id).await {
            if let Some(author_roles) = author_member.roles(&ctx.cache) {
                let roles_joined = author_roles
                    .iter()
                    .map(|role| role.name.as_str())
                    .intersperse(", ")
                    .collect::<String>();
                write!(
                    system_content,
                    "\n{author_name} has the following roles: {roles_joined}"
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
                        let target_name = target.display_name();
                        write!(
                            system_content,
                            "\n{target_name} was mentioned. Roles: {target_roles}. Profile picture: {pfp_desc}"
                        )?;
                    }
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
                    ref_channel.message(&ctx.http, message_id).await,
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
            let content_safe = message.content_safe(&ctx.cache);
            let mut history = conversations.entry(guild_id).or_default();

            if history.iter().any(|message| message.role.is_user()) {
                system_content.push_str("\nCurrent users in the conversation");
                let mut is_first = true;
                let seen_users = DashSet::new();
                for message in history.iter() {
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

            let system_message = history.iter_mut().find(|msg| msg.role.is_system());

            match system_message {
                Some(system_message) => {
                    system_message.content = system_content;
                }
                None => {
                    history.push(AIChatMessage::system(system_content));
                }
            }
            history.push(AIChatMessage::user(format!(
                "User: {author_name}: {content_safe}"
            )));
            ai_response(&history).await
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
            conversations
                .entry(guild_id)
                .or_default()
                .push(AIChatMessage::assistant(response));
        } else {
            let error_msg = "Sorry, I had to forget our convo, too boring!";
            {
                let mut history = conversations.entry(guild_id).or_default();
                history.clear();
                history.push(AIChatMessage::assistant(error_msg.to_string()));
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
}

pub async fn ai_response(content: &[AIChatMessage]) -> Option<String> {
    let request = ChatRequest { messages: content };
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
struct LocalAIResponse {
    message: LocalAIText,
}

#[derive(Deserialize)]
struct LocalAIText {
    content: String,
}

#[derive(Serialize)]
struct LocalAIRequest<'a> {
    model: &'static str,
    stream: bool,
    messages: &'a [AIChatMessage],
}

pub async fn ai_response_local(messages: &[AIChatMessage]) -> Option<String> {
    let request = LocalAIRequest {
        model: "meta-llama/Meta-Llama-3.1-8B-Instruct",
        stream: false,
        messages,
    };
    let resp = HTTP_CLIENT
        .post(
            &UTILS_CONFIG
                .get()
                .expect("UTILS_CONFIG must be set during initialization")
                .ai
                .base,
        )
        .json(&request)
        .send()
        .await
        .ok()?;
    resp.json::<LocalAIResponse>()
        .await
        .ok()
        .map(|output| output.message.content)
}

#[derive(Serialize)]
struct SimpleAIRequest<'a> {
    messages: [SimpleMessage<'a>; 2],
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
