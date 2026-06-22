use std::{fmt::Write as _, sync::Arc};

use anyhow::{Result as AResult, anyhow};
use bytes::Bytes;
use image::{ImageFormat, guess_format};
use jiff::{Timestamp, tz::TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use serenity::{
	all::{Context as SContext, GenericChannelId, GuildId, Http, Member, Message, MessageId, Role},
	nonmax::NonMaxU16,
	small_fixed_array::FixedString,
};
use songbird::{Call, input::Input};
use tokio::sync::{Mutex, MutexGuard};
use tracing::warn;
use winnow::Parser as _;

use crate::{
	config::types::{
		AIChatContext, AIChatMessage, AIChats, AIRole, HTTP_CLIENT, ToolCalls, utils_config,
	},
	utils::{
		helpers::{
			discord_message_link, encode_image, fetch_and_parse, get_gif, get_waifu, image_uri,
			member_pfp, non_empty_vec, user_roles_joined,
		},
		voice::get_configured_songbird_handler,
	},
};

#[derive(Deserialize)]
struct SearchResult {
	title: String,
	content: String,
	url: String,
}

#[derive(Deserialize)]
struct AnswerResult {
	answer: String,
	engine: String,
	url: String,
}

#[derive(Deserialize)]
struct SearchResponse {
	#[serde(deserialize_with = "non_empty_vec")]
	results: Vec<SearchResult>,
	answers: Option<Vec<AnswerResult>>,
}

async fn internet_search(input: &str, fabseserver_search: &str) -> AResult<String> {
	let response: SearchResponse = fetch_and_parse(
		HTTP_CLIENT
			.get(fabseserver_search)
			.query(&[("q", input), ("categories", "general"), ("format", "json")])
			.send(),
	)
	.await?;

	let mut summary = String::with_capacity(1024);

	if let Some(answers) = response.answers
		&& let Some(first_answer) = answers.first()
	{
		writeln!(
			summary,
			"• {}: {}: {}",
			first_answer.engine, first_answer.answer, first_answer.url
		)?;
	} else {
		for result in &response.results {
			writeln!(
				summary,
				"• {}: {}: {}",
				result.title, result.content, result.url
			)?;
		}
	}

	Ok(summary)
}

pub async fn uri_content(avatar_url: &str, chat_vec: &mut Vec<ContentPart>) -> AResult<()> {
	match HTTP_CLIENT.get(avatar_url).send().await {
		Ok(pfp) => image_content(chat_vec, &pfp.bytes().await?)?,
		Err(err) => {
			warn!("Failed to download pfp: {err}");
		}
	}

	Ok(())
}

pub async fn user_roles_pfp(
	roles: &[Role],
	member: &Member,
	chat_vec: &mut Vec<ContentPart>,
) -> AResult<String> {
	uri_content(&member_pfp(member), chat_vec).await?;
	Ok(user_roles_joined(roles))
}

pub fn image_content(chat_vec: &mut Vec<ContentPart>, content: &[u8]) -> AResult<()> {
	let uri = {
		let image_format = guess_format(content)?;
		if image_format == ImageFormat::Jpeg {
			image_uri(content, Some(image_format.to_mime_type()))
		} else {
			image_uri(&encode_image(content)?, Some(image_format.to_mime_type()))
		}
	};
	match uri {
		Ok(uri) => {
			chat_vec.push(ContentPart::ImageUrl {
				image_url: ImageUrl { url: uri },
			});
		}
		Err(err) => {
			warn!("Failed to create uri: {err}");
			return Err(err);
		}
	}

	Ok(())
}

pub async fn ai_chatbot(
	ctx: &SContext,
	message: &Message,
	chatbot_role: String,
	guild_id: GuildId,
	conversations: AIChats,
	voice_handle: Option<Arc<Mutex<Call>>>,
) -> AResult<()> {
	if message.content.eq_ignore_ascii_case("clear") {
		{
			let mut conversations = conversations.lock().await;
			*conversations = AIChatContext::default();
		}
		message.reply(&ctx.http, "Conversation cleared!").await?;
		return Ok(());
	}

	let typing = message
		.channel_id
		.start_typing(Arc::<Http>::clone(&ctx.http));
	let author_name = message.author.display_name();

	let mut system_content = String::new();
	let mut chat_vec = Vec::with_capacity(
		(1_u32.saturating_add(message.attachments.len()))
			.try_into()
			.unwrap(),
	);
	if let Some(reply) = &message.referenced_message {
		writeln!(
			system_content,
			"{author_name} replied to a message sent by {} with this content: {}",
			reply.author.display_name(),
			reply.content
		)?;
	}
	for attachment in &message.attachments {
		if let Some(content_type) = attachment.content_type.as_deref()
			&& content_type.starts_with("image")
			&& let Err(err) = image_content(&mut chat_vec, &attachment.download().await?)
		{
			writeln!(
				system_content,
				"{author_name} attached an image with an unsupported format: {err}",
			)?;
		}
	}
	if let Ok(link) = discord_message_link.parse_next(&mut message.content.as_str()) {
		let guild_id = GuildId::new(link.guild);
		if let Ok(ref_channel) = GenericChannelId::new(link.channel)
			.to_channel(&ctx.http, Some(guild_id))
			.await
		{
			let (guild_name, ref_msg) = (
				guild_id
					.name(&ctx.cache)
					.unwrap_or_else(|| "unknown".to_owned()),
				ref_channel
					.id()
					.message(&ctx.http, MessageId::new(link.message))
					.await,
			);
			if let Ok(linked_message) = ref_msg {
				writeln!(
					system_content,
					"{author_name} linked to a message sent in: {guild_name}, sent by: {} and had \
					 this content: {}",
					linked_message.author.display_name(),
					linked_message.content
				)?;
			} else {
				writeln!(
					system_content,
					"{author_name} linked to a message in non-accessible guild"
				)?;
			}
		}
	}

	for target in &message.mentions {
		if let Ok(member) = guild_id.member(&ctx.http, target.id).await {
			let username = member.display_name();
			writeln!(
				system_content,
				"Mentioned user: {username}. Call UserInfo(query=\"{username}\") for details"
			)?;
		}
	}

	let content_safe = message.content_safe(&ctx.cache);
	let author_nick = if let Some(member) = &message.member
		&& let Some(nick) = &member.nick
	{
		nick
	} else {
		&FixedString::new()
	};
	chat_vec.push(ContentPart::Text {
		text: format!(
			"[Context: {}] Message sent at {} by {author_name} (also known as {author_nick}): \
			 {content_safe}",
			system_content, message.timestamp,
		),
	});

	let mut conversations = conversations.lock().await;

	let response_opt = {
		if conversations.messages.is_empty() {
			let system_msg = AIChatMessage::system(chatbot_role);
			conversations.messages.push(system_msg);
		}
		conversations.messages.push(AIChatMessage::user(chat_vec));
		ai_response(&conversations.messages).await
	};

	match response_opt {
		Ok(response) => {
			let final_response = if response.has_tool_calls()
				&& let Ok(tool_calls) = response.get_tool_calls()
			{
				tool_calling(
					&response,
					tool_calls,
					&mut conversations,
					ctx,
					message,
					guild_id,
				)
				.await?
			} else {
				response.extract_content()?
			};
			if final_response.len() >= 2000 {
				let mut start = 0;
				while start < final_response.len() {
					let end = final_response[start..]
						.char_indices()
						.take_while(|(i, _)| *i < 2000)
						.last()
						.map_or(final_response.len(), |(i, c)| {
							start.saturating_add(i).saturating_add(c.len_utf8())
						});
					message
						.reply(&ctx.http, &final_response[start..end])
						.await?;
					start = end;
				}
			} else {
				message.reply(&ctx.http, final_response.as_str()).await?;
			}
			if let Some(handler_lock) = voice_handle {
				match ai_voice(&final_response).await {
					Ok(bytes) => {
						get_configured_songbird_handler(&handler_lock)
							.await
							.enqueue_input(Input::from(bytes))
							.await;
					}
					Err(err) => {
						warn!("Failed to transcribe text: {err}");
					}
				}
			}
			conversations
				.messages
				.push(AIChatMessage::assistant(final_response));
		}
		Err(err) => {
			*conversations = AIChatContext::default();
			drop(conversations);
			return Err(err);
		}
	}

	typing.stop();

	Ok(())
}

#[derive(Deserialize)]
pub struct ToolArgs {
	#[serde(default)]
	query: String,
}

async fn tool_calling(
	response: &AIResponse,
	tool_calls: &[ToolCall],
	conversations: &mut MutexGuard<'_, AIChatContext>,
	ctx: &SContext,
	message: &Message,
	guild_id: GuildId,
) -> AResult<String> {
	let utils_config = utils_config();
	let tool_content = response
		.choices
		.first()
		.and_then(|c| c.message.content.clone())
		.map(|choice| vec![ContentPart::Text { text: choice }]);

	conversations
		.messages
		.push(AIChatMessage::assistant_with_tools(
			tool_content,
			tool_calls.to_vec(),
		));

	for tool in tool_calls {
		let args = tool.extract_args()?;
		let mut chat_vec = Vec::with_capacity(1);
		let tool_output = match tool.function.name {
			ToolCalls::Web => {
				internet_search(&args.query, &utils_config.fabseserver.search).await?
			}
			ToolCalls::Gif => get_gif(ctx, &args.query).await,
			ToolCalls::Time => {
				let timezone = TimeZone::get(&args.query)?;
				let zone = Timestamp::now().to_zoned(timezone);
				zone.to_string()
			}
			ToolCalls::GuildInfo => {
				if let Some(guild) = message.guild(&ctx.cache) {
					format!(
						"The guild you're currently talking in is named {} with this description \
						 {}, have {} members and have {} channels with these names {}, current \
						 channel name is {}. {}\n",
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
							.intersperse(", ")
							.collect::<String>(),
						guild
							.channel(message.channel_id)
							.map_or("unknown", |channel| channel.base().name.as_str()),
						if message.author.id == guild.owner_id {
							"You're also talking to this guild's owner"
						} else {
							"But you're not talking to this guild's owner"
						}
					)
				} else {
					"Nothing is known about this guild".to_owned()
				}
			}
			ToolCalls::UserInfo => {
				if let Ok(members) = guild_id
					.search_members(&ctx.http, &args.query, NonMaxU16::new(1))
					.await && let Some(member) = members.first()
					&& let Some(roles) = member.roles(&ctx.cache)
				{
					let roles_joined = user_roles_pfp(&roles, member, &mut chat_vec).await?;
					let username = member.display_name();
					format!(
						"{username} has the following roles: {roles_joined}. The user joined this \
						 guild on this date {}\n",
						member.joined_at.unwrap_or_default()
					)
				} else {
					"Nothing is known about this user".to_owned()
				}
			}
			ToolCalls::Waifu => get_waifu(ctx).await,
		};
		chat_vec.push(ContentPart::Text { text: tool_output });
		conversations
			.messages
			.push(AIChatMessage::tool(chat_vec, tool.id.clone()));
	}

	let final_resp = ai_response(&conversations.messages).await?;
	final_resp.extract_content()
}

#[derive(Serialize)]
struct SimpleMessage<'a> {
	role: AIRole,
	content: &'a str,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
	Text { text: String },
	ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize)]
