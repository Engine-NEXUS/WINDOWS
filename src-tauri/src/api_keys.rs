//! API-key management for the Command Hub "API Keys" page: list (masked),
//! add / replace, delete. Keys live in the OS keychain (`auth_vault`) and are
//! scrubbed from settings.json; delete clears BOTH, so a deleted key cannot
//! linger in either place. A full key is never returned to the frontend — only
//! a masked tail.

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

/// Key storage abstraction for unit testing and vault integration.
pub trait KeyStore {
    fn get(&self, service: &str) -> Option<String>;
    fn set(&self, service: &str, key: &str);
}

/// Real KeyStore backed by Windows Credential Manager / OS keychain.
pub struct KeychainKeyStore;

impl KeyStore for KeychainKeyStore {
    fn get(&self, service: &str) -> Option<String> {
        crate::auth_vault::get_api_key(service)
    }

    fn set(&self, service: &str, key: &str) {
        crate::auth_vault::set_api_key(service, key);
    }
}

/// Counts of key-scrub outcomes. Never contains key strings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrubReport {
    pub scrubbed: usize,
    pub conflicts: usize,
    pub failed: usize,
}

/// (service id, display label, settings.json camelCase field)
pub const KEY_SERVICES: &[(&str, &str, &str)] = &[
    ("groq", "Groq", "groqApiKey"),
    ("gemini", "Gemini", "geminiApiKey"),
    ("cerebras", "Cerebras", "cerebrasApiKey"),
    ("deepgram", "Deepgram", "deepgramApiKey"),
    ("google_maps", "Google Maps & Routes", "routesApiKey"),
    ("google_places", "Google Places", "placesApiKey"),
    ("logo_dev", "Logo.dev", "logoDevPublishableKey"),
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApiKeyInfo {
    pub service: String,
    pub label: String,
    pub has_key: bool,
    /// e.g. `••••••••a1b2` — never the key itself.
    pub masked: String,
}

pub fn service_known(service: &str) -> bool {
    KEY_SERVICES.iter().any(|(s, _, _)| *s == service)
}

fn field_for(service: &str) -> Option<&'static str> {
    KEY_SERVICES.iter().find(|(s, _, _)| *s == service).map(|(_, _, f)| *f)
}

/// Mask a key: bullets plus the last 4 chars (only when the key is long
/// enough that 4 chars reveal nothing useful). Pure.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() < 12 {
        return "••••••••".to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••••••{tail}")
}

/// Structural validation for a pasted key (providers change their formats,
/// so no prefix is enforced). Pure.
pub fn validate_key(key: &str) -> Result<String, String> {
    let k = key.trim();
    if k.is_empty() {
        return Err("The key is empty.".to_string());
    }
    if k.chars().any(|c| c.is_whitespace()) {
        return Err("A key cannot contain spaces or line breaks.".to_string());
    }
    if k.len() < 16 {
        return Err("That looks too short to be an API key.".to_string());
    }
    if k.len() > 300 {
        return Err("That looks too long to be an API key.".to_string());
    }
    Ok(k.to_string())
}

/// Legacy and alternate field names in settings.json for each service.
pub fn legacy_fields_for(service: &str) -> &'static [&'static str] {
    match service {
        "groq" => &["groq_api_key"],
        "gemini" => &["gemini_api_key"],
        "cerebras" => &["cerebras_api_key"],
        "deepgram" => &["deepgram_api_key"],
        "google_maps" => &["routes_api_key", "googleMapsApiKey", "google_maps_api_key"],
        "google_places" => &["places_api_key", "googlePlacesApiKey", "google_places_api_key"],
        "logo_dev" => &["logo_dev_publishable_key"],
        _ => &[],
    }
}

/// Set (or clear, with `None`) one key field in settings.json text. Unknown
/// fields are preserved. Pure over the JSON value.
pub fn apply_key_field(json: &mut serde_json::Value, service: &str, key: Option<&str>) {
    let Some(field) = field_for(service) else { return };
    if !json.is_object() {
        *json = serde_json::json!({});
    }
    json[field] = serde_json::Value::String(key.unwrap_or("").to_string());
    // Legacy copies must not keep a deleted key alive.
    if let Some(obj) = json.as_object_mut() {
        for &legacy in legacy_fields_for(service) {
            if key.is_none() {
                obj.remove(legacy);
            } else {
                obj.insert(legacy.to_string(), serde_json::Value::String(key.unwrap_or("").to_string()));
            }
        }
    }
}

