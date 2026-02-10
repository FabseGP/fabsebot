use std::{
	borrow::Cow, cmp::Reverse, collections::HashSet, fmt::Write as _, io::Cursor, mem::take,
	process, sync::Arc, time::Duration,
};

use ab_glyph::FontArc;
use anyhow::Context as _;
use fabsebot_core::{
	config::{
		constants::{
			ANIMATED_QUOTE_VEC, AUTHOR_FONT, COLOUR_BLUE, COLOUR_RED, COLOUR_YELLOW, CONTENT_FONT,
			DEFAULT_THEME, FONTS, NOT_IN_GUILD_MSG, QUOTE_ANIMATED_FILENAME, QUOTE_STATIC_FILENAME,
			RANDOM_THEME, STATIC_QUOTE_VEC, THEMES,
		},
		types::{CLIENT_DATA, Error, HTTP_CLIENT, SContext, SYSTEM_STATS, UTILS_CONFIG},
	},
	utils::{
		ai::ai_response_simple,
		image::{
			QuoteImageConfig, TextLayout, avatar_position, get_theme, quote_animated_image,
			quote_static_image, resize_avatar,
		},
	},
};
use image::{ImageBuffer, Rgba};
use poise::{ChoiceParameter, CreateReply, builtins::register_globally};
use rayon::spawn;
use serenity::{
	all::{
		ActivityData, AutocompleteChoice, ButtonStyle, ComponentInteractionCollector,
		ComponentInteractionDataKind, CreateActionRow, CreateAllowedMentions, CreateAttachment,
		CreateAutocompleteResponse, CreateButton, CreateComponent, CreateEmbed, CreateEmbedFooter,
		CreateInteractionResponse, CreateMessage, CreateSelectMenu, CreateSelectMenuKind,
		CreateSelectMenuOption, EditChannel, EditMessage, GenericChannelId, GuildChannel, GuildId,
		Member, Message, MessageId, OnlineStatus, ShardRunnerMessage, User, UserId,
	},
	futures::StreamExt as _,
	nonmax::NonMaxU16,
};
use sqlx::query;
use systemstat::{Platform as _, saturating_sub_bytes};
use tokio::{
	sync::oneshot,
	time::{sleep, timeout},
};
use tracing::warn;

/// When you want to find the imposter
#[poise::command(
	slash_command,
	install_context = "User|Guild",
	interaction_context = "Guild"
)]
pub async fn anony_poll(
	ctx: SContext<'_>,
	#[description = "Question"] title: String,
	#[description = "Comma-separated options"] options: String,
	#[description = "Duration in minutes"]
	#[min = 0]
	duration: u64,
) -> Result<(), Error> {
	let options_list: Vec<_> = options
		.split(',')
		.map(str::trim)
		.filter(|s| !s.is_empty())
		.collect();
	let options_count = options_list.len();
	if options_count < 1 {
		ctx.say("Bruh, no options ain't gonna cut it for a poll!")
			.await?;
		return Ok(());
	}

	let mut embed = CreateEmbed::default()
		.title(title.as_str())
		.colour(COLOUR_RED)
		.fields(options_list.iter().map(|&option| (option, "0", false)));
	let mut final_embed = embed.clone();

	let ctx_id_copy = ctx.id().to_string();
	let mut buttons = Vec::with_capacity(options_count);
	for index in 0..options_count {
		buttons.push(
			CreateButton::new(format!("{ctx_id_copy}_{index}"))
				.style(ButtonStyle::Primary)
				.label((index.saturating_add(1)).to_string()),
		);
	}
	let action_row = [CreateComponent::ActionRow(CreateActionRow::buttons(
		&buttons,
	))];

	let message = ctx
		.send(
			CreateReply::default()
				.embed(embed.clone())
				.components(&action_row)
				.reply(true),
		)
		.await?;

	let mut vote_counts = vec![0; options_count];
	let mut voted_users = HashSet::new();

	let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(Duration::from_secs(duration.saturating_mul(60)))
		.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_copy.as_str()))
		.stream();

	while let Some(interaction) = collector_stream.next().await {
		interaction
			.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
			.await?;
		if voted_users.insert(interaction.user.id)
			&& let Some(index) = interaction
				.data
				.custom_id
				.split('_')
				.nth(1)
				.and_then(|s| s.parse::<usize>().ok())
			&& index < options_count
			&& let Some(vote_index) = vote_counts.get_mut(index)
		{
			*vote_index = i32::saturating_add(*vote_index, 1);

			embed = CreateEmbed::default()
				.title(&title)
				.colour(COLOUR_RED)
				.fields(
					options_list
						.iter()
						.zip(vote_counts.iter())
						.map(|(&option, &count)| (option, count.to_string(), false)),
				);
			final_embed = embed.clone();

			let mut msg = interaction.message;
			msg.edit(ctx.http(), EditMessage::default().embed(embed))
				.await?;
		} else {
			ctx.send(
				CreateReply::default()
					.content("bruh, you have already voted!")
					.ephemeral(true),
			)
			.await?;
		}
	}
	message
		.edit(
			ctx,
			CreateReply::default()
				.embed(final_embed)
				.components(&[])
				.reply(true),
		)
		.await?;

	Ok(())
}

