use ab_glyph::{FontArc, PxScale};
use image::{
    imageops::{overlay, resize, FilterType::CatmullRom},
    ImageFormat::WebP,
    Rgb, RgbImage,
};
use imageproc::drawing::{draw_text_mut, text_size};
use std::borrow::Cow;
use std::io::Cursor;
use textwrap::wrap;

pub async fn quote_image(
    avatar: &RgbImage,
    author_name: &str,
    quoted_content: &str,
) -> Option<Vec<u8>> {
    const MAX_ITERATIONS: u32 = 200;

    let width = 1200;
    let height = 630;

    let avatar_image = resize(avatar, height, height, CatmullRom);
    let mut img = RgbImage::from_pixel(width, height, Rgb([0, 0, 0]));
    overlay(&mut img, &avatar_image, 0, 0);

    let font_content =
        FontArc::try_from_slice(include_bytes!("../../fonts/NotoSansJP-Regular.ttf"))
            .expect("main font not in path");
    let font_author =
        FontArc::try_from_slice(include_bytes!("../../fonts/NotoSansJP-ExtraLight.ttf"))
            .expect("author font not in path");

    let content_scale = PxScale::from(128.0);
    let mut author_scale = PxScale::from(40.0);
    let white = Rgb([255, 255, 255]);

    let max_content_width = width - height - 96;
    let max_content_height = height - 64;

    let mut wrapped_length = 20;
    let mut wrapped_lines = wrap(quoted_content, wrapped_length);

    let mut text_offset = 320;

    let mut total_text_height = 0;
    let mut content_scale_adjusted = content_scale;
    let mut iteration_count = 0;

    loop {
        iteration_count += 1;
        if iteration_count > MAX_ITERATIONS {
            break;
        }

        let mut all_fit = true;
        total_text_height = 0;
        let mut line_height = 0;
        let mut line_width = 0;
        let mut dimensions;
        let padding = if wrapped_lines.len() == 1 { 32 } else { 16 };

        for line in &wrapped_lines {
            dimensions = text_size(content_scale_adjusted, &font_content, line);
            line_height = dimensions.1;
            line_width = dimensions.0;

            if total_text_height + line_height + padding > max_content_height
                || line_width > max_content_width
            {
                all_fit = false;
                break;
            }

            total_text_height += line_height + 10;
        }

        if all_fit {
            if wrapped_lines.len() > 16 {
                wrapped_length += 2;
                wrapped_lines = wrap(quoted_content, wrapped_length);
                content_scale_adjusted = content_scale;
            } else {
                if wrapped_lines.len() == 1 {
                    total_text_height = line_height + 40;
                    if wrapped_lines.first().unwrap().len() < 10 {
                        text_offset += 64;
                    }
                } else {
                    total_text_height += 16;
                }
                break;
            }
        } else {
            let new_scale = content_scale_adjusted.x - 2.0;
            if new_scale < 12.0 {
                wrapped_length = wrapped_length.max(20);
                wrapped_lines = wrap(quoted_content, wrapped_length);
                break;
            }

            content_scale_adjusted = PxScale::from(new_scale);

            if (content_scale_adjusted.x + 2.0 - author_scale.x).abs() < 0.1 {
                if author_scale.x > 18.0 {
                    author_scale = PxScale::from(author_scale.x - 1.0);
                } else if line_width > max_content_width {
                    wrapped_length = wrapped_length.saturating_sub(2);
                    wrapped_lines = wrap(quoted_content, wrapped_length);
                } else {
                    wrapped_length += 2;
                    wrapped_lines = wrap(quoted_content, wrapped_length);
                    dimensions = text_size(
                        content_scale_adjusted,
                        &font_content,
                        wrapped_lines.first().unwrap_or(&Cow::Borrowed("")),
                    );
                    if dimensions.0 > max_content_width {
                        wrapped_length = wrapped_length.saturating_sub(2);
                        wrapped_lines = wrap(quoted_content, wrapped_length);
                    }
                }
                content_scale_adjusted = content_scale;
            }
        }
    }

    let mut quoted_content_y = (height - total_text_height) / 2;
    let author_name_y = quoted_content_y + total_text_height + 16;

    let (author_name_width, _author_name_height) =
        text_size(author_scale, &font_author, author_name);

    let quoted_content_x = ((width - max_content_width) / 2) + text_offset;
    let author_name_x = ((width - author_name_width) / 2) + 320;

    for line in wrapped_lines {
        draw_text_mut(
            &mut img,
            white,
            quoted_content_x.try_into().unwrap(),
            quoted_content_y.try_into().unwrap(),
            content_scale_adjusted,
            &font_content,
            &line,
        );

        let dimensions = text_size(content_scale_adjusted, &font_content, &line);
        quoted_content_y += dimensions.1 + 10;
    }

    draw_text_mut(
        &mut img,
        white,
        author_name_x.try_into().unwrap(),
        author_name_y.try_into().unwrap(),
        author_scale,
        &font_author,
        author_name,
    );

    let mut buffer = Cursor::new(Vec::new());
    if img.write_to(&mut buffer, WebP).is_ok() {
        Some(buffer.into_inner())
    } else {
        None
    }
}
