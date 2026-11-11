//! Optional Windows APIs must never become loader-time imports.
use std::sync::OnceLock;
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
    },
    core::*,
};

unsafe fn symbol(module: PCWSTR, name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
    unsafe { GetProcAddress(LoadLibraryW(module).ok()?, name) }
}
pub fn initialize_dpi() {
    type Modern = unsafe extern "system" fn(isize) -> BOOL;
    unsafe {
        if let Some(p) = symbol(w!("user32.dll"), s!("SetProcessDpiAwarenessContext")) {
            let f: Modern = std::mem::transmute(p);
            if f(-4).as_bool() {
                return;
            }
        }
        if let Some(p) = symbol(w!("shcore.dll"), s!("SetProcessDpiAwareness")) {
            let f: unsafe extern "system" fn(i32) -> HRESULT = std::mem::transmute(p);
            if f(2).is_ok() {
                return;
            }
        }
        // Win7 uses system-wide DPI; existing manifest awareness is also respected.
        let _ = SetProcessDPIAware();
    }
}
pub fn system_dpi() -> u32 {
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return 96;
        }
        let dpi = GetDeviceCaps(Some(dc), LOGPIXELSX);
        ReleaseDC(None, dc);
        dpi.max(96) as u32
    }
}
pub fn window_dpi(hwnd: HWND) -> f32 {
    type Api = unsafe extern "system" fn(HWND) -> u32;
    static API: OnceLock<Option<Api>> = OnceLock::new();
    let f = API.get_or_init(|| unsafe {
        symbol(w!("user32.dll"), s!("GetDpiForWindow"))
            .map(|p| std::mem::transmute::<unsafe extern "system" fn() -> isize, Api>(p))
    });
    let dpi = f
        .map(|f| unsafe { f(hwnd) })
        .filter(|n| *n != 0)
        .unwrap_or_else(|| {
            monitor_dpi(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) })
        });
    dpi as f32 / 96.
}
pub fn monitor_dpi(monitor: HMONITOR) -> u32 {
    type Api = unsafe extern "system" fn(HMONITOR, i32, *mut u32, *mut u32) -> HRESULT;
    static API: OnceLock<Option<Api>> = OnceLock::new();
    let f = API.get_or_init(|| unsafe {
        symbol(w!("shcore.dll"), s!("GetDpiForMonitor"))
            .map(|p| std::mem::transmute::<unsafe extern "system" fn() -> isize, Api>(p))
    });
    let (mut x, mut y) = (96, 96);
    if let Some(f) = f
        && unsafe { f(monitor, 0, &mut x, &mut y) }.is_ok()
    {
        return x;
    }
    system_dpi()
}
