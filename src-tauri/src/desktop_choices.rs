//! Ephemeral, single-use clarification bound to an already authorized Ghost task.
use crate::desktop_control::{Candidate, ChoiceSlot, ChoiceView, PendingChoice, Verification};
use crate::ghost_center::{GhostStep, GhostTask, Outcome, Receipt, Route};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Manager, Runtime};
struct Pending {
    slot: ChoiceSlot,
    cancel: Arc<AtomicBool>,
    remaining: Vec<GhostStep>,
    store: Option<PathBuf>,
    lock_on: Option<bool>,
}
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static SIDEBAR_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static ACTIVE: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
fn finish_active(cancel: &Arc<AtomicBool>) {
    if let Ok(mut a) = ACTIVE.lock() {
        if a.as_ref().is_some_and(|v| Arc::ptr_eq(v, cancel)) {
            *a = None
        }
    }
}
fn now() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}
pub fn clear() {
    if let Ok(mut p) = PENDING.lock() {
        if let Some(p) = p.take() {
            p.cancel.store(true, Ordering::Relaxed)
        }
    }
    cancel_running();
}
/// A new capture may answer a waiting clarification. Explicit stop/cancel never preserves it.
pub fn capture_preserves_choice(reason: &str) -> bool {
    matches!(reason, "hotkey-wake" | "wake-fire" | "barge" | "continue")
}
pub fn cancel_running() {
    if let Ok(mut a) = ACTIVE.lock() {
        if let Some(a) = a.take() {
            a.store(true, Ordering::Relaxed)
        }
    }
}
pub fn offer(
    candidates: Vec<Candidate>,
    cancel: Arc<AtomicBool>,
    remaining: Vec<GhostStep>,
    store: Option<PathBuf>,
) {
    clear();
    let mut slot = ChoiceSlot::default();
    slot.replace(PendingChoice {
        token: crate::network::uuid_v4(),
        profile: crate::app_dirs::APP_ID.into(),
        created: now(),
        candidates,
    });
    if let Ok(mut p) = PENDING.lock() {
        *p = Some(Pending {
            slot,
            cancel,
            remaining,
            store,
            lock_on: None,
        })
    }
}
pub fn bind(cancel: &Arc<AtomicBool>, lock_on: bool) {
    if let Ok(mut p) = PENDING.lock() {
        if let Some(p) = p.as_mut() {
            if Arc::ptr_eq(&p.cancel, cancel) {
                p.lock_on = Some(lock_on)
            }
        }
    }
}
pub fn has_pending() -> bool {
    PENDING.lock().ok().is_some_and(|p| p.is_some())
}
pub fn view() -> Option<ChoiceView> {
    let p = PENDING.lock().ok()?;
    let p = p.as_ref()?;
    if p.lock_on.is_none()
        || p.cancel.load(Ordering::Relaxed)
        || crate::ghost::stop_requested()
        || !crate::ghost::session_active()
    {
        return None;
    }
    p.slot.view(now())
}
fn refused(reason: &str) -> Vec<Receipt> {
    vec![Receipt {
        step: 0,
        route: Route::AccessibilityCenter,
        outcome: Outcome::Blocked,
        detail: reason.into(),
        elapsed_ms: 0,
        operation_id: Some(crate::network::uuid_v4()),
        reason_code: Some("choice_rejected".into()),
    }]
}

