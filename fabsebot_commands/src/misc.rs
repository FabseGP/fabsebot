use std::{borrow::Cow, fmt::Write as _, io::Cursor, mem::take, time::Duration};

use ab_glyph::FontArc;
use anyhow::{Context as _, Result as AResult};
use fabsebot_core::{
	config::{
		constants::{
			ANIMATED_QUOTE_VEC, AUTHOR_FONT, CONTENT_FONT, DEFAULT_THEME, EMPTY_REPLY_MSG, FONTS,
			MESSAGE_LIMIT, MISSING_REPLY_MSG, STATIC_QUOTE_VEC, THEMES,
		},
		types::{AIChatMessage, Error, SContext, SYSTEM_STATS, utils_config},
	},
	errors::commands::{AIError, GuildError, InteractionError},
	utils::{
		ai::ai_response,
		helpers::{
			default_mentions, image_uri, media_gallery, member_pfp, reply_container, text_display,
			thumbnail_section, url_bytes, user_pfp,
		},
		image::{
			ImageType, QuoteImageConfig, TextLayout, avatar_position, get_theme, quote_image,
			resize_avatar,
		},
	},
};
use image::{ImageBuffer, Rgba};
use poise::{ChoiceParameter, CreateReply, ReplyHandle, builtins::register_globally};
use rayon::spawn;
use serenity::{
	all::{
		ActivityData, AutocompleteChoice, ButtonStyle, Colour, ComponentInteractionCollector,
		ComponentInteractionDataKind, CreateActionRow, CreateAttachment,
		CreateAutocompleteResponse, CreateButton, CreateComponent, CreateContainer,
		CreateInteractionResponse, CreateMessage, CreateSelectMenu, CreateSelectMenuKind,
		CreateSelectMenuOption, DataUri, EditChannel, EditCurrentMember, EditMessage,
		GenericChannelId, GuildChannel, GuildId, Message, MessageId, OnlineStatus, User,
	},
	builder::{CreateContainerComponent, CreateMediaGallery, CreateSection},
	futures::StreamExt as _,
	nonmax::NonMaxU16,
};
use sqlx::{query, query_as, query_scalar};
use systemstat::{Platform as _, saturating_sub_bytes};
use tokio::{
	sync::oneshot,
	time::{sleep, timeout},
};
use tracing::warn;

use crate::command_permissions;

/// Send a birthday wish to ań user
#[poise::command(
	prefix_command,
	slash_command,
	context_menu_command = "Birthday",
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn birthday(
	ctx: SContext<'_>,
	#[description = "User to congratulate"] user: User,
) -> Result<(), Error> {
	command_permissions(&ctx).await?;
	let avatar_url = user_pfp(&user);

	let title = format!("# HAPPY BIRTHDAY <@{}>!", user.id);

	let (text, thumbnail) = thumbnail_section(&title, avatar_url);
	let text_array = [text];
	let thumbnail_display = [CreateContainerComponent::Section(CreateSection::new(
		&text_array,
		thumbnail,
	))];

	let image = [media_gallery(
		"https://media.tenor.com/GiCE3Iq3_TIAAAAC/pokemon-happy-birthday.gif",
	)];

	let container = CreateContainer::new(&thumbnail_display)
		.add_component(CreateContainerComponent::MediaGallery(
			CreateMediaGallery::new(&image),
		))
		.accent_colour(Colour::BLITZ_BLUE);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}

#[derive(ChoiceParameter)]
enum BotStatus {
	#[name = "invisible"]
	Invisible,
	#[name = "dnd"]
	Dnd,
	#[name = "idle"]
	Idle,
}

impl BotStatus {
	const fn to_online_status(&self) -> OnlineStatus {
		match self {
			Self::Invisible => OnlineStatus::Invisible,
			Self::Dnd => OnlineStatus::DoNotDisturb,
			Self::Idle => OnlineStatus::Idle,
		}
	}
}

