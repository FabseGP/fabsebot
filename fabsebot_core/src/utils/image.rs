use std::io::Cursor;

use ab_glyph::{FontArc, PxScale};
use anyhow::{Context as _, Result as AResult, bail};
use image::{
	AnimationDecoder as _, Frame, GenericImage as _, ImageBuffer,
	ImageFormat::Avif,
	Rgba, RgbaImage,
	codecs::gif::{GifDecoder, GifEncoder, Repeat::Infinite},
	imageops::{FilterType, overlay, resize},
	load_from_memory,
};
use imageproc::drawing::{draw_text_mut, text_size};
use textwrap::wrap;

use crate::config::constants::{
	DARK_BASE_IMAGE, LIGHT_BASE_IMAGE, QUOTE_HEIGHT, QUOTE_WIDTH, RAINBOW_BASE_THEME,
};

const MIN_CONTENT_FONT_SIZE: f32 = 40.0;
const MAX_CONTENT_FONT_SIZE: f32 = 96.0;
const AUTHOR_FONT_SIZE: f32 = 32.0;
const MAX_LINES: usize = 16;
const ELLIPSIS: &str = "...";
const DEFAULT_WRAP_LENGTH: usize = 80;
const FONT_SIZE_DECREMENT: f32 = 2.0;
const WRAP_LENGTH_DECREMENT: usize = 5;
const LINE_SPACING: u32 = 10;

const R_WEIGHT: u32 = 77;
const G_WEIGHT: u32 = 150;
const B_WEIGHT: u32 = 29;

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

fn truncate_text(text: &str, max_width: u32, metrics: &FontMetrics, font: &FontArc) -> String {
	let mut end = text.len().saturating_sub(ELLIPSIS.len());
	let mut truncated = String::with_capacity(text.len().saturating_sub(ELLIPSIS.len()));

	loop {
		while end > 0 && !text.is_char_boundary(end) {
			end = end.saturating_sub(1);
		}

		truncated.clear();
		truncated.push_str(&text[..end]);
		truncated.push_str(ELLIPSIS);

		if text_size(metrics.scale, font, &truncated).0 <= max_width || end <= ELLIPSIS.len() {
			break;
		}

		end = end.saturating_sub(1);
	}

	truncated
}

fn apply_gradient_to_avatar(avatar: &mut RgbaImage, is_reverse: bool) -> AResult<()> {
	let gradient_width = avatar.width().saturating_div(2);
	let gradient_start = if is_reverse {
		0
	} else {
		avatar.width().saturating_sub(gradient_width)
	};

	for x in 0..gradient_width {
		let progress = x.saturating_mul(255).saturating_div(gradient_width);
		let alpha_multiplier = if is_reverse {
			u8::try_from(progress.saturating_pow(2).saturating_div(255))?
		} else {
			u8::try_from(
				255_u32
					.saturating_sub(progress)
					.saturating_pow(2)
					.saturating_div(255),
			)?
		};

		for y in 0..avatar.height() {
			let pixel = avatar.get_pixel_mut(gradient_start.saturating_add(x), y);
			pixel[3] = u8::try_from(
				u32::from(pixel[3])
					.saturating_mul(u32::from(alpha_multiplier))
					.saturating_div(255),
			)?;
		}
	}

	Ok(())
}

