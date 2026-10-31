//! Maps inside our own window. Google does not allow its normal Maps site in an iframe, so the old Maps window was an
//! ordinary web window (title bar, resizable, centred). Google's Maps Embed API is made for iframes (free, key needed,
//! "Maps Embed API" enabled on the key). `show_map` opens a plain frameless NEXUS panel (`maps-panel`, the pinned
//! `?pin=maps` view) that frames it, placed by `panel_layout` beside the other panels. If there is no key, the API is not
//! enabled for it, the setting `mapsEmbed` is off, or the request has no origin for a route, it falls back to the old
//! window, so a map always opens.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::google::types::TravelMode;

/// What to show.
#[derive(Debug, Clone, PartialEq)]
pub enum MapsTarget {
    /// One named place.
    Place(String),
    /// A search ("pharmacy near 12 High St").
    Search(String),
    /// A route. The embed needs both ends, so a route without an origin uses the old window.
    Directions { origin: Option<String>, destination: String, mode: TravelMode },
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn mode_word(m: TravelMode) -> &'static str {
    match m {
        TravelMode::Walking => "walking",
        TravelMode::Bicycling => "bicycling",
        TravelMode::Transit => "transit",
        _ => "driving",
    }
}

/// The Embed API URL for a target, or `None` when the embed cannot show it (a route with no origin, an empty query). Pure.
pub fn embed_url(key: &str, target: &MapsTarget) -> Option<String> {
    let key = key.trim();
    if key.is_empty() || key.contains(['&', '"', '\'', '<', '>', ' ']) {
        return None;
    }
    let base = "https://www.google.com/maps/embed/v1";
    match target {
        MapsTarget::Place(q) if !q.trim().is_empty() => Some(format!("{base}/place?key={key}&q={}", enc(q.trim()))),
        MapsTarget::Search(q) if !q.trim().is_empty() => Some(format!("{base}/search?key={key}&q={}", enc(q.trim()))),
        MapsTarget::Directions { origin: Some(o), destination, mode } if !o.trim().is_empty() && !destination.trim().is_empty() => Some(format!(
            "{base}/directions?key={key}&origin={}&destination={}&mode={}",
            enc(o.trim()),
            enc(destination.trim()),
            mode_word(*mode)
        )),
        _ => None,
    }
}

/// The old window's URL for the same target (the fallback). Pure.
pub fn legacy_url(target: &MapsTarget) -> String {
    use crate::google::maps::MapsService;
    match target {
        MapsTarget::Place(q) | MapsTarget::Search(q) => MapsService::build_place_search_url(q),
        MapsTarget::Directions { origin, destination, mode } => MapsService::build_directions_url(origin.as_deref(), destination, *mode),
    }
}

/// True when the Embed API answered with one of its own error pages instead of a map. Pure.
pub fn embed_error_in(body: &str) -> bool {
    [
        "ApiNotActivatedMapError",
        "InvalidKeyMapError",
        "MissingKeyMapError",
        "ExpiredKeyMapError",
        "RefererNotAllowedMapError",
        "ApiTargetBlockedMapError",
        "BillingNotEnabledMapError",
        "ApiKeyNotFoundMapError",
    ]
    .iter()
    .any(|m| body.contains(m))
}

/// Cached answer to "does the Embed API work with this key": (when checked, ok).
static PREFLIGHT: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

async fn embed_works(key: &str) -> bool {
    if let Some((at, ok)) = *PREFLIGHT.lock().unwrap() {
        if at.elapsed() < Duration::from_secs(600) {
            return ok;
        }
    }
    let ok = match embed_url(key, &MapsTarget::Place("Google".into())) {
        None => {
            tracing::warn!("maps: no usable Google Maps key for the embedded map - using the Maps window");
            false
        }
        Some(url) => {
            let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(12)).build().unwrap_or_default();
            match client.get(&url).header("Referer", "http://tauri.localhost/").send().await {
                Ok(r) if r.status().is_success() => {
                    let body = r.text().await.unwrap_or_default();
                    let bad = embed_error_in(&body);
                    if bad {
                        tracing::warn!("maps: the Embed API answered with an error page - using the Maps window");
                    }
                    !bad
                }
                Ok(r) => {
                    let status = r.status();
                    let body: String = r.text().await.unwrap_or_default().chars().take(140).collect();
                    tracing::warn!("maps: Embed API refused the key (HTTP {status}): {body} - enable \"Maps Embed API\" for this key in Google Cloud Console; using the Maps window meanwhile");
                    false
                }
                Err(e) => {
                    tracing::warn!("maps: Embed API check failed ({e}) - using the Maps window");
                    false
                }
            }
        }
    };
    *PREFLIGHT.lock().unwrap() = Some((Instant::now(), ok));
    ok
}

/// What the panel shows once it has mounted (the window can finish loading after the event was sent).
static PENDING: Mutex<Option<String>> = Mutex::new(None);

#[tauri::command]
pub fn get_pending_maps() -> Option<String> {
    PENDING.lock().unwrap().clone()
}

/// IPC: close the Maps panel (Escape inside it).
#[tauri::command]
pub async fn close_maps_panel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    crate::panel_layout::note_closed(crate::panel_layout::Panel::Maps);
    let r = crate::dyn_windows::destroy_window(&app, crate::panel_layout::Panel::Maps.label());
    crate::panel_layout::apply(&app);
    r
}

