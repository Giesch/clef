//! This is a way for the app to look up its own window handle during startup on windows.
//! It's a workaround for iced not yet providing a way to access the window handle.
//! The hwnd is necessary on windows for supporting os media controls / keys.

use std::sync::Mutex;

use log::trace;
use once_cell::sync::Lazy;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::FindWindowA;

static WINDOW_HANDLE: Lazy<Mutex<Option<HWND>>> = Lazy::new(|| Mutex::new(None));

#[allow(unsafe_code)]
pub fn set_hwnd() {
    // NOTE this must match the WINDOW_TITLE in app.rs
    // The windows::s macro requires a literal and not a const
    let window_name = windows::s!("Clef");

    let clef_window: HWND = unsafe { FindWindowA(None, window_name) };

    if clef_window.0 == 0 {
        // this can only happen if the function is incorrectly called
        // before the ui window is opened
        return;
    }

    trace!("setting hwnd: {clef_window:?}");

    let mut handle = WINDOW_HANDLE.lock().unwrap();
    *handle = Some(clef_window);
}

pub fn get_hwnd() -> Option<HWND> {
    *WINDOW_HANDLE.lock().unwrap()
}
