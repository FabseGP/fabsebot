use fabsebot_core::{
	config::{
		constants::{FAILED_SONG_FETCH, MISSING_REPLY_MSG, NOT_IN_VOICE_CHAN_MSG, QUEUEING_MSG},
		types::{Error, HTTP_CLIENT, SContext},
	},
	errors::commands::{AIError, InteractionError, MusicError},
	utils::{
		ai::ai_voice,
		helpers::{fetch_and_parse, non_empty_vec, url_bytes},
		voice::{
			PayloadType, add_payload, add_playlist, add_youtube_song, lavalink_delete,
			lavalink_join, lavalink_play, remove_handler, try_voice,
		},
	},
};
use poise::CreateReply;
use serde::Deserialize;
use serenity::{all::MessageId, model::channel::Attachment};
use tokio::process::Command;
use url::Url;

#[derive(Deserialize)]
struct DeezerResponse {
	tracks: DeezerData,
}

#[derive(Deserialize)]
struct DeezerData {
	#[serde(deserialize_with = "non_empty_vec")]
	data: Vec<DeezerTracks>,
}

#[derive(Deserialize)]
struct DeezerTracks {
	title: String,
	artist: DeezerArtist,
}

#[derive(Deserialize)]
struct DeezerArtist {
	name: String,
}

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
		return Err(InteractionError::EmptyMessage.into());
	};
	let (_typing, guild_id, handler_lock) = try_voice(ctx, false).await?;
	let bytes = match ai_voice(&payload).await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("I don't wanna speak now").await?;
			return Err(AIError::TTSFailed(err).into());
		}
	};
	add_payload(
		&ctx,
		handler_lock,
		bytes,
		PayloadType::TextToVoice,
		guild_id,
	)
	.await?;

	Ok(())
}

/// Add all songs in a playlist from Deezer to queue
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn add_deezer_playlist(
	ctx: SContext<'_>,
	#[description = "ID of the playlist in mind"]
	#[rest]
	playlist_id: String,
) -> Result<(), Error> {
	let (_typing, guild_id, handler_lock) = try_voice(ctx, false).await?;
	let payload: DeezerResponse = match fetch_and_parse(
		HTTP_CLIENT
			.get(format!("https://api.deezer.com/playlist/{playlist_id}"))
			.send(),
	)
	.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Invalid id for Deezer playlist").await?;
			return Err(err);
		}
	};
	let urls: Vec<String> = payload
		.tracks
		.data
		.iter()
		.map(|d| format!("{} {}", d.title, d.artist.name))
		.collect();
	add_playlist(ctx, guild_id, urls, handler_lock).await?;

	Ok(())
}

/// Add all songs in a playlist from ``YouTube`` to queue
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn add_youtube_playlist(
	ctx: SContext<'_>,
	#[description = "Url playlist in mind"]
	#[rest]
	playlist_url: String,
) -> Result<(), Error> {
	let (_typing, guild_id, handler_lock) = try_voice(ctx, false).await?;
	let yt_dlp_output = match Command::new("yt-dlp")
		.args([
			"--flat-playlist",
			"--print",
			"url",
			"--no-warnings",
			&playlist_url,
		])
		.output()
		.await
	{
		Ok(res) => res,
		Err(err) => {
			ctx.reply("YouTube bailed out :/").await?;
			return Err(MusicError::FailedFetchPlaylist(err).into());
		}
	};
	let urls: Vec<String> = String::from_utf8(yt_dlp_output.stdout)?
		.lines()
		.filter(|line| Url::parse(line).is_ok())
		.map(ToString::to_string)
		.collect();
	add_playlist(ctx, guild_id, urls, handler_lock).await?;

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
pub async fn join_voice(
	ctx: SContext<'_>,
	#[description = "Allow music playback across guilds"]
	#[flag]
	global: bool,
) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	if ctx.data().music_manager.get(guild_id).is_some() {
		ctx.reply(NOT_IN_VOICE_CHAN_MSG).await?;
		return Ok(());
	}
	let (_typing, _guild_id, _handler_lock) = try_voice(ctx, global).await?;

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
	remove_handler(ctx, guild_id).await?;
	ctx.reply("Left voice channel, don't forget me").await?;

	Ok(())
}

/// Add song to queue in the current voice channel
#[poise::command(
	prefix_command,
	slash_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn play_song(
	ctx: SContext<'_>,
	#[description = "YouTube link or query to search"]
	#[rest]
	url: String,
) -> Result<(), Error> {
	let (_typing, guild_id, handler_lock) = try_voice(ctx, false).await?;
	let reply = ctx.reply(QUEUEING_MSG).await?;
	let msg = reply.message().await?;
	if let Err(err) = add_youtube_song(
		url,
		handler_lock,
		guild_id,
		i64::from(msg.id),
		i64::from(msg.channel_id),
		i64::from(ctx.author().id),
		&ctx.data().db,
		Some(&ctx),
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

/// Join the current voice channel (lavalink)
#[poise::command(
	prefix_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn join_lavalink(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	if ctx.data().music_manager.get(guild_id).is_some() {
		ctx.reply("Bruh, I'm already in a voice channel! Use /leave_voice to drop the connection")
			.await?;
		return Ok(());
	}
	lavalink_join(ctx, guild_id).await?;
	Ok(())
}

/// Leave the current voice channel (lavalink)
#[poise::command(
	prefix_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | CONNECT \
	                            | SPEAK"
)]
pub async fn leave_lavalink(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	remove_handler(ctx, guild_id).await?;
	lavalink_delete(ctx, guild_id).await?;

	Ok(())
}

/// Add song to queue in the current voice channel (lavalink)
#[poise::command(
	prefix_command,
	guild_only,
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn play_lavalink(
	ctx: SContext<'_>,
	#[description = "YouTube link or query to search"]
	#[rest]
	url: String,
) -> Result<(), Error> {
	let guild_id = ctx.guild_id().unwrap();
	lavalink_play(ctx, guild_id, url).await?;

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
		let (_typing, guild_id, handler_lock) = try_voice(ctx, false).await?;
		if let Ok(bytes) = url_bytes(&attachment.url).await {
			add_payload(&ctx, handler_lock, bytes, PayloadType::Custom, guild_id).await?;
		} else {
			ctx.reply("Failed to fetch attachment :/").await?;
		}
	} else {
		ctx.reply("Why you give me an invalid audio format >:(")
			.await?;
	}

	Ok(())
}
