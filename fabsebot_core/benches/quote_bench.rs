use std::{hint::black_box, io::Cursor, time::Duration};

use ab_glyph::FontArc;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use fabsebot_core::{
	config::constants::{
		ANIMATED_QUOTE_VEC, AUTHOR_FONT, CONTENT_FONT, DEFAULT_THEME, FONTS, STATIC_QUOTE_VEC,
	},
	utils::image::{
		TextLayout, avatar_position, get_theme, quote_animated_image, quote_static_image,
		resize_avatar,
	},
};

#[derive(PartialEq)]
enum AssetType {
	Animated,
	Static,
}

fn load_assets(asset_type: AssetType) -> (FontArc, FontArc, &'static [u8]) {
	let content_font = FONTS.get(CONTENT_FONT).unwrap();
	let author_font = FONTS.get(AUTHOR_FONT).unwrap();
	let avatar: &[u8] = if asset_type == AssetType::Animated {
		include_bytes!("../../bench_assets/flopa.gif")
	} else {
		include_bytes!("../../bench_assets/avatar.webp")
	};
	(content_font.clone(), author_font.clone(), avatar)
}

fn benchmark_static(c: &mut Criterion) {
	let (content_font, author_font, avatar_image) = load_assets(AssetType::Static);
	let content = "bruh bre broo ".repeat(50);
	let avatar_resized = resize_avatar(&avatar_image).unwrap();
	let (img, text_colour) = get_theme(DEFAULT_THEME);
	let text_layout = TextLayout::default();
	let avatar_position = avatar_position(false);

	c.bench_function("baseline_static", |b| {
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

fn benchmark_animated(c: &mut Criterion) {
	let (content_font, author_font, avatar_image) = load_assets(AssetType::Animated);
	let avatar_bytes = avatar_image.to_vec();
	let content = "bruh bre broo ".repeat(50);
	let (img, text_colour) = get_theme(DEFAULT_THEME);
	let text_layout = TextLayout::default();
	let avatar_position = avatar_position(false);

	c.bench_function("baseline_animated", |b| {
		b.iter_batched(
			|| {
				(
					img.clone(),
					text_layout.clone(),
					Cursor::new(avatar_bytes.clone()),
					fastrand::bool(),
					fastrand::bool(),
					fastrand::bool(),
					Vec::with_capacity(ANIMATED_QUOTE_VEC),
				)
			},
			|(img, mut layout, mut cursor, is_colour, is_gradient, is_reverse, mut buffer)| {
				quote_animated_image(
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
					black_box(&mut buffer),
				)
			},
			BatchSize::SmallInput,
		);
	});
}

fn criterion_config_static() -> Criterion {
	Criterion::default()
		.sample_size(100)
		.measurement_time(Duration::from_secs(120))
}

fn criterion_config_animated() -> Criterion {
	Criterion::default()
		.sample_size(100)
		.measurement_time(Duration::from_secs(240))
}

criterion_group! {
	name = static_benches;
	config = criterion_config_static();
	targets = benchmark_static
}

criterion_group! {
	name = animated_benches;
	config = criterion_config_animated();
	targets = benchmark_animated
}

criterion_main!(static_benches, animated_benches);
