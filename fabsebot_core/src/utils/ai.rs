use std::{
	collections::HashSet,
	fmt::Write as _,
	sync::{Arc, Mutex as SMutex},
};

use anyhow::{Result as AResult, anyhow, bail};
use bytes::Bytes;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serenity::all::{
	Context as SContext, GenericChannelId, GuildId, Http, Member, Message, MessageId, Role,
	Timestamp,
};
use songbird::{Call, input::Input};
use tokio::sync::Mutex;
use winnow::Parser as _;

use crate::{
	config::types::{AIChatContext, AIChatMessage, AIChatStatic, HTTP_CLIENT, UTILS_CONFIG},
	utils::helpers::{discord_message_link, get_configured_songbird_handler},
};

async fn internet_search(message: &Message, fabseserver_search: &str) -> AResult<String> {
	let response = match HTTP_CLIENT
		.get(fabseserver_search)
		.query(&[("q", message.content.as_str()), ("categories", "general")])
		.send()
		.await
	{
		Ok(resp) => match resp.text().await {
			Ok(text) => text,
			Err(err) => {
				bail!("Failed to get response text: {err}");
			}
		},
		Err(err) => {
			bail!("Failed to search online: {err}");
		}
	};

	let parsed_page = Html::parse_document(&response);
	Selector::parse("article.result-default p.content").map_or_else(
		|err| bail!("Failed to parse article content: {err}"),
		|snippet_selector| {
			Ok(parsed_page
				.select(&snippet_selector)
				.fold(String::with_capacity(2048), |mut acc, element| {
					element.text().for_each(|text| acc.push_str(text));
					acc.push(' ');
					acc
				})
				.trim_end()
				.to_owned())
		},
	)
}

async fn user_role(
	author_roles: &[Role],
	author_member: Member,
	author_id_u64: u64,
	author_name: &str,
	message: &Message,
	conversations: Arc<SMutex<AIChatContext>>,
) -> AResult<()> {
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
			.unwrap_or_else(|_| "Unable to describe".to_owned()),
		Err(_) => "Unable to describe".to_owned(),
	};
	conversations.lock().unwrap().static_info.users.insert(
		author_id_u64,
		format!(
			"\n{author_name}'s pfp can be described as: {pfp_desc} and {author_name} has the \
			 following roles: {roles_joined}. Their nickname in the current guild is {} which \
			 they joined on this date {}",
			author_member.display_name(),
			author_member.joined_at.unwrap_or_default()
		),
	);

	Ok(())
}