pub struct ImageUrl {
	pub url: String,
}

#[derive(Deserialize)]
pub struct AIResponse {
	#[serde(deserialize_with = "non_empty_vec")]
	choices: Vec<AIChoice>,
}

#[derive(Deserialize)]
pub struct AIChoice {
	#[serde(default)]
	pub finish_reason: String,
	pub message: AIMessage,
}

#[derive(Deserialize)]
pub struct AIMessage {
	pub content: Option<String>,
	#[serde(default)]
	pub tool_calls: Vec<ToolCall>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ToolCall {
	pub id: String,
	#[serde(default, rename = "type")]
	pub call_type: String,
	pub function: FunctionCall,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct FunctionCall {
	pub name: ToolCalls,
	pub arguments: String,
}

impl ToolCall {
	pub fn extract_args(&self) -> AResult<ToolArgs> {
		from_str::<ToolArgs>(&self.function.arguments)
			.map_err(|e| anyhow!("Invalid tool arguments JSON: {e}"))
	}
}

impl AIResponse {
	pub fn extract_content(&self) -> AResult<String> {
		self.choices
			.first()
			.and_then(|c| c.message.content.as_deref())
			.map(ToOwned::to_owned)
			.ok_or_else(|| anyhow!("No content in AI response"))
	}

	#[must_use]
	pub fn has_tool_calls(&self) -> bool {
		self.choices
			.first()
			.is_some_and(|c| !c.message.tool_calls.is_empty())
	}

	pub fn get_tool_calls(&self) -> AResult<&[ToolCall]> {
		self.choices
			.first()
			.map(|c| c.message.tool_calls.as_slice())
			.ok_or_else(|| anyhow!("No choices in response"))
	}
}

#[derive(Serialize)]
struct SimpleAIRequest<'a> {
	messages: &'a [SimpleMessage<'a>],
	model: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	max_tokens: Option<u32>,
}