/// Bot control
#[poise::command(
	slash_command,
	guild_only,
	owners_only,
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn bot_control(
	ctx: SContext<'_>,
	#[description = "Activity"] activity: Option<String>,
	#[description = "Status"] status: Option<BotStatus>,
) -> Result<(), Error> {
	if let Some(new_activity) = activity {
		ctx.framework()
			.serenity_context
			.set_activity(Some(ActivityData::listening(new_activity)));
	}

	if let Some(new_status) = status {
		ctx.framework()
			.serenity_context
			.set_status(new_status.to_online_status());
	}

	ctx.send(
		CreateReply::new()
			.content(format!("{} rebranded!", utils_config().bot_name))
			.ephemeral(true),
	)
	.await?;

	Ok(())
}

/// Personalize the bot in your server
#[poise::command(
	slash_command,
	guild_only,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS",
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | CHANGE_NICKNAME"
)]
pub async fn bot_personalize(
	ctx: SContext<'_>,
	#[description = "Nickname"] nickname: Option<String>,
	#[description = "Bio"] bio: Option<String>,
	#[description = "Link to avatar"] avatar: Option<String>,
	#[description = "Link to banner"] banner: Option<String>,
) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	let mut edited_member = EditCurrentMember::new()
		.nickname(nickname.map(Cow::Owned))
		.bio(bio.map(Cow::Owned))
		.audit_log_reason("Requested by either an admin or mod");

	if let Some(new_avatar) = avatar
		&& let Ok(bytes) = url_bytes(&new_avatar).await
		&& let Ok(uri) = image_uri(&bytes, None)
	{
		let encoded_avatar = DataUri::from_base64(Cow::Owned(uri))?;
		edited_member = edited_member.avatar(Some(encoded_avatar));
	}
	if let Some(new_banner) = banner
		&& let Ok(bytes) = url_bytes(&new_banner).await
		&& let Ok(uri) = image_uri(&bytes, None)
	{
		let encoded_banner = DataUri::from_base64(Cow::Owned(uri))?;
		edited_member = edited_member.banner(Some(encoded_banner));
	}

	if let Err(err) = edited_member.execute(ctx.http(), guild_id).await {
		ctx.reply("Slow down, you're changing too quickly").await?;
		return Err(err.into());
	}

	ctx.reply(format!("{} rebranded!", utils_config().bot_name))
		.await?;

	Ok(())
}

/// Debugging the bot's host
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn debug(ctx: SContext<'_>) -> Result<(), Error> {
	let mut text = String::with_capacity(256);

	text.push_str("# Debug");

	let value = ctx.serenity_context().runner_info.read().latency;
	if let Some(latency) = value.map(|l| l.as_millis()) {
		write!(
			text,
			"\n**Shard ping:** {latency}\n**Shard id:** {}",
			ctx.serenity_context().shard_id
		)?;
	}

	let aggregate = SYSTEM_STATS.cpu_load_aggregate();
	sleep(Duration::from_secs(1)).await;

	if let Ok(aggregate) = aggregate
		&& let Ok(load) = aggregate.done()
	{
		write!(text, "\n**System load:** {}", load.system)?;
	}

	if let Ok(average_load) = SYSTEM_STATS.load_average() {
		write!(
			text,
			"**\nAverage system load (15m):** {}",
			average_load.fifteen
		)?;
	}

	if let Ok((mem, swap)) = SYSTEM_STATS.memory_and_swap() {
		write!(
			text,
			"\nSystem memory:** {}/{} used\n**System swap:** {}/{} used",
			saturating_sub_bytes(mem.total, mem.free),
			mem.total,
			saturating_sub_bytes(swap.total, swap.free),
			swap.total,
		)?;
	}

	if let Ok(cpu_temp) = SYSTEM_STATS.cpu_temp() {
		write!(text, "\n**System temperature:** {cpu_temp}")?;
	}

	if let Ok(system_uptime) = SYSTEM_STATS.uptime() {
		write!(text, "\n**System uptime:** {}", system_uptime.as_secs())?;
	}

	let text_display = [text_display(&text)];
	let container = CreateContainer::new(&text_display).accent_colour(Colour::BLITZ_BLUE);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}