pub async fn birthday_internal(
	ctx: SContext<'_>,
	avatar_url: &str,
	name: &str,
) -> Result<(), Error> {
	ctx.send(
		CreateReply::default()
			.embed(
				CreateEmbed::default()
					.title(format!("HAPPY BIRTHDAY {name}!"))
					.thumbnail(avatar_url)
					.image("https://media.tenor.com/GiCE3Iq3_TIAAAAC/pokemon-happy-birthday.gif")
					.colour(COLOUR_RED),
			)
			.reply(true),
	)
	.await?;
	Ok(())
}

/// Send a birthday wish to a member
#[poise::command(
	prefix_command,
	slash_command,
	context_menu_command = "Birthday",
	install_context = "User",
	interaction_context = "PrivateChannel"
)]
pub async fn birthday_user(
	ctx: SContext<'_>,
	#[description = "User to congratulate"] user: User,
) -> Result<(), Error> {
	let avatar_url = user
		.avatar_url()
		.unwrap_or_else(|| user.default_avatar_url());
	birthday_internal(ctx, &avatar_url, user.display_name()).await?;
	Ok(())
}

/// Send a birthday wish to a member
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild|User",
	interaction_context = "Guild"
)]
pub async fn birthday(
	ctx: SContext<'_>,
	#[description = "Member to congratulate"] member: Member,
) -> Result<(), Error> {
	let avatar_url = member.avatar_url().unwrap_or_else(|| {
		member
			.user
			.avatar_url()
			.unwrap_or_else(|| member.user.default_avatar_url())
	});
	birthday_internal(ctx, &avatar_url, member.display_name()).await?;
	Ok(())
}

#[derive(ChoiceParameter)]
pub enum BotStatus {
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

/// Fabsebot control
#[poise::command(
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	owners_only
)]
pub async fn bot_control(
	ctx: SContext<'_>,
	new_activity_opt: Option<String>,
	new_status_opt: Option<BotStatus>,
	new_nickname_opt: Option<String>,
) -> Result<(), Error> {
	if let Some(new_activity) = new_activity_opt {
		ctx.framework()
			.serenity_context
			.set_activity(Some(ActivityData::listening(new_activity)));
	}

	if let Some(new_status) = new_status_opt {
		ctx.framework()
			.serenity_context
			.set_status(new_status.to_online_status());
	}

	if new_nickname_opt.is_some()
		&& let Some(guild_id) = ctx.guild_id()
	{
		guild_id
			.edit_nickname(
				ctx.http(),
				new_nickname_opt.as_deref(),
				Some("Bot owner requested"),
			)
			.await?;
	}

	ctx.send(
		CreateReply::default()
			.content(format!(
				"{} rebranded!",
				UTILS_CONFIG.get().unwrap().bot_name
			))
			.ephemeral(true),
	)
	.await?;

	Ok(())
}

