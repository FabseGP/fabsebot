use core::time::Duration;

use fabsebot_core::{
	config::{
		constants::{
			FAILED_SONG_FETCH, INVALID_TRACK_SOURCE, MISSING_METADATA_MSG, MISSING_REPLY_MSG,
			QUEUE_MSG,
		},
		types::{Error, HTTP_CLIENT, SContext},
	},
	errors::commands::{AIError, HTTPError, InteractionError, MusicError},
	utils::{
		ai::ai_voice,
		helpers::non_empty_vec,
		voice::{get_configured_songbird_handler, queue_song, youtube_source},
	},
};
use serde::Deserialize;
use serenity::all::{CreateMessage, GenericChannelId, GuildId, MessageId};
use songbird::input::{Compose as _, Input, YoutubeDl};
use sqlx::{query, query_scalar};
use tokio::process::Command;
use tracing::warn;
use url::Url;

use crate::{remove_handler, require_guild_id, try_voice};

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
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK"
)]
pub async fn text_to_voice(ctx: SContext<'_>, input: Option<String>) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let payload = if let Some(input) = input {
		input
	} else if let Ok(msg) = ctx
		.channel_id()
		.message(&ctx.http(), MessageId::new(ctx.id()))
		.await && let Some(reply) = msg.referenced_message.map(|r| r.content)
	{
		reply.into_string()
	} else {
		ctx.reply(MISSING_REPLY_MSG).await?;
		return Err(InteractionError::EmptyMessage.into());
	};
	let handler_lock = try_voice(ctx, guild_id).await?;
	let _typing = ctx.defer_or_broadcast().await;

	let bytes = match ai_voice(&payload).await {
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("I don't wanna speak now").await?;
			return Err(AIError::TTSFailed(err).into());
		}
	};

	get_configured_songbird_handler(&handler_lock)
		.await
		.enqueue_input(Input::from(bytes))
		.await;
	ctx.reply("Here we go").await?;

	Ok(())
}

/// Play all songs in a playlist from Deezer
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK"
)]
pub async fn add_deezer_playlist(
	ctx: SContext<'_>,
	#[description = "ID of the playlist in mind"]
	#[rest]
	playlist_id: String,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let handler_lock = try_voice(ctx, guild_id).await?;
	let request = match HTTP_CLIENT
		.get(format!("https://api.deezer.com/playlist/{playlist_id}"))
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(err) => {
			ctx.reply("Deezer bailed out :/").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let payload = match request.json::<DeezerResponse>().await {
		Ok(parsed) => parsed,
		Err(err) => {
			ctx.reply("Invalid playlist-id for Deezer playlist").await?;
			return Err(HTTPError::Request(err).into());
		}
	};

	let _typing = ctx.defer_or_broadcast().await;
	let reply = ctx.reply(QUEUE_MSG).await?;
	let msg = reply.message().await?;
	let mut failed_songs: u32 = 0;
	for track in payload.tracks.data {
		let search = format!("{} {}", track.title, track.artist.name);
		let mut src = YoutubeDl::new_search(HTTP_CLIENT.clone(), search);
		let audio = match src.create_async().await {
			Ok(audio) => audio,
			Err(err) => {
				warn!("Failed to fetch song: {err}");
				failed_songs = failed_songs.saturating_add(1);
				continue;
			}
		};
		let metadata = match src.aux_metadata().await {
			Ok(metadata) => metadata,
			Err(err) => {
				warn!("Missing metadata for song: {err}");
				failed_songs = failed_songs.saturating_add(1);
				continue;
			}
		};
		queue_song(
			metadata,
			audio,
			src,
			handler_lock.clone(),
			guild_id,
			ctx.data(),
			msg.id,
			msg.channel_id,
			ctx.author().display_name(),
		)
		.await;
	}
	if failed_songs != 0 {
		ctx.reply(format!(
			"Couldn't queue {failed_songs} because of YouTube :/"
		))
		.await?;
	}

	Ok(())
}

/// Play all songs in a playlist from ``YouTube``
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK"
)]
pub async fn add_youtube_playlist(
	ctx: SContext<'_>,
	#[description = "Url playlist in mind"]
	#[rest]
	playlist_url: String,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let handler_lock = try_voice(ctx, guild_id).await?;
	let _typing = ctx.defer_or_broadcast().await;
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

	let reply = ctx.reply(QUEUE_MSG).await?;
	let msg = reply.message().await?;

	let mut failed_songs: u32 = 0;

	for url in urls {
		let mut src = YoutubeDl::new(HTTP_CLIENT.clone(), url);
		let audio = match src.create_async().await {
			Ok(audio) => audio,
			Err(err) => {
				warn!("Failed to fetch song: {err}");
				failed_songs = failed_songs.saturating_add(1);
				continue;
			}
		};
		let metadata = match src.aux_metadata().await {
			Ok(metadata) => metadata,
			Err(err) => {
				warn!("Missing metadata for song: {err}");
				failed_songs = failed_songs.saturating_add(1);
				continue;
			}
		};

		queue_song(
			metadata,
			audio,
			src,
			handler_lock.clone(),
			guild_id,
			ctx.data(),
			msg.id,
			msg.channel_id,
			ctx.author().display_name(),
		)
		.await;
	}

	if failed_songs != 0 {
		ctx.reply(format!(
			"Couldn't queue {failed_songs} because of YouTube :/"
		))
		.await?;
	}

	Ok(())
}

