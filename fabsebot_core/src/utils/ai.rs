use std::{collections::HashSet, fmt::Write as _, sync::Arc};

use anyhow::Result as AResult;
use bytes::Bytes;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serenity::all::{
	Context as SContext, GenericChannelId, GuildId, Http, Message, MessageId, Timestamp,
};
use songbird::{Call, input::Input};
use tokio::sync::Mutex;
use urlencoding::encode;
use winnow::Parser as _;

use crate::{
	config::types::{
		AIChatContext, AIChatMessage, AIChatStatic, AIModelDefaults, GEMMA_DEFAULTS, HTTP_CLIENT,
		LLAMA_DEFAULTS, QWEN_DEFAULTS, UTILS_CONFIG,
	},
	utils::helpers::{discord_message_link, get_configured_songbird_handler},
};

fn get_model_config(model_name: &str) -> (&'static str, &'static str, &'static AIModelDefaults) {
	let model_name_lower = model_name.to_lowercase();
	if model_name_lower.starts_with("gemma") {
		("<start_of_turn>{}", "<end_of_turn>", &GEMMA_DEFAULTS)
	} else if model_name_lower.starts_with("llama") {
		("[INST]{}", "[/INST]", &LLAMA_DEFAULTS)
	} else if model_name_lower.starts_with("qwen") {
		("<|im_start|>{}", "<|im_end|>", &QWEN_DEFAULTS)
	} else {
		("<start_of_turn>{}", "<end_of_turn>", &GEMMA_DEFAULTS)
	}
}

async fn internet_search(
	message: &Message,
	chatbot_internet_search: Option<bool>,
	fabseserver_search: &str,
) -> Option<String> {
	if let Some(internet_search) = chatbot_internet_search
		&& internet_search
		&& let Ok(resp) = HTTP_CLIENT
			.get(format!(
				"{fabseserver_search}/search?q={}&categories=general",
				encode(&message.content)
			))
			.send()
			.await
		&& let Ok(resp_text) = resp.text().await
	{
		let parsed_page = Html::parse_document(&resp_text);
		Selector::parse("article.result-default p.content").map_or(None, |snippet_selector| {
			Some(
				parsed_page
					.select(&snippet_selector)
					.fold(String::with_capacity(2048), |mut acc, element| {
						element.text().for_each(|text| acc.push_str(text));
						acc.push(' ');
						acc
					})
					.trim_end()
					.to_owned(),
			)
		})
	} else {
		None
	}
}

