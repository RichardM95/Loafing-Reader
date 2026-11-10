use crate::{
    book,
    settings::{Rect, Style},
};
use std::{cell::Cell, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Gdi::*, Imaging::*,
        },
        System::{Com::*, LibraryLoader::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::*,
};
pub const TRAY_ITEMS: [(usize, &str); 5] = [
    (1, "系统设置"),
    (4, "显示 / 隐藏"),
    (5, "自动翻页"),
    (6, "找回窗口"),
    (8, "退出"),
];
pub const TRAY: u32 = WM_APP + 7;
const TOPMOST_TIMER: usize = 16;
const RESIZE_TIMER: usize = 17;
#[derive(Clone, Copy)]
pub enum Action {
    Previous,
    Next,
    Boss,
    Auto,
    Rescue,
    Control,
    Quit,
    Geometry,
    ResizeBegin(Rect, u32),
    ResizeFrame,
    ResizeEnd,
    AutoTick,
    Topology,
}
static EVENT: OnceLock<Box<dyn Fn(Action) + Send + Sync>> = OnceLock::new();
thread_local! {
    static MOUSE: Cell<bool> = const { Cell::new(false) };
    static SIZING: Cell<bool> = const { Cell::new(false) };
    static SIZE_DIRTY: Cell<bool> = const { Cell::new(false) };
    static LAST_SIZE: Cell<isize> = const { Cell::new(0) };
    static MINIMUM: Cell<(f32, f32)> = const { Cell::new((100., 24.)) };
}
pub fn on_event(callback: impl Fn(Action) + Send + Sync + 'static) {
    let _ = EVENT.set(Box::new(callback));
}
fn emit(action: Action) {
    if let Some(callback) = EVENT.get() {
        callback(action);
    }
}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub struct Reader {
    pub hwnd: HWND,
}
impl Reader {
    pub fn new(rect: Rect) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("ThiefReaderLayer");
            let icon = LoadIconW(
                Some(instance.into()),
                PCWSTR(std::ptr::without_provenance(1)),
            )?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hIcon: icon,
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class,
                w!("Loafing Reader"),
                WS_POPUP,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let mut tray = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: TRAY,
                hIcon: icon,
                ..Default::default()
            };
            let tip = wide("Loafing Reader");
            tray.szTip[..tip.len()].copy_from_slice(&tip);
            let _ = Shell_NotifyIconW(NIM_ADD, &tray);
            Ok(Self { hwnd })
        }
    }
    pub fn rect(&self) -> Rect {
        unsafe {
            let mut r = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut r);
            Rect {
                x: r.left,
                y: r.top,
                width: r.right - r.left,
                height: r.bottom - r.top,
            }
        }
    }
    pub fn place(&self, rect: Rect) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                SWP_NOACTIVATE,
            );
        }
    }
    pub fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }
    pub fn show(&self, yes: bool) {
        unsafe {
            if yes {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                maintain_topmost(self.hwnd);
                SetTimer(Some(self.hwnd), TOPMOST_TIMER, 250, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TOPMOST_TIMER);
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }
    pub fn dpi(&self) -> f32 {
        unsafe { (GetDpiForWindow(self.hwnd).max(96) as f32) / 96. }
    }
    pub fn mouse(&self, yes: bool) {
        MOUSE.set(yes);
    }
    pub fn automatic(&self, seconds: Option<u32>) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), 15);
            if let Some(s) = seconds {
                SetTimer(Some(self.hwnd), 15, s.max(1) * 1000, None);
            }
        }
    }
    pub fn hotkeys(&self, keys: &[String]) -> Vec<String> {
        unsafe {
            let mut errors = Vec::new();
            for i in 0..5 {
                let _ = UnregisterHotKey(Some(self.hwnd), i);
            }
            for (i, key) in keys.iter().take(5).enumerate() {
                if let Some((modifiers, vk)) = parse_key(key) {
                    if RegisterHotKey(Some(self.hwnd), i as i32, modifiers | MOD_NOREPEAT, vk)
                        .is_err()
                    {
                        errors.push(format!("{} 已被占用", key));
                    }
                } else {
                    errors.push(format!("{} 无效", key));
                }
            }
            errors
        }
    }
    pub fn minimum_size(&self, size: (f32, f32)) {
        MINIMUM.set(size);
    }
    pub fn present(&self, pixels: &[u8], width: i32, height: i32) -> Result<()> {
        unsafe {
            let dc = GetDC(None);
            let memory = CreateCompatibleDC(Some(dc));
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
            let old = SelectObject(memory, bitmap.into());
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), pixels.len());
            let size = SIZE {
                cx: width,
                cy: height,
            };
            let source = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let result = UpdateLayeredWindow(
                self.hwnd,
                Some(dc),
                None,
                Some(&size),
                Some(memory),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            SelectObject(memory, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory);
            ReleaseDC(None, dc);
            if result.is_ok() {
                maintain_topmost(self.hwnd);
            }
            result
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        unsafe {
            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
/// Repair an overlapping window above the reader without showing, moving or activating it.
pub fn maintain_topmost(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let mut blocked = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize == 0;
        let mut previous = GetWindow(hwnd, GW_HWNDPREV).ok();
        // Bound traversal in case the desktop's z-order changes while it is being walked.
        for _ in 0..256 {
            let Some(above) = previous else { break };
            if IsWindowVisible(above).as_bool() {
                let mut other = RECT::default();
                if GetWindowRect(above, &mut other).is_ok()
                    && rect.left < other.right
                    && rect.right > other.left
                    && rect.top < other.bottom
                    && rect.bottom > other.top
                {
                    blocked = true;
                    break;
                }
            }
            previous = GetWindow(above, GW_HWNDPREV).ok();
        }
        blocked
            && SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
            .is_ok()
    }
}
fn parse_key(text: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let key = crate::hotkeys::parse(text)?;
    Some((HOT_KEY_MODIFIERS(key.modifiers), key.vk))
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCHITTEST => {
                let mut r = RECT::default();
                let _ = GetWindowRect(hwnd, &mut r);
                let x = (l.0 & 0xffff) as i16 as i32;
                let y = ((l.0 >> 16) & 0xffff) as i16 as i32;
                let left = x < r.left + 6;
                let right = x >= r.right - 6;
                let top = y < r.top + 6;
                let bottom = y >= r.bottom - 6;
                let hit = match (left, right, top, bottom) {
                    (true, _, true, _) => HTTOPLEFT,
                    (_, true, true, _) => HTTOPRIGHT,
                    (true, _, _, true) => HTBOTTOMLEFT,
                    (_, true, _, true) => HTBOTTOMRIGHT,
                    (true, _, _, _) => HTLEFT,
                    (_, true, _, _) => HTRIGHT,
                    (_, _, true, _) => HTTOP,
                    (_, _, _, true) => HTBOTTOM,
                    _ => {
                        if MOUSE.get() {
                            HTCLIENT
                        } else {
                            HTCAPTION
                        }
                    }
                };
                return LRESULT(hit as isize);
            }
            WM_LBUTTONUP if MOUSE.get() => emit(Action::Next),
            WM_RBUTTONUP if MOUSE.get() => emit(Action::Previous),
            WM_HOTKEY => emit(match w.0 {
                0 => Action::Previous,
                1 => Action::Next,
                2 => Action::Boss,
                3 => Action::Auto,
                _ => Action::Rescue,
            }),
            WM_TIMER if w.0 == 15 => emit(Action::AutoTick),
            WM_ENTERSIZEMOVE => {
                SIZING.set(true);
                SIZE_DIRTY.set(false);
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                LAST_SIZE.set(
                    ((rect.bottom - rect.top) as isize) << 16 | (rect.right - rect.left) as isize,
                );
                SetTimer(Some(hwnd), RESIZE_TIMER, 33, None);
                emit(Action::ResizeBegin(
                    Rect {
                        x: rect.left,
                        y: rect.top,
                        width: rect.right - rect.left,
                        height: rect.bottom - rect.top,
                    },
                    GetDpiForWindow(hwnd).max(96),
                ));
            }
            WM_SIZE if SIZING.get() => {
                if LAST_SIZE.replace(l.0) != l.0 {
                    SIZE_DIRTY.set(true);
                }
            }
            WM_TIMER if w.0 == RESIZE_TIMER => {
                if SIZE_DIRTY.replace(false) {
                    emit(Action::ResizeFrame);
                }
            }
            WM_GETMINMAXINFO => {
                let limits = &mut *(l.0 as *mut MINMAXINFO);
                let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.;
                let (width, height) = MINIMUM.get();
                limits.ptMinTrackSize.x = (width * scale).ceil() as i32;
                limits.ptMinTrackSize.y = (height * scale).ceil() as i32;
                return LRESULT(0);
            }
            WM_TIMER if w.0 == TOPMOST_TIMER => {
                maintain_topmost(hwnd);
            }
            WM_EXITSIZEMOVE => {
                SIZING.set(false);
                SIZE_DIRTY.set(false);
                let _ = KillTimer(Some(hwnd), RESIZE_TIMER);
                maintain_topmost(hwnd);
                emit(Action::ResizeEnd);
            }
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE => emit(Action::Topology),
            WM_DPICHANGED => {
                let r = &*(l.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                maintain_topmost(hwnd);
                emit(Action::Geometry);
            }
            TRAY => {
                if l.0 as u32 == WM_LBUTTONDBLCLK {
                    emit(Action::Control);
                }
                if l.0 as u32 == WM_RBUTTONUP
                    && let Ok(menu) = CreatePopupMenu()
                {
                    for (id, label) in TRAY_ITEMS {
                        let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(wide(label).as_ptr()));
                    }
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    let _ = SetForegroundWindow(hwnd);
                    let id = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_NONOTIFY,
                        point.x,
                        point.y,
                        None,
                        hwnd,
                        None,
                    )
                    .0;
                    let _ = DestroyMenu(menu);
                    match id {
                        1 => emit(Action::Control),
                        4 => emit(Action::Boss),
                        5 => emit(Action::Auto),
                        6 => emit(Action::Rescue),
                        8 => emit(Action::Quit),
                        _ => (),
                    }
                }
            }
            _ => (),
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
#[derive(Clone)]
pub struct Screen {
    pub name: String,
    pub id: String,
    pub area: Rect,
    pub primary: bool,
    pub scale: f32,
}
pub fn screens() -> Vec<Screen> {
    unsafe extern "system" fn enumerate(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        unsafe {
            let list = &mut *(data.0 as *mut Vec<Screen>);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(m, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
                let name = String::from_utf16_lossy(
                    &info.szDevice[..info.szDevice.iter().position(|x| *x == 0).unwrap_or(32)],
                );
                let r = info.monitorInfo.rcWork;
                let mut device = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                let id = if EnumDisplayDevicesW(
                    PCWSTR(info.szDevice.as_ptr()),
                    0,
                    &mut device,
                    EDD_GET_DEVICE_INTERFACE_NAME,
                )
                .as_bool()
                {
                    String::from_utf16_lossy(
                        &device.DeviceID
                            [..device.DeviceID.iter().position(|x| *x == 0).unwrap_or(128)],
                    )
                } else {
                    name.clone()
                };
                let (mut dx, mut dy) = (96, 96);
                let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
                list.push(Screen {
                    id,
                    name,
                    scale: dx as f32 / 96.,
                    primary: info.monitorInfo.dwFlags & 1 != 0,
                    area: Rect {
                        x: r.left,
                        y: r.top,
                        width: r.right - r.left,
                        height: r.bottom - r.top,
                    },
                });
            }
            BOOL(1)
        }
    }
    let mut list = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enumerate),
            LPARAM(&mut list as *mut _ as isize),
        );
    }
    list
}
pub fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}
pub struct TextEngine {
    factory: IDWriteFactory,
    d2d: ID2D1Factory,
    imaging: IWICImagingFactory,
    fonts: Option<IDWriteFontCollection>,
}
pub struct Page {
    pub end: usize,
    pub text: String,
}
impl Page {
    /// Paragraph separators at a page boundary must not create an empty footer line.
    pub fn display(&self, footer: &str) -> String {
        format!("{}{footer}", self.text.trim_matches(['\r', '\n']))
    }
}
impl TextEngine {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            Ok(Self {
                factory: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?,
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                imaging: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
                fonts: None,
            })
        }
    }

    pub fn load_fonts(&mut self, paths: &[String]) -> Result<Vec<String>> {
        unsafe {
            let factory: IDWriteFactory5 = self.factory.cast()?;
            let builder = factory.CreateFontSetBuilder()?;
            let factory3: IDWriteFactory3 = self.factory.cast()?;
            builder.AddFontSet(&factory3.GetSystemFontSet()?)?;
            for path in paths {
                let file = self
                    .factory
                    .CreateFontFileReference(PCWSTR(wide(path).as_ptr()), None)?;
                builder.AddFontFile(&file)?;
            }
            self.fonts = Some(
                factory3
                    .CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?
                    .cast()?,
            );
            let mut names = Vec::new();
            let collection = self.fonts.as_ref().unwrap();
            for i in 0..collection.GetFontFamilyCount() {
                let family = collection.GetFontFamily(i)?.GetFamilyNames()?;
                let len = family.GetStringLength(0)?;
                let mut buf = vec![0; len as usize + 1];
                family.GetString(0, &mut buf)?;
                names.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            names.sort();
            names.dedup();
            Ok(names)
        }
    }
    fn format(&self, style: &Style) -> Result<IDWriteTextFormat> {
        unsafe {
            let family = wide(&style.family);
            let format = self.factory.CreateTextFormat(
                PCWSTR(family.as_ptr()),
                self.fonts.as_ref(),
                DWRITE_FONT_WEIGHT(style.weight),
                if style.italic {
                    DWRITE_FONT_STYLE_ITALIC
                } else {
                    DWRITE_FONT_STYLE_NORMAL
                },
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                w!("zh-CN"),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            let mut line_height = style.size * 1.4;
            let mut baseline = style.size * 1.1;
            // Keep full glyph ascenders/descenders when users choose compact line spacing.
            if let Ok(collection) = format.GetFontCollection() {
                let mut index = 0;
                let mut exists = BOOL::default();
                if collection
                    .FindFamilyName(PCWSTR(family.as_ptr()), &mut index, &mut exists)
                    .is_ok()
                    && exists.as_bool()
                    && let Ok(font) = collection.GetFontFamily(index)?.GetFirstMatchingFont(
                        DWRITE_FONT_WEIGHT(style.weight),
                        DWRITE_FONT_STRETCH_NORMAL,
                        if style.italic {
                            DWRITE_FONT_STYLE_ITALIC
                        } else {
                            DWRITE_FONT_STYLE_NORMAL
                        },
                    )
                {
                    let mut metrics = DWRITE_FONT_METRICS::default();
                    font.GetMetrics(&mut metrics);
                    let scale = style.size / metrics.designUnitsPerEm.max(1) as f32;
                    let ascent = metrics.ascent as f32 * scale;
                    let glyph_height = (metrics.ascent as f32 + metrics.descent as f32) * scale;
                    let natural = glyph_height + (metrics.lineGap as f32 * scale).max(0.);
                    let normal_height = line_height.max(natural);
                    line_height = (normal_height * style.line_height).max(glyph_height);
                    baseline = (ascent + (normal_height - glyph_height) * 0.5)
                        .min(line_height - metrics.descent as f32 * scale);
                } else {
                    line_height *= style.line_height;
                }
            } else {
                line_height *= style.line_height;
            }
            format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, line_height, baseline)?;
            Ok(format)
        }
    }
    fn layout(
        &self,
        text: &str,
        style: &Style,
        format: &IDWriteTextFormat,
        width: f32,
        height: f32,
    ) -> Result<IDWriteTextLayout> {
        unsafe {
            let text: Vec<u16> = text.encode_utf16().collect();
            let layout =
                self.factory
                    .CreateTextLayout(&text, format, width.max(1.), height.max(1.))?;
            if let Ok(layout1) = layout.cast::<IDWriteTextLayout1>() {
                layout1.SetCharacterSpacing(
                    style.spacing,
                    0.,
                    0.,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: text.len() as u32,
                    },
                )?;
            }
            Ok(layout)
        }
    }
    /// Measure with the exact same format, wrapping and spacing as the rendered bitmap.
    pub fn text_fits(&self, text: &str, style: &Style, width: f32, height: f32) -> bool {
        let Ok(format) = self.format(style) else {
            return false;
        };
        self.fits_format(text, style, &format, width, height)
    }
    fn fits_format(
        &self,
        text: &str,
        style: &Style,
        format: &IDWriteTextFormat,
        width: f32,
        height: f32,
    ) -> bool {
        let Ok(layout) = self.layout(text, style, format, width, height) else {
            return false;
        };
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            let Ok(overhang) = layout.GetOverhangMetrics() else {
                return false;
            };
            layout.GetMetrics(&mut metrics).is_ok()
                && overhang.top <= 0.05
                && overhang.bottom <= 0.05
                && metrics.widthIncludingTrailingWhitespace <= width + 0.05
        }
    }
    pub fn minimum_viewport(&self, style: &Style, footer: &str) -> (f32, f32) {
        let fallback = (100., 24_f32.max(style.size * style.line_height * 1.6));
        let Ok(format) = self.format(style) else {
            return fallback;
        };
        let Ok(layout) = self.layout(&format!("国{footer}"), style, &format, 100000., 100000.)
        else {
            return fallback;
        };
        let mut metrics = DWRITE_TEXT_METRICS::default();
        if unsafe { layout.GetMetrics(&mut metrics) }.is_err() {
            return fallback;
        }
        (
            100_f32.max(metrics.widthIncludingTrailingWhitespace.ceil() + 20.),
            24_f32.max(metrics.height.ceil()),
        )
    }
    pub fn progress_reservation(&self, style: &Style) -> String {
        if !style.page_numbers {
            return String::new();
        }
        let Ok(format) = self.format(style) else {
            return "  本章 100.00%".into();
        };
        let mut widest = '8';
        let mut max_width = 0.;
        for digit in '0'..='9' {
            if let Ok(layout) = self.layout(&digit.to_string(), style, &format, 1000., 1000.) {
                let mut metrics = DWRITE_TEXT_METRICS::default();
                if unsafe { layout.GetMetrics(&mut metrics) }.is_ok()
                    && metrics.widthIncludingTrailingWhitespace > max_width
                {
                    max_width = metrics.widthIncludingTrailingWhitespace;
                    widest = digit;
                }
            }
        }
        ["本章", "全书"]
            .map(|label| format!("  {label} {widest}{widest}{widest}.{widest}{widest}%"))
            .into_iter()
            .max_by(|a, b| {
                let width = |text: &str| {
                    let mut m = DWRITE_TEXT_METRICS::default();
                    self.layout(text, style, &format, 10000., 10000.)
                        .and_then(|layout| unsafe { layout.GetMetrics(&mut m) })
                        .ok();
                    m.widthIncludingTrailingWhitespace
                };
                width(a).total_cmp(&width(b))
            })
            .unwrap_or_default()
    }
    /// Only inspect the suffix needed to fill one screen before an arbitrary anchor.
    pub fn previous_page(
        &self,
        text: &str,
        end: usize,
        style: &Style,
        width: f32,
        height: f32,
    ) -> Page {
        use unicode_segmentation::UnicodeSegmentation;
        let end = book::boundary(text, end);
        let footer = self.progress_reservation(style);
        let mut starts = Vec::new();
        let mut limit = 64;
        let mut graphemes = text[..end].grapheme_indices(true).rev();
        let fits = |start| {
            self.text_fits(
                &format!("{}{footer}", text[start..end].trim_matches(['\r', '\n'])),
                style,
                (width - 20.).max(1.),
                height,
            )
        };
        loop {
            let mut exhausted = false;
            while starts.len() < limit {
                if let Some((start, _)) = graphemes.next() {
                    starts.push(start);
                } else {
                    exhausted = true;
                    break;
                }
            }
            if exhausted || starts.last().is_some_and(|start| !fits(*start)) {
                break;
            }
            limit = limit.saturating_mul(2);
        }
        let mut low = 0;
        let mut high = starts.len();
        while low < high {
            let mid = (low + high).div_ceil(2);
            if fits(starts[mid - 1]) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let start = if starts.is_empty() {
            end
        } else {
            starts[low.max(1) - 1]
        };
        Page {
            end,
            text: text[start..end].into(),
        }
    }
    pub fn footer_reservation(&self, style: &Style, count: usize) -> String {
        if !style.page_numbers {
            return String::new();
        }
        let mut widest = '8';
        let mut max_width = 0.;
        if let Ok(format) = self.format(style) {
            for digit in '0'..='9' {
                if let Ok(layout) = self.layout(&digit.to_string(), style, &format, 1000., 1000.) {
                    let mut metrics = DWRITE_TEXT_METRICS::default();
                    if unsafe { layout.GetMetrics(&mut metrics) }.is_ok()
                        && metrics.widthIncludingTrailingWhitespace > max_width
                    {
                        max_width = metrics.widthIncludingTrailingWhitespace;
                        widest = digit;
                    }
                }
            }
        }
        let digits = widest.to_string().repeat(count.max(1).to_string().len());
        let number = format!("  {digits}/{digits}");
        [
            number,
            "  （页码计算中...）".into(),
            "  （页码不可用）".into(),
        ]
        .into_iter()
        .max_by(|a, b| {
            let width = |text: &str| {
                let mut m = DWRITE_TEXT_METRICS::default();
                self.format(style)
                    .and_then(|f| self.layout(text, style, &f, 100000., 100000.))
                    .and_then(|l| unsafe { l.GetMetrics(&mut m) })
                    .ok();
                m.widthIncludingTrailingWhitespace
            };
            width(a).total_cmp(&width(b))
        })
        .unwrap_or_default()
    }
    pub fn page(&self, text: &str, start: usize, style: &Style, width: f32, height: f32) -> Page {
        let footer = self.footer_reservation(style, 1);
        self.page_with_footer(text, start, style, width, height, &footer)
    }
    pub fn page_with_footer(
        &self,
        text: &str,
        start: usize,
        style: &Style,
        width: f32,
        height: f32,
        footer: &str,
    ) -> Page {
        use unicode_segmentation::UnicodeSegmentation;
        let start = book::boundary(text, start);
        if start >= text.len() {
            return Page {
                end: text.len(),
                text: String::new(),
            };
        }
        let format = self.format(style).ok();
        let fits = |body: &str| {
            let Some(format) = format.as_ref() else {
                return false;
            };
            let display = format!("{}{footer}", body.trim_matches(['\r', '\n']));
            self.fits_format(
                &display,
                style,
                format,
                (width - 20.).max(1.),
                height.max(1.),
            )
        };
        let mut graphemes = text[start..].grapheme_indices(true);
        let mut rendered = String::new();
        let mut ends = Vec::new();
        let mut limit = 64usize;
        loop {
            let mut exhausted = false;
            while ends.len() < limit {
                let Some((index, grapheme)) = graphemes.next() else {
                    exhausted = true;
                    break;
                };
                rendered.push_str(grapheme);
                ends.push((rendered.len(), start + index + grapheme.len()));
            }
            if exhausted || !fits(&rendered) {
                break;
            }
            limit = limit.saturating_mul(2);
        }
        let mut low = 0;
        let mut high = ends.len();
        while low < high {
            let mid = (low + high).div_ceil(2);
            if fits(&rendered[..ends[mid - 1].0]) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        // A viewport smaller than one glyph must still advance the source anchor.
        let n = low.max(1);
        Page {
            end: ends[n - 1].1,
            text: rendered[..ends[n - 1].0].to_owned(),
        }
    }
    pub fn paginate(
        &self,
        text: &str,
        style: &Style,
        width: f32,
        height: f32,
        cancelled: impl Fn() -> bool,
    ) -> std::result::Result<Vec<usize>, String> {
        let mut footer = self.footer_reservation(style, 1);
        loop {
            let mut pages = Vec::new();
            let mut position = 0;
            while position < text.len() {
                if cancelled() {
                    return Err("cancelled".into());
                }
                pages.push(position);
                let next = self
                    .page_with_footer(text, position, style, width, height, &footer)
                    .end;
                if next <= position {
                    return Err("分页未前进".into());
                }
                position = next;
            }
            let required = self.footer_reservation(style, pages.len());
            if required == footer {
                return Ok(pages);
            }
            footer = required;
        }
    }
    pub fn render(
        &self,
        text: &str,
        style: &Style,
        width: u32,
        height: u32,
        dpi: f32,
    ) -> Result<Vec<u8>> {
        self.render_aligned(text, style, width, height, dpi, false)
    }
    pub fn render_aligned(
        &self,
        text: &str,
        style: &Style,
        width: u32,
        height: u32,
        dpi: f32,
        center: bool,
    ) -> Result<Vec<u8>> {
        unsafe {
            let bitmap = self.imaging.CreateBitmap(
                width,
                height,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnLoad,
            )?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96. * dpi,
                dpiY: 96. * dpi,
                ..Default::default()
            };
            let target = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &props)?;
            target.BeginDraw();
            target.Clear(Some(&color(style.background)));
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = target.CreateSolidColorBrush(&color(style.foreground), None)?;
            let format = self.format(style)?;
            if center {
                format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
                format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            }
            let layout = self.layout(
                text,
                style,
                &format,
                width as f32 / dpi - 20.,
                height as f32 / dpi,
            )?;
            target.DrawTextLayout(
                windows_numerics::Vector2 { X: 10., Y: 0. },
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
            target.EndDraw(None, None)?;
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
            Ok(pixels)
        }
    }
}
fn color(argb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        a: ((argb >> 24) & 255) as f32 / 255.,
        r: ((argb >> 16) & 255) as f32 / 255.,
        g: ((argb >> 8) & 255) as f32 / 255.,
        b: (argb & 255) as f32 / 255.,
    }
}

