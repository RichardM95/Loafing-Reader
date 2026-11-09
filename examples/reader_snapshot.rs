//! Offscreen paragraph and line-spacing verification; no native reader or system input.
use std::{fs::File, io::BufWriter};
use thief_reader::{native::TextEngine, settings::Style};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("dist/reader-previews")?;
    let engine = TextEngine::new()?;
    let paragraphs="第一段：窗外的光落在书页上，阅读窗保留原始段落，并按照字体和窗口大小自动换行。\n第二段：较短的段落允许右侧留白。\n\n第三段：继续阅读，翻页后不漏字、不重复，也不会多出空白页。".repeat(20);
    let cases = [
        ("one-line", 856u32, 25u32, 1.0f32),
        ("two-lines", 856, 47, 1.0),
        ("paragraphs", 640, 160, 1.0),
        ("compact-lines", 640, 160, 0.8),
        ("wide-lines", 640, 180, 1.8),
    ];
    for (name, width, height, line_height) in cases {
        let style = Style {
            size: 16.,
            line_height,
            ..Style::default()
        };
        let pages = engine.paginate(&paragraphs, &style, width as f32, height as f32, || false)?;
        let reserve = engine.footer_reservation(&style, pages.len());
        let page = engine.page_with_footer(
            &paragraphs,
            0,
            &style,
            width as f32,
            height as f32,
            &reserve,
        );
        let display = page.display(&format!("  1/{}", pages.len()));
        assert!(engine.text_fits(&display, &style, width as f32 - 20., height as f32));
        let mut pixels = engine.render(&display, &style, width, height, 1.)?;
        for p in pixels.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
            let alpha = p[3] as u32;
            for value in &mut p[..3] {
                *value = (*value as u32 * 255)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
        }
        let mut out = png::Encoder::new(
            BufWriter::new(File::create(format!("dist/reader-previews/{name}.png"))?),
            width,
            height,
        );
        out.set_color(png::ColorType::Rgba);
        out.set_depth(png::BitDepth::Eight);
        out.write_header()?.write_image_data(&pixels)?;
        println!(
            "{name}: {} pages, first page {} UTF-8 bytes",
            pages.len(),
            page.end
        );
    }
    Ok(())
}
