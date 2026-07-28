use fabsebot_core::{
	config::{
		constants::{FAILED_SONG_FETCH, MISSING_REPLY_MSG, QUEUEING_MSG},
		types::{ContextType, Error, SContext},
	},
	errors::commands::AIError,
	utils::{
		ai::ai_voice,
		helpers::url_bytes,
		voice::{
			ALREADY_IN_VOICE_CHAN_MSG, PayloadType, add_payload, add_youtube_song,
			check_in_channel, lavalink_play, lavalink_try_join, remove_handler, try_voice,
		},
	},
};
use poise::CreateReply;
use serenity::{all::MessageId, model::channel::Attachment};

/// Text to voice, duh
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn text_to_voice(ctx: SContext<'_>, input: Option<String>) -> Result<(), Error> {
	let payload = if let Some(input) = input {
		input
	} else if let Ok(msg) = ctx
		.channel_id()
		.message(&ctx.http(), MessageId::new(ctx.id()))
		.await && let Some(reply) = msg.referenced_message
		&& !reply.content.is_empty()
	{
		reply.content.into_string()
	} else {
		ctx.reply(MISSING_REPLY_MSG).await?;
		return Ok(());
	};
	let Some((_typing, guild_id, handler_lock)) = try_voice(ctx, false).await? else {
		return Ok(());
	};
	let bytes = match ai_voice(&payload).await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("I don't wanna speak now").await?;
			return Err(AIError::TTSFailed(err).into());
		}
	};
	add_payload(
		&ctx,
		&handler_lock,
		bytes,
		PayloadType::TextToVoice,
		guild_id,
	)
	.await?;

	Ok(())
}

/// Join the current voice channel (old implementation)
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn join_voice_old(
	ctx: SContext<'_>,
	#[description = "Allow music playback across guilds"]
	#[flag]
	global: bool,
) -> Result<(), Error> {
	if check_in_channel(ctx, false).is_none() {
		ctx.reply(ALREADY_IN_VOICE_CHAN_MSG).await?;
		return Ok(());
	}
	try_voice(ctx, global).await?;
	Ok(())
}

/// Leave the current voice channel
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | CONNECT \
	                            | SPEAK"
)]
pub async fn leave_voice(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	if let Err(err) = remove_handler(guild_id).await {
		ctx.reply(
			"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel \
			 first",
		)
		.await?;
		return Err(err);
	}
	ctx.reply("Left voice channel, don't forget me").await?;

	Ok(())
}

#[expect(clippy::doc_markdown)]
/// Old implementation, prone to blocking from YouTube
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn play_song_old(
	ctx: SContext<'_>,
	#[description = "YouTube link or query to search"]
	#[rest]
	url: String,
) -> Result<(), Error> {
	let Some((_typing, guild_id, handler_lock)) = try_voice(ctx, false).await? else {
		return Ok(());
	};
	let reply = ctx.reply(QUEUEING_MSG).await?;
	let msg = reply.message().await?;
	if let Err(err) = add_youtube_song(
		url,
		&handler_lock,
		guild_id,
		msg.id,
		msg.channel_id,
		ctx.author().id,
		&ctx.data().db,
	)
	.await
	{
		reply
			.edit(ctx, CreateReply::new().content(FAILED_SONG_FETCH))
			.await?;
		return Err(err);
	}

	Ok(())
}

/// Join the current voice channel
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn join_voice(ctx: SContext<'_>) -> Result<(), Error> {
	let Some(guild_id) = check_in_channel(ctx, true) else {
		ctx.reply(ALREADY_IN_VOICE_CHAN_MSG).await?;
		return Ok(());
	};
	lavalink_try_join(ContextType::Poise(ctx), guild_id, ctx.author().id).await?;
	Ok(())
}

/// Add song(s) to queue in the current voice channel
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn play_song(
	ctx: SContext<'_>,
	#[description = "YouTube link to song or playlist OR query to search"]
	#[rest]
	url: String,
) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	let Some((_typing, player_context)) =
		lavalink_try_join(ContextType::Poise(ctx), guild_id, ctx.author().id).await?
	else {
		return Ok(());
	};
	let reply = ctx.reply(QUEUEING_MSG).await?;
	let msg = reply.message().await?;
	if let Err(err) = lavalink_play(
		ctx.serenity_context(),
		guild_id,
		msg.id,
		msg.channel_id,
		ctx.author().id,
		&url,
		player_context,
		&ctx.data().db,
	)
	.await
	{
		reply
			.edit(ctx, CreateReply::new().content(FAILED_SONG_FETCH))
			.await?;
		return Err(err);
	}

	Ok(())
}

/// Add a custom audio file to queue
#[poise::command(
	slash_command,
	install_context = "Guild | User",
	interaction_context = "Guild | PrivateChannel"
)]
pub async fn play_file(ctx: SContext<'_>, attachment: Attachment) -> Result<(), Error> {
	if let Some(content_type) = attachment.content_type.as_deref()
		&& content_type.starts_with("audio")
	{
		let Some((_typing, guild_id, handler_lock)) = try_voice(ctx, false).await? else {
			return Ok(());
		};
		if let Ok(bytes) = url_bytes(&attachment.url).await {
			add_payload(&ctx, &handler_lock, bytes, PayloadType::Custom, guild_id).await?;
		} else {
			ctx.reply("Failed to fetch attachment :/").await?;
		}
	} else {
		ctx.reply("Why you give me an invalid audio format >:(")
			.await?;
	}

	Ok(())
}