pub struct InstanceGuard(HANDLE);
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn single_instance() -> Result<Option<InstanceGuard>> {
    unsafe {
        let key = blake3::hash(crate::settings::data_dir().to_string_lossy().as_bytes()).to_hex();
        let name = wide(&format!("Local\\ThiefReader.{key}"));
        let mutex =
            windows::Win32::System::Threading::CreateMutexW(None, false, PCWSTR(name.as_ptr()))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(hwnd) = FindWindowW(w!("ThiefReaderLayer"), None) {
                let _ = PostMessageW(
                    Some(hwnd),
                    TRAY,
                    WPARAM(0),
                    LPARAM(WM_LBUTTONDBLCLK as isize),
                );
            }
            let _ = CloseHandle(mutex);
            return Ok(None);
        }
        Ok(Some(InstanceGuard(mutex)))
    }
}

#[cfg(test)]
mod resize_messages {
    use super::*;
    use std::sync::Mutex;
    static EVENTS: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    #[test]
    fn coalesces_size_messages_without_visibility_or_activation() {
        on_event(|e| {
            let v = match e {
                Action::ResizeBegin(..) => 1,
                Action::ResizeFrame => 2,
                Action::ResizeEnd => 3,
                _ => 0,
            };
            if v != 0 {
                EVENTS.lock().unwrap().push(v);
            }
        });
        unsafe {
            let foreground = GetForegroundWindow();
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("LoafingResizeOffscreenTest");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class,
                w!("offscreen"),
                WS_POPUP,
                -32000,
                -32000,
                120,
                60,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            SendMessageW(hwnd, WM_ENTERSIZEMOVE, None, None);
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                180,
                80,
                SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .unwrap();
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                200,
                90,
                SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .unwrap();
            SendMessageW(hwnd, WM_TIMER, Some(WPARAM(RESIZE_TIMER)), None);
            SendMessageW(hwnd, WM_TIMER, Some(WPARAM(RESIZE_TIMER)), None);
            SendMessageW(hwnd, WM_EXITSIZEMOVE, None, None);
            assert_eq!(*EVENTS.lock().unwrap(), vec![1, 2, 3]);
            EVENTS.lock().unwrap().clear();
            SendMessageW(hwnd, WM_ENTERSIZEMOVE, None, None);
            SetWindowPos(
                hwnd,
                None,
                -31999,
                -31999,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .unwrap();
            SendMessageW(hwnd, WM_TIMER, Some(WPARAM(RESIZE_TIMER)), None);
            SendMessageW(hwnd, WM_EXITSIZEMOVE, None, None);
            assert_eq!(*EVENTS.lock().unwrap(), vec![1, 3]);
            assert!(!IsWindowVisible(hwnd).as_bool());
            assert_eq!(GetForegroundWindow(), foreground);
            DestroyWindow(hwnd).unwrap();
        }
    }
}