/// Join the current voice channel
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK | \
	                            CONNECT"
)]
pub async fn join_voice(
	ctx: SContext<'_>,
	#[description = "Allow music playback across guilds"]
	#[flag]
	global: bool,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	if ctx.data().music_manager.get(guild_id).is_some() {
		ctx.reply("Bruh, I'm already in a voice channel! Use /leave_voice to drop the connection")
			.await?;
		return Ok(());
	}
	let _typing = ctx.defer_or_broadcast().await;
	let _handler_lock = try_voice(ctx, guild_id).await?;
	if global {
		query!(
			r#"
			INSERT INTO guild_settings (guild_id, global_call)
            VALUES ($1, TRUE)
            ON CONFLICT (guild_id)
            DO UPDATE SET global_call = TRUE, global_music = TRUE
            "#,
			i64::from(guild_id),
		)
		.execute(&mut *ctx.data().db.acquire().await?)
		.await?;
	}
	Ok(())
}

/// Leave the current voice channel
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | CONNECT"
)]
pub async fn leave_voice(ctx: SContext<'_>) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	remove_handler(ctx, guild_id).await?;

	ctx.reply("Left voice channel, don't forget me").await?;
	query!(
		r#"
		INSERT INTO guild_settings (guild_id, global_music, global_call)
        VALUES ($1, FALSE, FALSE)
        ON CONFLICT (guild_id)
        DO UPDATE SET global_music = FALSE, global_call = FALSE
        "#,
		i64::from(guild_id),
	)
	.execute(&mut *ctx.data().db.acquire().await?)
	.await?;

	Ok(())
}

