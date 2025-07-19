use std::{process, sync::Arc, time::Duration};

use ab_glyph::FontArc;
use anyhow::Context as _;
use dashmap::DashSet;
use fabsebot_core::{
	config::{
		constants::{COLOUR_RED, FONTS},
		types::{CLIENT_DATA, Error, HTTP_CLIENT, SContext, SYSTEM_STATS, UTILS_CONFIG},
	},
	utils::{
		ai::ai_response_simple,
		image::{TextLayout, quote_image},
	},
};
use image::{ImageBuffer, Rgba};
use poise::{CreateReply, builtins::register_globally};
use rayon::spawn;
use serenity::{
	all::{
		ActivityData, ButtonStyle, ComponentInteractionCollector, ComponentInteractionDataKind,
		CreateActionRow, CreateAllowedMentions, CreateAttachment, CreateButton, CreateComponent,
		CreateEmbed, CreateInteractionResponse, CreateMessage, CreateSelectMenu,
		CreateSelectMenuKind, CreateSelectMenuOption, EditChannel, EditMessage, GenericChannelId,
		GuildChannel, Member, Message, MessageId, OnlineStatus, ShardRunnerMessage, UserId,
	},
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
#[poise::command(slash_command)]
pub async fn anony_poll(
	ctx: SContext<'_>,
	#[description = "Question"] title: String,
	#[description = "Comma-separated options"] options: String,
	#[description = "Duration in minutes"] duration: u64,
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

	let ctx_id_copy = ctx.id();
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
				.embed(embed)
				.components(&action_row)
				.reply(true),
		)
		.await?;

	let mut vote_counts = vec![0; options_count];
	let voted_users = DashSet::new();

	while let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(Duration::from_secs(duration.saturating_mul(60)))
		.filter(move |interaction| {
			interaction
				.data
				.custom_id
				.starts_with(ctx_id_copy.to_string().as_str())
		})
		.await
	{
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

/// Send a birthday wish to a member
#[poise::command(prefix_command, slash_command)]
pub async fn birthday(
	ctx: SContext<'_>,
	#[description = "Member to congratulate"]
	#[rest]
	member: Member,
) -> Result<(), Error> {
	let avatar_url = member.avatar_url().unwrap_or_else(|| {
		member.user.avatar_url().unwrap_or_else(|| {
			member
				.user
				.avatar_url()
				.unwrap_or_else(|| member.user.default_avatar_url())
		})
	});
	let name = member.display_name();
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

/// Fabsebot control
#[poise::command(slash_command, owners_only)]
pub async fn bot_control(
	ctx: SContext<'_>,
	new_activity_opt: Option<String>,
	new_status_opt: Option<String>,
	new_nickname_opt: Option<String>,
) -> Result<(), Error> {
	if let Some(new_activity) = new_activity_opt {
		ctx.framework()
			.serenity_context
			.set_activity(Some(ActivityData::listening(new_activity)));
	}

	if let Some(new_status_str) = new_status_opt {
		let new_status = match new_status_str.as_str() {
			"invisible" => OnlineStatus::Invisible,
			"dnd" => OnlineStatus::DoNotDisturb,
			"idle" => OnlineStatus::Idle,
			_ => OnlineStatus::Online,
		};
		ctx.framework().serenity_context.set_status(new_status);
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
			.content("Fabsebot rebranded!")
			.ephemeral(true),
	)
	.await?;

	Ok(())
}

/// Debugging fabsebot's host
#[poise::command(prefix_command, slash_command)]
pub async fn debug(ctx: SContext<'_>) -> Result<(), Error> {
	ctx.framework()
		.serenity_context
		.set_activity(Some(ActivityData::playing("pizza")));

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

	let button = [CreateButton::new(format!("{}_shard_restart", ctx.id()))
		.style(ButtonStyle::Primary)
		.label("Restart shard")];

	let message = ctx
		.send(
			CreateReply::default()
				.embed(embed.clone())
				.reply(true)
				.components(&[CreateComponent::ActionRow(CreateActionRow::Buttons(
					Cow::Borrowed(&button),
				))]),
		)
		.await?;

	let ctx_id_copy = ctx.id();
	if let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
		.timeout(Duration::from_secs(60))
		.filter(move |interaction| {
			interaction
				.data
				.custom_id
				.starts_with(ctx_id_copy.to_string().as_str())
		})
		.await
	{
		let mut msg = interaction.message;
		if let Some(runner) = ctx
			.serenity_context()
			.runners
			.get(&ctx.serenity_context().shard_id)
			.map(|entry| entry.1.clone())
		{
			if let Err(err) = runner.unbounded_send(ShardRunnerMessage::Restart) {
				warn!("Failed to queue restart of shard: {:?}", err);
				msg.edit(
					ctx.http(),
					EditMessage::default()
						.embed(embed)
						.components(vec![])
						.content("Rip, failed to restart shard!"),
				)
				.await?;
			} else {
				msg.edit(
					ctx.http(),
					EditMessage::default()
						.embed(embed)
						.components(vec![])
						.content("Woah shard restarted!"),
				)
				.await?;
			}
		} else {
			warn!("No shard runner found in runners map");
			msg.edit(
				ctx.http(),
				EditMessage::default()
					.embed(embed)
					.components(vec![])
					.content("Rip, shard doesn't exist!"),
			)
			.await?;
		}
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
#[poise::command(prefix_command, owners_only)]
#[expect(clippy::unused_async)]
pub async fn end_pgo(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(shutdown_trigger) = CLIENT_DATA
		.get()
		.map(|c| c.shard_manager.get_shutdown_trigger())
	{
		ctx.serenity_context().shutdown_all();
		if shutdown_trigger() {
			warn!("Successfully triggered shutdown for all shards");
		} else {
			warn!("Failed to trigger shutdown, shards may have already stopped");
			process::exit(1);
		}
	} else {
		process::exit(1);
	}

	Ok(())
}

/// When you're not lonely anymore
#[poise::command(prefix_command, slash_command)]
pub async fn global_chat_end(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
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
		{
			let ctx_data = ctx.data();
			let guild_settings_lock = ctx_data.guild_data.lock().await;
			let mut current_settings_opt = guild_settings_lock.get(&guild_id);
			let mut modified_settings = current_settings_opt
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.settings.global_chat = false;
			modified_settings.settings.global_chat_channel = None;
			guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
		}
		ctx.reply("Call ended...").await?;
	}
	Ok(())
}

/// When you're lonely and need someone to chat with
#[poise::command(prefix_command, slash_command)]
pub async fn global_chat_start(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
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
		let ctx_data = ctx.data();
		{
			let guild_settings_lock = ctx_data.guild_data.lock().await;
			let mut current_settings_opt = guild_settings_lock.get(&guild_id);
			let mut modified_settings = current_settings_opt
				.get_or_insert_default()
				.as_ref()
				.clone();
			modified_settings.settings.global_chat = true;
			modified_settings.settings.global_chat_channel = Some(channel_id_i64);
			guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
		}
		let message = ctx.reply("Calling...").await?;
		let result = timeout(Duration::from_secs(60), async {
			loop {
				let has_other_calls = ctx_data.guild_data.lock().await.iter().any(|entry| {
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
		if result.is_ok() {
			message
				.edit(
					ctx,
					CreateReply::default()
						.reply(true)
						.content("Connected to global call!"),
				)
				.await?;
		} else {
			query!(
				"UPDATE guild_settings SET global_chat = FALSE, global_chat_channel = NULL WHERE \
				 guild_id = $1",
				guild_id_i64
			)
			.execute(&mut *tx)
			.await?;
			{
				let guild_settings_lock = ctx_data.guild_data.lock().await;
				let mut current_settings_opt = guild_settings_lock.get(&guild_id);
				let mut modified_settings = current_settings_opt
					.get_or_insert_default()
					.as_ref()
					.clone();
				modified_settings.settings.global_chat = false;
				modified_settings.settings.global_chat_channel = None;
				guild_settings_lock.insert(guild_id, Arc::new(modified_settings));
			}
			message
				.edit(
					ctx,
					CreateReply::default()
						.reply(true)
						.content("No one joined the call within 1 minute 😢"),
				)
				.await?;
		}

		tx.commit()
			.await
			.context("Failed to commit sql-transaction")?;
	}
	Ok(())
}

/// When you need some help
#[poise::command(prefix_command, slash_command)]
pub async fn help(
	ctx: SContext<'_>,
	#[description = "Command to get help with"] command: Option<String>,
) -> Result<(), Error> {
	ctx.say("help").await?;
	Ok(())
}

struct UserCount {
	id: i64,
	count: i32,
}

/// Leaderboard of lifeless ppl
#[poise::command(prefix_command, slash_command)]
pub async fn leaderboard(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
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

		let mut users = ctx
			.data()
			.user_settings
			.lock()
			.await
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

		users.sort_by(|a, b| b.count.cmp(&a.count));
		users.truncate(25);

		let mut embed = CreateEmbed::default()
			.title(format!("Top {} users by message count", users.len()))
			.thumbnail(thumbnail)
			.colour(COLOUR_RED);

		for (index, user) in users.iter().enumerate() {
			if let Ok(user_id_u64) = u64::try_from(user.id) {
				if let Ok(target) = guild_id.member(&ctx.http(), UserId::new(user_id_u64)).await {
					embed = embed.field(
						format!("#{} {}", index.saturating_add(1), target.display_name()),
						user.count.to_string(),
						false,
					);
				}
			} else {
				warn!("Failed to convert user id to u64");
			}
		}

		ctx.send(CreateReply::default().reply(true).embed(embed))
			.await?;
	}
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
	if let Some(utils_config) = UTILS_CONFIG.get() {
		if let Some(resp) = ai_response_simple(
			"you're a tsundere",
			"generate a one-line love-hate greeting",
			&utils_config.fabseserver.text_gen_model,
		)
		.await
		{
			ctx.reply(resp).await?;
		} else {
			ctx.reply(
				"Ugh, fine. It's nice to see you again, I suppose... 
                for now, don't get any ideas thinking this means I actually like you or anything",
			)
			.await?;
		}
	} else {
		ctx.reply("User err- I mean, system error. Please standby")
			.await?;
	}
	Ok(())
}

struct ImageInfo {
	avatar_image: Option<Arc<Vec<u8>>>,
	avatar_resized: Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>>,
	author_name: String,
	content: String,
	current: Vec<u8>,
	is_animated: bool,
	is_bw: bool,
	is_reverse: bool,
	is_light: bool,
	is_gradient: bool,
	new_font: bool,
	content_font: FontArc,
	author_font: FontArc,
	current_font_name: String,
	text_layout: Option<TextLayout>,
}

impl ImageInfo {
	async fn new(
		avatar_image: &[u8],
		author_name: String,
		content: String,
		is_animated: bool,
	) -> Result<Self, Error> {
		let (content_font_data, author_font_data) = FONTS
			.first()
			.and_then(|content| FONTS.get(1).map(|author| (&content.1, &author.1)))
			.context("Missing default fonts in FONTS array")?;
		let content_font =
			FontArc::try_from_slice(content_font_data).context("Failed to load content font")?;
		let author_font =
			FontArc::try_from_slice(author_font_data).context("Failed to load author font")?;

		let current_font_name = FONTS
			.first()
			.map(|(name, _)| (*name).to_owned())
			.context("No fonts available in FONTS array")?;
		let (tx, rx) = oneshot::channel();

		let avatar_image_clone = avatar_image.to_vec();
		let author_name_clone = author_name.clone();
		let content_clone = content.clone();
		let content_font_clone = content_font.clone();
		let author_font_clone = author_font.clone();

		spawn(move || {
			let result = quote_image(
				Some(&avatar_image_clone),
				None,
				&author_name_clone,
				&content_clone,
				&author_font_clone,
				&content_font_clone,
				None,
				None,
				false,
				false,
				false,
				false,
				is_animated,
				false,
			);
			if let Err(err) = tx.send(result) {
				warn!("Sender failed to send result: {:?}", err);
			}
		});
		match rx.await.context("Rayon task for quote image panicked")? {
			Ok(img_gen) => {
				let (image, text_layout, avatar_resized) = img_gen;
				Ok(Self {
					avatar_image: if avatar_resized.is_some() {
						None
					} else {
						Some(Arc::new(avatar_image.to_vec()))
					},
					avatar_resized: avatar_resized.map(Arc::new),
					author_name,
					content,
					current: image,
					is_animated,
					is_bw: false,
					is_reverse: false,
					is_light: false,
					is_gradient: false,
					new_font: false,
					author_font,
					content_font,
					current_font_name,
					text_layout,
				})
			}
			Err(err) => {
				warn!("Failed to generate quote image: {:?}", err);
				Err(err)
			}
		}
	}

	async fn toggle_bw(&mut self) -> Result<(), Error> {
		self.is_bw = !self.is_bw;
		self.image_gen().await?;

		Ok(())
	}

	async fn toggle_reverse(&mut self) -> Result<(), Error> {
		self.is_reverse = !self.is_reverse;
		self.image_gen().await?;

		Ok(())
	}

	async fn toggle_light(&mut self) -> Result<(), Error> {
		self.is_light = !self.is_light;
		self.image_gen().await?;

		Ok(())
	}

	async fn toggle_gradient(&mut self) -> Result<(), Error> {
		self.is_gradient = !self.is_gradient;
		self.image_gen().await?;

		Ok(())
	}

	async fn new_font(&mut self, font_name: &str, new_font: FontArc) -> Result<(), Error> {
		self.content_font = new_font;
		font_name.clone_into(&mut self.current_font_name);
		self.new_font = true;
		self.image_gen().await?;

		Ok(())
	}

	async fn image_gen(&mut self) -> Result<(), Error> {
		let avatar_image = self.avatar_image.clone();
		let avatar_resized = self.avatar_resized.clone();
		let author_name = self.author_name.clone();
		let content = self.content.clone();
		let author_font = self.author_font.clone();
		let content_font = self.content_font.clone();
		let text_layout = self.text_layout.clone();
		let is_reverse = self.is_reverse;
		let is_light = self.is_light;
		let is_bw = self.is_bw;
		let is_gradient = self.is_gradient;
		let is_animated = self.is_animated;
		let new_font = self.new_font;

		let (tx, rx) = oneshot::channel();

		spawn(move || {
			let avatar_bytes = avatar_image.as_ref().map(|arc_vec| &arc_vec[..]);
			let result = quote_image(
				avatar_bytes,
				avatar_resized.as_deref().cloned(),
				&author_name,
				&content,
				&author_font,
				&content_font,
				None,
				text_layout.as_ref(),
				is_reverse,
				is_light,
				is_bw,
				is_gradient,
				is_animated,
				new_font,
			);

			if let Err(err) = tx.send(result) {
				warn!("Sender failed to send result: {:?}", err);
			}
		});
		match rx.await.context("Rayon task for quote image panicked")? {
			Ok(img_gen) => {
				let (image, text_layout, _) = img_gen;
				if new_font {
					self.new_font = false;
					self.text_layout = text_layout;
				}
				self.current = image;
				Ok(())
			}
			Err(err) => {
				warn!("Failed to generate quote image: {:?}", err);
				Err(err)
			}
		}
	}
}

/// When your memory is not enough
#[poise::command(prefix_command)]
pub async fn quote(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
		let msg = ctx
			.channel_id()
			.message(&ctx.http(), MessageId::new(ctx.id()))
			.await?;

		let Some(ref reply) = msg.referenced_message else {
			ctx.reply("Bruh, reply to a message").await?;
			return Ok(());
		};

		ctx.defer().await?;

		let mut image_handle = {
			let (avatar_image, is_animated, author_name) = if reply.webhook_id.is_some() {
				let avatar_url = reply.author.avatar_url().unwrap_or_else(|| {
					reply
						.author
						.static_avatar_url()
						.unwrap_or_else(|| reply.author.default_avatar_url())
				});
				(
					HTTP_CLIENT.get(&avatar_url).send().await?.bytes().await?,
					avatar_url.contains(".gif"),
					format!("- {}", reply.author.name),
				)
			} else {
				let member = guild_id.member(&ctx.http(), reply.author.id).await?;
				let avatar_url = member.avatar_url().unwrap_or_else(|| {
					reply.author.avatar_url().unwrap_or_else(|| {
						member
							.user
							.static_avatar_url()
							.unwrap_or_else(|| member.user.default_avatar_url())
					})
				});
				(
					HTTP_CLIENT.get(&avatar_url).send().await?.bytes().await?,
					avatar_url.contains(".gif"),
					format!("- {}", member.user.name),
				)
			};

			ImageInfo::new(
				&avatar_image,
				author_name,
				reply.content.to_string(),
				is_animated,
			)
			.await?
		};
		let message_url = reply.link();
		let attachment = CreateAttachment::bytes(
			image_handle.current.clone(),
			if image_handle.is_animated {
				"quote.gif"
			} else {
				"quote.webp"
			},
		);
		let buttons = [
			CreateButton::new(format!("{}_bw", ctx.id()))
				.style(ButtonStyle::Primary)
				.label("🎨"),
			CreateButton::new(format!("{}_reverse", ctx.id()))
				.style(ButtonStyle::Primary)
				.label("🪞"),
			CreateButton::new(format!("{}_light", ctx.id()))
				.style(ButtonStyle::Primary)
				.label("🔆"),
			CreateButton::new(format!("{}_gradient", ctx.id()))
				.style(ButtonStyle::Primary)
				.label("🌫️"),
		];
		let mut font_select: Vec<CreateSelectMenuOption> = Vec::with_capacity(FONTS.len());

		for font in FONTS {
			font_select.push(CreateSelectMenuOption::new(font.0, font.0));
		}

		let font_menu = CreateSelectMenu::new(
			format!("{}_font_option", ctx.id()),
			CreateSelectMenuKind::String {
				options: font_select.into(),
			},
		)
		.placeholder("Font")
		.min_values(1)
		.max_values(1);
		let action_row = [CreateComponent::ActionRow(CreateActionRow::buttons(
			&buttons,
		))];
		let mut message = ctx
			.channel_id()
			.send_message(
				ctx.http(),
				CreateMessage::default()
					.add_file(attachment.clone())
					.reference_message(&msg)
					.content(&message_url)
					.components(&action_row)
					.select_menu(font_menu)
					.allowed_mentions(CreateAllowedMentions::default().replied_user(false)),
			)
			.await?;
		if let Some(guild_data) = ctx.data().guild_data.lock().await.get(&guild_id)
			&& let Some(channel) = guild_data.settings.quotes_channel
		{
			if let Ok(channel_u64) = u64::try_from(channel) {
				let quote_channel = GenericChannelId::new(channel_u64);
				quote_channel
					.send_message(
						ctx.http(),
						CreateMessage::default()
							.add_file(attachment.clone())
							.content(&message_url),
					)
					.await?;
			} else {
				warn!("Failed to convert quotes channel id to u64");
			}
		}
		let ctx_id_copy = ctx.id();
		let mut final_attachment = attachment.clone();
		while let Some(interaction) = ComponentInteractionCollector::new(ctx.serenity_context())
			.timeout(Duration::from_secs(60))
			.filter(move |interaction| {
				interaction
					.data
					.custom_id
					.starts_with(ctx_id_copy.to_string().as_str())
			})
			.await
		{
			interaction
				.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
				.await?;

			let menu_choice = match &interaction.data.kind {
				ComponentInteractionDataKind::StringSelect { values } => values.first(),
				_ => None,
			};

			if let Some(font_choice) = menu_choice
				&& let Some(font) = FONTS.iter().find(|font| font.0 == font_choice)
				&& font.0 != image_handle.current_font_name
				&& let Ok(new_font) = FontArc::try_from_slice(font.1)
			{
				image_handle.new_font(font.0, new_font).await?;
			} else if interaction.data.custom_id.ends_with("bw") {
				image_handle.toggle_bw().await?;
			} else if interaction.data.custom_id.ends_with("reverse") {
				image_handle.toggle_reverse().await?;
			} else if interaction.data.custom_id.ends_with("light") {
				image_handle.toggle_light().await?;
			} else if interaction.data.custom_id.ends_with("gradient") {
				image_handle.toggle_gradient().await?;
			}
			let mut msg = interaction.message;
			final_attachment = CreateAttachment::bytes(
				image_handle.current.clone(),
				if image_handle.is_animated {
					"quote.gif"
				} else {
					"quote.webp"
				},
			);
			msg.edit(
				ctx.http(),
				EditMessage::default().new_attachment(final_attachment.clone()),
			)
			.await?;
		}
		message
			.edit(
				ctx,
				EditMessage::default()
					.new_attachment(final_attachment)
					.components(&[]),
			)
			.await?;
	}
	Ok(())
}

#[poise::command(prefix_command, owners_only)]
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
	if let Some(utils_config) = UTILS_CONFIG.get()
		&& let Some(resp) = ai_response_simple(
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
	required_permissions = "ADMINISTRATOR | MODERATE_MEMBERS"
)]
pub async fn slow_mode(
	ctx: SContext<'_>,
	#[description = "Channel to rate limit"] mut channel: GuildChannel,
	#[description = "Duration of rate limit in seconds"] duration: NonMaxU16,
) -> Result<(), Error> {
	let settings = EditChannel::default().rate_limit_per_user(duration);
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
#[poise::command(prefix_command, slash_command)]
pub async fn word_count(ctx: SContext<'_>) -> Result<(), Error> {
	if let Some(guild_id) = ctx.guild_id() {
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

		let mut words = ctx
			.data()
			.guild_data
			.lock()
			.await
			.get(&guild_id)
			.map_or_else(Vec::new, |guild_data| {
				let capacity = guild_data.word_tracking.len();
				let mut result = Vec::with_capacity(capacity);

				for entry in &guild_data.word_tracking {
					result.push(WordCount {
						word: entry.word.clone(),
						count: entry.count,
					});
				}

				result
			});

		words.sort_by(|a, b| b.count.cmp(&a.count));
		words.truncate(25);

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
	}
	Ok(())
}