pub async fn ai_response_simple(
	role: &str,
	prompt: &str,
	max_tokens: Option<u32>,
) -> AResult<String> {
	let utils_config = utils_config();
	let request = SimpleAIRequest {
		messages: &[
			SimpleMessage {
				role: AIRole::System,
				content: role,
			},
			SimpleMessage {
				role: AIRole::User,
				content: prompt,
			},
		],
		model: &utils_config.fabseserver.text_gen_model,
		max_tokens,
	};

	let response: AIResponse = fetch_and_parse(
		HTTP_CLIENT
			.post(&utils_config.fabseserver.llm_host_text)
			.json(&request)
			.send(),
	)
	.await?;

	response.extract_content()
}

#[derive(Serialize)]
struct AITools<'a> {
	#[serde(rename = "type")]
	tool_type: &'a str,
	function: &'a AIToolsFunction<'a>,
}

#[derive(Serialize)]
struct AIToolsFunction<'a> {
	name: ToolCalls,
	description: &'a str,
	parameters: &'a AIToolsParameters<'a>,
}

#[derive(Serialize)]
struct AIToolsParameters<'a> {
	#[serde(rename = "type")]
	tool_type: &'a str,
	properties: &'a AIToolsProperties<'a>,
	required: &'a [&'a str],
}

