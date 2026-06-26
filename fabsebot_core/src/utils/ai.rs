use std::{fmt::Write as _, sync::Arc};

use anyhow::{Result as AResult, anyhow};
use bytes::Bytes;
use image::{ImageFormat, guess_format};
use jiff::{Timestamp, tz::TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use serenity::{
	all::{Context as SContext, GenericChannelId, GuildId, Http, Message, MessageId, Role},
	nonmax::NonMaxU16,
	small_fixed_array::FixedString,
};
use songbird::{Call, input::Input};
use tokio::sync::Mutex;
use tracing::warn;
use winnow::Parser as _;

use crate::{
	config::{
		constants::CONTENT_LIMIT,
		types::{AIChatContext, AIChatMessage, AIChats, HTTP_CLIENT, utils_config},
	},
	utils::{
		helpers::{
			discord_message_link, encode_image, fetch_and_parse, get_gif, get_waifu, image_uri,
			non_empty_vec, user_roles_joined,
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
	avatar_url: &str,
	chat_vec: &mut Vec<ContentPart>,
) -> AResult<String> {
	uri_content(avatar_url, chat_vec).await?;
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
		ai_response(
			&mut conversations.messages,
			ctx,
			guild_id,
			Some(message),
			true,
		)
		.await
	};

	match response_opt {
		Ok(response) => {
			if response.len() >= CONTENT_LIMIT {
				let mut start = 0;
				while start < response.len() {
					let end = response[start..]
						.char_indices()
						.take_while(|(i, _)| *i < 2000)
						.last()
						.map_or(response.len(), |(i, c)| {
							start.saturating_add(i).saturating_add(c.len_utf8())
						});
					message.reply(&ctx.http, &response[start..end]).await?;
					start = end;
				}
			} else {
				message.reply(&ctx.http, response.as_str()).await?;
			}
			if let Some(handler_lock) = voice_handle {
				match ai_voice(&response).await {
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
				.push(AIChatMessage::assistant(response));
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
struct ToolArgs {
	#[serde(default)]
	query: String,
}

async fn tool_calling(
	response: &AIResponse,
	tool_calls: &[ToolCall],
	conversations: &mut Vec<AIChatMessage>,
	ctx: &SContext,
	message: Option<&Message>,
	guild_id: GuildId,
) -> AResult<String> {
	let utils_config = utils_config();
	let tool_content = response
		.choices
		.first()
		.and_then(|c| c.message.content.clone())
		.map(|choice| vec![ContentPart::Text { text: choice }]);

	conversations.push(AIChatMessage::assistant_with_tools(
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
				if let Some(message) = message
					&& let Some(guild) = message.guild(&ctx.cache)
				{
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
					&& let Some(avatar) = member.avatar_url().or_else(|| member.user.avatar_url())
				{
					let roles_joined = user_roles_pfp(&roles, &avatar, &mut chat_vec).await?;
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
		conversations.push(AIChatMessage::tool(chat_vec, tool.id.clone()));
	}

	let final_resp = ai_response_internal(conversations, true, true).await?;
	final_resp.extract_content()
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
struct AIResponse {
	#[serde(deserialize_with = "non_empty_vec")]
	choices: Vec<AIChoice>,
}

#[derive(Deserialize, PartialEq)]
enum FinishReasons {
	#[serde(rename = "stop")]
	Stop,
	#[serde(rename = "length")]
	Length,
	#[serde(rename = "tool_calls")]
	ToolCalls,
	#[serde(rename = "content_filter")]
	ContentFilter,
}

#[derive(Deserialize)]
struct AIChoice {
	finish_reason: FinishReasons,
	message: AIMessage,
}

#[derive(Deserialize)]
struct AIMessage {
	content: Option<String>,
	#[serde(default)]
	tool_calls: Vec<ToolCall>,
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

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ToolCalls {
	Web,
	Time,
	Gif,
	GuildInfo,
	UserInfo,
	Waifu,
}

impl ToolCall {
	fn extract_args(&self) -> AResult<ToolArgs> {
		from_str::<ToolArgs>(&self.function.arguments)
			.map_err(|e| anyhow!("Invalid tool arguments JSON: {e}"))
	}
}

impl AIResponse {
	fn extract_content(&self) -> AResult<String> {
		self.choices
			.first()
			.and_then(|c| c.message.content.as_deref())
			.map(ToOwned::to_owned)
			.ok_or_else(|| anyhow!("No content in AI response"))
	}

	#[must_use]
	fn has_tool_calls(&self) -> bool {
		self.choices
			.first()
			.is_some_and(|c| c.finish_reason == FinishReasons::ToolCalls)
	}

	fn get_tool_calls(&self) -> AResult<&[ToolCall]> {
		self.choices
			.first()
			.map(|c| c.message.tool_calls.as_slice())
			.ok_or_else(|| anyhow!("No choices in response"))
	}
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
#[serde(rename_all = "lowercase")]
enum ToolChoice {
	None,
	Auto,
	Required,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
	messages: &'a [AIChatMessage],
	model: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	tools: Option<&'a [AITools<'a>; 6]>,
	#[serde(skip_serializing_if = "Option::is_none")]
	tool_choice: Option<ToolChoice>,
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

async fn ai_response_internal(
	messages: &[AIChatMessage],
	tools_calling: bool,
	force_no_tools: bool,
) -> AResult<AIResponse> {
	let utils_config = utils_config();
	let tools_list = tools_calling.then_some(get_available_tools());
	let tool_choice = force_no_tools.then_some(ToolChoice::None);
	let request = ChatRequest {
		model: &utils_config.fabseserver.text_gen_model,
		messages,
		tools: tools_list.as_ref(),
		tool_choice,
	};

	fetch_and_parse::<AIResponse>(
		HTTP_CLIENT
			.post(&utils_config.fabseserver.llm_host_text)
			.json(&request)
			.send(),
	)
	.await
}

pub async fn ai_response(
	messages: &mut Vec<AIChatMessage>,
	ctx: &SContext,
	guild_id: GuildId,
	message: Option<&Message>,
	tools: bool,
) -> AResult<String> {
	let response = ai_response_internal(messages, tools, false).await?;

	let output = if tools
		&& response.has_tool_calls()
		&& let Ok(tool_calls) = response.get_tool_calls()
	{
		tool_calling(&response, tool_calls, messages, ctx, message, guild_id).await?
	} else {
		response.extract_content()?
	};

	Ok(output)
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
