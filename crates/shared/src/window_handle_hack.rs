//! This is a way for the app to look up its own window handle during startup on windows.
//! It's a workaround for iced not yet providing a way to access the window handle.
//! The hwnd is necessary on windows for supporting os media controls / keys.

use std::sync::OnceLock;

use log::{error, trace};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

use super::WINDOW_TITLE;

static WINDOW_HANDLE: OnceLock<HWND> = OnceLock::new();

#[allow(unsafe_code)]
pub fn set_hwnd() {
    let window_hstring = HSTRING::from(WINDOW_TITLE);
    let window_pcwstr = PCWSTR(window_hstring.as_ptr());
    let window: HWND = unsafe { FindWindowW(None, window_pcwstr) };

    if window.0 == 0 {
        error!("invalid hwnd for window");
        return;
    }

    trace!("setting hwnd: {window:?}");

    WINDOW_HANDLE.set(window).ok();
}

pub fn get_hwnd() -> Option<HWND> {
    WINDOW_HANDLE.get().copied()
}