/// IPC: Shift + Left/Right inside a pinned panel. `delta` is logical px (positive = wider).
#[tauri::command]
pub fn panel_resize<R: Runtime>(app: AppHandle<R>, label: String, delta: f64) -> Result<f64, String> {
    let p = crate::panel_layout::Panel::from_label(&label).ok_or("unknown panel")?;
    let w = crate::panel_layout::resize(p, delta.clamp(-400.0, 400.0));
    crate::panel_layout::apply(&app);
    Ok(w)
}

fn embed_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Ok(dir) = app.path().app_data_dir() else { return true };
    std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|j| j.get("mapsEmbed").and_then(|v| v.as_bool()))
        .unwrap_or(true)
}

/// Show a map: in the NEXUS panel when the embed works, otherwise in the old window. Returns which one was used.
pub async fn show_map<R: Runtime>(app: &AppHandle<R>, target: MapsTarget) -> Result<&'static str, String> {
    // The embed key is the Google Maps key the user already saved (the same Google Cloud project).
    let key = [crate::commands::read_api_key(app, "google_maps"), crate::commands::read_api_key(app, "google_places")]
        .into_iter()
        .find(|k| !k.trim().is_empty())
        .unwrap_or_default();
    if embed_enabled(app) {
        if let Some(src) = embed_url(&key, &target) {
            if embed_works(&key).await {
                crate::panel_layout::note_opened(crate::panel_layout::Panel::Maps);
                *PENDING.lock().unwrap() = Some(src.clone());
                let existed = app.get_webview_window(crate::panel_layout::Panel::Maps.label()).is_some();
                let win = crate::dyn_windows::get_or_create_window(app, crate::dyn_windows::WindowConfig::maps_panel())?;
                crate::panel_layout::apply(app);
                win.show().map_err(|e| format!("maps panel show: {e}"))?;
                #[cfg(target_os = "windows")]
                crate::dwm_corners::round_corners(&win);
                if existed {
                    let _ = app.emit_to(crate::panel_layout::Panel::Maps.label(), "maps:show", src);
                }
                return Ok("panel");
            }
        }
    }
    // No key needed: the normal Google Maps site as the page of our own plain panel.
    match show_web_panel(app, &legacy_url(&target)) {
        Ok(()) => Ok("web-panel"),
        Err(e) => {
            tracing::warn!("maps: web panel failed ({e}) - using the Maps window");
            crate::commands::open_maps_url(app, &legacy_url(&target))?;
            Ok("window")
        }
    }
}

fn web_panel_closed<R: Runtime>(app: &AppHandle<R>) {
    crate::panel_layout::note_closed(crate::panel_layout::Panel::MapsWeb);
    crate::panel_layout::apply(app);
}

fn show_web_panel<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<(), String> {
    use crate::panel_layout::Panel;
    let win = crate::dyn_windows::open_external_panel(app, Panel::MapsWeb.label(), url, "NEXUS Map", (Panel::MapsWeb.widths().0, 950.0), web_panel_closed::<R>)?;
    crate::panel_layout::note_opened(Panel::MapsWeb);
    crate::panel_layout::apply(app);
    win.show().map_err(|e| format!("maps web panel show: {e}"))?;
    #[cfg(target_os = "windows")]
    crate::dwm_corners::round_corners(&win);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_urls_per_target() {
        let k = "AIzaKEY123";
        assert_eq!(
            embed_url(k, &MapsTarget::Place("Charminar, Hyderabad".into())).as_deref(),
            Some("https://www.google.com/maps/embed/v1/place?key=AIzaKEY123&q=Charminar%2C+Hyderabad")
        );
        assert!(embed_url(k, &MapsTarget::Search("pharmacy near me".into())).unwrap().contains("/search?key=AIzaKEY123&q=pharmacy+near+me"));
        let route = MapsTarget::Directions { origin: Some("Home".into()), destination: "T-Hub".into(), mode: TravelMode::Walking };
        let u = embed_url(k, &route).unwrap();
        assert!(u.contains("/directions?") && u.contains("origin=Home") && u.contains("destination=T-Hub") && u.ends_with("&mode=walking"), "{u}");
    }

    #[test]
    fn no_embed_for_routes_without_origin_empty_queries_or_bad_keys() {
        let k = "AIzaKEY123";
        assert!(embed_url(k, &MapsTarget::Directions { origin: None, destination: "x".into(), mode: TravelMode::Driving }).is_none());
        assert!(embed_url(k, &MapsTarget::Place("  ".into())).is_none());
        assert!(embed_url("", &MapsTarget::Place("x".into())).is_none());
        // A key that could break out of the URL is refused, never concatenated.
        assert!(embed_url("a&b=c", &MapsTarget::Place("x".into())).is_none());
        assert!(embed_url("a\"b", &MapsTarget::Place("x".into())).is_none());
    }

    #[test]
    fn query_text_is_encoded_not_injected() {
        let u = embed_url("K", &MapsTarget::Place("a&key=EVIL#<script>".into())).unwrap();
        assert_eq!(u.matches("key=").count(), 1, "{u}");
        assert!(!u.contains('<') && !u.contains('#'), "{u}");
    }

    #[test]
    fn embed_error_pages_are_recognised() {
        assert!(embed_error_in("... ApiNotActivatedMapError ..."));
        assert!(embed_error_in("InvalidKeyMapError"));
        assert!(!embed_error_in("<html>a normal map page</html>"));
    }

    #[test]
    fn the_fallback_url_still_works_for_every_target() {
        assert!(legacy_url(&MapsTarget::Place("Charminar".into())).starts_with("https://www.google.com/maps/"));
        let d = legacy_url(&MapsTarget::Directions { origin: None, destination: "T-Hub".into(), mode: TravelMode::Driving });
        assert!(d.contains("T-Hub") && d.starts_with("https://"));
    }
}