/// Debugging fabsebot's host
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn debug(ctx: SContext<'_>) -> Result<(), Error> {
	let mut embed = CreateEmbed::default().title("Debug");
	let latency = if let Some(shard_runner) = ctx
		.serenity_context()
		.runners
		.get(&ctx.serenity_context().shard_id)
		&& let Some(latency) = shard_runner.0.latency
	{
		latency.as_millis()
	} else {
		0
	};
	embed = embed.field("Shard ping:", format!("{latency}ms"), true);
	embed = embed.field(
		"Shard id:",
		ctx.serenity_context().shard_id.to_string(),
		true,
	);
	embed = embed.field("", "", false);
	let cpu_load = SYSTEM_STATS.cpu_load_aggregate();
	sleep(Duration::from_secs(1)).await;
	if let Ok(cpu_load) = cpu_load.and_then(|f| f.done()) {
		embed = embed.field("System load:", format!("{}%", cpu_load.system), true);
	}
	if let Ok(avg_lod) = SYSTEM_STATS.load_average() {
		embed = embed.field(
			"Average system load (15m):",
			avg_lod.fifteen.to_string(),
			true,
		);
	}
	embed = embed.field("", "", false);
	if let Ok((mem, swap)) = SYSTEM_STATS.memory_and_swap() {
		embed = embed.field(
			"System memory:",
			format!(
				"{} / {} used",
				saturating_sub_bytes(mem.total, mem.free),
				mem.total
			),
			true,
		);
		embed = embed.field(
			"System swap:",
			format!(
				"{} / {} used",
				saturating_sub_bytes(swap.total, swap.free),
				swap.total
			),
			true,
		);
	}
	embed = embed.field("", "", false);
	if let Ok(temp) = SYSTEM_STATS.cpu_temp() {
		embed = embed.field("System temperature:", format!("{temp} ℃"), true);
	}
	if let Ok(uptime) = SYSTEM_STATS.uptime() {
		embed = embed.field("System uptime:", format!("{}s", uptime.as_secs()), true);
	}

	let mut reply = CreateReply::default().embed(embed.clone()).reply(true);

	let owner_id = UTILS_CONFIG.get().unwrap().owner_id;

	if ctx.author().id != owner_id {
		ctx.send(reply).await?;
		return Ok(());
	}

	let button = [CreateButton::new(format!("{}_shard_restart", ctx.id()))
		.style(ButtonStyle::Primary)
		.label("Restart shard")];
	let component = [CreateComponent::ActionRow(CreateActionRow::Buttons(
		Cow::Borrowed(&button),
	))];

	reply = reply.components(&component);

	let message = ctx.send(reply).await?;

	let ctx_id_str = ctx.id().to_string();
	if let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(Duration::from_mins(1))
		.filter(move |interaction| {
			interaction.data.custom_id.starts_with(ctx_id_str.as_str())
				&& interaction.user.id.get() == owner_id
		})
		.await
	{
		let mut msg = interaction.message;

		let response = ctx
			.serenity_context()
			.runners
			.get(&ctx.serenity_context().shard_id)
			.map(|entry| entry.1.clone())
			.map_or_else(
				|| {
					warn!("No shard runner found in runners map");
					"Rip, shard doesn't exist!"
				},
				|runner| {
					if let Err(err) = runner.unbounded_send(ShardRunnerMessage::Restart) {
						warn!("Failed to queue restart of shard: {:?}", err);
						"Rip, failed to restart shard!"
					} else {
						"Woah shard restarted!"
					}
				},
			);

		msg.edit(
			ctx.http(),
			EditMessage::default()
				.embed(embed)
				.components(vec![])
				.content(response),
		)
		.await?;
	} else {
		message
			.edit(
				ctx,
				CreateReply::default()
					.reply(true)
					.embed(embed)
					.components(&[]),
			)
			.await?;
	}

	Ok(())
}

/// Ignore this command
#[poise::command(
	prefix_command,
	install_context = "Guild",
	interaction_context = "Guild",
	owners_only
)]
#[expect(clippy::unused_async)]
pub async fn end_pgo(ctx: SContext<'_>) -> Result<(), Error> {
	let shutdown_trigger = CLIENT_DATA
		.get()
		.map(|c| c.shard_manager.get_shutdown_trigger())
		.unwrap();
	ctx.serenity_context().shutdown_all();
	if shutdown_trigger() {
		warn!("Successfully triggered shutdown for all shards");
	} else {
		warn!("Failed to trigger shutdown, shards may have already stopped");
		process::exit(1);
	}

	Ok(())
}

/// When you're not lonely anymore
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn global_chat_end(ctx: SContext<'_>) -> Result<(), Error> {
	let Some(guild_id) = ctx.guild_id() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Ok(());
	};
	query!(
		"INSERT INTO guild_settings (guild_id, global_chat)
            VALUES ($1, FALSE)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                global_chat = FALSE",
		i64::from(guild_id),
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;
	ctx.data().global_chats.invalidate(&guild_id);
	let mut modified_settings = ctx
		.data()
		.guilds
		.get(&guild_id)
		.get_or_insert_default()
		.as_ref()
		.clone();
	modified_settings.settings.global_chat = false;
	modified_settings.settings.global_chat_channel = None;
	ctx.data()
		.guilds
		.insert(guild_id, Arc::new(modified_settings));
	ctx.reply("Call ended...").await?;

	Ok(())
}