/// When you're not lonely anymore
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn global_chat_end(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id_i64 = i64::from(ctx.guild_id().unwrap());
	query!(
		r#"
		UPDATE guild_settings
		SET global_chat = FALSE, global_chat_channel = NULL
		WHERE guild_id = $1
        "#,
		guild_id_i64,
	)
	.execute(&ctx.data().db)
	.await?;
	ctx.reply("Call ended...").await?;

	Ok(())
}

/// When you're lonely and need someone to chat with
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | \
	                            MANAGE_WEBHOOKS"
)]
pub async fn global_chat_start(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id_i64 = i64::from(ctx.guild_id().unwrap());
	let channel_id_i64 = i64::from(ctx.channel_id());
	query!(
		r#"
		UPDATE guild_settings
		SET global_chat = TRUE, global_chat_channel = $2
		WHERE guild_id = $1
        "#,
		guild_id_i64,
		channel_id_i64,
	)
	.execute(&ctx.data().db)
	.await?;
	let message = ctx.reply("Calling...").await?;
	let result = timeout(Duration::from_mins(1), async {
		loop {
			let has_other_calls = query_scalar!(
				r#"
				SELECT EXISTS(SELECT 1 FROM guild_settings
				WHERE guild_id != $1
					AND global_chat IS TRUE)
				"#,
				guild_id_i64
			)
			.fetch_one(&ctx.data().db)
			.await?
			.unwrap_or(false);
			if has_other_calls {
				return Ok::<_, Error>(true);
			}
			sleep(Duration::from_secs(5)).await;
		}
	})
	.await;
	let response = if result.is_ok() {
		"Connected to global chat!"
	} else {
		query!(
			r#"
			UPDATE guild_settings
			SET global_chat = FALSE, global_chat_channel = NULL
			WHERE guild_id = $1
			"#,
			guild_id_i64
		)
		.execute(&ctx.data().db)
		.await?;
		"No one joined the chat within 1 minute 😢"
	};

	message
		.edit(ctx, CreateReply::new().reply(true).content(response))
		.await?;

	Ok(())
}

#[expect(clippy::unused_async)]
async fn autocomplete_command<'a>(
	ctx: SContext<'_>,
	partial: &'a str,
) -> CreateAutocompleteResponse<'a> {
	let choices: Vec<_> = ctx
		.framework()
		.options()
		.commands
		.iter()
		.filter(move |cmd| cmd.name.starts_with(partial))
		.take(25)
		.map(|cmd| AutocompleteChoice::from(cmd.name.clone()))
		.collect();
	CreateAutocompleteResponse::new().set_choices(choices)
}

/// When you need some help
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn help(
	ctx: SContext<'_>,
	#[description = "Command to get help with"]
	#[autocomplete = "autocomplete_command"]
	command: Option<String>,
) -> Result<(), Error> {
	let text = command.map_or_else(
		|| {
			let mut text = String::with_capacity(MESSAGE_LIMIT);
			text.push_str("# Available commands\n**Description:**\n");
			for command in &ctx.framework().options().commands {
				writeln!(
					text,
					"`{}` - {}",
					command.name,
					command.description.as_deref().unwrap_or("No description")
				)
				.unwrap();
			}
			text.push_str("*Use /help <command> for detailed info");
			text.truncate(MESSAGE_LIMIT);
			text
		},
		|cmd_name| {
			let command = ctx
				.framework()
				.options()
				.commands
				.iter()
				.find(|cmd| cmd.name == cmd_name)
				.unwrap();
			let description = command
				.description
				.as_deref()
				.unwrap_or("No description available");
			format!(
				"# Help: {}\n**Description:**\n{description}\n**Usage:**\n`{}{}`",
				command.name,
				ctx.prefix(),
				command.name
			)
		},
	);

	let text_display = [text_display(&text)];
	let container = CreateContainer::new(&text_display).accent_colour(Colour::GOLD);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}

