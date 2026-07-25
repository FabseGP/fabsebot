use std::{collections::HashMap, sync::LazyLock};

use ab_glyph::FontArc;
use image::{Rgba, RgbaImage};

use crate::utils::image::create_solid_theme;

pub const MESSAGE_LIMIT: usize = 4000;
pub const CONTENT_LIMIT: usize = 2000;

pub const DEFAULT_PREFIX: &str = "!";

pub const MISSING_REPLY_MSG: &str = "Bruh, reply to a message";
pub const EMPTY_REPLY_MSG: &str = "Bruh, this message is empty";
pub const EMPTY_VOICE_CHAN_MSG: &str = "No voice channel with at least 1 user found :/";
pub const QUEUEING_MSG: &str = "Adding song to queue";
pub const FAILED_SONG_FETCH: &str = "Failed to fetch song from YouTube :/";

pub const CONTENT_FONT: &str = "NotoSansJP-Regular";
pub const AUTHOR_FONT: &str = "Satoshi-LightItalic";

#[expect(clippy::large_include_file)]
pub static FONTS: LazyLock<HashMap<&'static str, FontArc>> = LazyLock::new(|| {
	HashMap::from([
		(
			CONTENT_FONT,
			FontArc::try_from_slice(include_bytes!("../../../fonts/NotoSansJP-Regular.ttf"))
				.unwrap(),
		),
		(
			AUTHOR_FONT,
			FontArc::try_from_slice(include_bytes!("../../../fonts/Satoshi-LightItalic.otf"))
				.unwrap(),
		),
		(
			"RampartOne-Regular",
			FontArc::try_from_slice(include_bytes!("../../../fonts/RampartOne-Regular.ttf"))
				.unwrap(),
		),
	])
});

pub const STATIC_QUOTE_VEC: usize = 131_072;
pub const ANIMATED_QUOTE_VEC: usize = 1_048_576;

pub const DEFAULT_THEME: &str = "dark";

pub static THEMES: LazyLock<HashMap<&'static str, (RgbaImage, Rgba<u8>)>> = LazyLock::new(|| {
	HashMap::from([
		(
			DEFAULT_THEME,
			(
				create_solid_theme([0, 0, 0, 255]),
				Rgba([255, 255, 255, 255]),
			),
		),
		(
			"light",
			(
				create_solid_theme([255, 255, 255, 255]),
				Rgba([0, 0, 0, 255]),
			),
		),
	])
});
