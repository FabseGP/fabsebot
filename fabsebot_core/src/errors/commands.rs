use std::{io::Error as IOError, num::ParseIntError, time::SystemTimeError};

use anyhow::Error as AError;
use base64::DecodeError as DError;
use reqwest::Error as RError;
use serenity::Error as SError;
use songbird::{error::ControlError, input::AudioStreamError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GuildError {
	#[error("Command executed not in a guild")]
	NotInGuild,
	#[error("Failed to fetch guild data")]
	FailedFetch,
}

#[derive(Error, Debug)]
pub enum InteractionError {
	#[error("Author choose a bot")]
	NotHuman,
	#[error("Less than one option given")]
	MissingOptions,
	#[error("Empty message")]
	EmptyMessage,
	#[error("Missing reply")]
	MissingReply,
}

#[derive(Error, Debug)]
pub enum WebhookError {
	#[error("Couldn't find a suitable webhook")]
	NotFound(#[from] AError),
}

#[derive(Error, Debug)]
pub enum InternalError {
	#[error("System time unavailable")]
	MissingSystemTime(#[source] SystemTimeError),
}

#[derive(Error, Debug)]
pub enum EmojiError {
	#[error("Failed to fetch app emojis")]
	FailedFetch(#[from] SError),
	#[error("Couldn't find emoji")]
	MissingEmoji,
}

#[derive(Error, Debug)]
pub enum HTTPError {
	#[error("Request failed")]
	Request(#[from] RError),
}

#[derive(Error, Debug)]
pub enum Base64Error {
	#[error("Failed to decode to String")]
	FailedBytesDecode(#[source] DError),
}

#[derive(Error, Debug)]
pub enum AIError {
	#[error("TTS failed")]
	TTSFailed(#[source] AError),
	#[error("Unexpected response")]
	UnexpectedResponse(#[source] AError),
}

#[derive(Error, Debug)]
pub enum MusicError {
	#[error("Failed to fetch YouTube-playlist")]
	FailedFetchPlaylist(#[source] IOError),
	#[error("Missing metadata")]
	MissingMetadata(#[source] AudioStreamError),
	#[error("Missing track data")]
	MissingTrackData(#[source] ControlError),
	#[error("Invalid seek")]
	InvalidSeek(#[source] ParseIntError),
	#[error("Failed seek")]
	FailedSeek(#[source] ControlError),
	#[error("Not in voice channel")]
	NotInVoiceChan,
	#[error("Unknown source")]
	UnknownSource,
	#[error("Failed to fetch song")]
	FailedFetch(#[source] AudioStreamError),
	#[error("Unknown track in queue")]
	UnknownQueueTrack,
}