struct UserCount {
	user_id: i64,
	message_count: i32,
}

/// Leaderboard of lifeless ppl
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn leaderboard(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	let thumbnail = match ctx.guild() {
		Some(guild) => guild.banner_url().map_or_else(
			|| {
				guild.icon_url().map_or_else(
					|| Cow::Borrowed("https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif"),
					Cow::Owned,
				)
			},
			Cow::Owned,
		),
		None => {
			return Err(GuildError::NotInGuild.into());
		}
	};
	let _typing = ctx.defer_or_broadcast().await;

	let users = query_as!(
		UserCount,
		r#"
		SELECT user_id, message_count
		FROM user_settings
		WHERE guild_id = $1
		ORDER BY message_count
		DESC LIMIT 25
		"#,
		i64::from(guild_id)
	)
	.fetch_all(&ctx.data().db)
	.await?;

	let mut list = String::with_capacity(users.len().saturating_mul(4));

	writeln!(list, "# Top {} user(s) by message count", users.len())?;

	for (index, user) in users.iter().enumerate() {
		let rank = index.saturating_add(1);
		writeln!(
			list,
			"**#{rank} <@{}>:** {}",
			user.user_id, user.message_count
		)?;
	}

	let (text, thumbnail) = thumbnail_section(&list, thumbnail);
	let text_array = [text];
	let thumbnail_display = [CreateContainerComponent::Section(CreateSection::new(
		&text_array,
		thumbnail,
	))];

	let container = CreateContainer::new(&thumbnail_display).accent_colour(Colour::RED);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}

/// Oh it's you
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn ohitsyou(ctx: SContext<'_>) -> Result<(), Error> {
	command_permissions(&ctx).await?;

	let _typing = ctx.defer_or_broadcast().await;
	let messages = [
		AIChatMessage::system(Cow::Borrowed(
			"you're a tsundere. no commentary, no alternatives, no meta-text. just the one line.",
		)),
		AIChatMessage::user_text(Cow::Borrowed("generate a one-line love-hate greeting")),
	];

	let resp = match ai_response(&messages, &utils_config().fabseserver.text_model_small).await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply(
				"Ugh, fine. It's nice to see you again, I suppose... for now, don't get any ideas \
				 thinking this means I actually like you or anything",
			)
			.await?;
			return Err(AIError::UnexpectedResponse(err).into());
		}
	};
	ctx.reply(resp).await?;

	Ok(())
}

struct ImageInfo {
	author_name: String,
	content: String,
	new_font: bool,
	config: QuoteImageConfig,
	content_font: (String, FontArc),
	author_font: FontArc,
	text_colour: Rgba<u8>,
	img: ImageBuffer<Rgba<u8>, Vec<u8>>,
	text_layout: TextLayout,
	buffer: Vec<u8>,
	avatar_position: i64,
	current_theme_name: String,
	filename: &'static str,
	image: ImageType,
}