/// When you're lonely and need someone to chat with
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn global_chat_start(ctx: SContext<'_>) -> Result<(), Error> {
	let Some(guild_id) = ctx.guild_id() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Ok(());
	};
	let guild_id_i64 = i64::from(guild_id);
	let channel_id_i64 = i64::from(ctx.channel_id());
	let mut tx = ctx.data().db.begin().await?;
	query!(
		"INSERT INTO guild_settings (guild_id, global_chat, global_chat_channel)
            VALUES ($1, TRUE, $2)
            ON CONFLICT(guild_id)
            DO UPDATE SET
                global_chat = TRUE,
                global_chat_channel = $2",
		guild_id_i64,
		channel_id_i64,
	)
	.execute(&mut *tx)
	.await?;
	let mut modified_settings = ctx
		.data()
		.guilds
		.get(&guild_id)
		.get_or_insert_default()
		.as_ref()
		.clone();
	modified_settings.settings.global_chat = true;
	modified_settings.settings.global_chat_channel = Some(channel_id_i64);
	ctx.data()
		.guilds
		.insert(guild_id, Arc::new(modified_settings));
	let message = ctx.reply("Calling...").await?;
	let result = timeout(Duration::from_mins(1), async {
		loop {
			let has_other_calls = ctx.data().guilds.iter().any(|entry| {
				entry.key() != &guild_id
					&& entry.value().settings.global_chat
					&& entry.value().settings.global_chat_channel.is_some()
			});
			if has_other_calls {
				return Ok::<_, Error>(true);
			}
			sleep(Duration::from_secs(5)).await;
		}
	})
	.await;
	let response = if result.is_ok() {
		"Connected to global call!"
	} else {
		query!(
			"UPDATE guild_settings SET global_chat = FALSE, global_chat_channel = NULL WHERE \
			 guild_id = $1",
			guild_id_i64
		)
		.execute(&mut *tx)
		.await?;
		let mut modified_settings = ctx
			.data()
			.guilds
			.get(&guild_id)
			.get_or_insert_default()
			.as_ref()
			.clone();
		modified_settings.settings.global_chat = false;
		modified_settings.settings.global_chat_channel = None;
		ctx.data()
			.guilds
			.insert(guild_id, Arc::new(modified_settings));
		"No one joined the call within 1 minute 😢"
	};

	message
		.edit(ctx, CreateReply::default().reply(true).content(response))
		.await?;

	tx.commit()
		.await
		.context("Failed to commit sql-transaction")?;

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
	CreateAutocompleteResponse::default().set_choices(choices)
}

/// When you need some help
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn help(
	ctx: SContext<'_>,
	#[description = "Command to get help with"]
	#[autocomplete = "autocomplete_command"]
	command: Option<String>,
) -> Result<(), Error> {
	if let Some(cmd_name) = command {
		if let Some(command) = ctx
			.framework()
			.options()
			.commands
			.iter()
			.find(|cmd| cmd.name == cmd_name)
		{
			let embed = CreateEmbed::new()
				.title(format!("Help: {}", command.name))
				.description(
					command
						.description
						.as_deref()
						.unwrap_or("No description available"),
				)
				.color(COLOUR_YELLOW)
				.field(
					"Usage",
					format!("`{}{}`", ctx.prefix(), command.name),
					false,
				);

			ctx.send(CreateReply::default().embed(embed).ephemeral(true))
				.await?;
		} else {
			ctx.say("Rip, you're hallucinating").await?;
		}
	} else {
		let commands: String =
			ctx.framework()
				.options()
				.commands
				.iter()
				.fold(String::new(), |mut output, cmd| {
					let _ = writeln!(
						output,
						"`{}` - {}",
						cmd.name,
						cmd.description.as_deref().unwrap_or("No description")
					);
					output
				});

		let embed = CreateEmbed::new()
			.title("Available Commands")
			.description(commands)
			.color(COLOUR_BLUE)
			.footer(CreateEmbedFooter::new(
				"Use /help <command> for detailed info",
			));

		ctx.send(CreateReply::default().embed(embed).ephemeral(true))
			.await?;
	}

	Ok(())
}

struct UserCount {
	id: i64,
	count: i32,
}

