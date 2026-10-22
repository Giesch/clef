//! This is a way for the app to look up its own window handle during startup on windows.
//! It's a workaround for iced not yet providing a way to access the window handle.
//! The hwnd is necessary on windows for supporting os media controls / keys.
//!
//! NOTE This is brittle, and should be replaced with whatever way to expose
//! the handle iced decides on.
//! https://github.com/iced-rs/iced/issues/576

use std::sync::OnceLock;

use log::{error, trace};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

use super::window_handle_conversion::window_handle_token;
use super::WINDOW_TITLE;

static WINDOW_HANDLE: OnceLock<usize> = OnceLock::new();

#[allow(unsafe_code)]
pub fn set_hwnd() {
    let window_hstring = HSTRING::from(WINDOW_TITLE);
    let window_pcwstr = PCWSTR(window_hstring.as_ptr());
    let result = unsafe { FindWindowW(None, window_pcwstr) };
    let Some(window) = window_handle_token(result.map(|handle| handle.0)) else {
        error!("invalid hwnd for window");
        return;
    };

    trace!("setting hwnd: {window:?}");

    WINDOW_HANDLE.set(window).ok();
}

/// Returns the cached window handle as an integer token, not a shared raw pointer.
pub fn get_hwnd() -> Option<usize> {
    WINDOW_HANDLE.get().copied()
}