/// Pure, deterministic core: migrate plaintext API keys from settings JSON
/// to the key store (OS keychain in production).
/// For each known service and candidate field:
/// - if store already holds the SAME value -> blank the file field ("");
/// - if store has nothing -> set, verify with get, and blank if read-back matches;
/// - if store holds a DIFFERENT value -> leave file alone and count as conflict;
/// - if read-back fails -> leave file alone and count as failed.
pub fn scrub_plaintext_keys(json: &mut serde_json::Value, store: &dyn KeyStore) -> ScrubReport {
    let Some(obj) = json.as_object_mut() else {
        return ScrubReport::default();
    };

    let mut report = ScrubReport::default();

    for &(service, _, primary_field) in KEY_SERVICES {
        let all_fields = std::iter::once(primary_field)
            .chain(legacy_fields_for(service).iter().copied());

        for field in all_fields {
            let val = match obj.get(field).and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => continue,
            };

            match store.get(service).filter(|k| !k.is_empty()) {
                Some(stored) => {
                    if stored == val {
                        obj.insert(field.to_string(), serde_json::Value::String(String::new()));
                        report.scrubbed += 1;
                    } else {
                        report.conflicts += 1;
                    }
                }
                None => {
                    store.set(service, &val);
                    if store.get(service).as_deref() == Some(&val) {
                        obj.insert(field.to_string(), serde_json::Value::String(String::new()));
                        report.scrubbed += 1;
                    } else {
                        report.failed += 1;
                    }
                }
            }
        }
    }

    report
}

/// Read settings.json from `dir`, scrub plaintext keys, and atomically write back
/// only if keys were scrubbed. Unreadable or non-object files are left untouched.
pub fn scrub_settings_file(dir: &std::path::Path) -> ScrubReport {
    let path = dir.join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return ScrubReport::default();
    };
    let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return ScrubReport::default();
    };
    if !json.is_object() {
        return ScrubReport::default();
    }
    let store = KeychainKeyStore;
    let report = scrub_plaintext_keys(&mut json, &store);
    if report.scrubbed > 0 {
        if let Ok(pretty) = serde_json::to_string_pretty(&json) {
            crate::memory::atomic_write(&path, pretty.as_bytes());
        }
    }
    tracing::info!(
        "api_keys: settings file scrub completed (scrubbed: {}, conflicts: {}, failed: {})",
        report.scrubbed,
        report.conflicts,
        report.failed
    );
    report
}

/// Pure helper for `api_key_set`: stores key in `store`.
/// If verified by read-back, writes an EMPTY string to the file fields.
/// If rejected by store, falls back to settings.json copy with a warning.
pub fn set_key_with_store(
    store: &dyn KeyStore,
    service: &str,
    clean_key: &str,
    json: &mut serde_json::Value,
) -> bool {
    store.set(service, clean_key);
    let ok = store.get(service).as_deref() == Some(clean_key);
    if ok {
        apply_key_field(json, service, None);
    } else {
        tracing::warn!("api_keys: keychain storage failed for {service}, falling back to settings.json");
        apply_key_field(json, service, Some(clean_key));
    }
    ok
}

/// Pure helper for `save_settings` key handling.
/// Incoming non-empty key goes to the keychain (verified by read-back).
/// If accepted, returns `""` so the file gets an empty string.
/// If rejected, falls back to the key string with a warning.
/// An incoming EMPTY key never deletes a keychain key and returns whatever
/// `merge_key` resolves for the file copy.
pub fn handle_save_key(
    store: &dyn KeyStore,
    service: &str,
    field: &str,
    existing: &serde_json::Value,
    incoming: &str,
) -> String {
    let merged = merge_key(existing, field, incoming);
    if incoming.is_empty() {
        return merged;
    }
    store.set(service, incoming);
    if store.get(service).as_deref() == Some(incoming) {
        String::new()
    } else {
        tracing::warn!("api_keys: keychain write failed for {service} in save_settings, falling back to file");
        incoming.to_string()
    }
}