pub async fn ai_chatbot(
	ctx: &SContext,
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
) -> AResult<()> {
	if message.content.eq_ignore_ascii_case("clear") {
		{
			let mut convo_lock = conversations.lock().await;
			convo_lock.messages.clear();
			convo_lock.messages.shrink_to_fit();
			convo_lock.static_info = AIChatStatic::default();
		}
		message.reply(&ctx.http, "Conversation cleared!").await?;
		return Ok(());
	} else if !message.content.starts_with('#')
		&& let Some(utils_config) = UTILS_CONFIG.get()
	{
		let typing = message
			.channel_id
			.start_typing(Arc::<Http>::clone(&ctx.http));
		let author_name = message.author.name.as_str();
		let author_id_u64 = message.author.id.get();
		let internet_search_opt = internet_search(
			message,
			chatbot_internet_search,
			&utils_config.fabseserver.search,
		)
		.await;

		let mut system_content = String::new();
		if let Some(reply) = &message.referenced_message {
			let ref_name = reply.author.display_name();
			write!(
				system_content,
				"\n{author_name} replied to a message sent by: {ref_name} and had this content: {}",
				reply.content
			)?;
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
					&& let Some(desc) =
						ai_image_desc(&attachment.download().await?, Some(&message.content)).await
				{
					write!(system_content, "\n{desc}")?;
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
				if let Ok(linked_message) = ref_msg {
					let link_author = linked_message.author.display_name();
					let link_content = linked_message.content;
					write!(
						system_content,
						"\n{author_name} linked to a message sent in: {guild_name}, sent by: \
						 {link_author} and had this content: {link_content}"
					)?;
				} else {
					write!(
						system_content,
						"\n{author_name} linked to a message in non-accessible guild"
					)?;
				}
			}
		}
		let mut convo_lock = conversations.lock().await;
		let (start_tag, end_tag, model_defaults) =
			get_model_config(&utils_config.fabseserver.text_gen_model);
		{
			let (static_set, known_user, same_bot_role) = (
				convo_lock.static_info.is_set,
				convo_lock.static_info.users.contains_key(&author_id_u64),
				convo_lock.static_info.chatbot_role == chatbot_role,
			);
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
					convo_lock.static_info.users.insert(
						author_id_u64,
						format!(
							"\n{author_name}'s pfp can be described as: {pfp_desc} and \
							 {author_name} has the following roles: {roles_joined}. Their \
							 nickname in the current guild is {author_name_guild} which they \
							 joined on this date {author_joined_guild}"
						),
					);
				}
				if !same_bot_role {
					convo_lock.static_info.chatbot_role = chatbot_role;
				}
			} else {
				convo_lock.static_info.chatbot_role = chatbot_role;
				if let Some(guild) = message.guild(&ctx.cache) {
					convo_lock.static_info.guild_desc = format!(
						"\nThe guild you're currently talking in is named {} with this \
						 description {}, have {} members and have {} channels with these names \
						 {}, current channel name is {}. {}",
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
					);
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
					convo_lock.static_info.users.insert(
						author_id_u64,
						format!(
							"\n{author_name}'s pfp can be described as: {pfp_desc} and \
							 {author_name} has the following roles: {roles_joined}. Their \
							 nickname in the current guild is {author_name_guild} which they \
							 joined on this date {author_joined_guild}"
						),
					);
				}
			}
		}
		if !message.mentions.is_empty() {
			write!(
				system_content,
				"\n{} user(s) were mentioned:",
				message.mentions.len()
			)?;
			for target in &message.mentions {
				if let Some(target_info) = convo_lock.static_info.users.get(&target.id.get()) {
					write!(system_content, "{target_info}",)?;
				} else if let Ok(target_member) = guild_id.member(&ctx.http, target.id).await {
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
						"\n{target_name} was mentioned (global name is {target_global_name}). \
						 Roles: {target_roles}. Profile picture: {pfp_desc}. Joined this guild at \
						 this date: {target_joined_guild}"
					);
					write!(system_content, "{}", target_desc.as_str())?;
					convo_lock
						.static_info
						.users
						.insert(target.id.get(), target_desc);
				}
			}
		}
		let response_opt = {
			let convo_copy = {
				let content_safe = message.content_safe(&ctx.cache);
				if convo_lock
					.messages
					.iter()
					.any(|message| message.role.is_user())
				{
					system_content.push_str("\nCurrent users in the conversation");
					let mut is_first = true;
					let mut seen_users: HashSet<&str, _> = HashSet::new();
					for message in convo_lock.messages.iter().filter(|m| m.role.is_user()) {
						if let Some(user) = message.content.split(':').next().map(str::trim)
							&& seen_users.insert(user)
						{
							if !is_first {
								system_content.push('\n');
							}
							system_content.push_str(user);
							is_first = false;
						}
					}
				}
				let bot_context = format!(
					"{}{}{}\nCurrently the date and time in UTC-timezone is{}\nScraping the \
					 internet for the user's message gives this: {}\n{}",
					convo_lock.static_info.chatbot_role,
					convo_lock.static_info.guild_desc,
					convo_lock
						.static_info
						.users
						.get(&author_id_u64)
						.map_or_else(|| "Nothing is known about this user", |user| user),
					Timestamp::now(),
					internet_search_opt.map_or_else(
						|| "Nothing scraped from the internet".to_owned(),
						|internet_search| internet_search
					),
					system_content
				);
				if let Some(system_message) = convo_lock
					.messages
					.iter_mut()
					.find(|msg| msg.role.is_system())
				{
					system_message.content = bot_context;
				} else {
					let system_msg = AIChatMessage::system(bot_context);
					convo_lock.messages.push(system_msg);
				}
				convo_lock.static_info.is_set = true;
				convo_lock.messages.push(AIChatMessage::user(format!(
					"Message sent at: {} by user: {author_name}: {content_safe}",
					message.timestamp
				)));
				convo_lock.messages.clone()
			};
			drop(convo_lock);
			ai_response(
				&convo_copy,
				chatbot_temperature.unwrap_or(model_defaults.temperature),
				chatbot_top_p.unwrap_or(model_defaults.top_p),
				chatbot_top_k.unwrap_or(model_defaults.top_k),
				chatbot_repetition_penalty.unwrap_or(model_defaults.repetition_penalty),
				chatbot_frequency_penalty.unwrap_or(model_defaults.frequency_penalty),
				chatbot_presence_penalty.unwrap_or(model_defaults.presence_penalty),
			)
			.await
		};

		if let Some(mut response) = response_opt {
			if response.starts_with(start_tag) {
				response = response.trim_start_matches(start_tag).to_owned();
			}
			if response.ends_with(end_tag) {
				response = response.trim_end_matches(end_tag).to_owned();
			}
			if response.len() >= 2000 {
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
			if let Some(handler_lock) = voice_handle
				&& let Some(bytes) = ai_voice(&response).await
			{
				get_configured_songbird_handler(&handler_lock)
					.await
					.enqueue_input(Input::from(bytes))
					.await;
			}
			conversations
				.lock()
				.await
				.messages
				.push(AIChatMessage::assistant(response));
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
	let utils_config = UTILS_CONFIG.get()?;
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
		.map(|output| output.choices.into_iter().next().map(|o| o.message.content))?
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
	let utils_config = UTILS_CONFIG.get()?;
	let resp = HTTP_CLIENT
		.post(&utils_config.fabseserver.llm_host_text)
		.json(&request)
		.send()
		.await
		.ok()?;
	resp.json::<AIReponse>()
		.await
		.ok()
		.map(|output| output.choices.into_iter().next().map(|o| o.message.content))?
}

#[derive(Serialize)]
struct ChatRequest<'a> {
	messages: &'a [AIChatMessage],
	model: &'a str,
	temperature: f32,
	top_p: f32,
	top_k: i32,
	repetition_penalty: f32,
	frequency_penalty: f32,
	presence_penalty: f32,
}

pub async fn ai_response(
	messages: &[AIChatMessage],
	temperature: f32,
	top_p: f32,
	top_k: i32,
	repetition_penalty: f32,
	frequency_penalty: f32,
	presence_penalty: f32,
) -> Option<String> {
	let utils_config = UTILS_CONFIG.get()?;
	let request = ChatRequest {
		model: &utils_config.fabseserver.text_gen_model,
		messages,
		temperature,
		top_p,
		top_k,
		repetition_penalty,
		frequency_penalty,
		presence_penalty,
	};
	let resp = HTTP_CLIENT
		.post(&utils_config.fabseserver.llm_host_text)
		.json(&request)
		.send()
		.await
		.ok()?;

	let output = resp.json::<AIReponse>().await.ok()?;
	output
		.choices
		.into_iter()
		.next()
		.map(|r| r.message.content)
		.filter(|response_content| !response_content.trim().is_empty())
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
	let utils_config = UTILS_CONFIG.get()?;
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
		.await
		.ok()?;

	resp.bytes().await.ok()
}