fn get_base_image(theme: Option<&str>, is_light: bool) -> (RgbaImage, Rgba<u8>) {
	theme.map_or_else(
		|| {
			if is_light {
				(
					LIGHT_BASE_IMAGE
						.get_or_init(|| {
							RgbaImage::from_pixel(
								QUOTE_WIDTH,
								QUOTE_HEIGHT,
								Rgba([255, 255, 255, 255]),
							)
						})
						.clone(),
					Rgba([0, 0, 0, 255]),
				)
			} else {
				(
					DARK_BASE_IMAGE
						.get_or_init(|| {
							RgbaImage::from_pixel(QUOTE_WIDTH, QUOTE_HEIGHT, Rgba([0, 0, 0, 255]))
						})
						.clone(),
					Rgba([255, 255, 255, 255]),
				)
			}
		},
		|theme| {
			if theme == "rainbow" {
				(
					RAINBOW_BASE_THEME
						.get_or_init(|| {
							RgbaImage::from_pixel(
								QUOTE_WIDTH,
								QUOTE_HEIGHT,
								Rgba([255, 0, 128, 128]),
							)
						})
						.clone(),
					Rgba([255, 255, 255, 255]),
				)
			} else if theme == "dark" {
				(
					DARK_BASE_IMAGE
						.get_or_init(|| {
							RgbaImage::from_pixel(QUOTE_WIDTH, QUOTE_HEIGHT, Rgba([0, 0, 0, 255]))
						})
						.clone(),
					Rgba([255, 255, 255, 255]),
				)
			} else if theme == "light" {
				(
					LIGHT_BASE_IMAGE
						.get_or_init(|| {
							RgbaImage::from_pixel(
								QUOTE_WIDTH,
								QUOTE_HEIGHT,
								Rgba([255, 255, 255, 255]),
							)
						})
						.clone(),
					Rgba([0, 0, 0, 255]),
				)
			} else {
				(
					DARK_BASE_IMAGE
						.get_or_init(|| {
							RgbaImage::from_pixel(QUOTE_WIDTH, QUOTE_HEIGHT, Rgba([0, 0, 0, 255]))
						})
						.clone(),
					Rgba([255, 255, 255, 255]),
				)
			}
		},
	)
}

