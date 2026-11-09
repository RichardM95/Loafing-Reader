#![cfg(windows)]
use thief_reader::{native::TextEngine, settings::Style};
#[test]
fn layered_pixels_preserve_background_alpha() {
    let engine = TextEngine::new().unwrap();
    let pixels = engine
        .render("测试 Abc", &Style::default(), 856, 47, 1.)
        .unwrap();
    assert_eq!(pixels.len(), 856 * 47 * 4);
    assert_eq!(pixels[3], 128);
    assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 128));
}
#[test]
fn unicode_pages_cover_the_source_exactly() {
    let engine = TextEngine::new().unwrap();
    let text = "第一章😀 Á 新的一天\r\n下一段。".repeat(80);
    let style = Style::default();
    let mut cursor = 0;
    while cursor < text.len() {
        let page = engine.page(&text, cursor, &style, 420., 47.);
        assert!(page.end > cursor);
        assert!(text.is_char_boundary(page.end));
        cursor = page.end;
    }
    assert_eq!(cursor, text.len());
}

#[test]
fn renderer_coexists_with_ui_sta_apartment() {
    let _engine = TextEngine::new().unwrap();
    unsafe {
        let result = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
        assert!(
            result.is_ok(),
            "Renderer must preserve the UI STA apartment: {result:?}"
        );
        windows::Win32::System::Com::CoUninitialize();
    }
}

#[test]
fn camouflage_is_centered() {
    let engine = TextEngine::new().unwrap();
    let pixels = engine
        .render_aligned("Hello", &Style::default(), 400, 60, 1., true)
        .unwrap();
    let ink: Vec<(usize, usize)> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, p)| p[3] > 200)
        .map(|(i, _)| (i % 400, i / 400))
        .collect();
    assert!(ink.iter().map(|p| p.0).min().unwrap() > 150);
    assert!(ink.iter().map(|p| p.1).min().unwrap() > 15);
}

#[test]
fn tray_matches_requested_actions() {
    let labels: Vec<_> = thief_reader::native::TRAY_ITEMS
        .iter()
        .map(|(_, label)| *label)
        .collect();
    assert_eq!(
        labels,
        vec!["系统设置", "显示 / 隐藏", "自动翻页", "窗口找回", "退出"]
    );
}
