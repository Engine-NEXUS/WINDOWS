//! Opens the page being scanned in its OWN browser window, so the owner's window is never moved, resized or switched
//! to full screen: the new window is sized flush beside the NEXUS sidebar and the scan runs against it.
//!
//! The window is found by exe + a handle that did not exist before the spawn (not by pid: Chromium browsers hand a
//! `--new-window` request to the instance that is already running and the launched process exits).

use crate::window_tile::TopWin;

/// The first window of `exe` that was not there before. `now` is topmost-first, so a freshly created window wins.
pub(crate) fn pick_new_window(before: &[isize], now: &[TopWin], exe: &str) -> Option<isize> {
    let exe = exe.to_lowercase();
    now.iter()
        .find(|w| w.exe.to_lowercase() == exe && !before.contains(&w.hwnd))
        .map(|w| w.hwnd)
}

/// Arguments that open `url` in a new window of a Chromium-family browser.
pub(crate) fn new_window_args(url: &str) -> Vec<String> {
    vec!["--new-window".to_string(), url.to_string()]
}

/// Open `url` in a new window of the browser at `exe_path` and return that window's handle, waiting up to ~7 s.
#[cfg(target_os = "windows")]
pub(crate) async fn open_new_window(exe_path: &std::path::Path, exe_name: &str, url: &str) -> Option<isize> {
    let before: Vec<isize> = tokio::task::spawn_blocking(crate::window_tile::list_top_windows)
        .await
        .ok()?
        .into_iter()
        .map(|w| w.hwnd)
        .collect();
    crate::no_window::command(exe_path).args(new_window_args(url)).spawn().ok()?;
    for _ in 0..18 {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let now = tokio::task::spawn_blocking(crate::window_tile::list_top_windows).await.ok()?;
        if let Some(h) = pick_new_window(&before, &now, exe_name) {
            return Some(h);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(hwnd: isize, exe: &str) -> TopWin {
        TopWin { hwnd, pid: 1, exe: exe.into(), class: "Chrome_WidgetWin_1".into(), title: "t".into() }
    }

    #[test]
    fn the_new_window_is_the_one_that_was_not_there_before() {
        let now = vec![win(30, "brave.exe"), win(20, "notepad.exe"), win(10, "brave.exe")];
        assert_eq!(pick_new_window(&[10, 20], &now, "brave.exe"), Some(30));
        assert_eq!(pick_new_window(&[10, 20, 30], &now, "brave.exe"), None);
        // another program's new window is never taken, and exe case does not matter
        assert_eq!(pick_new_window(&[10], &[win(40, "notepad.exe")], "brave.exe"), None);
        assert_eq!(pick_new_window(&[], &[win(5, "Brave.EXE")], "brave.exe"), Some(5));
    }

    #[test]
    fn the_scan_never_maximises_or_restores_a_window() {
        // The old restore step re-maximised a window that had been maximised: the "it went full screen again" bug.
        let src = include_str!("reverse_engineer.rs");
        assert!(!src.contains(&format!("SW_{}", "MAXIMIZE")), "reverse engineer must never maximise a window");
        assert!(!src.contains(&format!("SW_{}", "RESTORE")), "reverse engineer must never restore a window");
        assert!(!src.contains("set_fullscreen"));
    }

    #[test]
    fn asks_the_browser_for_a_new_window() {
        assert_eq!(new_window_args("https://a.test/x"), vec!["--new-window", "https://a.test/x"]);
    }
}