fn prepare_text_layout(
	quoted_content: &str,
	author_name: &str,
	content_font: &FontArc,
	author_font: &FontArc,
) -> AResult<TextLayout> {
	let max_content_width = QUOTE_WIDTH.saturating_sub(QUOTE_HEIGHT).saturating_sub(64);
	let max_content_height = QUOTE_HEIGHT.saturating_sub(64);

	let mut content_metrics = FontMetrics::new(content_font, PxScale::from(MAX_CONTENT_FONT_SIZE));
	let author_metrics = FontMetrics::new(author_font, PxScale::from(AUTHOR_FONT_SIZE));

	let mut wrapped_length = DEFAULT_WRAP_LENGTH;
	let mut final_lines = Vec::with_capacity(MAX_LINES);
	let (mut line_positions, mut line_positions_reverse) =
		(Vec::with_capacity(MAX_LINES), Vec::with_capacity(MAX_LINES));

	loop {
		let wrapped_lines = wrap(quoted_content, wrapped_length);

		if let Some(first_line) = wrapped_lines.first()
			&& text_size(content_metrics.scale, content_font, first_line).0 > max_content_width
		{
			if content_metrics.scale.x == MIN_CONTENT_FONT_SIZE {
				wrapped_length = wrapped_length.saturating_sub(WRAP_LENGTH_DECREMENT);
				if wrapped_length < 20 {
					break;
				}
			} else {
				content_metrics.recalculate(
					content_font,
					PxScale::from(
						(content_metrics.scale.x.algebraic_sub(FONT_SIZE_DECREMENT))
							.max(MIN_CONTENT_FONT_SIZE),
					),
				);
			}
			continue;
		}

		final_lines.clear();
		let max_possible_lines =
			usize::try_from(max_content_height.saturating_div(content_metrics.line_height))?
				.min(MAX_LINES);

		for (i, line) in wrapped_lines.iter().take(max_possible_lines).enumerate() {
			let line_str = if (text_size(content_metrics.scale, content_font, line).0
				> max_content_width
				|| (i == max_possible_lines.saturating_sub(1)
					&& wrapped_lines.len() > max_possible_lines))
				&& line.len() > ELLIPSIS.len()
			{
				truncate_text(line, max_content_width, &content_metrics, content_font)
			} else {
				line.to_string()
			};

			final_lines.push(line_str);
		}

		if !final_lines.is_empty() && content_metrics.scale.x >= MIN_CONTENT_FONT_SIZE {
			break;
		}

		if content_metrics.scale.x == MIN_CONTENT_FONT_SIZE {
			wrapped_length = wrapped_length.saturating_sub(WRAP_LENGTH_DECREMENT);
			if wrapped_length < 20 {
				break;
			}
		} else {
			content_metrics.recalculate(
				content_font,
				PxScale::from(
					(content_metrics.scale.x - FONT_SIZE_DECREMENT).max(MIN_CONTENT_FONT_SIZE),
				),
			);
		}
	}

	let lines_count = u32::try_from(final_lines.len())?;
	let total_text_height = lines_count
		.saturating_mul(content_metrics.line_height)
		.saturating_add(lines_count.saturating_sub(1).saturating_mul(LINE_SPACING));
	let quoted_content_y = QUOTE_HEIGHT
		.saturating_sub(total_text_height)
		.saturating_div(2);

	let mut current_y = i32::try_from(quoted_content_y)?;

	for line in final_lines {
		let line_width = text_size(content_metrics.scale, content_font, &line).0;
		let (line_x, line_x_reverse) = (
			i32::try_from(
				QUOTE_HEIGHT.saturating_add(
					(QUOTE_WIDTH
						.saturating_sub(QUOTE_HEIGHT)
						.saturating_sub(line_width))
					.saturating_div(2),
				),
			)?,
			i32::try_from(
				QUOTE_WIDTH
					.saturating_sub(QUOTE_HEIGHT)
					.saturating_sub(line_width)
					.saturating_div(2),
			)?,
		);

		line_positions.push((line.clone(), line_x, current_y));
		line_positions_reverse.push((line, line_x_reverse, current_y));
		current_y = current_y.saturating_add(i32::try_from(
			content_metrics.line_height.saturating_add(LINE_SPACING),
		)?);
	}

	let author_name_width = text_size(author_metrics.scale, author_font, author_name).0;
	let (author_x, author_x_reverse) = (
		i32::try_from(
			QUOTE_WIDTH
				.saturating_sub(author_name_width)
				.saturating_div(2)
				.saturating_add(QUOTE_HEIGHT.saturating_div(2)),
		)?,
		i32::try_from(
			QUOTE_WIDTH
				.saturating_sub(QUOTE_HEIGHT)
				.saturating_sub(author_name_width)
				.saturating_div(2),
		)?,
	);

	let author_y = if line_positions.len() == 1 {
		i32::try_from(
			quoted_content_y
				.saturating_add(total_text_height)
				.saturating_add(LINE_SPACING)
				.saturating_mul(3),
		)?
	} else {
		i32::try_from(
			quoted_content_y
				.saturating_add(total_text_height)
				.saturating_add(LINE_SPACING),
		)?
	};

	Ok(TextLayout {
		content_lines: line_positions,
		content_lines_reverse: line_positions_reverse,
		author_position: (author_name.to_owned(), author_x, author_x_reverse, author_y),
		content_scale: content_metrics.scale,
		author_scale: author_metrics.scale,
	})
}

fn apply_text_layout(
	img: &mut RgbaImage,
	layout: &TextLayout,
	text_colour: Rgba<u8>,
	content_font: &FontArc,
	author_font: &FontArc,
	is_reverse: bool,
) {
	for (line, x, y) in if is_reverse {
		&layout.content_lines_reverse
	} else {
		&layout.content_lines
	} {
		draw_text_mut(
			img,
			text_colour,
			*x,
			*y,
			layout.content_scale,
			content_font,
			line,
		);
	}

	let (author_text, x, y) = &(
		&layout.author_position.0,
		if is_reverse {
			layout.author_position.2
		} else {
			layout.author_position.1
		},
		layout.author_position.3,
	);
	draw_text_mut(
		img,
		text_colour,
		*x,
		*y,
		layout.author_scale,
		author_font,
		author_text,
	);
}

type ImagePayload = (
	Vec<u8>,
	Option<TextLayout>,
	Option<ImageBuffer<Rgba<u8>, Vec<u8>>>,
);

