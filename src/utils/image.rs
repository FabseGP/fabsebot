use crate::config::constants::{QUOTE_HEIGHT, QUOTE_WIDTH};

use ab_glyph::{FontArc, PxScale};
use image::{
    imageops::{overlay, resize, FilterType::CatmullRom},
    Rgba, RgbaImage,
};
use imageproc::drawing::{draw_text_mut, text_size};
use textwrap::wrap;

const MIN_FONT_SIZE: f32 = 32.0;
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

fn truncate_text(text: &str, max_width: u32, metrics: &FontMetrics, font: &FontArc) -> String {
    if text.len() <= ELLIPSIS.len() {
        return text.to_string();
    }

    let mut end = text.len() - ELLIPSIS.len();
    let mut truncated = format!("{}{}", &text[..end], ELLIPSIS);

    while text_size(metrics.scale, font, &truncated).0 > max_width && end > ELLIPSIS.len() {
        end -= 1;
        truncated = format!("{}{}", &text[..end], ELLIPSIS);
    }

    truncated
}

pub fn quote_image(
    avatar: &RgbaImage,
    author_name: &str,
    quoted_content: &str,
    author_font: &FontArc,
    content_font: &FontArc,
) -> RgbaImage {
    let max_content_width = QUOTE_WIDTH - QUOTE_HEIGHT - 64;
    let max_content_height = QUOTE_HEIGHT - 64;
    let text_offset = QUOTE_HEIGHT / 2;

    let mut img = RgbaImage::from_pixel(QUOTE_WIDTH, QUOTE_HEIGHT, Rgba([0, 0, 0, 255]));

    overlay(
        &mut img,
        &resize(avatar, QUOTE_HEIGHT, QUOTE_HEIGHT, CatmullRom),
        0,
        0,
    );

    let mut content_metrics = FontMetrics::new(content_font, PxScale::from(96.0));
    let author_metrics = FontMetrics::new(author_font, PxScale::from(40.0));

    let mut wrapped_length = DEFAULT_WRAP_LENGTH;
    let mut final_lines = Vec::with_capacity(MAX_LINES);

    loop {
        let wrapped_lines = wrap(quoted_content, wrapped_length);

        if let Some(first_line) = wrapped_lines.first() {
            if text_size(content_metrics.scale, &content_font, first_line).0 > max_content_width {
                if content_metrics.scale.x == MIN_FONT_SIZE {
                    wrapped_length = wrapped_length.saturating_sub(WRAP_LENGTH_DECREMENT);
                    if wrapped_length < 20 {
                        break;
                    }
                } else {
                    content_metrics.recalculate(
                        content_font,
                        PxScale::from(
                            (content_metrics.scale.x - FONT_SIZE_DECREMENT).max(MIN_FONT_SIZE),
                        ),
                    );
                }
                continue;
            }
        }

        final_lines.clear();
        let max_total_height = max_content_height - 64;
        let max_possible_lines =
            ((max_total_height / content_metrics.line_height) as usize).min(MAX_LINES);

        for (i, line) in wrapped_lines.iter().take(max_possible_lines).enumerate() {
            let mut line_str = line.to_string();

            if text_size(content_metrics.scale, &content_font, &line_str).0 > max_content_width
                || (i == max_possible_lines - 1 && wrapped_lines.len() > max_possible_lines)
            {
                line_str =
                    truncate_text(&line_str, max_content_width, &content_metrics, content_font);
            }

            final_lines.push(line_str);
        }

        if !final_lines.is_empty() && content_metrics.scale.x >= MIN_FONT_SIZE {
            break;
        }

        if content_metrics.scale.x == MIN_FONT_SIZE {
            wrapped_length = wrapped_length.saturating_sub(WRAP_LENGTH_DECREMENT);
            if wrapped_length < 20 {
                break;
            }
        } else {
            content_metrics.recalculate(
                content_font,
                PxScale::from((content_metrics.scale.x - FONT_SIZE_DECREMENT).max(MIN_FONT_SIZE)),
            );
        }
    }

    let lines_count =
        u32::try_from(final_lines.len()).expect("amount of lines out of bounds for u32");
    let total_text_height =
        (lines_count * content_metrics.line_height) + ((lines_count - 1) * LINE_SPACING);
    let quoted_content_y = (QUOTE_HEIGHT - total_text_height) / 2;

    let author_name_width = text_size(author_metrics.scale, &author_font, author_name).0;
    let author_name_x = i32::try_from(((QUOTE_WIDTH - author_name_width) / 2) + text_offset)
        .expect("wrapped around value");
    let author_name_y =
        i32::try_from(quoted_content_y + total_text_height + 16).expect("wrapped around value");

    let white = Rgba([255, 255, 255, 255]);
    let mut current_y = i32::try_from(quoted_content_y).expect("wrapped around value");

    for line in &final_lines {
        let line_width = text_size(content_metrics.scale, &content_font, line).0;
        let line_x = i32::try_from(QUOTE_HEIGHT + (QUOTE_WIDTH - QUOTE_HEIGHT - line_width) / 2)
            .expect("wrapped around value");

        draw_text_mut(
            &mut img,
            white,
            line_x,
            current_y,
            content_metrics.scale,
            &content_font,
            line,
        );

        current_y += i32::try_from(content_metrics.line_height + LINE_SPACING)
            .expect("wrapped around value");
    }

    draw_text_mut(
        &mut img,
        white,
        author_name_x,
        if final_lines.len() == 1 {
            author_name_y + i32::try_from(LINE_SPACING * 2).expect("wrapped around value")
        } else {
            author_name_y
        },
        author_metrics.scale,
        &author_font,
        author_name,
    );

    img
}

pub fn convert_to_bw(image: &mut RgbaImage, avatar_size: u32) {
    const R_FACTOR: u32 = (0.299 * 256.0) as u32;
    const G_FACTOR: u32 = (0.587 * 256.0) as u32;
    const B_FACTOR: u32 = (0.114 * 256.0) as u32;

    for y in 0..avatar_size {
        for x in 0..avatar_size {
            let pixel = image.get_pixel_mut(x, y);

            let gray = u8::try_from(
                (u32::from(pixel[0]) * R_FACTOR
                    + u32::from(pixel[1]) * G_FACTOR
                    + u32::from(pixel[2]) * B_FACTOR)
                    >> 8,
            )
            .expect("out of bounds for u8");

            pixel[0] = gray;
            pixel[1] = gray;
            pixel[2] = gray;
        }
    }
}