/// `save_settings` must never delete or resurrect keys: an EMPTY incoming key
/// keeps what is on disk (deletion happens only through `api_key_delete`).
/// Returns the value to persist for a field. Pure.
pub fn merge_key(existing: &serde_json::Value, field: &str, incoming: &str) -> String {
    if !incoming.is_empty() {
        return incoming.to_string();
    }
    existing
        .get(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn mutate_settings_file<R: Runtime>(
    app: &AppHandle<R>,
    f: impl FnOnce(&mut serde_json::Value),
) -> Result<(), String> {
    let path = settings_path(app)?;
    let mut json = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    f(&mut json);
    let pretty = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    if crate::memory::atomic_write(&path, pretty.as_bytes()) {
        Ok(())
    } else {
        Err("failed to write settings file atomically".to_string())
    }
}

/// IPC: which keys exist (masked).
#[tauri::command]
pub fn api_keys_list<R: Runtime>(app: AppHandle<R>) -> Vec<ApiKeyInfo> {
    KEY_SERVICES
        .iter()
        .map(|(service, label, _)| {
            let key = crate::commands::read_api_key(&app, service);
            ApiKeyInfo {
                service: service.to_string(),
                label: label.to_string(),
                has_key: !key.is_empty(),
                masked: mask(&key),
            }
        })
        .collect()
}

/// IPC: add or replace a key.
#[tauri::command]
pub fn api_key_set<R: Runtime>(app: AppHandle<R>, service: String, key: String) -> Result<ApiKeyInfo, String> {
    if !service_known(&service) {
        return Err(format!("unknown service: {service}"));
    }
    let clean = validate_key(&key)?;
    let store = KeychainKeyStore;
    mutate_settings_file(&app, |j| {
        set_key_with_store(&store, &service, &clean, j);
    })?;
    tracing::info!("api_keys: {service} key saved");
    let label = KEY_SERVICES.iter().find(|(s, _, _)| *s == service).map(|(_, l, _)| *l).unwrap_or("");
    Ok(ApiKeyInfo {
        service,
        label: label.to_string(),
        has_key: true,
        masked: mask(&clean),
    })
}

/// IPC: delete a key from the keychain AND settings.json.
#[tauri::command]
pub fn api_key_delete<R: Runtime>(app: AppHandle<R>, service: String) -> Result<(), String> {
    if !service_known(&service) {
        return Err(format!("unknown service: {service}"));
    }
    crate::auth_vault::clear_api_key(&service);
    mutate_settings_file(&app, |j| apply_key_field(j, &service, None))?;
    tracing::info!("api_keys: {service} key deleted");
    Ok(())
}

/// Number of configured keys (Command Hub insight).
pub fn count_keys<R: Runtime>(app: &AppHandle<R>) -> usize {
    KEY_SERVICES
        .iter()
        .filter(|(s, _, _)| !crate::commands::read_api_key(app, s).is_empty())
        .count()
}

/// IPC: number of configured API keys.
#[tauri::command]
pub fn api_keys_count<R: Runtime>(app: AppHandle<R>) -> usize {
    count_keys(&app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockKeyStore {
        entries: Mutex<HashMap<String, String>>,
        fail_set: bool,
    }

    impl KeyStore for MockKeyStore {
        fn get(&self, service: &str) -> Option<String> {
            self.entries.lock().unwrap().get(service).cloned()
        }

        fn set(&self, service: &str, key: &str) {
            if !self.fail_set {
                self.entries
                    .lock()
                    .unwrap()
                    .insert(service.to_string(), key.to_string());
            }
        }
    }

    #[test]
    fn mask_never_reveals_more_than_four_chars() {
        assert_eq!(mask(""), "");
        assert_eq!(mask("short"), "••••••••");
        assert_eq!(mask("AIzaSyA-1234567890abcdefXYZ9"), "••••••••XYZ9");
        assert!(!mask("gsk_1234567890abcdef").contains("1234"));
    }

    #[test]
    fn validation() {
        assert!(validate_key("").is_err());
        assert!(validate_key("   ").is_err());
        assert!(validate_key("has space inside 1234567890").is_err());
        assert!(validate_key("short").is_err());
        assert!(validate_key(&"x".repeat(301)).is_err());
        assert_eq!(validate_key("  AIzaSyA-1234567890abcdef  ").unwrap(), "AIzaSyA-1234567890abcdef");
    }

    #[test]
    fn set_and_delete_edit_only_their_field() {
        let mut j = serde_json::json!({"orbColor": "#fff", "groqApiKey": "old", "groq_api_key": "old", "geminiApiKey": "g"});
        apply_key_field(&mut j, "groq", Some("NEWKEY0123456789"));
        assert_eq!(j["groqApiKey"], "NEWKEY0123456789");
        assert_eq!(j["geminiApiKey"], "g");
        assert_eq!(j["orbColor"], "#fff");
        apply_key_field(&mut j, "groq", None);
        assert_eq!(j["groqApiKey"], "");
        assert!(j.get("groq_api_key").is_none()); // legacy copy cannot keep it alive
        assert_eq!(j["geminiApiKey"], "g");
        // Unknown service is a no-op.
        let before = j.clone();
        apply_key_field(&mut j, "nope", Some("x"));
        assert_eq!(j, before);
        // Non-object JSON heals into an object.
        let mut bad = serde_json::json!(null);
        apply_key_field(&mut bad, "gemini", Some("K0123456789012345"));
        assert_eq!(bad["geminiApiKey"], "K0123456789012345");
    }

    #[test]
    fn save_settings_cannot_delete_or_resurrect_via_empty_payload() {
        let disk = serde_json::json!({"groqApiKey": "ondisk"});
        // Empty incoming keeps the disk value (no accidental wipe)…
        assert_eq!(merge_key(&disk, "groqApiKey", ""), "ondisk");
        // …a non-empty incoming key (setup wizard) wins…
        assert_eq!(merge_key(&disk, "groqApiKey", "fromwizard"), "fromwizard");
        // …and after api_key_delete the disk is empty, so an empty payload stays empty.
        let deleted = serde_json::json!({"groqApiKey": ""});
        assert_eq!(merge_key(&deleted, "groqApiKey", ""), "");
        assert_eq!(merge_key(&serde_json::json!({}), "geminiApiKey", ""), "");
    }

    #[test]
    fn known_services() {
        assert!(service_known("groq") && service_known("gemini") && service_known("cerebras") && service_known("deepgram"));
        assert!(!service_known("openai"));
    }

    #[test]
    fn scrub_with_store_already_holding_same_key_blanks_field() {
        let store = MockKeyStore::default();
        store.set("groq", "gsk_existing123456789");
        let mut json = serde_json::json!({
            "groqApiKey": "gsk_existing123456789",
            "otherField": "untouched"
        });
        let report = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(report.scrubbed, 1);
        assert_eq!(report.conflicts, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(json["groqApiKey"], "");
        assert_eq!(json["otherField"], "untouched");
    }

    #[test]
    fn scrub_with_empty_store_sets_and_reads_back() {
        let store = MockKeyStore::default();
        let mut json = serde_json::json!({
            "geminiApiKey": "AIzaSyTestKey123456"
        });
        let report = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(report.scrubbed, 1);
        assert_eq!(report.conflicts, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(store.get("gemini").as_deref(), Some("AIzaSyTestKey123456"));
        assert_eq!(json["geminiApiKey"], "");
    }

    #[test]
    fn scrub_with_different_stored_value_conflicts_and_leaves_file_untouched() {
        let store = MockKeyStore::default();
        store.set("groq", "gsk_store_value_123");
        let mut json = serde_json::json!({
            "groqApiKey": "gsk_file_value_different"
        });
        let before = json.clone();
        let report = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(report.conflicts, 1);
        assert_eq!(report.scrubbed, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(json, before);
        assert_eq!(store.get("groq").as_deref(), Some("gsk_store_value_123"));
    }

    #[test]
    fn scrub_with_failing_set_leaves_file_untouched() {
        let store = MockKeyStore {
            entries: Mutex::new(HashMap::new()),
            fail_set: true,
        };
        let mut json = serde_json::json!({
            "cerebrasApiKey": "csk_testkey123456"
        });
        let before = json.clone();
        let report = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(report.failed, 1);
        assert_eq!(report.scrubbed, 0);
        assert_eq!(report.conflicts, 0);
        assert_eq!(json, before);
    }

    #[test]
    fn scrub_snake_case_and_camel_case_both_present() {
        let store = MockKeyStore::default();
        let mut json = serde_json::json!({
            "groqApiKey": "gsk_same_key_12345",
            "groq_api_key": "gsk_same_key_12345"
        });
        let report = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(report.scrubbed, 2);
        assert_eq!(report.conflicts, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(json["groqApiKey"], "");
        assert_eq!(json["groq_api_key"], "");
        assert_eq!(store.get("groq").as_deref(), Some("gsk_same_key_12345"));
    }

    #[test]
    fn scrub_non_key_fields_byte_identical() {
        let store = MockKeyStore::default();
        let mut json = serde_json::json!({
            "orbColor": "#ffffff",
            "wakeSoundStyle": "triplet",
            "serverUrl": "https://example.com",
            "routesApiKey": "route_key_123456"
        });
        let report = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(report.scrubbed, 1);
        assert_eq!(json["orbColor"], "#ffffff");
        assert_eq!(json["wakeSoundStyle"], "triplet");
        assert_eq!(json["serverUrl"], "https://example.com");
        assert_eq!(json["routesApiKey"], "");
    }

    #[test]
    fn scrub_empty_invalid_array_json_untouched() {
        let store = MockKeyStore::default();

        let mut arr = serde_json::json!([1, 2, "groqApiKey"]);
        let arr_clone = arr.clone();
        assert_eq!(scrub_plaintext_keys(&mut arr, &store), ScrubReport::default());
        assert_eq!(arr, arr_clone);

        let mut str_val = serde_json::json!("plain string");
        let str_clone = str_val.clone();
        assert_eq!(scrub_plaintext_keys(&mut str_val, &store), ScrubReport::default());
        assert_eq!(str_val, str_clone);

        let mut empty_obj = serde_json::json!({});
        let report = scrub_plaintext_keys(&mut empty_obj, &store);
        assert_eq!(report, ScrubReport::default());
        assert_eq!(empty_obj, serde_json::json!({}));
    }

    #[test]
    fn scrub_idempotent_second_run_reports_zero() {
        let store = MockKeyStore::default();
        let mut json = serde_json::json!({
            "deepgramApiKey": "token1234567890",
            "placesApiKey": "places_test_key_123"
        });
        let r1 = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(r1.scrubbed, 2);
        let r2 = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(r2, ScrubReport::default());
        assert_eq!(r2.scrubbed, 0);
        assert_eq!(r2.conflicts, 0);
        assert_eq!(r2.failed, 0);
    }

    #[test]
    fn no_key_value_appears_in_scrub_report_debug() {
        let report = ScrubReport {
            scrubbed: 42,
            conflicts: 13,
            failed: 7,
        };
        let debug_str = format!("{report:?}");
        assert!(!debug_str.contains("key"));
        assert!(!debug_str.contains("secret"));
        assert_eq!(
            debug_str,
            "ScrubReport { scrubbed: 42, conflicts: 13, failed: 7 }"
        );
    }

    #[test]
    fn pure_helper_api_key_set_path() {
        let store = MockKeyStore::default();
        let mut json = serde_json::json!({"groqApiKey": "old"});
        let ok = set_key_with_store(&store, "groq", "gsk_new1234567890", &mut json);
        assert!(ok);
        assert_eq!(store.get("groq").as_deref(), Some("gsk_new1234567890"));
        assert_eq!(json["groqApiKey"], "");

        let fail_store = MockKeyStore {
            entries: Mutex::new(HashMap::new()),
            fail_set: true,
        };
        let mut json2 = serde_json::json!({"groqApiKey": ""});
        let ok2 = set_key_with_store(&fail_store, "groq", "gsk_fallback_key", &mut json2);
        assert!(!ok2);
        assert_eq!(json2["groqApiKey"], "gsk_fallback_key");
    }

    #[test]
    fn pure_helper_save_settings_path() {
        let store = MockKeyStore::default();
        let existing = serde_json::json!({"groqApiKey": ""});

        let val = handle_save_key(&store, "groq", "groqApiKey", &existing, "incoming_key_123");
        assert_eq!(val, "");
        assert_eq!(store.get("groq").as_deref(), Some("incoming_key_123"));

        let val_empty = handle_save_key(&store, "groq", "groqApiKey", &existing, "");
        assert_eq!(val_empty, "");
        assert_eq!(store.get("groq").as_deref(), Some("incoming_key_123"));

        let fail_store = MockKeyStore {
            entries: Mutex::new(HashMap::new()),
            fail_set: true,
        };
        let val_fail = handle_save_key(&fail_store, "groq", "groqApiKey", &existing, "fallback_incoming");
        assert_eq!(val_fail, "fallback_incoming");
    }

    #[test]
    fn google_maps_and_places_scrubbing_and_alias() {
        let store = MockKeyStore::default();
        let mut json = serde_json::json!({
            "googleMapsApiKey": "gmaps_key_123",
            "placesApiKey": "places_key_456"
        });
        let report = scrub_plaintext_keys(&mut json, &store);
        assert_eq!(report.scrubbed, 2);
        assert_eq!(json["googleMapsApiKey"], "");
        assert_eq!(json["placesApiKey"], "");
        assert_eq!(store.get("google_maps").as_deref(), Some("gmaps_key_123"));
        assert_eq!(store.get("google_places").as_deref(), Some("places_key_456"));
    }

    #[test]
    fn scrub_settings_file_atomic_roundtrip() {
        let pid = std::process::id();
        let temp_dir = std::env::temp_dir().join(format!("nexus_test_scrub_{pid}"));
        let _ = std::fs::create_dir_all(&temp_dir);
        let settings_file = temp_dir.join("settings.json");

        let initial = serde_json::json!({
            "orbColor": "#abc",
            "groqApiKey": "gsk_file_test_roundtrip"
        });
        std::fs::write(&settings_file, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        let report = scrub_settings_file(&temp_dir);
        assert_eq!(report.scrubbed, 1);

        let content = std::fs::read_to_string(&settings_file).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["groqApiKey"], "");
        assert_eq!(parsed["orbColor"], "#abc");

        let _ = std::fs::remove_file(&settings_file);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
