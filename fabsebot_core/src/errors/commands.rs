use anyhow::Error as AError;
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
	NotFound(#[source] AError),
}

#[derive(Error, Debug)]
pub enum InternalError {
	#[error("System time unavailable")]
	MissingSystemTime,
}

#[derive(Error, Debug)]
pub enum AIError {
	#[error("Servers busy")]
	ServersBusy,
}

#[derive(Error, Debug)]
pub enum MusicError {
	#[error("Failed to fetch YouTube-playlist")]
	FailedFetchPlaylist,
	#[error("Missing metadata")]
	MissingMetadata,
	#[error("Invalid seek")]
	InvalidSeek,
	#[error("Not in voice channel")]
	NotInVoiceChan,
	#[error("Unknown source")]
	UnknownSource,
}
