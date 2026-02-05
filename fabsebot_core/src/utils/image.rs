use std::{clone::Clone, io::Cursor, result::Result};

use ab_glyph::{FontArc, PxScale};
use anyhow::Result as AResult;
#[cfg(not(feature = "quote_webp"))]
use image::ImageFormat::Avif as STATIC_FORMAT;
#[cfg(feature = "quote_webp")]
use image::ImageFormat::WebP as STATIC_FORMAT;
use image::{
	AnimationDecoder as _, Frame, ImageBuffer, Rgba, RgbaImage,
	codecs::gif::{GifDecoder, GifEncoder, Repeat},
	imageops::{FilterType, overlay, resize},
	load_from_memory,
};
use imageproc::drawing::{draw_text_mut, text_size};
use rayon::prelude::*;
use textwrap::wrap;

use crate::config::constants::{DEFAULT_THEME, EMOJI_FONT, THEMES};

const QUOTE_WIDTH: u32 = 1200;
const QUOTE_HEIGHT: u32 = 630;
const CONTENT_BOUND: u32 = 64;
const MAX_CONTENT_WIDTH: u32 = QUOTE_WIDTH - QUOTE_HEIGHT - CONTENT_BOUND;
const MAX_CONTENT_HEIGHT: u32 = QUOTE_HEIGHT - CONTENT_BOUND;

const MIN_CONTENT_FONT_SIZE: f32 = 40.0;
const MAX_CONTENT_FONT_SIZE: f32 = 96.0;
const AUTHOR_FONT_SIZE: f32 = 32.0;
const MAX_LINES: usize = 16;
const ELLIPSIS: &str = "...";
const DEFAULT_WRAP_LENGTH: usize = 80;
const FONT_SIZE_DECREMENT: f32 = 2.0;
const WRAP_LENGTH_DECREMENT: usize = 5;
const LINE_SPACING: u32 = 10;

struct FontMetrics {
	line_height: u32,
	scale: PxScale,
}

impl FontMetrics {
	fn new(font: &FontArc, scale: PxScale) -> Self {
		let line_height = text_size(scale, font, "Tg").1;
		Self { line_height, scale }
	}

	fn recalculate(&mut self, font: &FontArc, new_scale: PxScale) {
		self.line_height = text_size(new_scale, font, "Tg").1;
		self.scale = new_scale;
	}
}

#[derive(Clone)]
pub struct TextLayout {
	content_lines: Vec<(String, i32, i32)>,
	content_lines_reverse: Vec<(String, i32, i32)>,
	author_position: (String, i32, i32, i32),
	content_scale: PxScale,
	author_scale: PxScale,
}

impl Default for TextLayout {
	fn default() -> Self {
		Self {
			content_lines: Vec::new(),
			content_lines_reverse: Vec::new(),
			author_position: (String::new(), 0, 0, 0),
			content_scale: PxScale::from(0.0),
			author_scale: PxScale::from(0.0),
		}
	}
}

#[must_use]
pub fn create_solid_theme(color: [u8; 4]) -> RgbaImage {
	RgbaImage::from_pixel(QUOTE_WIDTH, QUOTE_HEIGHT, Rgba(color))
}

fn truncate_text(text: &str, max_width: u32, metrics: &FontMetrics, font: &FontArc) -> String {
	let mut end = text.len() - ELLIPSIS.len();
	let mut truncated = String::with_capacity(end);

	loop {
		end = text.floor_char_boundary(end);

		truncated.clear();
		truncated.push_str(&text[..end]);
		truncated.push_str(ELLIPSIS);

		if text_size(metrics.scale, font, &truncated).0 <= max_width || end <= ELLIPSIS.len() {
			break;
		}

		end -= 1;
	}

	truncated
}

fn apply_gradient_to_avatar(avatar: &mut RgbaImage, is_reverse: bool) {
	let width = avatar.width();
	let height = avatar.height();
	let gradient_width = width / 2;
	let gradient_start = if is_reverse {
		0
	} else {
		width - gradient_width
	};

	let alpha_lut: Vec<u8> = (0..gradient_width)
		.map(|x| {
			let progress = x * 255 / gradient_width;
			if is_reverse {
				(progress.pow(2) / 255) as u8
			} else {
				((255 - progress).pow(2) / 255) as u8
			}
		})
		.collect();

	for y in 0..height {
		for x in 0..gradient_width {
			let pixel = avatar.get_pixel_mut(gradient_start + x, y);
			pixel[3] = ((u32::from(pixel[3]) * u32::from(alpha_lut[x as usize])) / 255) as u8;
		}
	}
}