impl ImageInfo {
	async fn new(
		avatar_image: Vec<u8>,
		author_name: String,
		content: String,
		is_animated: bool,
	) -> AResult<Self> {
		let content_font = FONTS.get(CONTENT_FONT).unwrap();
		let author_font = FONTS.get(AUTHOR_FONT).unwrap();
		let author_name_clone = author_name.clone();
		let content_clone = content.clone();
		let content_font_clone = content_font.clone();
		let author_font_clone = author_font.clone();

		let (img, text_colour) = get_theme(DEFAULT_THEME);
		let img_clone = img.clone();
		let avatar_position = avatar_position(false);

		let image_config = QuoteImageConfig::default();

		let mut text_layout = TextLayout::default();

		let (text_layout, image, output) = {
			let (tx, rx) = oneshot::channel();
			let avatar_image_clone = avatar_image.clone();
			spawn(move || {
				let (mut cursor, mut image, mut buffer) = if is_animated {
					(
						Cursor::new(avatar_image_clone),
						ImageType::Animated,
						Some(Vec::with_capacity(ANIMATED_QUOTE_VEC)),
					)
				} else {
					let buffer = Vec::with_capacity(STATIC_QUOTE_VEC);
					(
						Cursor::new(buffer),
						ImageType::Static(resize_avatar(&avatar_image_clone).unwrap()),
						None,
					)
				};

				let result = quote_image(
					&mut image,
					&author_name_clone,
					&content_clone,
					&author_font_clone,
					&content_font_clone,
					text_colour,
					img_clone,
					&mut text_layout,
					avatar_position,
					image_config,
					true,
					&mut cursor,
					buffer.as_mut(),
				);
				let buffer = if is_animated {
					buffer.unwrap()
				} else {
					cursor.into_inner()
				};
				if tx.send((result, text_layout, image, buffer)).is_err() {
					warn!("Sender failed to send result");
				}
			});
			let (result, text_layout, image, buffer) =
				rx.await.context("Rayon task for quote image panicked")?;
			match result {
				Ok(()) => (text_layout, image, buffer),
				Err(err) => {
					return Err(err);
				}
			}
		};

		let filename = if is_animated {
			"quote.gif"
		} else {
			"quote.avif"
		};

		Ok(Self {
			image: if image == ImageType::Animated {
				ImageType::AnimatedPayload(avatar_image)
			} else {
				image
			},
			author_name,
			content,
			config: image_config,
			author_font: author_font.clone(),
			content_font: (CONTENT_FONT.to_owned(), content_font.clone()),
			text_layout,
			new_font: false,
			buffer: output,
			img,
			text_colour,
			avatar_position,
			current_theme_name: DEFAULT_THEME.to_owned(),
			filename,
		})
	}

	async fn toggle_bw(&mut self) -> Result<(), Error> {
		self.config.bw = !self.config.bw;
		self.image_gen().await
	}

	async fn toggle_reverse(&mut self) -> Result<(), Error> {
		self.config.reverse = !self.config.reverse;
		self.avatar_position = avatar_position(self.config.reverse);
		self.image_gen().await
	}

	async fn toggle_gradient(&mut self) -> Result<(), Error> {
		self.config.gradient = !self.config.gradient;
		self.image_gen().await
	}

	async fn random_theme(&mut self) -> Result<(), Error> {
		(self.img, self.text_colour) = get_theme("random");
		self.image_gen().await
	}

	async fn new_font(&mut self, font_name: &str, new_font: FontArc) -> Result<(), Error> {
		self.content_font.1 = new_font;
		font_name.clone_into(&mut self.content_font.0);
		self.new_font = true;
		self.image_gen().await?;
		self.new_font = false;

		Ok(())
	}

	async fn new_theme(&mut self, theme_name: &str) -> Result<(), Error> {
		theme_name.clone_into(&mut self.current_theme_name);
		(self.img, self.text_colour) = get_theme(theme_name);
		self.image_gen().await
	}

	async fn image_gen(&mut self) -> Result<(), Error> {
		let author_name = self.author_name.clone();
		let content = self.content.clone();
		let author_font = self.author_font.clone();
		let content_font = self.content_font.clone();
		let mut text_layout = take(&mut self.text_layout);
		let config = self.config;
		let text_colour = self.text_colour;
		let img = self.img.clone();
		let avatar_position = self.avatar_position;
		let new_font = self.new_font;

		let mut buffer = take(&mut self.buffer);

		let (tx, rx) = oneshot::channel();

		let mut image_clone = self.image.clone();

		spawn(move || {
			let (mut cursor, mut buffer) = match image_clone {
				ImageType::Static(_) => (Cursor::new(buffer.clone()), None),
				ImageType::AnimatedPayload(ref avatar_bytes) => {
					buffer.clear();
					(Cursor::new(avatar_bytes.clone()), Some(buffer))
				}
				ImageType::Animated => return,
			};

			let result = quote_image(
				&mut image_clone,
				&author_name,
				&content,
				&author_font,
				&content_font.1,
				text_colour,
				img,
				&mut text_layout,
				avatar_position,
				config,
				new_font,
				&mut cursor,
				buffer.as_mut(),
			);

			let output = if let ImageType::Static(_) = image_clone {
				cursor.into_inner()
			} else {
				buffer.unwrap()
			};

			if tx.send((result, text_layout, output)).is_err() {
				warn!("Sender failed to send result");
			}
		});

		let (result, text_layout, output) =
			rx.await.context("Rayon task for quote image panicked")?;
		match result {
			Ok(()) => {
				self.text_layout = text_layout;
				self.buffer = output;
				Ok(())
			}
			Err(err) => Err(err),
		}
	}
}

