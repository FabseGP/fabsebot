use std::{hint::black_box, io::Cursor, time::Duration};

use ab_glyph::FontArc;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use fabsebot_core::{
	config::constants::{AUTHOR_FONT, CONTENT_FONT, DEFAULT_THEME, FONTS, STATIC_QUOTE_VEC},
	utils::image::{TextLayout, avatar_position, get_theme, quote_static_image, resize_avatar},
};

fn load_assets() -> (FontArc, FontArc, [u8; 8640]) {
	let content_font = FONTS.get(CONTENT_FONT).unwrap();
	let author_font = FONTS.get(AUTHOR_FONT).unwrap();
	let avatar = include_bytes!("../../bench_assets/avatar.webp");
	(content_font.clone(), author_font.clone(), *avatar)
}

fn benchmark_baseline(c: &mut Criterion) {
	let (content_font, author_font, avatar_image) = load_assets();
	let content = "bruh bre broo ".repeat(50);
	let avatar_resized = resize_avatar(&avatar_image).unwrap();
	let (img, text_colour) = get_theme(DEFAULT_THEME);
	let text_layout = TextLayout::default();
	let avatar_position = avatar_position(false);

	c.bench_function("baseline", |b| {
		b.iter_batched(
			|| {
				(
					avatar_resized.clone(),
					img.clone(),
					text_layout.clone(),
					Cursor::new(Vec::with_capacity(STATIC_QUOTE_VEC)),
					fastrand::bool(),
					fastrand::bool(),
					fastrand::bool(),
				)
			},
			|(avatar, img, mut layout, mut cursor, is_colour, is_gradient, is_reverse)| {
				quote_static_image(
					black_box(avatar),
					black_box("Author"),
					black_box(&content),
					black_box(&author_font),
					black_box(&content_font),
					black_box(text_colour),
					black_box(img),
					black_box(&mut layout),
					black_box(avatar_position),
					black_box(is_colour),
					black_box(is_gradient),
					black_box(is_reverse),
					black_box(true),
					black_box(&mut cursor),
				)
			},
			BatchSize::SmallInput,
		);
	});
}

fn criterion_config() -> Criterion {
	Criterion::default()
		.sample_size(100)
		.measurement_time(Duration::from_secs(120))
}

criterion_group! {
	name = benches;
	config = criterion_config();
	targets = benchmark_baseline
}
criterion_main!(benches);
