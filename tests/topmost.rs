#![cfg(windows)]
use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::w,
};
struct TestWindow(HWND);
impl TestWindow {
    fn new(topmost: bool) -> Self {
        unsafe {
            let ex = WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE
                | if topmost {
                    WS_EX_TOPMOST
                } else {
                    WINDOW_EX_STYLE(0)
                };
            let hwnd = CreateWindowExW(
                ex,
                w!("STATIC"),
                w!("LoafingReader offscreen z-order test"),
                WS_POPUP,
                -32000,
                -32000,
                2,
                2,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            SetWindowPos(
                hwnd,
                Some(if topmost { HWND_TOPMOST } else { HWND_TOP }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .unwrap();
            Self(hwnd)
        }
    }
}
impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
#[test]
fn overlapping_topmost_order_recovers_without_activation_or_revealing_hidden_window() {
    unsafe {
        let reader = TestWindow::new(false);
        let blocker = TestWindow::new(true);
        assert!(thief_reader::native::maintain_topmost(reader.0));
        assert_ne!(
            GetWindowLongPtrW(reader.0, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize,
            0
        );
        let mut cursor = GetWindow(blocker.0, GW_HWNDPREV).ok();
        let mut reader_above = false;
        while let Some(hwnd) = cursor {
            if hwnd == reader.0 {
                reader_above = true;
                break;
            }
            cursor = GetWindow(hwnd, GW_HWNDPREV).ok();
        }
        assert!(reader_above);
        assert!(!thief_reader::native::maintain_topmost(reader.0));
        assert_ne!(GetForegroundWindow(), reader.0);
        assert_ne!(GetForegroundWindow(), blocker.0);
        let mut before = RECT::default();
        GetWindowRect(reader.0, &mut before).unwrap();
        let _ = ShowWindow(reader.0, SW_HIDE);
        assert!(!thief_reader::native::maintain_topmost(reader.0));
        assert!(!IsWindowVisible(reader.0).as_bool());
        let mut after = RECT::default();
        GetWindowRect(reader.0, &mut after).unwrap();
        assert_eq!(before, after);
    }
}
