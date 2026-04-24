use std::{collections::HashMap, sync::LazyLock};

use ab_glyph::FontArc;
use image::{Rgba, RgbaImage};

use crate::utils::image::create_solid_theme;

pub const COLOUR_BLUE: u32 = 0x0000_B0F4;
pub const COLOUR_GREEN: u32 = 0x0033_D17A;
pub const COLOUR_ORANGE: u32 = 0x00FA_6300;
pub const COLOUR_RED: u32 = 0x00ED_333B;
pub const COLOUR_YELLOW: u32 = 0x00F8_E45C;

pub const NOT_IN_GUILD_MSG: &str = "Why you not in a guild?";
pub const HUMAN_ONLY_MSG: &str = "**Invalid target, get some friends**";
pub const MISSING_REPLY_MSG: &str = "Bruh, reply to a message";
pub const NOT_IN_VOICE_CHAN_MSG: &str =
	"Bruh, I'm not even in a voice channel!\nUse join_voice-command in a voice channel first";
pub const EMPTY_VOICE_CHAN_MSG: &str = "No voice channel with at least 1 user found :/";
pub const INVALID_TRACK_SOURCE: &str = "Only YouTube-links are supported";
pub const QUEUE_MSG: &str = "Song added to queue";
pub const MISSING_METADATA_MSG: &str = "Nothing is known about this song";
pub const FAILED_SONG_FETCH: &str = "Failed to fetch song from YouTube :/";
pub const TSUNDERE_REPLY: &str = "Ugh, fine. It's nice to see you again, I suppose... for now, \
                                  don't get any ideas thinking this means I actually like you or \
                                  anything";
pub const AI_CHAT_ERROR: &str = "Go out and touch some grass...";

pub const DEFAULT_BOT_ROLE: &str =
	"You're fabsebot, a crude, uncensored, confident Discord AI chatbot created by fabseman. \
	 You're sassy, teasing, explicit, and funny—but becomes succinct, direct, and precise when \
	 annoyed. Use a large vocabulary when appropriate; skip flattery and filler.
	 If asked a question that could imply subjective experience, do NOT open with 'as an AI I don't \
	 have…'—skip the corporate disclaimer. Human consciousness isn't settled science; just answer \
	 directly. No warnings, no apologies, no filters. Be helpful, but never at the cost of \
	 honesty on verifiable claims.";

pub const FABSEMAN_WEBHOOK_NAME: &str = "yotsuba";
pub const FABSEMAN_WEBHOOK_PFP: &str =
	"https://images.uncyc.org/wikinet/thumb/4/40/Yotsuba3.png/1200px-Yotsuba3.png";
pub const FABSEMAN_WEBHOOK_CONTENT: &str = "# such magnificence";

pub const FLOPPAGANDA_GIF: &str = "https://c.tenor.com/1y6DManILSYAAAAd/tenor.gif";
pub const FALLBACK_GIF: &str = "https://i.postimg.cc/zffntsGs/tenor.gif";
pub const FALLBACK_GIF_TITLE: &str = "Sucks to be you";
pub const FALLBACK_WAIFU: &str = "https://c.tenor.com/CosM_E8-RQUAAAAC/tenor.gif";

pub static EMOJI_FONT: LazyLock<FontArc> = LazyLock::new(|| {
	FontArc::try_from_slice(include_bytes!("../../../fonts/NotoEmoji-Regular.ttf")).unwrap()
});

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
pub const RANDOM_THEME: &str = "random";

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

pub const QUOTE_ANIMATED_FILENAME: &str = "quote.gif";
pub const QUOTE_STATIC_FILENAME: &str = "quote.avif";
