//! Window focus management — bring an app to the foreground.
//!
//! On Windows, `SetForegroundWindow` silently fails when called from a
//! background process (Windows' foreground lock). The workaround is the
//! `AttachThreadInput` trick (from ghost-hands window.rs):
//!
//!   1. Get the foreground window's thread ID
//!   2. Attach our input queue to that thread
//!   3. Call SetForegroundWindow (now succeeds because we share input state)
//!   4. Detach the input queue
//!
//! This is the same pattern used by ghost-hands, nuphus-mcp, and many
//! other Windows automation tools.

/// Pure UIPI gate: Some(reason) when a click must not be attempted —
/// target elevated, sender not. Returned verbatim as the spoken error
/// (reroute, never a blind retry into the void). Shared by all platforms.
pub fn elevated_block_reason(target_elevated: bool, self_elevated: bool) -> Option<String> {
    if target_elevated && !self_elevated {
        Some(
            "that's an admin window, sir — Windows blocks my clicks there. \
             Click it yourself, or restart NEXUS as admin."
                .to_string(),
        )
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::*;

#[cfg(not(target_os = "windows"))]
pub use unix_impl::*;

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::sync::Mutex;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow, SW_MAXIMIZE,
        SW_MINIMIZE, SW_RESTORE,
    };

    // Thread-local storage for the search target and result.
    // EnumWindows requires a C callback, so we can't capture closures.
    static SEARCH_TARGET: Mutex<Option<String>> = Mutex::new(None);
    static FOUND_HWND: Mutex<Option<HWND>> = Mutex::new(None);

    unsafe extern "system" fn enum_proc(hwnd: HWND, _lparam: LPARAM) -> BOOL {
        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title);
        if len <= 0 {
            return BOOL(1); // continue enumeration
        }
        let title_str = String::from_utf16_lossy(&title[..len as usize]);
        let lower = title_str.to_lowercase();

        let target = SEARCH_TARGET.lock().unwrap();
        if let Some(search) = target.as_ref() {
            if lower.contains(&search.to_lowercase()) {
                drop(target); // release lock before acquiring FOUND_HWND
                *FOUND_HWND.lock().unwrap() = Some(hwnd);
                return BOOL(0); // stop enumeration
            }
        }
        BOOL(1) // continue
    }

    /// Find a window whose title contains `partial_title` (case-insensitive).
    pub fn find_window(partial_title: &str) -> Option<HWND> {
        *SEARCH_TARGET.lock().unwrap() = Some(partial_title.to_string());
        *FOUND_HWND.lock().unwrap() = None;

        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(0));
        }

        FOUND_HWND.lock().unwrap().take()
    }

    /// Bring a window to the foreground using the AttachThreadInput trick.
    /// Returns true if the window is now in the foreground.
    pub fn focus_window(hwnd: HWND) -> bool {
        unsafe {
            // Restore if minimized
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }

            // AttachThreadInput trick: attach our input queue to the
            // foreground thread's so SetForegroundWindow succeeds.
            let fg = GetForegroundWindow();
            let mut fg_pid = 0u32;
            let fg_thread = GetWindowThreadProcessId(fg, &mut fg_pid as *mut u32);
            let this_thread = GetCurrentThreadId();

            let attached = fg_thread != 0
                && fg_thread != this_thread
                && AttachThreadInput(this_thread, fg_thread, true).as_bool();

            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);

            if attached {
                let _ = AttachThreadInput(this_thread, fg_thread, false);
            }

            // Verify success
            GetForegroundWindow() == hwnd
        }
    }

    /// Find a window by partial title and bring it to the foreground.
    /// Returns true if the window was found and focused.
    pub fn focus_app_by_title(partial_title: &str) -> bool {
        if let Some(hwnd) = find_window(partial_title) {
            focus_window(hwnd)
        } else {
            false
        }
    }

    /// Minimize the current foreground window. `ShowWindow`'s return value
    /// reports the window's PRIOR visibility, not call success, so it's
    /// discarded (same convention as the `SW_RESTORE` call above) — this
    /// returns whether a foreground window existed to act on at all.
    pub fn minimize_foreground_window() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0 == 0 {
                return false;
            }
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
            true
        }
    }

    /// Maximize the current foreground window. Same fire-and-forget
    /// convention as `minimize_foreground_window`.
    pub fn maximize_foreground_window() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0 == 0 {
                return false;
            }
            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            true
        }
    }

    /// True if OUR process runs elevated (admin). Clicks from a
    /// non-elevated sender into an elevated window are silently eaten by
    /// UIPI — detect first, reroute with speech instead.
    pub fn our_process_elevated() -> bool {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        unsafe {
            let mut token: HANDLE = HANDLE(0);
            if !OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).as_bool() {
                return false;
            }
            let mut elev: TOKEN_ELEVATION = std::mem::zeroed();
            let mut ret_len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elev as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            )
            .as_bool();
            ok && elev.TokenIsElevated != 0
        }
    }

    fn process_token_elevated(proc: windows::Win32::Foundation::HANDLE) -> bool {
        use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
        use windows::Win32::System::Threading::OpenProcessToken;
        unsafe {
            let mut token: windows::Win32::Foundation::HANDLE =
                windows::Win32::Foundation::HANDLE(0);
            if !OpenProcessToken(proc, TOKEN_QUERY, &mut token).as_bool() {
                return false;
            }
            let mut elev: TOKEN_ELEVATION = std::mem::zeroed();
            let mut ret_len = 0u32;
            GetTokenInformation(
                token,
                TokenElevation,
                &mut elev as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            )
            .as_bool()
                && elev.TokenIsElevated != 0
        }
    }

    /// True if the process owning `hwnd` runs elevated (admin).
    pub fn window_process_elevated(hwnd: HWND) -> bool {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid as *mut u32);
            if pid == 0 {
                return false;
            }
            let Ok(proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let elevated = process_token_elevated(proc);
            let _ = CloseHandle(proc);
            elevated
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod unix_impl {
    /// On Unix, focus is managed by the window manager.
    /// For now, we just return true (no-op).
    /// A future implementation could use xdotool or wmctrl.
    pub fn focus_app_by_title(_partial_title: &str) -> bool {
        tracing::warn!("live: window focus not implemented on this platform");
        true
    }

    /// Not implemented on this platform — always reports "nothing to act on".
    pub fn minimize_foreground_window() -> bool {
        false
    }

    /// Not implemented on this platform — always reports "nothing to act on".
    pub fn maximize_foreground_window() -> bool {
        false
    }

    /// No UAC/UIPI concept — never elevated-blocked.
    pub fn our_process_elevated() -> bool {
        false
    }

    /// No UAC/UIPI concept — never elevated-blocked.
    pub fn window_process_elevated(_hwnd: ()) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::elevated_block_reason;

    #[test]
    fn test_elevated_gate_truth_table() {
        // Unelevated sender vs elevated target: blocked with guidance.
        let reason = elevated_block_reason(true, false);
        assert!(reason.is_some());
        let text = reason.unwrap().to_lowercase();
        assert!(text.contains("admin"));
        // All other combinations: no block.
        assert!(elevated_block_reason(false, false).is_none());
        assert!(elevated_block_reason(true, true).is_none());
        assert!(elevated_block_reason(false, true).is_none());
    }

    #[test]
    fn test_own_elevation_check_runs() {
        // Must never panic; value depends on how the test runner launched.
        let _ = super::our_process_elevated();
    }
}