fn prepare_text_layout(
	quoted_content: &str,
	author_name: &str,
	content_font: &FontArc,
	author_font: &FontArc,
	text_layout: &mut TextLayout,
) {
	let mut content_metrics = FontMetrics::new(content_font, PxScale::from(MAX_CONTENT_FONT_SIZE));
	let author_metrics = FontMetrics::new(author_font, PxScale::from(AUTHOR_FONT_SIZE));

	let mut wrapped_length = DEFAULT_WRAP_LENGTH;
	let mut final_lines = Vec::with_capacity(MAX_LINES);

	loop {
		let wrapped_lines = wrap(quoted_content, wrapped_length);

		if let Some(first_line) = wrapped_lines.first()
			&& text_size(content_metrics.scale, content_font, first_line).0 > MAX_CONTENT_WIDTH
		{
			if content_metrics.scale.x == MIN_CONTENT_FONT_SIZE {
				wrapped_length -= WRAP_LENGTH_DECREMENT;
				if wrapped_length < 20 {
					break;
				}
			} else {
				let new_size =
					(content_metrics.scale.x - FONT_SIZE_DECREMENT).max(MIN_CONTENT_FONT_SIZE);
				content_metrics.recalculate(content_font, PxScale::from(new_size));
			}
			continue;
		}

		final_lines.clear();
		let max_possible_lines = {
			let height_per_line = content_metrics.line_height + LINE_SPACING;
			((MAX_CONTENT_HEIGHT + LINE_SPACING) / height_per_line).min(MAX_LINES as u32) as usize
		};

		for (i, line) in wrapped_lines.iter().take(max_possible_lines).enumerate() {
			let is_last_line = i == max_possible_lines - 1;
			let needs_truncation = text_size(content_metrics.scale, content_font, line).0
				> MAX_CONTENT_WIDTH
				|| (is_last_line && wrapped_lines.len() > max_possible_lines);
			let line_str = if needs_truncation && line.len() > ELLIPSIS.len() {
				truncate_text(line, MAX_CONTENT_WIDTH, &content_metrics, content_font)
			} else {
				line.to_string()
			};

			final_lines.push(line_str);
		}

		if !final_lines.is_empty() && content_metrics.scale.x >= MIN_CONTENT_FONT_SIZE {
			break;
		}

		if content_metrics.scale.x == MIN_CONTENT_FONT_SIZE {
			wrapped_length -= WRAP_LENGTH_DECREMENT;
			if wrapped_length < 20 {
				break;
			}
		} else {
			let new_size =
				(content_metrics.scale.x - FONT_SIZE_DECREMENT).max(MIN_CONTENT_FONT_SIZE);
			content_metrics.recalculate(content_font, PxScale::from(new_size));
		}
	}

	let lines_count = final_lines.len() as u32;
	let total_text_height = (lines_count * content_metrics.line_height)
		+ (lines_count.saturating_sub(1) * LINE_SPACING);

	let quoted_content_y = (QUOTE_HEIGHT - total_text_height) / 2;

	let mut current_y = quoted_content_y.cast_signed();

	let mut line_positions = Vec::with_capacity(final_lines.len());
	let mut line_positions_reverse = Vec::with_capacity(final_lines.len());

	for line in final_lines {
		let line_width = text_size(content_metrics.scale, content_font, &line).0;
		let centered_offset = (QUOTE_WIDTH - QUOTE_HEIGHT - line_width) / 2;

		let line_x = (QUOTE_HEIGHT + centered_offset).cast_signed();
		let line_x_reverse = centered_offset.cast_signed();

		line_positions.push((line.clone(), line_x, current_y));
		line_positions_reverse.push((line, line_x_reverse, current_y));

		current_y += (content_metrics.line_height + LINE_SPACING).cast_signed();
	}

	let author_name_width = text_size(author_metrics.scale, author_font, author_name).0;
	let author_x = ((QUOTE_WIDTH - author_name_width) / 2 + QUOTE_HEIGHT / 2).cast_signed();
	let author_x_reverse = ((QUOTE_WIDTH - QUOTE_HEIGHT - author_name_width) / 2).cast_signed();

	let author_y_offset = if line_positions.len() == 1 {
		LINE_SPACING * 3
	} else {
		LINE_SPACING
	};
	let author_y = (quoted_content_y + total_text_height + author_y_offset).cast_signed();

	text_layout.content_lines = line_positions;
	text_layout.content_lines_reverse = line_positions_reverse;
	text_layout.author_position = (author_name.to_owned(), author_x, author_x_reverse, author_y);
	text_layout.content_scale = content_metrics.scale;
	text_layout.author_scale = author_metrics.scale;
}

fn apply_text_layout(
	img: &mut RgbaImage,
	layout: &TextLayout,
	text_colour: Rgba<u8>,
	content_font: &FontArc,
	author_font: &FontArc,
	is_reverse: bool,
) {
	let content_lines = if is_reverse {
		&layout.content_lines_reverse
	} else {
		&layout.content_lines
	};
	for (line, x, y) in content_lines {
		let mut step = *x;
		for c in line.chars().map(|c| c.to_string()) {
			let font = if emojis::get(&c).is_some() {
				&EMOJI_FONT
			} else {
				content_font
			};

			draw_text_mut(img, text_colour, step, *y, layout.content_scale, font, &c);

			step += text_size(layout.content_scale, font, &c).0.cast_signed();
		}
	}

	let author_x = if is_reverse {
		layout.author_position.2
	} else {
		layout.author_position.1
	};

	draw_text_mut(
		img,
		text_colour,
		author_x,
		layout.author_position.3,
		layout.author_scale,
		author_font,
		&layout.author_position.0,
	);
}

