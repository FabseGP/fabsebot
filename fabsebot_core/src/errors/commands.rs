use anyhow::Error as AError;
use base64::DecodeError as DError;
use reqwest::Error as RError;
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
	#[error("Empty message")]
	EmptyMessage,
	#[error("Missing reply")]
	MissingReply,
}

#[derive(Error, Debug)]
pub enum HTTPError {
	#[error("Request failed: {0}")]
	Request(#[source] RError),
	#[error("Parsing failed: {0}")]
	Parsing(#[source] RError),
}

#[derive(Error, Debug)]
pub enum Base64Error {
	#[error("Failed to decode to String: {0}")]
	FailedBytesDecode(#[source] DError),
}

#[derive(Error, Debug)]
pub enum AIError {
	#[error("TTS failed: {0}")]
	TTSFailed(#[source] RError),
	#[error("Unexpected response: {0}")]
	UnexpectedResponse(#[source] AError),
}
