use std::hint::black_box;

use ab_glyph::FontArc;
use criterion::{Criterion, criterion_group, criterion_main};
use fabsebot_core::utils::image::quote_image;

fn load_assets() -> (FontArc, FontArc, [u8; 8640]) {
	let content_font =
		FontArc::try_from_slice(include_bytes!("../../fonts/NotoSansJP-Regular.ttf")).unwrap();
	let author_font =
		FontArc::try_from_slice(include_bytes!("../../fonts/Satoshi-LightItalic.otf")).unwrap();
	let avatar = include_bytes!("../../bench_assets/avatar.webp");

	(content_font, author_font, *avatar)
}

fn benchmark_baseline(c: &mut Criterion) {
	let (content_font, author_font, avatar) = load_assets();
	let content = "bruh bre broo ".repeat(50);

	c.bench_function("baseline", |b| {
		b.iter(|| {
			quote_image(
				black_box(Some(&avatar)),
				None,
				black_box("Author"),
				black_box(&content),
				black_box(&author_font),
				black_box(&content_font),
				None,
				None,
				false,
				false,
				true,
				false,
				false,
				false,
			)
		});
	});
}

criterion_group!(benches, benchmark_baseline);
criterion_main!(benches);