enum MessageTypes<'a> {
	Reply(ReplyHandle<'a>),
	Message(Box<Message>),
}

async fn quote_internal(
	ctx: SContext<'_>,
	msg: &Message,
	reply: Option<(&Message, GuildId)>,
) -> AResult<()> {
	ctx.defer().await?;
	let mut image_handle = {
		let (avatar_url, author_name, text) = if let Some((reply, guild_id)) = reply {
			let (url, name) = if reply.webhook_id.is_some() {
				let avatar = user_pfp(&reply.author);
				(avatar, reply.author.name.clone())
			} else {
				let member = guild_id.member(&ctx.http(), reply.author.id).await?;
				let avatar = member_pfp(&member);
				(avatar, member.user.name)
			};
			(url, format!("- {name}"), reply.content.to_string())
		} else {
			let avatar = user_pfp(&msg.author);
			(
				avatar,
				format!("- {}", msg.author.name),
				msg.content.to_string(),
			)
		};
		let (avatar_image, is_animated) = (
			url_bytes(&avatar_url).await?.to_vec(),
			avatar_url.contains(".gif") || avatar_url.contains("format=gif"),
		);

		ImageInfo::new(avatar_image, author_name, text, is_animated).await?
	};
	let attachment = CreateAttachment::bytes(image_handle.buffer.clone(), image_handle.filename);
	let buttons = [
		CreateButton::new("bw")
			.style(ButtonStyle::Primary)
			.label("🎨"),
		CreateButton::new("reverse")
			.style(ButtonStyle::Primary)
			.label("🪞"),
		CreateButton::new("gradient")
			.style(ButtonStyle::Primary)
			.label("🌫️"),
		CreateButton::new("random")
			.style(ButtonStyle::Primary)
			.label("🎲"),
	];
	let mut font_select: Vec<CreateSelectMenuOption> = Vec::with_capacity(FONTS.len());

	for font in FONTS.iter() {
		font_select.push(CreateSelectMenuOption::new(*font.0, *font.0));
	}

	let font_menu = CreateSelectMenu::new(
		"font_option",
		CreateSelectMenuKind::String {
			options: Cow::Owned(font_select),
		},
	)
	.placeholder("Font")
	.min_values(1)
	.max_values(1);

	let mut theme_select: Vec<CreateSelectMenuOption> = Vec::with_capacity(THEMES.len());

	for theme in THEMES.iter() {
		theme_select.push(CreateSelectMenuOption::new(*theme.0, *theme.0));
	}

	let theme_menu = CreateSelectMenu::new(
		"theme_option",
		CreateSelectMenuKind::String {
			options: Cow::Owned(theme_select),
		},
	)
	.placeholder("Theme")
	.min_values(1)
	.max_values(1);

	let action_row = [CreateComponent::ActionRow(CreateActionRow::buttons(
		&buttons,
	))];

	let message_handle = if let Some((reply, guild_id_i64)) = reply.map(|r| (r.0, i64::from(r.1))) {
		let message_url = reply.link().to_string();

		let quote_channel_opt: Option<i64> = query_scalar!(
			"SELECT quotes_channel FROM guild_settings WHERE guild_id = $1",
			guild_id_i64
		)
		.fetch_one(&ctx.data().db)
		.await?;

		if let Some(channel) = quote_channel_opt {
			let quote_channel = GenericChannelId::new(channel.cast_unsigned());
			quote_channel
				.send_message(
					ctx.http(),
					CreateMessage::new()
						.add_file(attachment.clone())
						.content(&message_url),
				)
				.await?;
		}

		MessageTypes::Message(Box::new(
			ctx.channel_id()
				.send_message(
					ctx.http(),
					CreateMessage::new()
						.add_file(attachment.clone())
						.reference_message(msg)
						.content(message_url)
						.components(&action_row)
						.select_menu(font_menu)
						.select_menu(theme_menu)
						.allowed_mentions(default_mentions()),
				)
				.await?,
		))
	} else {
		MessageTypes::Reply(
			ctx.send(
				CreateReply::new()
					.attachment(attachment.clone())
					.components(&action_row)
					.allowed_mentions(default_mentions()),
			)
			.await?,
		)
	};

	let mut final_attachment = attachment.clone();

	let message_id = match &message_handle {
		MessageTypes::Reply(reply) => reply.message().await?.id,
		MessageTypes::Message(message) => message.id,
	};

	let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(Duration::from_mins(5))
		.message_id(message_id)
		.stream();

	while let Some(interaction) = collector_stream.next().await {
		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;

		let menu_choice_opt = match &interaction.data.kind {
			ComponentInteractionDataKind::StringSelect { values } => values.first(),
			_ => None,
		};

		if let Some(menu_choice) = menu_choice_opt {
			if let Some(new_font) = FONTS.get(menu_choice.as_str())
				&& *menu_choice != image_handle.content_font.0
			{
				image_handle.new_font(menu_choice, new_font.clone()).await?;
			} else if THEMES.contains_key(menu_choice.as_str())
				&& *menu_choice != image_handle.current_theme_name
			{
				image_handle.new_theme(menu_choice).await?;
			}
		} else if interaction.data.custom_id == "bw" {
			image_handle.toggle_bw().await?;
		} else if interaction.data.custom_id == "reverse" {
			image_handle.toggle_reverse().await?;
		} else if interaction.data.custom_id == "gradient" {
			image_handle.toggle_gradient().await?;
		} else if interaction.data.custom_id == "random" {
			image_handle.random_theme().await?;
		}
		let mut msg = interaction.message;
		final_attachment =
			CreateAttachment::bytes(image_handle.buffer.clone(), final_attachment.filename);
		msg.edit(
			ctx.http(),
			EditMessage::new().new_attachment(final_attachment.clone()),
		)
		.await?;
	}

	match message_handle {
		MessageTypes::Reply(reply) => {
			reply
				.edit(
					ctx,
					CreateReply::new()
						.attachment(final_attachment)
						.components(&[]),
				)
				.await?;
		}
		MessageTypes::Message(mut message) => {
			message
				.edit(
					ctx,
					EditMessage::new()
						.new_attachment(final_attachment)
						.components(&[]),
				)
				.await?;
		}
	}

	Ok(())
}

