use thief_reader::settings::*;
#[test]
fn negative_monitor_and_gap_are_handled() {
    let work = Rect {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1040,
    };
    let r = Rect {
        x: 9000,
        y: -500,
        width: 856,
        height: 47,
    }
    .fit(work);
    assert_eq!((r.x, r.y), (-856, 0));
    let r = Rect {
        x: 0,
        y: 0,
        width: 9000,
        height: 9000,
    }
    .fit(work);
    assert_eq!((r.width, r.height), (1920, 1040));
}
#[test]
fn independent_alpha_colors() {
    assert_eq!(parse_color("rgba(0, 0, 0, 0.5)"), Some(0x80000000));
    assert_eq!(parse_color("#fff"), Some(0xffffffff));
    assert_eq!(parse_color("#123456"), Some(0xff123456));
    assert_eq!(parse_color("oops"), None);
}
#[test]
fn settings_roundtrip_preserves_unicode_and_anchor() {
    let dir = std::env::temp_dir().join(format!("thief-reader-test-{}", std::process::id()));
    let mut s = Settings::default();
    s.books.insert(
        "甲.txt".into(),
        Memo {
            offset: 12345,
            rule: "第.*章".into(),
            encoding: "auto".into(),
        },
    );
    save(&dir, &s).unwrap();
    s.seconds = 12;
    save(&dir, &s).unwrap();
    let read = load(&dir).unwrap();
    assert_eq!(read.seconds, 12);
    assert_eq!(read.books["甲.txt"].offset, 12345);
    std::fs::remove_file(dir.join("settings.json")).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
