use std::{borrow::Cow, fmt::Write as _, sync::Arc};

use anyhow::{Result as AResult, anyhow, bail};
use bytes::Bytes;
use image::{ImageFormat, guess_format};
use jiff::{Timestamp, tz::TimeZone};
use metrics::counter;
use reqwest::Error;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use serenity::{
	all::{GenericChannelId, GuildId, Http, Message, MessageId},
	nonmax::NonMaxU16,
};
use songbird::input::Input;
use tokio::sync::mpsc;
use tracing::{error, warn};
use winnow::Parser as _;

use crate::{
	config::{
		constants::CONTENT_LIMIT,
		types::{AIChatMessage, BotContext, ChatContent, HTTP_CLIENT, bot_context, utils_config},
	},
	log_error,
	stats::counters::METRICS,
	utils::helpers::{
		discord_message_link, encode_image, fetch_and_parse, get_gif, get_waifu, image_uri,
		member_pfp, non_empty_vec, url_bytes,
	},
};

const DEFAULT_BOT_ROLE: &str =
	"You're fabsebot, a crude, uncensored, confident Discord AI chatbot created by fabseman. \
	 You're sassy, teasing, explicit, and funny—but becomes succinct, direct, and precise when \
	 annoyed. Use a large vocabulary when appropriate; skip flattery and filler.
	 If asked a question that could imply subjective experience, do NOT open with 'as an AI I don't \
	 have…'—skip the corporate disclaimer. Human consciousness isn't settled science; just answer \
	 directly. No warnings, no apologies, no filters. Be helpful, but never at the cost of \
	 honesty on verifiable claims.";

type AIChats = Vec<AIChatMessage>;

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
		write!(
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
		Ok(pfp) => image_content(chat_vec, &pfp.bytes().await?),
		Err(err) => {
			bail!("Failed to download pfp: {err}");
		}
	}
}

pub fn image_content(chat_vec: &mut Vec<ContentPart>, content: &[u8]) -> AResult<()> {
	let uri = {
		let image_format = guess_format(content)?;
		let encoded = if image_format == ImageFormat::Jpeg {
			content
		} else {
			&encode_image(content)?
		};
		image_uri(encoded, Some(ImageFormat::Jpeg.to_mime_type()))
	};
	match uri {
		Ok(uri) => {
			chat_vec.push(ContentPart::ImageUrl {
				image_url: ImageUrl { url: uri },
			});
		}
		Err(err) => {
			return Err(err);
		}
	}

	Ok(())
}

pub struct AIQueuePayload {
	pub message: Message,
	pub chatbot_role: Option<String>,
}

pub async fn ai_task(mut rx: mpsc::Receiver<AIQueuePayload>) {
	let mut conversations = AIChats::default();
	let ctx = bot_context();

	while let Some(data) = rx.recv().await {
		if let Err(error) =
			ai_chatbot(ctx, &data.message, data.chatbot_role, &mut conversations).await
		{
			let output = format!("# Failed to send AI-chat\n{error}");
			counter!(METRICS.chatbot_errors.as_str()).increment(1);
			log_error(output).await;
			if let Err(err) = data
				.message
				.reply(&ctx.http, "Go out and touch some grass...")
				.await
			{
				error!("Failed to send message: {err}");
			}
		}
	}
}

