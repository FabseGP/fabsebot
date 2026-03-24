use std::{io::Error as IOError, num::ParseIntError};

use anyhow::Error as AError;
use base64::DecodeError as DError;
use reqwest::Error as RError;
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
pub enum HTTPError {
	#[error("Request failed: {0}")]
	Request(#[source] RError),
}

#[derive(Error, Debug)]
pub enum Base64Error {
	#[error("Failed to decode to String: {0}")]
	FailedBytesDecode(#[source] DError),
}

#[derive(Error, Debug)]
pub enum AIError {
	#[error("TTS failed: {0}")]
	TTSFailed(#[source] AError),
	#[error("Unexpected response: {0}")]
	UnexpectedResponse(#[source] AError),
}

#[derive(Error, Debug)]
pub enum MusicError {
	#[error("Failed to fetch YouTube-playlist: {0}")]
	FailedFetchPlaylist(#[source] IOError),
	#[error("Missing metadata: {0}")]
	MissingMetadata(#[source] AudioStreamError),
	#[error("Missing track data: {0}")]
	MissingTrackData(#[source] ControlError),
	#[error("Invalid seek: {0}")]
	InvalidSeek(#[source] ParseIntError),
	#[error("Failed seek: {0}")]
	FailedSeek(#[source] ControlError),
	#[error("Not in voice channel")]
	NotInVoiceChan,
	#[error("Unknown source")]
	UnknownSource,
	#[error("Failed to fetch song: {0}")]
	FailedFetch(#[source] AudioStreamError),
	#[error("Unknown track in queue")]
	UnknownQueueTrack,
}
