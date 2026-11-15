//! App vs browser preference memory (persisted to `<app data>/app_web_pref.json`).
//!
//! Maps normalised spoken app name -> `Pref::{App, Web}`.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pref {
    App,
    Web,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAction {
    OpenApp,
    OpenWeb,
    AppNotFoundFallbackWeb,
    AppNotFoundNoFallback,
}

static PREFS: Lazy<RwLock<Option<HashMap<String, Pref>>>> = Lazy::new(|| RwLock::new(None));

#[cfg(test)]
static TEST_PREFS: Lazy<RwLock<Option<HashMap<String, Pref>>>> = Lazy::new(|| RwLock::new(None));

pub(crate) fn pref_file_path() -> PathBuf {
    crate::app_dirs::nexus_data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("app_web_pref.json")
}

pub fn normalise_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn load_from_disk() -> HashMap<String, Pref> {
    let path = pref_file_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, Pref>>(&data) {
            return map;
        }
    }
    HashMap::new()
}

fn save_to_disk(map: &HashMap<String, Pref>) -> bool {
    let path = pref_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(map) {
        crate::memory::atomic_write(&path, json.as_bytes())
    } else {
        false
    }
}

pub fn get(name: &str) -> Option<Pref> {
    #[cfg(test)]
    {
        if let Some(ref map) = *TEST_PREFS.read() {
            let key = normalise_name(name);
            return map.get(&key).copied();
        }
    }
    let key = normalise_name(name);
    let mut lock = PREFS.write();
    let map = lock.get_or_insert_with(load_from_disk);
    map.get(&key).copied()
}

pub fn set(name: &str, pref: Pref) {
    let key = normalise_name(name);
    if key.is_empty() {
        return;
    }
    #[cfg(test)]
    {
        let mut test_lock = TEST_PREFS.write();
        if let Some(ref mut map) = *test_lock {
            map.insert(key.clone(), pref);
        }
    }
    let mut lock = PREFS.write();
    let map = lock.get_or_insert_with(load_from_disk);
    map.insert(key, pref);
    save_to_disk(map);
}

pub fn clear(name: &str) {
    let key = normalise_name(name);
    if key.is_empty() {
        return;
    }
    #[cfg(test)]
    {
        let mut test_lock = TEST_PREFS.write();
        if let Some(ref mut map) = *test_lock {
            map.remove(&key);
        }
    }
    let mut lock = PREFS.write();
    let map = lock.get_or_insert_with(load_from_disk);
    if map.remove(&key).is_some() {
        save_to_disk(map);
    }
}

/// Resolve which action to take given the app's availability, web fallback availability,
/// and the user's stored preference.
///
/// Rules:
/// - Pref Web: always open web in browser even if an app is installed.
/// - Pref App: open app if installed; if not installed, say so truthfully and fall back to web if available.
/// - Default (no pref): open app if installed; otherwise open web.
#[cfg(test)]
pub fn resolve_open_action(has_app: bool, has_web: bool, pref: Option<Pref>) -> OpenAction {
    match pref {
        Some(Pref::Web) => {
            if has_web {
                OpenAction::OpenWeb
            } else if has_app {
                OpenAction::OpenApp
            } else {
                OpenAction::AppNotFoundNoFallback
            }
        }
        Some(Pref::App) => {
            if has_app {
                OpenAction::OpenApp
            } else if has_web {
                OpenAction::AppNotFoundFallbackWeb
            } else {
                OpenAction::AppNotFoundNoFallback
            }
        }
        None => {
            if has_app {
                OpenAction::OpenApp
            } else if has_web {
                OpenAction::OpenWeb
            } else {
                OpenAction::AppNotFoundNoFallback
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn with_test_prefs<F: FnOnce()>(f: F) {
    *TEST_PREFS.write() = Some(HashMap::new());
    f();
    *TEST_PREFS.write() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pref_resolution_table() {
        // Full table: app installed/not x pref none/App/Web -> chosen action
        // (has_app, has_web, pref) -> expected OpenAction

        // Default (None):
        assert_eq!(resolve_open_action(true, true, None), OpenAction::OpenApp);
        assert_eq!(resolve_open_action(true, false, None), OpenAction::OpenApp);
        assert_eq!(resolve_open_action(false, true, None), OpenAction::OpenWeb);
        assert_eq!(resolve_open_action(false, false, None), OpenAction::AppNotFoundNoFallback);

        // Pref Web:
        assert_eq!(resolve_open_action(true, true, Some(Pref::Web)), OpenAction::OpenWeb);
        assert_eq!(resolve_open_action(false, true, Some(Pref::Web)), OpenAction::OpenWeb);
        // If web fallback doesn't exist, fall back to app or none:
        assert_eq!(resolve_open_action(true, false, Some(Pref::Web)), OpenAction::OpenApp);
        assert_eq!(resolve_open_action(false, false, Some(Pref::Web)), OpenAction::AppNotFoundNoFallback);

        // Pref App:
        assert_eq!(resolve_open_action(true, true, Some(Pref::App)), OpenAction::OpenApp);
        assert_eq!(resolve_open_action(true, false, Some(Pref::App)), OpenAction::OpenApp);
        // App not installed: truthful fallback to web
        assert_eq!(resolve_open_action(false, true, Some(Pref::App)), OpenAction::AppNotFoundFallbackWeb);
        // App not installed, no web either
        assert_eq!(resolve_open_action(false, false, Some(Pref::App)), OpenAction::AppNotFoundNoFallback);
    }

    #[test]
    fn test_get_set_clear_roundtrip() {
        with_test_prefs(|| {
            assert_eq!(get("YouTube"), None);

            set("YouTube", Pref::Web);
            assert_eq!(get("youtube"), Some(Pref::Web));
            assert_eq!(get("  YOUTUBE  "), Some(Pref::Web));

            set("youtube", Pref::App);
            assert_eq!(get("YouTube"), Some(Pref::App));

            clear("YouTube");
            assert_eq!(get("youtube"), None);
        });
    }
}
