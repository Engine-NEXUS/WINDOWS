//! DWM window corner rounding (Windows 11 22000+).
//!
//! Tauri/tao create frameless (`decorations: false`) windows as plain
//! rectangles — Windows does NOT auto-round these at the OS level (that
//! only happens for windows with a standard caption/frame). Meanwhile our
//! CSS gives the in-page card a `border-radius`. WebView2 cannot clip its
//! own surface to match that CSS radius (no `CornerRadius` support), so
//! without this fix the native DWM blur paints a sharp-cornered rectangle
//! behind a rounded CSS card, producing a mismatched "double panel" look
//! at the edges/corners.
//!
//! `DWMWA_WINDOW_CORNER_PREFERENCE` (attribute 33, Windows 11 build 22000+)
//! tells DWM to round the actual window rectangle so it matches. We call
//! `DwmSetWindowAttribute` directly via a minimal FFI binding rather than
//! going through the pinned `windows` crate version, since the attribute
//! is a plain `i32` and this avoids any type-shape mismatch across
//! `windows`-crate versions.
//!
//! Safe no-op on Windows 10 and earlier: `DwmSetWindowAttribute` returns a
//! failure HRESULT for an attribute the OS doesn't recognize, which we
//! ignore.

use std::ffi::c_void;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[allow(non_snake_case)]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: isize,
        dwAttribute: u32,
        pvAttribute: *const c_void,
        cbAttribute: u32,
    ) -> i32;
}

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: i32 = 2;
const DWMWCP_DONOTROUND: i32 = 1;
const DWMWA_BORDER_COLOR: u32 = 34;
/// DWMWA_COLOR_NONE: draw no border at all.
const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

/// For a transparent window that draws its own rounded shape (the Agent Island): no OS rounding and no OS border.
/// Windows 11 outlines every rounded window with a thin border; on a window much bigger than what it shows that
/// reads as an empty frame around the bar.
pub fn no_frame<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32_handle) = handle.as_raw() else {
        return;
    };
    let hwnd = win32_handle.hwnd.get() as isize;
    // SAFETY: both attributes are plain 32-bit values; an OS that does not know them returns a failure code we ignore.
    unsafe {
        let pref: i32 = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const i32 as *const c_void, 4);
        let none: u32 = DWMWA_COLOR_NONE;
        let hr = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &none as *const u32 as *const c_void, 4);
        tracing::info!("agent island: window frame removed (hr={hr:#x})");
    }
}

/// Rounds the given Tauri window's corners to match the CSS card's
/// `border-radius`, so the native DWM-painted window rectangle and the
/// in-page rounded card align instead of showing a mismatched double
/// outline.
pub fn round_corners<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32_handle) = handle.as_raw() else {
        return;
    };
    let hwnd = win32_handle.hwnd.get() as isize;

    unsafe {
        let pref: i32 = DWMWCP_ROUND;
        let hr = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        if hr == 0 {
            tracing::info!("sidebar: DWM window corner rounding applied");
        } else {
            // Non-zero HRESULT: likely pre-Win11 22000, where this
            // attribute doesn't exist. Harmless — CSS radius alone still
            // applies to the in-page content.
            tracing::debug!("sidebar: DWM corner rounding unavailable (hr={hr:#x}), likely pre-Win11");
        }
    }
}
