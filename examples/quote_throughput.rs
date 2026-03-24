use std::{hint::black_box, io::Cursor, time::Instant};

use fabsebot_core::{
	config::constants::{AUTHOR_FONT, CONTENT_FONT, DEFAULT_THEME, FONTS, STATIC_QUOTE_VEC},
	utils::image::{
		QuoteImageConfig, TextLayout, avatar_position, get_theme, quote_static_image, resize_avatar,
	},
};
use fastrand::bool;

const TOTAL: usize = 100_000;

fn main() {
	let content_font = FONTS.get(CONTENT_FONT).unwrap();
	let author_font = FONTS.get(AUTHOR_FONT).unwrap();
	let avatar_image = include_bytes!("../bench_assets/avatar.webp");
	let avatar_resized = resize_avatar(avatar_image).unwrap();
	let (img, text_colour) = get_theme(DEFAULT_THEME);
	let avatar_pos = avatar_position(false);
	let content = "bruh bre broo ".repeat(20);

	println!("Generating {TOTAL} quotes...");

	let start = Instant::now();

	for i in 0..TOTAL {
		let mut layout = TextLayout::default();
		let mut cursor = Cursor::new(Vec::with_capacity(STATIC_QUOTE_VEC));

		let _ = black_box(quote_static_image(
			black_box(avatar_resized.clone()),
			black_box("Author"),
			black_box(&content),
			black_box(author_font),
			black_box(content_font),
			black_box(text_colour),
			black_box(img.clone()),
			black_box(&mut layout),
			black_box(avatar_pos),
			black_box(QuoteImageConfig {
				bw: bool(),
				gradient: bool(),
				new_font: true,
				reverse: bool(),
			}),
			black_box(&mut cursor),
		));

		if (i + 1) % 10_000 == 0 {
			println!("{} / {TOTAL}", i + 1);
		}
	}

	let duration = start.elapsed();
	println!(
		"Finished {TOTAL} quotes in {:.2}s ({:.2} min)",
		duration.as_secs_f64(),
		duration.as_secs_f64() / 60.0
	);
}