pub async fn ai_chatbot(
	ctx: &SContext,
	message: &Message,
	chatbot_role: String,
	chatbot_internet_search: Option<bool>,
	guild_id: GuildId,
	conversations: Arc<SMutex<AIChatContext>>,
	voice_handle: Option<Arc<Mutex<Call>>>,
) -> AResult<()> {
	if message.content.eq_ignore_ascii_case("clear") {
		{
			let mut conversations = conversations.lock().unwrap();
			conversations.messages.clear();
			conversations.messages.shrink_to_fit();
			conversations.static_info = AIChatStatic::default();
		}
		message.reply(&ctx.http, "Conversation cleared!").await?;
		return Ok(());
	}

	let utils_config = UTILS_CONFIG.get().unwrap();

	let typing = message
		.channel_id
		.start_typing(Arc::<Http>::clone(&ctx.http));
	let author_name = message.author.name.as_str();
	let author_id_u64 = message.author.id.get();
	let internet_search = if chatbot_internet_search.is_some_and(|c| c) {
		internet_search(message, &utils_config.fabseserver.search).await?
	} else {
		"Nothing scraped from the internet".to_owned()
	};

	let mut system_content = String::new();
	if let Some(reply) = &message.referenced_message {
		writeln!(
			system_content,
			"{author_name} replied to a message sent by: {} and had this content: {}",
			reply.author.display_name(),
			reply.content
		)?;
	}
	if !message.attachments.is_empty() {
		writeln!(
			system_content,
			"{} attachment(s) were sent:",
			message.attachments.len()
		)?;
		for attachment in &message.attachments {
			if let Some(content_type) = attachment.content_type.as_deref()
				&& content_type.starts_with("image")
				&& let Ok(desc) =
					ai_image_desc(&attachment.download().await?, Some(&message.content)).await
			{
				writeln!(system_content, "{desc}")?;
			}
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
	let (static_set, known_user) = {
		let mut conversations = conversations.lock().unwrap();
		if conversations.static_info.chatbot_role != chatbot_role {
			conversations.static_info.chatbot_role = chatbot_role;
		}
		(
			conversations.static_info.is_set,
			conversations.static_info.users.contains_key(&author_id_u64),
		)
	};
	if static_set {
		if !known_user
			&& let Ok(author_member) = guild_id.member(&ctx.http, message.author.id).await
			&& let Some(author_roles) = author_member.roles(&ctx.cache)
		{
			user_role(
				&author_roles,
				author_member,
				author_id_u64,
				author_name,
				message,
				conversations.clone(),
			)
			.await?;
		}
	} else {
		if let Some(guild) = message.guild(&ctx.cache) {
			conversations.lock().unwrap().static_info.guild_desc = format!(
				"\nThe guild you're currently talking in is named {} with this description {}, \
				 have {} members and have {} channels with these names {}, current channel name \
				 is {}. {}",
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
			user_role(
				&author_roles,
				author_member,
				author_id_u64,
				author_name,
				message,
				conversations.clone(),
			)
			.await?;
		}
	}
	if !message.mentions.is_empty() {
		writeln!(
			system_content,
			"{} user(s) were mentioned:",
			message.mentions.len()
		)?;
		for target in &message.mentions {
			if let Some(target_info) = conversations
				.lock()
				.unwrap()
				.static_info
				.users
				.get(&target.id.get())
			{
				writeln!(system_content, "{target_info}",)?;
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
						.unwrap_or_else(|_| "Unable to describe".to_owned()),
					Err(_) => "Unable to describe".to_owned(),
				};
				let target_desc = format!(
					"{} was mentioned (global name is {}). Roles: {target_roles}. Profile \
					 picture: {pfp_desc}. Joined this guild at this date: {}",
					target_member.display_name(),
					target.name.as_str(),
					target_member.joined_at.unwrap_or_default()
				);
				writeln!(system_content, "{}", target_desc.as_str())?;
				conversations
					.lock()
					.unwrap()
					.static_info
					.users
					.insert(target.id.get(), target_desc);
			}
		}
	}
	let response_opt = {
		let convo_copy = {
			let content_safe = message.content_safe(&ctx.cache);
			let mut conversations = conversations.lock().unwrap();
			let mut seen_users: HashSet<&str> = HashSet::new();
			for message in conversations.messages.iter().filter(|m| m.role.is_user()) {
				if let Some(user) = message.content.split(':').next().map(str::trim)
					&& seen_users.insert(user)
				{
					if seen_users.len() == 1 {
						system_content.push_str("Current users in the conversation\n");
					}
					system_content.push_str(user);
					system_content.push('\n');
				}
			}
			let bot_context = format!(
				"{}{}{}Currently the date and time in UTC-timezone is{}\nScraping the internet \
				 for the user's message gives this: {}\n{}",
				conversations.static_info.chatbot_role,
				conversations.static_info.guild_desc,
				conversations
					.static_info
					.users
					.get(&author_id_u64)
					.map_or_else(|| "Nothing is known about this user", |user| user.as_str()),
				Timestamp::now(),
				internet_search,
				system_content
			);
			if let Some(system_message) = conversations
				.messages
				.iter_mut()
				.find(|msg| msg.role.is_system())
			{
				system_message.content = bot_context;
			} else {
				let system_msg = AIChatMessage::system(bot_context);
				conversations.messages.push(system_msg);
			}
			conversations.static_info.is_set = true;
			conversations.messages.push(AIChatMessage::user(format!(
				"Message sent at: {} by user: {author_name}: {content_safe}",
				message.timestamp
			)));
			conversations.messages.clone()
		};
		ai_response(&convo_copy).await
	};

	if let Ok(response) = response_opt {
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
			&& let Ok(bytes) = ai_voice(&response).await
		{
			get_configured_songbird_handler(&handler_lock)
				.await
				.enqueue_input(Input::from(bytes))
				.await;
		}
		conversations
			.lock()
			.unwrap()
			.messages
			.push(AIChatMessage::assistant(response));
	} else {
		{
			let mut conversations = conversations.lock().unwrap();
			conversations.messages.clear();
			conversations.messages.shrink_to_fit();
			conversations.static_info = AIChatStatic::default();
		}

		message
			.reply(&ctx.http, "Sorry, I had to forget our convo, too boring!")
			.await?;
	}

	typing.stop();

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

pub async fn ai_image_desc(content: &[u8], user_context: Option<&str>) -> AResult<String> {
	let utils_config = UTILS_CONFIG.get().unwrap();
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
		.await?;

	let ai_response = resp.json::<AIReponse>().await?;

	ai_response
		.choices
		.first()
		.ok_or_else(|| anyhow!("Failed to describe image"))
		.map(|choice| choice.message.content.clone())
}

#[derive(Serialize)]
struct SimpleAIRequest<'a> {
	messages: [SimpleMessage<'a>; 2],
	model: &'a str,
}

pub async fn ai_response_simple(role: &str, prompt: &str, model: &str) -> AResult<String> {
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
	let utils_config = UTILS_CONFIG.get().unwrap();
	let resp = HTTP_CLIENT
		.post(&utils_config.fabseserver.llm_host_text)
		.json(&request)
		.send()
		.await?;

	let ai_response = resp.json::<AIReponse>().await?;

	ai_response
		.choices
		.first()
		.ok_or_else(|| anyhow!("Failed to get response"))
		.map(|choice| choice.message.content.clone())
}

#[derive(Serialize)]
struct ChatRequest<'a> {
	messages: &'a [AIChatMessage],
	model: &'a str,
}

pub async fn ai_response(messages: &[AIChatMessage]) -> AResult<String> {
	let utils_config = UTILS_CONFIG.get().unwrap();
	let request = ChatRequest {
		model: &utils_config.fabseserver.text_gen_model,
		messages,
	};
	let resp = HTTP_CLIENT
		.post(&utils_config.fabseserver.llm_host_text)
		.json(&request)
		.send()
		.await?;

	let output = resp.json::<AIReponse>().await?;

	output
		.choices
		.into_iter()
		.find(|choice| !choice.message.content.trim().is_empty())
		.map(|choice| choice.message.content)
		.ok_or_else(|| anyhow!("No valid response from AI"))
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
	let utils_config = UTILS_CONFIG.get().unwrap();
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