/// Play song / add song to queue in the current voice channel
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK"
)]
pub async fn play_song(
	ctx: SContext<'_>,
	#[description = "YouTube link or query to search"]
	#[rest]
	url: String,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let handler_lock = try_voice(ctx, guild_id).await?;
	let typing = ctx.defer_or_broadcast().await;
	let Some(mut src) = youtube_source(url).await else {
		ctx.reply(INVALID_TRACK_SOURCE).await?;
		return Err(MusicError::UnknownSource.into());
	};
	let audio = match src.create_async().await {
		Ok(audio) => audio,
		Err(err) => {
			ctx.reply(FAILED_SONG_FETCH).await?;
			return Err(MusicError::FailedFetch(err).into());
		}
	};

	let metadata = match src.aux_metadata().await {
		Ok(metadata) => metadata,
		Err(err) => {
			ctx.reply(MISSING_METADATA_MSG).await?;
			return Err(MusicError::MissingMetadata(err).into());
		}
	};
	let reply = ctx.reply(QUEUE_MSG).await?;
	let msg = reply.message().await?;
	queue_song(
		metadata.clone(),
		audio,
		src.clone(),
		handler_lock.clone(),
		guild_id,
		ctx.data(),
		msg.id,
		msg.channel_id,
		ctx.author().display_name(),
	)
	.await;

	drop(typing);

	let is_global = query_scalar!(
		r#"
		SELECT global_music FROM guild_settings
		WHERE guild_id = $1
		"#,
		guild_id.get().cast_signed()
	)
	.fetch_one(&mut *ctx.data().db.acquire().await?)
	.await?;

	if is_global {
		let guild_global_playback = query_scalar!(
			r#"
		SELECT guild_id FROM guild_settings
		WHERE global_music IS TRUE
		AND guild_id != $1
		"#,
			guild_id.get().cast_signed()
		)
		.fetch_all(&mut *ctx.data().db.acquire().await?)
		.await?;

		for global_guild in guild_global_playback {
			let Some(global_handler_lock) = ctx
				.data()
				.music_manager
				.get(GuildId::new(global_guild.cast_unsigned()))
			else {
				continue;
			};
			if let Some(id) = get_configured_songbird_handler(&handler_lock)
				.await
				.current_channel()
				&& let Ok(channel) = ctx
					.http()
					.get_channel(GenericChannelId::new(id.get()))
					.await && let Some(guild_channel) = channel.guild()
				&& let Ok(global_audio) = src.create_async().await
			{
				let msg = guild_channel
					.send_message(ctx.http(), CreateMessage::default().content(QUEUE_MSG))
					.await?;
				queue_song(
					metadata.clone(),
					global_audio,
					src.clone(),
					global_handler_lock.clone(),
					guild_id,
					ctx.data(),
					msg.id,
					msg.channel_id,
					ctx.author().display_name(),
				)
				.await;
			}
		}
	}
	Ok(())
}

/// Seek current playing song backward
#[poise::command(
	prefix_command,
	slash_command,
	install_context = "Guild",
	interaction_context = "Guild",
	required_bot_permissions = "VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | SPEAK"
)]
pub async fn seek_song(
	ctx: SContext<'_>,
	#[description = "Seconds to seek, i.e. '-20' or '+20'"] seconds: String,
) -> Result<(), Error> {
	let guild_id = require_guild_id(ctx).await?;
	let handler_lock = try_voice(ctx, guild_id).await?;
	let _typing = ctx.defer_or_broadcast().await;
	let Some(current_playback) = get_configured_songbird_handler(&handler_lock)
		.await
		.queue()
		.current()
	else {
		ctx.reply(MISSING_METADATA_MSG).await?;
		return Err(MusicError::UnknownQueueTrack.into());
	};
	let current_playback_info = match current_playback.get_info().await {
		Ok(info) => info,
		Err(err) => {
			ctx.reply("No info about current song :/").await?;
			return Err(MusicError::MissingTrackData(err).into());
		}
	};
	let current_position = current_playback_info.position;
	let seconds_value = match seconds.parse::<i64>() {
		Ok(value) => value,
		Err(err) => {
			ctx.reply("Bruh, provide a valid number with a sign (e.g. '+20' or '-20')!")
				.await?;
			return Err(MusicError::InvalidSeek(err).into());
		}
	};
	let current_secs = current_position.as_secs().cast_signed();
	if seconds_value.is_negative() {
		let new_position = current_secs.saturating_add(seconds_value).cast_unsigned();
		let seek = Duration::from_secs(new_position);
		if seek.is_zero() {
			ctx.reply("Bruh, wanting to seek more seconds back than what have been played")
				.await?;
		} else if let Err(err) = current_playback.seek_async(seek).await {
			ctx.reply("Failed to seek song backwards").await?;
			return Err(MusicError::FailedSeek(err).into());
		} else {
			ctx.reply(format!("Seeked {}s backward", seconds_value.abs()))
				.await?;
		}
	} else {
		let seconds_to_add = seconds_value.cast_unsigned();
		let seek = current_position.saturating_add(Duration::from_secs(seconds_to_add));
		if let Err(err) = current_playback.seek_async(seek).await {
			ctx.reply("Bruh, you seeked more forward than the length of the song! I'm bailing out")
				.await?;
			return Err(MusicError::FailedSeek(err).into());
		}
		ctx.reply(format!("Seeked {seconds_value}s forward"))
			.await?;
	}

	Ok(())
}
