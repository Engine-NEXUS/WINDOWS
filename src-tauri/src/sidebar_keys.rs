//! Esc and the arrow keys for the sidebar while ANOTHER app has focus.
//!
//! A window only hears keys while it is focused, so the sidebar's Left/Right (dock), Up/Down (front/back) and Esc
//! (close) used to need a click first. While the sidebar is open and not focused, these keys are registered as global
//! shortcuts and forwarded to the sidebar as `sidebar:key`. The moment the sidebar itself gets focus they are released
//! again (so typing in its search box is untouched), and they are released while the sidebar is sent behind every
//! window (then the owner is working elsewhere and needs their own Esc and arrows).
//!
//! Esc is left alone while a Ghost session is running: Ghost already owns the global Esc as its cancel button.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Runtime, WebviewWindow};

/// Keys forwarded to the sidebar (the event payload is the DOM `KeyboardEvent.key` value).
pub(crate) const KEYS: &[&str] = &["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Escape"];

static HOOKED: AtomicBool = AtomicBool::new(false);
static REGISTERED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// Pure: which keys may be armed right now. Esc is skipped while Ghost owns it; nothing is armed while the sidebar is
/// focused or sent behind the other windows.
pub(crate) fn keys_to_arm(sidebar_focused: bool, sent_back: bool, ghost_active: bool) -> Vec<&'static str> {
    if sidebar_focused || sent_back {
        return Vec::new();
    }
    KEYS.iter().copied().filter(|k| !(ghost_active && *k == "Escape")).collect()
}

/// Called whenever the sidebar is shown: hooks its focus changes once per window and arms the keys if it is not focused.
pub(crate) fn attach<R: Runtime>(app: &AppHandle<R>, win: &WebviewWindow<R>) {
    #[cfg(not(target_os = "linux"))]
    {
        if !HOOKED.swap(true, Ordering::SeqCst) {
            let a = app.clone();
            win.on_window_event(move |ev| match ev {
                tauri::WindowEvent::Focused(true) => disarm(&a),
                tauri::WindowEvent::Focused(false) => refresh(&a, false),
                tauri::WindowEvent::Destroyed => {
                    disarm(&a);
                    HOOKED.store(false, Ordering::SeqCst);
                }
                _ => {}
            });
        }
        refresh(app, win.is_focused().unwrap_or(false));
    }
    #[cfg(target_os = "linux")]
    let _ = (app, win);
}

/// Re-evaluate: arm what may be armed, release the rest.
#[cfg(not(target_os = "linux"))]
pub(crate) fn refresh<R: Runtime>(app: &AppHandle<R>, sidebar_focused: bool) {
    let want = keys_to_arm(
        sidebar_focused,
        crate::commands::sidebar_sent_back(),
        crate::ghost::session_active(),
    );
    if want.is_empty() {
        disarm(app);
    } else {
        arm(app, &want);
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn refresh<R: Runtime>(_app: &AppHandle<R>, _sidebar_focused: bool) {}

#[cfg(not(target_os = "linux"))]
fn arm<R: Runtime>(app: &AppHandle<R>, keys: &[&'static str]) {
    use tauri::Emitter;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
    let Ok(mut reg) = REGISTERED.lock() else { return };
    for &key in keys {
        if reg.contains(&key) {
            continue;
        }
        let Ok(sc) = key.parse::<Shortcut>() else { continue };
        let handle = app.clone();
        let res = app.global_shortcut().on_shortcut(sc, move |_a, _s, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = handle.emit_to("sidebar", "sidebar:key", key);
            }
        });
        match res {
            Ok(()) => reg.push(key),
            Err(e) => tracing::debug!("sidebar keys: could not arm {key}: {e}"),
        }
    }
}

/// Release every key this module armed (Esc stays registered if Ghost took it over meanwhile).
pub(crate) fn disarm<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(not(target_os = "linux"))]
    {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let Ok(mut reg) = REGISTERED.lock() else { return };
        for key in reg.drain(..) {
            if key == "Escape" && crate::ghost::session_active() {
                continue;
            }
            let _ = app.global_shortcut().unregister(key);
        }
    }
    #[cfg(target_os = "linux")]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_armed_only_while_the_sidebar_is_open_unfocused_and_in_front() {
        assert_eq!(keys_to_arm(false, false, false), KEYS.to_vec());
        // focused: the window hears its own keys, nothing is stolen
        assert!(keys_to_arm(true, false, false).is_empty());
        // sent behind every window: the owner works elsewhere and keeps their own Esc and arrows
        assert!(keys_to_arm(false, true, false).is_empty());
    }

    #[test]
    fn esc_is_left_to_ghost_while_a_ghost_session_runs() {
        let k = keys_to_arm(false, false, true);
        assert!(!k.contains(&"Escape"));
        assert!(k.contains(&"ArrowLeft") && k.contains(&"ArrowDown"));
    }
}