/// When your memory is not enough
#[poise::command(
	context_menu_command = "Quote",
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn quote_menu(
	ctx: SContext<'_>,
	#[description = "Message"] msg: Message,
) -> Result<(), Error> {
	if msg.content.is_empty() {
		ctx.reply(EMPTY_REPLY_MSG).await?;
		return Err(InteractionError::EmptyMessage.into());
	}
	quote_internal(ctx, &msg, None).await?;
	Ok(())
}

/// When your memory is not enough
#[poise::command(
	prefix_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn quote(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	let msg = ctx
		.channel_id()
		.message(&ctx.http(), MessageId::new(ctx.id()))
		.await?;

	let Some(ref reply) = msg.referenced_message else {
		ctx.reply(MISSING_REPLY_MSG).await?;
		return Err(InteractionError::MissingReply.into());
	};

	if reply.content.is_empty() {
		ctx.reply(EMPTY_REPLY_MSG).await?;
		return Err(InteractionError::EmptyMessage.into());
	}

	quote_internal(ctx, &msg, Some((reply, guild_id))).await?;

	Ok(())
}

#[poise::command(
	prefix_command,
	guild_only,
	owners_only,
	required_bot_permissions = "SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn register_commands(ctx: SContext<'_>) -> Result<(), Error> {
	let commands = &ctx.framework().options().commands;
	register_globally(ctx.http(), commands).await?;
	ctx.say("Successfully registered nucle- I mean, slash commands!")
		.await?;
	Ok(())
}