/// Leaderboard of lifeless ppl
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn leaderboard(ctx: SContext<'_>) -> Result<(), Error> {
	let Some(guild_id) = ctx.guild_id() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Ok(());
	};
	let thumbnail = match ctx.guild() {
		Some(guild) => guild.banner_url().unwrap_or_else(|| {
			guild
				.icon_url()
				.unwrap_or_else(|| "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned())
		}),
		None => {
			return Ok(());
		}
	};
	ctx.defer().await?;

	let mut users =
		ctx.data()
			.user_settings
			.get(&guild_id)
			.map_or_else(Vec::new, |user_settings| {
				let capacity = user_settings.len();
				let mut result = Vec::with_capacity(capacity);

				for entry in user_settings.iter() {
					result.push(UserCount {
						id: entry.1.user_id,
						count: entry.1.message_count,
					});
				}

				result
			});

	users.sort_by_key(|b| Reverse(b.count));
	users.truncate(25);

	let mut embed = CreateEmbed::default()
		.title(format!("Top {} users by message count", users.len()))
		.thumbnail(thumbnail)
		.colour(COLOUR_RED);

	for (index, user) in users.iter().enumerate() {
		if let Ok(target) = guild_id
			.member(&ctx.http(), UserId::new(user.id.cast_unsigned()))
			.await
		{
			embed = embed.field(
				format!("#{} {}", index.saturating_add(1), target.display_name()),
				user.count.to_string(),
				false,
			);
		}
	}

	ctx.send(CreateReply::default().reply(true).embed(embed))
		.await?;

	Ok(())
}

/// Oh it's you
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild|User",
	interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn ohitsyou(ctx: SContext<'_>) -> Result<(), Error> {
	ctx.defer().await?;
	let utils_config = UTILS_CONFIG.get().unwrap();

	if let Ok(resp) = ai_response_simple(
		"you're a tsundere",
		"generate a one-line love-hate greeting",
		&utils_config.fabseserver.text_gen_model,
	)
	.await && !resp.is_empty()
	{
		ctx.reply(resp).await?;
	} else {
		ctx.reply(
			"Ugh, fine. It's nice to see you again, I suppose... 
                for now, don't get any ideas thinking this means I actually like you or anything",
		)
		.await?;
	}
	Ok(())
}

struct ImageInfo {
	author_name: String,
	content: String,
	config: QuoteImageConfig,
	content_font: (String, FontArc),
	author_font: FontArc,
	text_colour: Rgba<u8>,
	img: ImageBuffer<Rgba<u8>, Vec<u8>>,
	text_layout: TextLayout,
	buffer: Vec<u8>,
	avatar_position: i64,
	current_theme_name: String,
	filename: String,
	static_image: Option<StaticImage>,
	animated_image: Option<AnimatedImage>,
}

struct StaticImage {
	avatar_image: ImageBuffer<Rgba<u8>, Vec<u8>>,
}

struct AnimatedImage {
	avatar_bytes: Vec<u8>,
}

impl ImageInfo {
	async fn new(
		avatar_image: Vec<u8>,
		author_name: String,
		content: String,
		is_animated: bool,
	) -> Result<Self, Error> {
		let content_font = FONTS.get(CONTENT_FONT).unwrap();
		let author_font = FONTS.get(AUTHOR_FONT).unwrap();
		let author_name_clone = author_name.clone();
		let content_clone = content.clone();
		let content_font_clone = content_font.clone();
		let author_font_clone = author_font.clone();

		let (img, text_colour) = get_theme(DEFAULT_THEME);
		let img_clone = img.clone();
		let avatar_position = avatar_position(false);

		let mut image_config = QuoteImageConfig {
			bw: false,
			gradient: false,
			new_font: true,
			reverse: false,
		};

		let mut text_layout = TextLayout::default();

		let (text_layout, output, static_image, animated_image) = if is_animated {
			let (tx, rx) = oneshot::channel();
			let avatar_image_clone = avatar_image.clone();
			spawn(move || {
				let mut buffer = Vec::with_capacity(ANIMATED_QUOTE_VEC);
				let mut cursor = Cursor::new(avatar_image_clone);
				let result = quote_animated_image(
					&author_name_clone,
					&content_clone,
					&author_font_clone,
					&content_font_clone,
					text_colour,
					img_clone,
					&mut text_layout,
					avatar_position,
					image_config,
					&mut cursor,
					&mut buffer,
				);
				if tx.send((result, text_layout, buffer)).is_err() {
					warn!("Sender failed to send result");
				}
			});
			let (result, text_layout, output) =
				rx.await.context("Rayon task for quote image panicked")?;
			match result {
				Ok(()) => (
					text_layout,
					output,
					None,
					Some(AnimatedImage {
						avatar_bytes: avatar_image,
					}),
				),
				Err(err) => {
					warn!("Failed to generate animated quote image: {:?}", err);
					return Err(err);
				}
			}
		} else {
			let (tx, rx) = oneshot::channel();
			spawn(move || {
				let buffer = Vec::with_capacity(STATIC_QUOTE_VEC);
				let avatar_resized = resize_avatar(&avatar_image).unwrap();
				let mut cursor = Cursor::new(buffer);
				let result = quote_static_image(
					avatar_resized.clone(),
					&author_name_clone,
					&content_clone,
					&author_font_clone,
					&content_font_clone,
					text_colour,
					img_clone,
					&mut text_layout,
					avatar_position,
					image_config,
					&mut cursor,
				);

				if tx
					.send((result, avatar_resized, text_layout, cursor.into_inner()))
					.is_err()
				{
					warn!("Sender failed to send result");
				}
			});
			let (result, avatar_resized, text_layout, output) =
				rx.await.context("Rayon task for quote image panicked")?;

			match result {
				Ok(()) => (
					text_layout,
					output,
					Some(StaticImage {
						avatar_image: avatar_resized,
					}),
					None,
				),
				Err(err) => {
					warn!("Failed to generate static quote image: {:?}", err);
					return Err(err);
				}
			}
		};
		image_config.new_font = false;
		Ok(Self {
			static_image,
			animated_image,
			author_name,
			content,
			config: image_config,
			author_font: author_font.clone(),
			content_font: (CONTENT_FONT.to_owned(), content_font.clone()),
			text_layout,
			buffer: output,
			img,
			text_colour,
			avatar_position,
			current_theme_name: DEFAULT_THEME.to_owned(),
			filename: if is_animated {
				QUOTE_ANIMATED_FILENAME
			} else {
				QUOTE_STATIC_FILENAME
			}
			.to_owned(),
		})
	}

