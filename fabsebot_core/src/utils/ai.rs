use std::{collections::HashSet, fmt::Write as _, io::Cursor, sync::Arc};

use anyhow::{Result as AResult, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bytes::Bytes;
use image::{ImageFormat, guess_format, load_from_memory};
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
	config::types::{AIChatContext, AIChatMessage, HTTP_CLIENT, UTILS_CONFIG},
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

async fn user_roles_pfp(roles: &[Role], member: &Member) -> AResult<(String, String)> {
	let roles_joined = roles
		.iter()
		.map(|role| role.name.as_str())
		.intersperse(", ")
		.collect::<String>();
	let avatar_url = member.avatar_url().unwrap_or_else(|| {
		member
			.user
			.avatar_url()
			.unwrap_or_else(|| member.user.default_avatar_url())
	});
	let pfp_desc = match HTTP_CLIENT.get(avatar_url).send().await {
		Ok(pfp) => (ai_image_desc(&pfp.bytes().await?, None).await)
			.unwrap_or_else(|_| "Unable to describe".to_owned()),
		Err(_) => "Unable to describe".to_owned(),
	};

	Ok((roles_joined, pfp_desc))
}

pub async fn ai_chatbot(
	ctx: &SContext,
	message: &Message,
	chatbot_role: String,
	chatbot_internet_search: bool,
	guild_id: GuildId,
	conversations: Arc<Mutex<AIChatContext>>,
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

	let mut conversations = conversations.lock().await;

	let utils_config = UTILS_CONFIG.get().unwrap();

	let typing = message
		.channel_id
		.start_typing(Arc::<Http>::clone(&ctx.http));
	let author_name = message.author.name.as_str();
	let author_id_u64 = message.author.id.get();
	let internet_search = if chatbot_internet_search {
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
	if conversations.static_info.chatbot_role != chatbot_role {
		conversations.static_info.chatbot_role = chatbot_role;
	}
	if !conversations.static_info.is_set
		&& let Some(guild) = message.guild(&ctx.cache)
	{
		conversations.static_info.guild_desc = format!(
			"The guild you're currently talking in is named {} with this description {}, have {} \
			 members and have {} channels with these names {}, current channel name is {}. {}\n",
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
		conversations.static_info.is_set = true;
	}

	if !conversations.static_info.users.contains_key(&author_id_u64)
		&& let Ok(author_member) = guild_id.member(&ctx.http, message.author.id).await
		&& let Some(author_roles) = author_member.roles(&ctx.cache)
	{
		let (roles_joined, pfp_desc) = user_roles_pfp(&author_roles, &author_member).await?;
		conversations.static_info.users.insert(
			author_id_u64,
			format!(
				"{author_name}'s pfp can be described as: {pfp_desc} and {author_name} has the \
				 following roles: {roles_joined}. Their nickname in the current guild is {} which \
				 they joined on this date {}\n",
				author_member.display_name(),
				author_member.joined_at.unwrap_or_default()
			),
		);
	}
	if !message.mentions.is_empty() {
		writeln!(
			system_content,
			"{} user(s) were mentioned:",
			message.mentions.len()
		)?;
		for target in &message.mentions {
			if let Some(target_info) = conversations.static_info.users.get(&target.id.get()) {
				writeln!(system_content, "{target_info}",)?;
			} else if let Ok(target_member) = guild_id.member(&ctx.http, target.id).await
				&& let Some(roles) = target_member.roles(&ctx.cache)
			{
				let (target_roles, pfp_desc) = user_roles_pfp(&roles, &target_member).await?;
				let target_desc = format!(
					"{} was mentioned (global name is {}). Roles: {target_roles}. Profile \
					 picture: {pfp_desc}. Joined this guild at this date: {}",
					target_member.display_name(),
					target.name.as_str(),
					target_member.joined_at.unwrap_or_default()
				);
				writeln!(system_content, "{}", target_desc.as_str())?;
				conversations
					.static_info
					.users
					.insert(target.id.get(), target_desc);
			}
		}
	}
	let response_opt = {
		let content_safe = message.content_safe(&ctx.cache);
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
			"{}{}{}Currently the date and time in UTC-timezone is{}\nScraping the internet for \
			 the user's message gives this: {}\n{}",
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
		let index = conversations.system_msg_index;
		if !conversations.messages.is_empty()
			&& let Some(system_message) = conversations.messages.get_mut(index)
		{
			system_message.content = bot_context;
		} else {
			let system_msg = AIChatMessage::system(bot_context);
			conversations.messages.push(system_msg);
			conversations.system_msg_index = conversations.messages.len() - 1;
		}
		conversations.messages.push(AIChatMessage::user(format!(
			"Message sent at: {} by user: {author_name}: {content_safe}",
			message.timestamp
		)));
		ai_response(&conversations.messages).await
	};

	match response_opt {
		Ok(response) => {
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
				.messages
				.push(AIChatMessage::assistant(response));
		}
		Err(err) => {
			*conversations = AIChatContext::default();
			drop(conversations);

			message
				.reply(&ctx.http, "Sorry, I had to forget our convo, too boring!")
				.await?;
			bail!(err);
		}
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
struct SimpleMessageImage<'a> {
	role: &'a str,
	content: [ContentPart<'a>; 2],
}

#[derive(Serialize)]
struct ImageUrl<'a> {
	url: &'a str,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentPart<'a> {
	Text { text: &'a str },
	ImageUrl { image_url: ImageUrl<'a> },
}

#[derive(Serialize)]
struct ImageDesc<'a> {
	messages: [SimpleMessageImage<'a>; 1],
	model: &'a str,
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

async fn ai_request_internal<T: Serialize + Send + Sync>(
	endpoint: &str,
	request: &T,
) -> AResult<String> {
	let resp = HTTP_CLIENT.post(endpoint).json(request).send().await?;

	let ai_response = resp.json::<AIReponse>().await?;
	ai_response
		.choices
		.into_iter()
		.next()
		.ok_or_else(|| anyhow!("Failed to get AI response"))
		.map(|choice| choice.message.content)
}

pub async fn ai_image_desc(content: &[u8], user_context: Option<&str>) -> AResult<String> {
	let image_format = guess_format(content)?;
	let base64_image = if image_format == ImageFormat::WebP {
		let img = load_from_memory(content)?;
		let mut png_bytes = Vec::with_capacity(content.len());
		img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Jpeg)?;
		BASE64.encode(&png_bytes)
	} else {
		BASE64.encode(content)
	};

	let data_uri = format!(
		"data:{};base64,{}",
		image_format.to_mime_type(),
		base64_image
	);

	let utils_config = UTILS_CONFIG.get().unwrap();

	let request = ImageDesc {
		model: &utils_config.fabseserver.image_to_text_model,
		messages: [SimpleMessageImage {
			role: "user",
			content: [
				ContentPart::Text {
					text: user_context.map_or("What is in this image?", |context| context),
				},
				ContentPart::ImageUrl {
					image_url: ImageUrl { url: &data_uri },
				},
			],
		}],
	};

	ai_request_internal(&utils_config.fabseserver.llm_host_text, &request).await
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

	ai_request_internal(
		&UTILS_CONFIG.get().unwrap().fabseserver.llm_host_text,
		&request,
	)
	.await
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

	ai_request_internal(&utils_config.fabseserver.llm_host_text, &request).await
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