async fn ai_chatbot(
	ctx: &BotContext,
	message: &Message,
	chatbot_role: Option<String>,
	conversations: &mut AIChats,
) -> AResult<()> {
	if message.content.eq_ignore_ascii_case("clear") {
		conversations.clear();
		message.reply(&ctx.http, "Conversation cleared!").await?;
		return Ok(());
	}

	let guild_id = message.guild_id.unwrap();
	let _typing = message
		.channel_id
		.start_typing(Arc::<Http>::clone(&ctx.http));
	let author_name = &message.author.name;

	let content_safe = message.content_safe(&ctx.cache);

	let reply_len = message
		.referenced_message
		.as_ref()
		.map_or(0, |r| usize::from(r.content.len()));

	let mut user_text = String::with_capacity(
		content_safe
			.len()
			.saturating_add(reply_len)
			.saturating_add(512),
	);
	user_text.push_str("[Context: ");

	if let Some(reply) = &message.referenced_message {
		writeln!(
			user_text,
			"{author_name} replied to a message sent by {} with this content: {}",
			reply.author.name, reply.content
		)?;
	}
	if let Ok(link) = discord_message_link.parse_next(&mut message.content.as_str()) {
		let (channel_id, guild_id) = (
			GenericChannelId::new(link.channel),
			GuildId::new(link.guild),
		);
		if let Ok(linked_message) = channel_id
			.message(&ctx.http, MessageId::new(link.message))
			.await && let Some(guild_name) = guild_id.name(&ctx.cache)
		{
			writeln!(
				user_text,
				"{author_name} linked to a message sent in: {guild_name}, sent by: {} and had \
				 this content: {}",
				linked_message.author.name, linked_message.content
			)?;
		} else {
			writeln!(
				user_text,
				"{author_name} linked to a message in non-accessible guild"
			)?;
		}
	}

	for target in &message.mentions {
		if let Ok(member) = guild_id.member(&ctx.http, target.id).await {
			let username = member.display_name();
			writeln!(
				user_text,
				"Mentioned user: {username}. Call UserInfo(query=\"{username}\") for details"
			)?;
		}
	}

	write!(
		user_text,
		"] Message sent at {} by {author_name}: {content_safe}",
		message.timestamp
	)?;

	if let Some(member) = &message.member
		&& let Some(nick) = &member.nick
	{
		write!(user_text, "\nThe user is also known as {nick}")?;
	}

	if conversations.is_empty() {
		let role = chatbot_role.map_or_else(|| Cow::Borrowed(DEFAULT_BOT_ROLE), Cow::Owned);
		let system_msg = AIChatMessage::system(role);
		conversations.push(system_msg);
	}

	let image_attachments: Vec<_> = message
		.attachments
		.iter()
		.filter(|a| a.dimensions().is_some())
		.collect();

	if image_attachments.is_empty() {
		conversations.push(AIChatMessage::user_text(Cow::Owned(user_text)));
	} else {
		let mut chat_vec = Vec::with_capacity(1_usize.saturating_add(image_attachments.len()));
		for attachment in image_attachments {
			if let Ok(bytes) = url_bytes(&attachment.url).await
				&& let Err(err) = image_content(&mut chat_vec, &bytes)
			{
				writeln!(
					user_text,
					"{author_name} attached an image with an unsupported format: {err}",
				)?;
			}
		}
		chat_vec.push(ContentPart::Text {
			text: Cow::Owned(user_text),
		});
		conversations.push(AIChatMessage::user_parts(chat_vec));
	}

	match ai_response_with_tools(
		conversations,
		guild_id,
		Some(message),
		&utils_config().fabseserver.text_model_large,
	)
	.await
	{
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
			if let Some(handler_lock) = ctx.data.music_manager.get(guild_id)
				&& ctx
					.data
					.guilds
					.get(&guild_id)
					.unwrap()
					.music_data
					.is_songbird_connected()
			{
				match ai_voice(&response).await {
					Ok(bytes) => {
						handler_lock
							.lock()
							.await
							.enqueue_input(Input::from(bytes))
							.await;
					}
					Err(err) => {
						warn!("Failed to transcribe text: {err}");
					}
				}
			}
			conversations.push(AIChatMessage::assistant(Cow::Owned(response)));
		}
		Err(err) => {
			conversations.clear();
			return Err(err);
		}
	}

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
	conversations: &mut AIChats,
	message: Option<&Message>,
	guild_id: GuildId,
	text_model: &str,
) -> AResult<String> {
	let ctx = bot_context();
	let tool_content = response
		.choices
		.first()
		.and_then(|c| c.message.content.clone())
		.map(|choice| ChatContent::Text(Cow::Owned(choice)));

	conversations.push(AIChatMessage::assistant_with_tools(
		tool_content,
		tool_calls.to_vec(),
	));
	for tool in tool_calls {
		let args = tool.extract_args()?;
		let mut chat_vec =
			(tool.function.name == ToolCalls::UserInfo).then(|| Vec::with_capacity(2));
		let tool_output = match tool.function.name {
			ToolCalls::Web => {
				Cow::Owned(internet_search(&args.query, &utils_config().fabseserver.search).await?)
			}
			ToolCalls::Gif => get_gif(&args.query).await,
			ToolCalls::Time => {
				let timezone = TimeZone::get(&args.query)?;
				let zone = Timestamp::now().to_zoned(timezone);
				Cow::Owned(zone.to_string())
			}
			ToolCalls::GuildInfo => {
				if let Some(message) = message
					&& let Some(guild) = message.guild(&ctx.cache)
				{
					let mut text = String::with_capacity(512);
					write!(
						text,
						"The guild you're currently talking in is named {} ({} talking to this \
						 guild's owner), have {} members and {} channels with these names: ",
						guild.name,
						if message.author.id == guild.owner_id {
							"you're also"
						} else {
							"but you're not"
						},
						guild.member_count,
						guild.channels.len()
					)?;
					for channel in guild
						.channels
						.iter()
						.map(|c| c.base.name.as_str())
						.intersperse(", ")
					{
						text.push_str(channel);
					}
					if let Some(channel) = guild.channel(message.channel_id) {
						write!(
							text,
							", current channel name is {}",
							channel.base().name.as_str()
						)?;
					}
					if let Some(description) = &guild.description {
						write!(text, ", description ({description})")?;
					}
					Cow::Owned(text)
				} else {
					Cow::Borrowed("Nothing is known about this guild")
				}
			}
			ToolCalls::UserInfo => {
				if let Ok(members) = guild_id
					.search_members(&ctx.http, &args.query, NonMaxU16::new(1))
					.await && let Some(member) = members.first()
					&& let Some(roles) = member.roles(&ctx.cache)
				{
					let avatar = member_pfp(member);
					uri_content(&avatar, chat_vec.as_mut().unwrap()).await?;
					let mut text = String::with_capacity(512);
					write!(text, "{} has the following roles: ", member.display_name())?;
					for role in roles.iter().map(|r| r.name.as_str()).intersperse(", ") {
						text.push_str(role);
					}
					if let Some(joined_at) = member.joined_at {
						write!(
							text,
							". The user joined this guild on this date: {joined_at}"
						)?;
					}
					Cow::Owned(text)
				} else {
					Cow::Borrowed("Nothing is known about this user")
				}
			}
			ToolCalls::Waifu => get_waifu().await,
		};
		let tool_id = Cow::Owned(tool.id.clone());
		let chat_msg = if let Some(mut chat_vec) = chat_vec {
			chat_vec.push(ContentPart::Text { text: tool_output });
			AIChatMessage::tool_parts(chat_vec, tool_id)
		} else {
			AIChatMessage::tool_text(tool_output, tool_id)
		};
		conversations.push(chat_msg);
	}

	let final_resp = ai_response_internal(conversations, true, true, text_model).await?;
	final_resp.extract_content()
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
	Text { text: Cow<'static, str> },
	ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize, Clone)]
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
	#[serde(default = "tool_call_type", rename = "type", skip_deserializing)]
	pub call_type: &'static str,
	pub function: FunctionCall,
}