	async fn toggle_bw(&mut self) -> Result<(), Error> {
		self.config.bw = !self.config.bw;
		self.image_gen().await?;

		Ok(())
	}

	async fn toggle_reverse(&mut self) -> Result<(), Error> {
		self.config.reverse = !self.config.reverse;
		self.avatar_position = avatar_position(self.config.reverse);
		self.image_gen().await?;

		Ok(())
	}

	async fn toggle_gradient(&mut self) -> Result<(), Error> {
		self.config.gradient = !self.config.gradient;
		self.image_gen().await?;

		Ok(())
	}

	async fn random_theme(&mut self) -> Result<(), Error> {
		(self.img, self.text_colour) = get_theme(RANDOM_THEME);
		self.image_gen().await?;

		Ok(())
	}

	async fn new_font(&mut self, font_name: &str, new_font: FontArc) -> Result<(), Error> {
		self.content_font.1 = new_font;
		font_name.clone_into(&mut self.content_font.0);
		self.config.new_font = true;
		self.image_gen().await?;
		self.config.new_font = false;

		Ok(())
	}

	async fn new_theme(&mut self, theme_name: &str) -> Result<(), Error> {
		theme_name.clone_into(&mut self.current_theme_name);
		(self.img, self.text_colour) = get_theme(theme_name);
		self.image_gen().await?;

		Ok(())
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

		let mut buffer = take(&mut self.buffer);

		let (tx, rx) = oneshot::channel();

		if let Some(ref animated_image) = self.animated_image {
			let avatar_bytes = animated_image.avatar_bytes.clone();
			buffer.clear();
			spawn(move || {
				let mut cursor = Cursor::new(avatar_bytes);
				let result = quote_animated_image(
					&author_name,
					&content,
					&author_font,
					&content_font.1,
					text_colour,
					img,
					&mut text_layout,
					avatar_position,
					config,
					&mut cursor,
					&mut buffer,
				);
				if tx.send((result, text_layout, buffer)).is_err() {
					warn!("Sender failed to send result");
				}
			});
		} else {
			let avatar_image = self.static_image.as_ref().unwrap().avatar_image.clone();
			spawn(move || {
				let mut cursor = Cursor::new(buffer);
				let result = quote_static_image(
					avatar_image,
					&author_name,
					&content,
					&author_font,
					&content_font.1,
					text_colour,
					img,
					&mut text_layout,
					avatar_position,
					config,
					&mut cursor,
				);

				if tx.send((result, text_layout, cursor.into_inner())).is_err() {
					warn!("Sender failed to send result");
				}
			});
		}
		let (result, text_layout, output) =
			rx.await.context("Rayon task for quote image panicked")?;
		match result {
			Ok(()) => {
				self.text_layout = text_layout;
				self.buffer = output;
				Ok(())
			}
			Err(err) => {
				warn!("Failed to generate quote image: {:?}", err);
				Err(err)
			}
		}
	}
}

