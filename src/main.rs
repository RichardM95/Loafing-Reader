#![windows_subsystem = "windows"]
use thief_reader::native;
mod controller;
use controller::Controller;
use std::{cell::RefCell, rc::Rc};
slint::include_modules!();
thread_local! { static APP: RefCell<Option<Rc<RefCell<Controller>>>> = const { RefCell::new(None) }; }
fn main() {
    if let Err(e) = run() {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                windows::core::PCWSTR(native::wide(&e).as_ptr()),
                windows::core::w!("Thief Reader"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}
fn run() -> Result<(), String> {
    let Some(_instance) = native::single_instance().map_err(|e| e.to_string())? else {
        return Ok(());
    };
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("skia".into())
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