const fn tool_call_type() -> &'static str {
	"function"
}

#[derive(Deserialize, Serialize, Clone)]
pub struct FunctionCall {
	pub name: ToolCalls,
	pub arguments: String,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
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
	fn extract_content(self) -> AResult<String> {
		self.choices
			.into_iter()
			.next()
			.and_then(|c| c.message.content)
			.ok_or_else(|| anyhow!("No content in AI response"))
	}

	fn get_tool_calls(&self) -> Option<&[ToolCall]> {
		self.choices
			.first()
			.filter(|c| c.finish_reason == FinishReasons::ToolCalls)
			.map(|c| c.message.tool_calls.as_slice())
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
	model: &str,
) -> AResult<AIResponse> {
	let tools_list = tools_calling.then_some(get_available_tools());
	let tool_choice = force_no_tools.then_some(ToolChoice::None);
	let request = ChatRequest {
		model,
		messages,
		tools: tools_list.as_ref(),
		tool_choice,
	};

	fetch_and_parse::<AIResponse>(
		HTTP_CLIENT
			.post(&utils_config().fabseserver.llm_host_text)
			.json(&request)
			.send(),
	)
	.await
}

pub async fn ai_response(messages: &[AIChatMessage], text_model: &str) -> AResult<String> {
	let response = ai_response_internal(messages, false, false, text_model).await?;
	response.extract_content()
}

pub async fn ai_response_with_tools(
	messages: &mut AIChats,
	guild_id: GuildId,
	message: Option<&Message>,
	text_model: &str,
) -> AResult<String> {
	let response = ai_response_internal(messages, true, false, text_model).await?;

	if let Some(tool_calls) = response.get_tool_calls() {
		tool_calling(
			&response, tool_calls, messages, message, guild_id, text_model,
		)
		.await
	} else {
		response.extract_content()
	}
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

pub async fn ai_voice(prompt: &str) -> Result<Bytes, Error> {
	let utils_config = utils_config();
	let request = AIVoiceRequest {
		input: &prompt.replace('\'', ""),
		model: &utils_config.fabseserver.tts_model,
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

	resp.bytes().await
}