fn choose(token: &str, number: usize, lock_on: bool, sidebar: bool) -> Vec<Receipt> {
    #[cfg(target_os = "windows")]
    {
        let pending = {
            let Ok(mut p) = PENDING.lock() else {
                return refused("Choice store unavailable.");
            };
            if p.as_ref()
                .and_then(|p| p.slot.view(now()))
                .is_none_or(|v| v.token != token)
            {
                return refused("That choice expired or was replaced.");
            }
            p.take().unwrap()
        };
        if pending.lock_on != Some(lock_on)
            || pending.cancel.load(Ordering::Relaxed)
            || crate::ghost::stop_requested()
            || !crate::ghost::session_active()
        {
            return refused("Permissions or cancellation state changed; ask again.");
        }
        if pending
            .slot
            .view(now())
            .is_none_or(|v| !v.choices.iter().any(|c| c.number == number))
        {
            return refused("That choice number is invalid.");
        }
        // Register before any focus restoration so Escape can cancel that work too.
        if let Ok(mut active) = ACTIVE.lock() {
            *active = Some(pending.cancel.clone())
        }
        struct ActiveGuard(Arc<AtomicBool>);
        impl Drop for ActiveGuard {
            fn drop(&mut self) {
                finish_active(&self.0)
            }
        }
        let _active_guard = ActiveGuard(pending.cancel.clone());
        let Some(window) = pending.slot.window().cloned() else {
            return refused("No target window.");
        };
        if crate::desktop_native::check_identity(&window).is_err() {
            return refused("The original window is protected or unavailable.");
        }
        if crate::desktop_native::window_identity(window.hwnd).as_ref() != Some(&window) {
            return refused("The original application changed.");
        }
        if crate::desktop_native::foreground().as_ref() != Some(&window) {
            // Restoring is allowed only from the explicitly selected NEXUS sidebar, never from another app.
            if !sidebar {
                return refused("The foreground application changed.");
            }
            if pending.cancel.load(Ordering::Relaxed) || crate::ghost::stop_requested() {
                return refused("Stopped.");
            }
            if !crate::live::commands::window::focus_window(windows::Win32::Foundation::HWND(
                window.hwnd,
            )) {
                return refused("Could not restore the original window.");
            }
        }
        let mut slot = pending.slot;
        let candidate = match slot.take(token, number, crate::app_dirs::APP_ID, now(), &window) {
            Ok(c) => c,
            Err(_) => return refused("That choice is no longer valid."),
        };
        let start = Instant::now();
        let result = crate::desktop_native::perform(&candidate, &pending.cancel);
        let receipt = match result {
            Ok((route, v)) => Receipt {
                step: 0,
                route,
                outcome: match v {
                    Verification::Verified => Outcome::Verified,
                    Verification::Stopped => Outcome::Stopped,
                    Verification::ForegroundChanged => Outcome::Blocked,
                    _ => Outcome::Unconfirmed,
                },
                detail: "Selected control action processed".into(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                operation_id: Some(crate::network::uuid_v4()),
                reason_code: Some("choice_effect".into()),
            },
            Err(_) => {
                finish_active(&pending.cancel);
                return refused("The selected control changed or could not be verified.");
            }
        };
        let verified = receipt.outcome == Outcome::Verified;
        let mut results = vec![receipt];
        // Release the active slot before continuing; the pending request's cancellation flag remains authoritative.
        finish_active(&pending.cancel);
        if verified && !pending.remaining.is_empty() && !pending.cancel.load(Ordering::Relaxed) {
            results.extend(crate::ghost_center::execute_authorized(
                &GhostTask {
                    steps: pending.remaining,
                },
                pending.cancel.clone(),
                pending.store.as_deref(),
            ));
            bind(&pending.cancel, lock_on);
        }
        results
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (token, number, lock_on, sidebar);
        refused("Native desktop control requires Windows.")
    }
}
pub fn choose_voice(number: usize, cancel: &AtomicBool) -> Vec<Receipt> {
    let lock = PENDING
        .lock()
        .ok()
        .and_then(|p| p.as_ref().and_then(|p| p.lock_on))
        .unwrap_or(true);
    choose_voice_with_lock(number, cancel, lock)
}
pub fn choose_voice_with_lock(number: usize, cancel: &AtomicBool, lock: bool) -> Vec<Receipt> {
    if cancel.load(Ordering::Relaxed) {
        return refused("Stopped.");
    }
    let Some(v) = view() else {
        return refused("There is no current control choice.");
    };
    // The normal voice ownership gates already authorized this turn. Only our own sidebar may restore focus.
    #[cfg(target_os = "windows")]
    let sidebar = crate::desktop_native::foreground().is_some_and(|w| {
        w.pid == std::process::id() && w.hwnd == SIDEBAR_HWND.load(Ordering::Relaxed)
    });
    #[cfg(not(target_os = "windows"))]
    let sidebar = false;
    choose(&v.token, number, lock, sidebar)
}
pub async fn publish<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(initial) = view() else { return };
    let current = crate::commands::get_pending_sidebar_view()
        .ok()
        .flatten()
        .unwrap_or_else(|| "assistant".into());
    let _ = crate::commands::show_sidebar_view(app.clone(), current, None).await;
    #[cfg(target_os = "windows")]
    SIDEBAR_HWND.store(
        app.get_webview_window("sidebar")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as isize)
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while view().is_some_and(|v| v.token == initial.token) {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if view().is_none_or(|v| v.token != initial.token) {
                break;
            }
            #[cfg(target_os = "windows")]
            {
                let target = PENDING
                    .lock()
                    .ok()
                    .and_then(|p| p.as_ref().and_then(|p| p.slot.window().cloned()));
                let own = app
                    .get_webview_window("sidebar")
                    .and_then(|w| w.hwnd().ok())
                    .map(|h| h.0 as isize);
                let current = crate::desktop_native::foreground();
                if current.as_ref() != target.as_ref()
                    && !current
                        .as_ref()
                        .is_some_and(|w| Some(w.hwnd) == own && w.pid == std::process::id())
                {
                    clear();
                    break;
                }
            }
        }
        let same = PENDING.lock().ok().is_some_and(|p| {
            p.as_ref()
                .is_some_and(|p| p.slot.view(now()).is_none_or(|v| v.token == initial.token))
        });
        if same && view().is_none() {
            clear()
        }
    });
}
#[tauri::command]
pub fn ghost_pending_choice<R: Runtime>(
    window: tauri::WebviewWindow<R>,
) -> Result<Option<ChoiceView>, String> {
    if window.label() != "sidebar" {
        return Err("Choices belong to the sidebar.".into());
    }
    Ok(view())
}
#[tauri::command]
pub fn ghost_cancel_choice<R: Runtime>(window: tauri::WebviewWindow<R>) -> Result<(), String> {
    if window.label() != "sidebar" {
        return Err("Choices belong to the sidebar.".into());
    }
    clear();
    Ok(())
}
#[tauri::command]
pub async fn ghost_choose_control<R: Runtime>(
    app: tauri::AppHandle<R>,
    window: tauri::WebviewWindow<R>,
    token: String,
    number: usize,
) -> Result<Vec<Receipt>, String> {
    if window.label() != "sidebar" {
        return Err("Choices belong to the sidebar.".into());
    }
    #[cfg(target_os = "windows")]
    let owns_focus = window
        .hwnd()
        .ok()
        .zip(crate::desktop_native::foreground())
        .is_some_and(|(h, w)| h.0 as isize == w.hwnd && w.pid == std::process::id());
    #[cfg(not(target_os = "windows"))]
    let owns_focus = false;
    if !owns_focus {
        clear();
        return Err("The sidebar is not the active window.".into());
    }
    let lock = crate::commands::read_speaker_verification(&app);
    let _drill = crate::ghost::GhostDrillGuard::new();
    let worker = tokio::task::spawn_blocking(move || choose(&token, number, lock, true));
    let result = match tokio::time::timeout(std::time::Duration::from_secs(20), worker).await {
        Ok(result) => result.map_err(|_| "Control selection worker failed.".to_string())?,
        Err(_) => {
            clear();
            return Err("Control observation timed out; no automatic retry.".into());
        }
    };
    publish(&app).await;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_and_explicit_cancellation_are_distinct() {
        for reason in ["hotkey-wake", "wake-fire", "barge", "continue"] {
            assert!(capture_preserves_choice(reason));
        }
        for reason in [
            "ipc-cancel",
            "barge-stop",
            "drill-stop",
            "hotkey-stage-kill",
            "resume-from-sleep",
            "",
        ] {
            assert!(!capture_preserves_choice(reason));
        }
    }
    #[test]
    fn clarification_requires_binding_and_keeps_original_cancellation() {
        clear();
        let cancel = Arc::new(AtomicBool::new(false));
        let make = |n| Candidate {
            target: crate::desktop_control::TargetSnapshot {
                window: crate::desktop_control::WindowIdentity {
                    hwnd: 1,
                    pid: 2,
                    created: 3,
                    exe: "fixture.exe".into(),
                },
                runtime_id: vec![n],
                automation_id: n.to_string(),
                generation: 1,
            },
            label: "Open".into(),
            role: "button".into(),
            container: "Toolbar".into(),
            enabled: true,
            visible: true,
            password: false,
            permitted: true,
        };
        offer(vec![make(1), make(2)], cancel.clone(), vec![], None);
        assert!(view().is_none());
        bind(&cancel, false);
        // The fixture boundary disables Ghost; production tests may also run while no Ghost session exists.
        assert!(has_pending());
        if crate::ghost::session_active() {
            assert!(view().is_some())
        }
        cancel.store(true, Ordering::Relaxed);
        assert!(view().is_none());
        clear();
        assert!(!has_pending());
    }
}