pub async fn quote_internal(
	ctx: SContext<'_>,
	msg: &Message,
	reply: Option<(&Message, GuildId)>,
) -> Result<(), Error> {
	ctx.defer().await?;
	let mut image_handle = {
		let (avatar_url, author_name, text) = if let Some((reply, guild_id)) = reply {
			let (url, name) = if reply.webhook_id.is_some() {
				(
					reply.author.avatar_url().unwrap_or_else(|| {
						reply
							.author
							.static_avatar_url()
							.unwrap_or_else(|| reply.author.default_avatar_url())
					}),
					reply.author.name.clone(),
				)
			} else {
				let member = guild_id.member(&ctx.http(), reply.author.id).await?;
				(
					member.avatar_url().unwrap_or_else(|| {
						reply.author.avatar_url().unwrap_or_else(|| {
							member
								.user
								.static_avatar_url()
								.unwrap_or_else(|| member.user.default_avatar_url())
						})
					}),
					member.user.name,
				)
			};
			(url, format!("- {name}"), reply.content.to_string())
		} else {
			(
				msg.author
					.avatar_url()
					.unwrap_or_else(|| msg.author.default_avatar_url()),
				format!("- {}", msg.author.name),
				msg.content.to_string(),
			)
		};
		let (avatar_image, is_animated) = (
			HTTP_CLIENT
				.get(&avatar_url)
				.send()
				.await?
				.bytes()
				.await?
				.to_vec(),
			avatar_url.contains(".gif") || avatar_url.contains("format=gif"),
		);

		ImageInfo::new(avatar_image, author_name, text, is_animated).await?
	};
	let attachment =
		CreateAttachment::bytes(image_handle.buffer.clone(), image_handle.filename.clone());
	let buttons = [
		CreateButton::new(format!("{}_bw", ctx.id()))
			.style(ButtonStyle::Primary)
			.label("🎨"),
		CreateButton::new(format!("{}_reverse", ctx.id()))
			.style(ButtonStyle::Primary)
			.label("🪞"),
		CreateButton::new(format!("{}_gradient", ctx.id()))
			.style(ButtonStyle::Primary)
			.label("🌫️"),
		CreateButton::new(format!("{}_random", ctx.id()))
			.style(ButtonStyle::Primary)
			.label("🎲"),
	];
	let mut font_select: Vec<CreateSelectMenuOption> = Vec::with_capacity(FONTS.len());

	for font in FONTS.iter() {
		font_select.push(CreateSelectMenuOption::new(*font.0, *font.0));
	}

	let font_menu = CreateSelectMenu::new(
		format!("{}_font_option", ctx.id()),
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
		format!("{}_theme_option", ctx.id()),
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

	let allowed_mentions = CreateAllowedMentions::default().replied_user(false);

	let (message_handle, reply_handle) = if let Some((reply, guild_id)) = reply
		&& let Some(guild_data) = ctx.data().guilds.get(&guild_id)
	{
		let message_url = reply.link().to_string();
		if let Some(channel) = guild_data.settings.quotes_channel {
			let quote_channel = GenericChannelId::new(channel.cast_unsigned());
			quote_channel
				.send_message(
					ctx.http(),
					CreateMessage::default()
						.add_file(attachment.clone())
						.content(&message_url),
				)
				.await?;
		}
		(
			Some(
				ctx.channel_id()
					.send_message(
						ctx.http(),
						CreateMessage::default()
							.add_file(attachment.clone())
							.reference_message(msg)
							.content(message_url)
							.components(&action_row)
							.select_menu(font_menu)
							.select_menu(theme_menu)
							.allowed_mentions(allowed_mentions),
					)
					.await?,
			),
			None,
		)
	} else {
		(
			None,
			Some(
				ctx.send(
					CreateReply::default()
						.attachment(attachment.clone())
						.components(&action_row)
						.allowed_mentions(allowed_mentions),
				)
				.await?,
			),
		)
	};

	let ctx_id_str = ctx.id().to_string();
	let mut final_attachment = attachment.clone();

	let mut collector_stream = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(Duration::from_mins(5))
		.filter(move |interaction| interaction.data.custom_id.starts_with(ctx_id_str.as_str()))
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
		} else if interaction.data.custom_id.ends_with("bw") {
			image_handle.toggle_bw().await?;
		} else if interaction.data.custom_id.ends_with("reverse") {
			image_handle.toggle_reverse().await?;
		} else if interaction.data.custom_id.ends_with("gradient") {
			image_handle.toggle_gradient().await?;
		} else if interaction.data.custom_id.ends_with("random") {
			image_handle.random_theme().await?;
		}
		let mut msg = interaction.message;
		final_attachment =
			CreateAttachment::bytes(image_handle.buffer.clone(), image_handle.filename.clone());
		msg.edit(
			ctx.http(),
			EditMessage::default().new_attachment(final_attachment.clone()),
		)
		.await?;
	}

	if let Some(mut message) = message_handle {
		message
			.edit(
				ctx,
				EditMessage::default()
					.new_attachment(final_attachment)
					.components(&[]),
			)
			.await?;
	} else if let Some(reply) = reply_handle {
		reply
			.edit(
				ctx,
				CreateReply::default()
					.attachment(final_attachment)
					.components(&[]),
			)
			.await?;
	}

	Ok(())
}

