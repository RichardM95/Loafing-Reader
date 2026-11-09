#![cfg(windows)]
use thief_reader::{native::TextEngine, settings::Style};
#[test]
fn single_line_is_not_reduced_to_one_character_by_line_spacing() {
    let engine = TextEngine::new().unwrap();
    for line_height in [1., 2., 3.] {
        let style = Style {
            page_numbers: false,
            line_height,
            ..Style::default()
        };
        let page = engine.page(&"汉".repeat(1000), 0, &style, 856., 25.);
        assert!(
            page.text.chars().count() > 50,
            "line height {line_height}: only {} characters",
            page.text.chars().count()
        );
        assert!(engine.text_fits(&page.display(""), &style, 836., 25.));
    }
}
#[test]
fn multiline_uses_full_width_except_for_the_final_footer() {
    let engine = TextEngine::new().unwrap();
    let style = Style::default();
    let page = engine.page(&"汉".repeat(5000), 0, &style, 420., 160.);
    assert!(
        page.text.chars().count() > 200,
        "only {} characters",
        page.text.chars().count()
    );
}
#[test]
fn adaptive_candidates_fill_narrow_glyph_pages() {
    let engine = TextEngine::new().unwrap();
    let style = Style {
        page_numbers: false,
        spacing: -2.,
        ..Style::default()
    };
    let page = engine.page(&"i".repeat(20000), 0, &style, 856., 47.);
    assert!(engine.text_fits(&page.display(""), &style, 836., 47.));
    assert!(!engine.text_fits(&(page.text + "i"), &style, 836., 47.));
}
#[test]
fn original_paragraphs_are_preserved_and_height_controls_line_count() {
    let engine = TextEngine::new().unwrap();
    let style = Style {
        page_numbers: false,
        ..Style::default()
    };
    let text = "第一条内容\n第二条内容\n第三条内容\n第四条内容";
    let short = engine.page(text, 0, &style, 856., 25.);
    let tall = engine.page(text, 0, &style, 856., 80.);
    assert_eq!(short.display(""), "第一条内容");
    assert!(short.text.ends_with('\n'));
    assert!(tall.text.contains('\n'));
    assert!(tall.end > short.end);
}
#[test]
fn final_page_index_and_measured_pages_agree() {
    let engine = TextEngine::new().unwrap();
    let style = Style::default();
    let text = "第一段😀á文字。\n下一段文字。".repeat(200);
    let pages = engine
        .paginate(&text, &style, 420., 100., || false)
        .unwrap();
    let footer = engine.footer_reservation(&style, pages.len());
    for (i, start) in pages.iter().enumerate() {
        let page = engine.page_with_footer(&text, *start, &style, 420., 100., &footer);
        assert_eq!(page.end, pages.get(i + 1).copied().unwrap_or(text.len()));
        assert_eq!(page.text, &text[*start..page.end]);
        assert!(engine.text_fits(&page.display(&footer), &style, 400., 100.));
        if page.end < text.len() {
            use unicode_segmentation::UnicodeSegmentation;
            let next = text[page.end..].graphemes(true).next().unwrap();
            assert!(!engine.text_fits(&(page.text + next + &footer), &style, 400., 100.));
        }
    }
    assert!(engine.paginate(&text, &style, 420., 100., || true).is_err());
}
#[test]
fn old_multiline_flag_cannot_disable_automatic_wrapping() {
    let settings: thief_reader::settings::Settings =
        serde_json::from_str(r#"{"style":{"multiline":false}}"#).unwrap();
    let engine = TextEngine::new().unwrap();
    let page = engine.page(&"汉".repeat(1000), 0, &settings.style, 420., 160.);
    assert!(page.text.chars().count() > 200);
}

#[test]
fn paragraph_boundaries_do_not_create_empty_pages_or_footer_rows() {
    let engine = TextEngine::new().unwrap();
    let style = Style {
        page_numbers: false,
        ..Style::default()
    };
    let text = "第一行\n第二行\n第三行\n";
    let mut offset = 0;
    let mut visible = Vec::new();
    while offset < text.len() {
        let page = engine.page(text, offset, &style, 420., 25.);
        visible.push(page.display(""));
        assert!(page.end > offset);
        offset = page.end;
    }
    assert_eq!(visible, vec!["第一行", "第二行", "第三行"]);
    let text = "第一段\n\n第二段";
    let page = engine.page(text, 0, &style, 420., 100.);
    assert_eq!(page.display(""), text);
}

#[test]
fn line_spacing_keeps_first_line_position_and_compact_lines_separate() {
    let engine = TextEngine::new().unwrap();
    let style = Style {
        size: 16.,
        page_numbers: false,
        background: 0,
        ..Style::default()
    };
    let ink_rows = |pixels: &[u8]| -> Vec<bool> {
        pixels
            .as_chunks::<{ 240 * 4 }>()
            .0
            .iter()
            .map(|row| row.as_chunks::<4>().0.iter().any(|p| p[3] > 64))
            .collect()
    };
    let normal = engine
        .render("国国国\n国国国\n国国国", &style, 240, 160, 1.)
        .unwrap();
    let normal_rows = ink_rows(&normal);
    let top = normal_rows.iter().position(|v| *v).unwrap();
    let wide = Style {
        line_height: 1.8,
        ..style.clone()
    };
    let pixels = engine
        .render("国国国\n国国国\n国国国", &wide, 240, 160, 1.)
        .unwrap();
    assert_eq!(ink_rows(&pixels).iter().position(|v| *v), Some(top));
    let compact = Style {
        line_height: 0.8,
        ..style
    };
    let pixels = engine
        .render("国国国\n国国国\n国国国", &compact, 240, 160, 1.)
        .unwrap();
    let rows = ink_rows(&pixels);
    let bands = rows
        .iter()
        .enumerate()
        .filter(|(i, on)| **on && (*i == 0 || !rows[*i - 1]))
        .count();
    assert_eq!(bands, 3);
}
