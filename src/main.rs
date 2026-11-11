// Win7 x86 uses emulated TLS; Clippy flags std macro internals despite const initializers.
#![cfg_attr(target_arch = "x86", allow(clippy::missing_const_for_thread_local))]
#![windows_subsystem = "windows"]
use thief_reader::native;
mod controller;
use controller::Controller;
use std::{cell::RefCell, rc::Rc};
slint::include_modules!();
thread_local! { static APP: RefCell<Option<Rc<RefCell<Controller>>>> = const { RefCell::new(None) }; }
fn main() {
    // CI/legacy-host probe: no settings, tray, visible window, hotkeys, or message box.
    if std::env::args().any(|arg| arg == "--compat-check") {
        let result = (|| -> Result<usize, String> {
            thief_reader::compat::initialize_dpi();
            slint::BackendSelector::new()
                .backend_name("winit".into())
                .renderer_name("software".into())
                .select()
                .map_err(|e| e.to_string())?;
            let mut engine = native::TextEngine::new().map_err(|e| e.to_string())?;
            let fonts = engine.load_fonts(&[]).map_err(|e| e.to_string())?;
            engine.force_legacy_spacing(true);
            let style = thief_reader::settings::Style {
                spacing: 1.,
                ..Default::default()
            };
            engine
                .render("兼容检查 Loafing Reader 12.34%", &style, 780, 46, 1.)
                .map_err(|e| e.to_string())?;
            Ok(fonts.len())
        })();
        match result {
            Ok(fonts) => {
                println!(
                    "{}",
                    serde_json::json!({"ok":true,"arch":std::env::consts::ARCH,"renderer":"software","fonts":fonts})
                );
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = run() {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                windows::core::PCWSTR(native::wide(&e).as_ptr()),
                windows::core::w!("Loafing Reader"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}
fn run() -> Result<(), String> {
    let Some(_instance) = native::single_instance().map_err(|e| e.to_string())? else {
        return Ok(());
    };
    thief_reader::compat::initialize_dpi();
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()
        .map_err(|e| e.to_string())?;
    let window = AppWindow::new().map_err(|e| e.to_string())?;
    let model = Rc::new(RefCell::new(Controller::new(window.as_weak())?));
    APP.with(|app| *app.borrow_mut() = Some(model.clone()));
    native::on_event(|event| {
        let _ = slint::invoke_from_event_loop(move || {
            APP.with(|app| {
                if let Some(model) = app.borrow().as_ref() {
                    model.borrow_mut().action(event);
                }
            });
        });
    });
    controller::connect(&window, &model);
    window.window().on_close_requested({
        let weak = window.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.invoke_cancel_recording(-1);
                let _ = ui.hide();
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    window.show().map_err(|e| e.to_string())?;
    model.borrow_mut().startup();
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    model.borrow_mut().persist();
    APP.with(|app| app.borrow_mut().take());
    Ok(())
}
