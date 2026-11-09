use thief_reader::hotkeys::{self, Recorder};
#[test]
fn aliases_and_modifier_order_are_canonical() {
    assert_eq!(
        hotkeys::canonical("shift+control+m").as_deref(),
        Some("Ctrl+Shift+M")
    );
    assert_eq!(
        hotkeys::canonical("meta+alt+F8").as_deref(),
        Some("Alt+Win+F8")
    );
    assert_eq!(hotkeys::parse("Ctrl+Alt+, ").unwrap().vk, 0xbc);
    assert!(hotkeys::parse("Ctrl+M+N").is_none());
    assert!(hotkeys::parse("Ctrl").is_none());
}
#[test]
fn recorder_waits_for_release_and_ignores_repeat() {
    let mut recorder = Recorder::default();
    recorder.begin(0);
    assert_eq!(recorder.press("\u{11}", 2).unwrap(), "Ctrl");
    assert!(recorder.release().is_none());
    assert_eq!(recorder.press("m", 3).unwrap(), "Ctrl+Alt+M");
    assert_eq!(recorder.press("p", 3).unwrap(), "Ctrl+Alt+M");
    assert_eq!(recorder.release(), Some((0, "Ctrl+Alt+M".into())));
    assert!(recorder.target.is_none());
}
#[test]
fn cancellation_and_switching_discard_pending_keys() {
    let mut r = Recorder::default();
    r.begin(0);
    r.press("m", 3).unwrap();
    r.cancel();
    assert!(r.release().is_none());
    r.begin(1);
    r.press("a", 2).unwrap();
    r.begin(2);
    assert!(r.release().is_none());
    assert_eq!(r.target, Some(2));
}
#[test]
fn named_keys_and_invalid_text_are_handled() {
    let mut r = Recorder::default();
    r.begin(4);
    assert_eq!(r.press("\u{f70b}", 2).unwrap(), "Ctrl+F8");
    assert_eq!(r.release(), Some((4, "Ctrl+F8".into())));
    r.begin(1);
    assert!(r.press("中文", 2).is_err());
    assert!(r.release().is_none());
    assert!(hotkeys::parse("Ctrl+PageDown").is_some());
    assert!(hotkeys::parse("Ctrl+Plus").is_some());
}
#[test]
fn duplicate_chords_are_reported_on_each_row() {
    let keys = vec![
        "Ctrl+Alt+M".into(),
        "alt+control+m".into(),
        "Ctrl+F8".into(),
    ];
    let errors = hotkeys::validate(&keys);
    assert!(!errors[0].is_empty());
    assert!(!errors[1].is_empty());
    assert!(errors[2].is_empty());
}

#[test]
fn system_conflict_is_attached_to_the_exact_chord() {
    let errors = vec!["Ctrl+Alt+Minus 已被占用".into()];
    assert!(hotkeys::registration_error(&errors, "Ctrl+Alt+M").is_empty());
    assert_eq!(
        hotkeys::registration_error(&errors, "Ctrl+Alt+Minus"),
        errors[0]
    );
}
