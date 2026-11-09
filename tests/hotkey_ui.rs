//! Virtual events stay inside Slint's headless backend; no OS input or registrations.
#![cfg(all(windows, feature = "desktop"))]
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc};
use thief_reader::hotkeys::{self, Recorder};
slint::include_modules!();
fn caps(value: &str) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        hotkeys::keycaps(value)
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
    .into()
}
#[test]
fn headless_capture_release_escape_and_focus_lifecycle() {
    i_slint_backend_testing::init_no_event_loop();
    let ui = AppWindow::new().unwrap();
    ui.set_section(4);
    ui.window().set_size(slint::LogicalSize::new(1000., 720.));
    ui.on_key_caps(|value| caps(&value));
    let recorder = Rc::new(RefCell::new(Recorder::default()));
    let saved = Rc::new(RefCell::new(Vec::new()));
    {
        let r = recorder.clone();
        let weak = ui.as_weak();
        ui.on_begin_recording(move |index| {
            r.borrow_mut().begin(index as usize);
            let ui = weak.unwrap();
            ui.set_recording_index(index);
            ui.set_recording_preview("".into());
        });
    }
    {
        let r = recorder.clone();
        let weak = ui.as_weak();
        ui.on_cancel_recording(move |index| {
            let mut r = r.borrow_mut();
            if r.target.is_some() && (index < 0 || r.target == Some(index as usize)) {
                r.cancel();
                weak.unwrap().set_recording_index(-1);
            }
        });
    }
    {
        let r = recorder.clone();
        let weak = ui.as_weak();
        ui.on_record_pressed(move |_, text, ctrl, alt, shift, meta| {
            let mask =
                u32::from(ctrl) * 2 + u32::from(alt) + u32::from(shift) * 4 + u32::from(meta) * 8;
            let preview = r.borrow_mut().press(&text, mask).unwrap();
            weak.unwrap().set_recording_preview(preview.into());
        });
    }
    {
        let r = recorder.clone();
        let weak = ui.as_weak();
        let saved = saved.clone();
        ui.on_record_released(move |_| {
            if let Some((index, key)) = r.borrow_mut().release() {
                saved.borrow_mut().push((index, key.clone()));
                let ui = weak.unwrap();
                ui.set_previous_key(key.into());
                ui.set_recording_index(-1);
            }
        });
    }
    let tick = || i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(20));
    let press = |text: &str| {
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() })
    };
    let release = |text: &str| {
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: text.into() })
    };
    let click = || {
        use slint::platform::{PointerEventButton, WindowEvent};
        let position = slint::LogicalPosition::new(300., 168.);
        ui.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        tick();
    };
    ui.show().unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    tick();
    click();
    assert_eq!(
        ui.get_recording_index(),
        0,
        "virtual click should start first recorder"
    );
    press("\u{11}");
    press("\u{12}");
    press("k");
    tick();
    assert_eq!(ui.get_recording_preview(), "Ctrl+Alt+K");
    assert!(saved.borrow().is_empty());
    release("k");
    tick();
    assert_eq!(saved.borrow().as_slice(), &[(0, "Ctrl+Alt+K".to_string())]);
    release("\u{11}");
    release("\u{12}");
    assert_eq!(ui.get_recording_index(), -1);
    click();
    press("\u{1b}");
    tick();
    assert_eq!(ui.get_recording_index(), -1);
    assert_eq!(saved.borrow().len(), 1);
    click();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(false));
    tick();
    assert_eq!(
        ui.get_recording_index(),
        -1,
        "losing window focus must cancel capture"
    );
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    tick();
    click();
    ui.set_section(0);
    tick();
    assert_eq!(ui.get_recording_index(), -1);
    ui.set_section(4);
    tick();
    click();
    ui.window().set_minimized(true);
    tick();
    assert_eq!(ui.get_recording_index(), -1);
    ui.hide().unwrap();
}