pub fn resize_avatar(avatar_bytes: &[u8]) -> AResult<ImageBuffer<Rgba<u8>, Vec<u8>>> {
	Ok(resize(
		&load_from_memory(avatar_bytes)?.to_rgba8(),
		QUOTE_HEIGHT,
		QUOTE_HEIGHT,
		FilterType::Triangle,
	))
}

#[must_use]
pub const fn avatar_position(is_reverse: bool) -> i64 {
	if is_reverse {
		i64::from(QUOTE_WIDTH - QUOTE_HEIGHT)
	} else {
		0
	}
}

pub fn get_theme(theme: &str) -> (ImageBuffer<Rgba<u8>, Vec<u8>>, Rgba<u8>) {
	match theme {
		"random" => {
			let random_base =
				create_solid_theme([fastrand::u8(..), fastrand::u8(..), fastrand::u8(..), 255]);
			let random_color = Rgba([fastrand::u8(..), fastrand::u8(..), fastrand::u8(..), 255]);
			(random_base, random_color)
		}
		_ => THEMES
			.get(theme)
			.map_or_else(|| THEMES.get(DEFAULT_THEME).unwrap().clone(), Clone::clone),
	}
}

pub fn convert_to_bw(image: &mut RgbaImage) {
	let pixels = image.as_flat_samples_mut();
	pixels.samples.par_chunks_exact_mut(4).for_each(|chunk| {
		let gray = ((u32::from(chunk[0]) * 77
			+ u32::from(chunk[1]) * 150
			+ u32::from(chunk[2]) * 29)
			>> 8) as u8;
		chunk[0] = gray;
		chunk[1] = gray;
		chunk[2] = gray;
	});
}

pub fn quote_static_image(
	mut avatar_image: ImageBuffer<Rgba<u8>, Vec<u8>>,
	author_name: &str,
	quoted_content: &str,
	author_font: &FontArc,
	content_font: &FontArc,
	text_colour: Rgba<u8>,
	mut img: ImageBuffer<Rgba<u8>, Vec<u8>>,
	text_layout: &mut TextLayout,
	avatar_position: i64,
	is_colour: bool,
	is_gradient: bool,
	is_reverse: bool,
	new_font: bool,
	cursor: &mut Cursor<Vec<u8>>,
) -> AResult<()> {
	if new_font {
		prepare_text_layout(
			quoted_content,
			author_name,
			content_font,
			author_font,
			text_layout,
		);
	}
	if !is_colour {
		convert_to_bw(&mut avatar_image);
	}
	if is_gradient {
		apply_gradient_to_avatar(&mut avatar_image, is_reverse);
	}

	overlay(&mut img, &avatar_image, avatar_position, 0);

	apply_text_layout(
		&mut img,
		text_layout,
		text_colour,
		content_font,
		author_font,
		is_reverse,
	);

	Ok(img.write_to(cursor, STATIC_FORMAT)?)
}

pub fn quote_animated_image(
	author_name: &str,
	quoted_content: &str,
	author_font: &FontArc,
	content_font: &FontArc,
	text_colour: Rgba<u8>,
	mut img: ImageBuffer<Rgba<u8>, Vec<u8>>,
	text_layout: &mut TextLayout,
	avatar_position: i64,
	is_colour: bool,
	is_gradient: bool,
	is_reverse: bool,
	new_font: bool,
	cursor: &mut Cursor<Vec<u8>>,
	output: &mut Vec<u8>,
) -> AResult<()> {
	if new_font {
		prepare_text_layout(
			quoted_content,
			author_name,
			content_font,
			author_font,
			text_layout,
		);
	}

	apply_text_layout(
		&mut img,
		text_layout,
		text_colour,
		content_font,
		author_font,
		is_reverse,
	);

	let frames: Vec<_> = GifDecoder::new(cursor)?
		.into_frames()
		.take(30)
		.filter_map(Result::ok)
		.collect();

	let processed_frames: Vec<_> = frames
		.par_iter()
		.map(|frame| {
			let mut avatar_frame = resize(
				frame.buffer(),
				QUOTE_HEIGHT,
				QUOTE_HEIGHT,
				FilterType::Nearest,
			);
			if !is_colour {
				convert_to_bw(&mut avatar_frame);
			}
			if is_gradient {
				apply_gradient_to_avatar(&mut avatar_frame, is_reverse);
			}

			let mut quote_frame = img.clone();
			overlay(&mut quote_frame, &avatar_frame, avatar_position, 0);
			Frame::from_parts(quote_frame, 0, 0, frame.delay())
		})
		.collect();

	let mut encoder = GifEncoder::new_with_speed(output, 10);
	encoder.set_repeat(Repeat::Infinite)?;
	encoder.encode_frames(processed_frames)?;

	Ok(())
}