/// When you need some help responding
#[poise::command(context_menu_command = "Respond")]
pub async fn respond(
	ctx: SContext<'_>,
	#[description = "Message"] message: Message,
) -> Result<(), Error> {
	if message.content.is_empty() {
		ctx.reply(EMPTY_REPLY_MSG).await?;
		return Err(InteractionError::EmptyMessage.into());
	}
	ctx.defer().await?;
	let messages = [
		AIChatMessage::system(Cow::Borrowed(
			"Mock this Discord message someone posted. Just give the roast, nothing else.",
		)),
		AIChatMessage::user_text(Cow::Owned(message.content.into_string())),
	];

	let resp = match ai_response(&messages, &utils_config().fabseserver.text_model_small).await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("stfu").await?;
			return Err(AIError::UnexpectedResponse(err).into());
		}
	};
	ctx.say(resp).await?;
	Ok(())
}

/// When your users are yapping
#[poise::command(
	slash_command,
	guild_only,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | \
	                            MANAGE_CHANNELS"
)]
pub async fn slow_mode(
	ctx: SContext<'_>,
	#[description = "Channel to rate limit"] mut channel: GuildChannel,
	#[description = "Duration of rate limit in seconds"]
	#[min = 300]
	#[max = 21600]
	duration: u16,
) -> Result<(), Error> {
	let settings = EditChannel::new().rate_limit_per_user(NonMaxU16::new(duration).unwrap());
	channel.edit(ctx.http(), settings).await?;
	ctx.send(
		CreateReply::new()
			.content(format!("{channel} is ratelimited for {duration}s"))
			.ephemeral(true),
	)
	.await?;
	Ok(())
}

struct WordCount {
	word: String,
	count: i64,
}

/// Count of tracked words
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS"
)]
pub async fn word_count(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id_i64 = i64::from(ctx.guild_id().unwrap());
	let thumbnail = {
		let guild = ctx.guild().unwrap();
		guild.banner_url().map_or_else(
			|| {
				guild.icon_url().map_or(
					Cow::Borrowed("https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif"),
					Cow::Owned,
				)
			},
			Cow::Owned,
		)
	};

	let words = query_as!(
		WordCount,
		r#"
		SELECT word, count
		FROM guild_word_tracking
		WHERE guild_id = $1
		ORDER BY count
		DESC LIMIT 25
		"#,
		guild_id_i64
	)
	.fetch_all(&ctx.data().db)
	.await?;

	let mut list = String::with_capacity(words.len().saturating_mul(4));

	writeln!(list, "# Top {} words tracked by count!", words.len())?;

	for (index, word) in words.iter().enumerate() {
		let rank = index.saturating_add(1);
		writeln!(list, "#{rank} {}: {}", word.word, word.count)?;
	}

	let (text, thumbnail) = thumbnail_section(&list, thumbnail);
	let text_array = [text];
	let thumbnail_display = [CreateContainerComponent::Section(CreateSection::new(
		&text_array,
		thumbnail,
	))];
	let container = CreateContainer::new(&thumbnail_display).accent_colour(Colour::RED);
	let component = [CreateComponent::Container(container)];

	ctx.send(reply_container(&component)).await?;

	Ok(())
}
