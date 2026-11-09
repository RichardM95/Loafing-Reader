use std::time::Instant;
use thief_reader::{book, native::TextEngine, settings};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: cargo run --example inspect -- <TXT path>")?;
    let start = Instant::now();
    let doc = book::open(std::path::Path::new(&path), "auto", None)?;
    let load = start.elapsed();
    let mut engine = TextEngine::new()?;
    engine.load_fonts(&[])?;
    let style = settings::Style::default();
    let mut offset = 0;
    let mut times = Vec::new();
    for _ in 0..200 {
        let start = Instant::now();
        let page = engine.page(&doc.text, offset, &style, 856., 47.);
        let pixels = engine.render(&page.text, &style, 856, 47, 1.)?;
        assert_eq!(pixels[3], 128);
        offset = page.end;
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({
            "decoded_utf8_bytes":doc.text.len(),"chapters":doc.chapters.len(),
            "load_and_parse_ms":load.as_secs_f64()*1000.,
            "page_and_bitmap_p50_ms":times[100],"page_and_bitmap_p95_ms":times[190],
            "monitors":thief_reader::native::screens().len(),
            "measurement":if cfg!(debug_assertions) {"debug in-process layout and bitmap only"} else {"release in-process layout and bitmap only"}
        })
    );
    Ok(())
}