pub fn quote_image(
	avatar_bytes: Option<&[u8]>,
	avatar_resized: Option<ImageBuffer<Rgba<u8>, Vec<u8>>>,
	author_name: &str,
	quoted_content: &str,
	author_font: &FontArc,
	content_font: &FontArc,
	theme: Option<&str>,
	text: Option<&TextLayout>,
	is_reverse: bool,
	is_light: bool,
	is_colour: bool,
	is_gradient: bool,
	is_animated: bool,
	new_font: bool,
) -> AResult<ImagePayload> {
	let avatar_position = if is_reverse {
		i64::from(QUOTE_WIDTH.saturating_sub(QUOTE_HEIGHT))
	} else {
		0
	};

	let (mut img, text_colour) = get_base_image(theme, is_light);

	let text_layout = if let Some(text_layout) = text
		&& !new_font
	{
		text_layout
	} else {
		&prepare_text_layout(quoted_content, author_name, content_font, author_font)?
	};

	if is_animated {
		if let Some(avatar_bytes) = avatar_bytes {
			let mut frames = GifDecoder::new(Cursor::new(avatar_bytes))?.into_frames();

			let mut output = Vec::with_capacity(img.len().saturating_mul(2));

			apply_text_layout(
				&mut img,
				text_layout,
				text_colour,
				content_font,
				author_font,
				is_reverse,
			);

			let mut quote_frame = img.clone();
			let mut avatar_frame;

			{
				let mut gif_encoder = GifEncoder::new_with_speed(&mut output, 10);
				gif_encoder.set_repeat(Infinite)?;

				let mut count: u32 = 0;

				while let Some(Ok(frame)) = frames.next() {
					avatar_frame = resize(
						frame.buffer(),
						QUOTE_HEIGHT,
						QUOTE_HEIGHT,
						FilterType::Nearest,
					);

					if !is_colour {
						convert_to_bw(&mut avatar_frame)?;
					}
					if is_gradient {
						apply_gradient_to_avatar(&mut avatar_frame, is_reverse)?;
					}

					quote_frame.copy_from(&img, 0, 0)?;
					overlay(&mut quote_frame, &avatar_frame, avatar_position, 0);

					gif_encoder.encode_frame(Frame::from_parts(
						quote_frame.clone(),
						0,
						0,
						frame.delay(),
					))?;

					count = count.saturating_add(1);

					if count > 30 {
						break;
					}
				}
			}

			return Ok((
				output,
				(text.is_none() || new_font).then(|| text_layout.clone()),
				None,
			));
		}
		bail!("Missing avatar bytes");
	}

	let mut avatar_image = if let Some(avatar_resized) = avatar_resized {
		avatar_resized
	} else if let Some(avatar_bytes) = avatar_bytes {
		if let Ok(avatar_mem) = load_from_memory(avatar_bytes) {
			resize(
				&avatar_mem.to_rgba8(),
				QUOTE_HEIGHT,
				QUOTE_HEIGHT,
				FilterType::Triangle,
			)
		} else {
			bail!("Failed to load avatar into memory");
		}
	} else {
		bail!("Missing avatar");
	};

	if !is_colour {
		convert_to_bw(&mut avatar_image)?;
	}
	if is_gradient {
		apply_gradient_to_avatar(&mut avatar_image, is_reverse)?;
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

	let mut output = Vec::with_capacity(img.len());
	img.write_to(&mut Cursor::new(&mut output), Avif)?;

	Ok((
		output,
		(text.is_none() || new_font).then(|| text_layout.clone()),
		avatar_bytes.is_none().then_some(avatar_image),
	))
}

pub fn convert_to_bw(image: &mut RgbaImage) -> AResult<()> {
	for pixel in image.pixels_mut() {
		let gray = u8::try_from(
			(u32::from(pixel[0])
				.saturating_mul(R_WEIGHT)
				.saturating_add(u32::from(pixel[1]).saturating_mul(G_WEIGHT))
				.saturating_add(u32::from(pixel[2]).saturating_mul(B_WEIGHT)))
				>> 8,
		)
		.context("Pixel values out of bounds for u8")?;
		pixel[0] = gray;
		pixel[1] = gray;
		pixel[2] = gray;
	}

	Ok(())
}