#[derive(Serialize)]
struct AIToolsProperties<'a> {
	#[serde(skip_serializing_if = "Option::is_none")]
	query: Option<&'a AIToolsQuery<'a>>,
}

#[derive(Serialize)]
struct AIToolsQuery<'a> {
	#[serde(rename = "type")]
	query_type: &'a str,
	description: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
	messages: &'a [AIChatMessage],
	model: &'a str,
	tools: &'a [AITools<'a>],
}

const fn get_available_tools() -> [AITools<'static>; 6] {
	[
		AITools {
			tool_type: "function",
			function: &AIToolsFunction {
				name: ToolCalls::Web,
				description: "Search the internet for current information...",
				parameters: &AIToolsParameters {
					tool_type: "object",
					properties: &AIToolsProperties {
						query: Some(&AIToolsQuery {
							query_type: "string",
							description: "The search query to use",
						}),
					},
					required: &["query"],
				},
			},
		},
		AITools {
			tool_type: "function",
			function: &AIToolsFunction {
				name: ToolCalls::Gif,
				description: "Retrieve a gif to express emotions, reactions or visual responses. \
				              Use this tool when: User explicitly asks for a 'gif', 'image', \
				              'picture'; you want to react emotionally (happy, sad, excited, \
				              annoyed, facepalm, laughing, etc.); the conversation is looping; \
				              you want to remain silent and send a reaction. This tool returns a \
				              direct gif url which you must include on its own line in your \
				              response so Discord can auto-embed it. Do not wrap it in markdown \
				              or alter it.",
				parameters: &AIToolsParameters {
					tool_type: "object",
					properties: &AIToolsProperties {
						query: Some(&AIToolsQuery {
							query_type: "string",
							description: "Emotion, action, or theme for the GIF (e.g., 'excited \
							              celebration', 'annoyed sigh', 'happy cat', 'facepalm')",
						}),
					},
					required: &["query"],
				},
			},
		},
		AITools {
			tool_type: "function",
			function: &AIToolsFunction {
				name: ToolCalls::Time,
				description: "Get the current time  and date in an IANA time zone",
				parameters: &AIToolsParameters {
					tool_type: "object",
					properties: &AIToolsProperties {
						query: Some(&AIToolsQuery {
							query_type: "string",
							description: "Time zone in IANA format, e.g. Europe/Copenhagen",
						}),
					},
					required: &["query"],
				},
			},
		},
		AITools {
			tool_type: "function",
			function: &AIToolsFunction {
				name: ToolCalls::UserInfo,
				description: "Retrieve detailed information about a mentioned user, including \
				              their profile picture base encoded. Always call this tool when a \
				              user is mentioned by name, ID or reference in the conversation. The \
				              'query' parameter should be the exact username or display name of \
				              the mentioned user.",
				parameters: &AIToolsParameters {
					tool_type: "object",
					properties: &AIToolsProperties {
						query: Some(&AIToolsQuery {
							query_type: "string",
							description: "The exact username or display name of the mentioned user",
						}),
					},
					required: &["query"],
				},
			},
		},
		AITools {
			tool_type: "function",
			function: &AIToolsFunction {
				name: ToolCalls::GuildInfo,
				description: "Get information about the current Discord guild/server. Use this \
				              tool when the user asks about the server name, description, member \
				              count, channels, owner, rules, or general opinions like 'what do \
				              you think of this guild', 'tell me about this server', 'how many \
				              members are here', 'who owns this guild', etc. This tool requires \
				              no parameters, just call it with empty arguments.",
				parameters: &AIToolsParameters {
					tool_type: "object",
					properties: &AIToolsProperties { query: None },
					required: &[],
				},
			},
		},
		AITools {
			tool_type: "function",
			function: &AIToolsFunction {
				name: ToolCalls::Waifu,
				description: "Retrieve a random waifu. Use this tool when: User explicitly asks \
				              for a waifu. This tool returns a direct waifu url which you must \
				              include in your response on its own line so Discord can auto-embed \
				              it. Do not wrap it in markdown or alter it.",
				parameters: &AIToolsParameters {
					tool_type: "object",
					properties: &AIToolsProperties { query: None },
					required: &[],
				},
			},
		},
	]
}

pub async fn ai_response(messages: &[AIChatMessage]) -> AResult<AIResponse> {
	let tools = get_available_tools();
	let utils_config = utils_config();
	let request = ChatRequest {
		model: &utils_config.fabseserver.text_gen_model,
		messages,
		tools: &tools,
	};

	fetch_and_parse::<AIResponse>(
		HTTP_CLIENT
			.post(&utils_config.fabseserver.llm_host_text)
			.json(&request)
			.send(),
	)
	.await
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

pub async fn ai_voice(prompt: &str) -> AResult<Bytes> {
	let utils_config = utils_config();
	let request = AIVoiceRequest {
		input: &prompt.replace('\'', ""),
		model: &utils_config.fabseserver.text_to_speech_model,
		voice: "af_heart",
		response_format: "wav",
		return_timestamps: false,
		stream: false,
		speed: 1.1,
		normalization_options: NormalizationOptions {
			unit_normalization: true,
		},
	};
	let resp = HTTP_CLIENT
		.post(&utils_config.fabseserver.llm_host_tts)
		.json(&request)
		.send()
		.await?;

	Ok(resp.bytes().await?)
}