/// When your memory is not enough
#[poise::command(
	context_menu_command = "Quote",
	install_context = "Guild|User",
	interaction_context = "Guild|PrivateChannel"
)]
pub async fn quote_menu(
	ctx: SContext<'_>,
	#[description = "Message"] msg: Message,
) -> Result<(), Error> {
	if msg.content.is_empty() {
		ctx.reply("Bruh, this message is empty").await?;
		return Ok(());
	}
	quote_internal(ctx, &msg, None).await?;
	Ok(())
}

/// When your memory is not enough
#[poise::command(
	prefix_command,
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn quote(ctx: SContext<'_>) -> Result<(), Error> {
	let Some(guild_id) = ctx.guild_id() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Ok(());
	};
	let msg = ctx
		.channel_id()
		.message(&ctx.http(), MessageId::new(ctx.id()))
		.await?;

	let Some(ref reply) = msg.referenced_message else {
		ctx.reply("Bruh, reply to a message").await?;
		return Ok(());
	};

	if reply.content.is_empty() {
		ctx.reply("Bruh, this message is empty").await?;
		return Ok(());
	}

	quote_internal(ctx, &msg, Some((reply, guild_id))).await?;

	Ok(())
}

#[poise::command(
	prefix_command,
	install_context = "Guild",
	interaction_context = "Guild",
	owners_only
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
	ctx.defer().await?;
	let utils_config = UTILS_CONFIG.get().unwrap();

	if let Ok(resp) = ai_response_simple(
		"Mock this Discord message someone posted. Just give the roast, nothing else.",
		&message.content,
		&utils_config.fabseserver.text_gen_model,
	)
	.await && !resp.is_empty()
	{
		ctx.say(resp).await?;
	} else {
		ctx.say("stfu").await?;
	}
	Ok(())
}

/// When your users are yapping
#[poise::command(
	slash_command,
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS",
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn slow_mode(
	ctx: SContext<'_>,
	#[description = "Channel to rate limit"] mut channel: GuildChannel,
	#[description = "Duration of rate limit in seconds"]
	#[min = 300]
	#[max = 21600]
	duration: u16,
) -> Result<(), Error> {
	let settings = EditChannel::default().rate_limit_per_user(NonMaxU16::new(duration).unwrap());
	channel.edit(ctx.http(), settings).await?;
	ctx.send(
		CreateReply::default()
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
	install_context = "Guild",
	interaction_context = "Guild"
)]
pub async fn word_count(ctx: SContext<'_>) -> Result<(), Error> {
	let Some(guild_id) = ctx.guild_id() else {
		ctx.reply(NOT_IN_GUILD_MSG).await?;
		return Ok(());
	};
	let thumbnail = {
		let guild = ctx.guild().unwrap();
		guild.banner_url().unwrap_or_else(|| {
			guild
				.icon_url()
				.unwrap_or_else(|| "https://c.tenor.com/SgNWLvwATMkAAAAC/bruh.gif".to_owned())
		})
	};

	let words: Vec<_> = ctx
		.data()
		.guilds
		.get(&guild_id)
		.map_or_else(Vec::new, |guild_data| {
			let mut result: Vec<_> = guild_data
				.word_tracking
				.iter()
				.map(|entry| WordCount {
					word: entry.word.clone(),
					count: entry.count,
				})
				.collect();
			result.sort_by_key(|b| Reverse(b.count));
			result.truncate(25);
			result
		});

	let mut embed = CreateEmbed::default()
		.title(format!("Top {} word tracked by count", words.len()))
		.thumbnail(thumbnail)
		.colour(COLOUR_RED);
	for (index, word) in words.iter().enumerate() {
		let rank = index.saturating_add(1);
		embed = embed.field(
			format!("#{rank} {}", word.word),
			word.count.to_string(),
			false,
		);
	}
	ctx.send(CreateReply::default().reply(true).embed(embed))
		.await?;

	Ok(())
}
