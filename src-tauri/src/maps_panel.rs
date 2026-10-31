//! Maps inside our own window, with no API key. Google does not allow its normal Maps site in an iframe, but the
//! "Share -> Embed a map" page (`/maps?...&output=embed`, which redirects to `/maps/embed?pb=...`) is served framable
//! and needs no key. `show_map` opens a plain frameless NEXUS panel (`maps-panel`, the pinned `?pin=maps` view) that
//! frames it, placed by `panel_layout` beside the other panels. If the setting `mapsEmbed` is off or the embed cannot
//! show the target, the real Maps site opens as the page of a plain panel (`maps-web`) instead, so a map always opens.

use std::sync::Mutex;

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

/// The keyless embed URL for a target ("Share -> Embed a map": Google serves it framable and needs no API key), or
/// `None` when it cannot show the target (empty query / destination). A route with no origin shows the destination;
/// the user starts directions from the map itself. Pure.
pub fn embed_url(target: &MapsTarget) -> Option<String> {
    let base = "https://www.google.com/maps";
    match target {
        MapsTarget::Place(q) | MapsTarget::Search(q) if !q.trim().is_empty() => Some(format!("{base}?q={}&output=embed", enc(q.trim()))),
        MapsTarget::Directions { origin, destination, mode } if !destination.trim().is_empty() => {
            let flag = match mode {
                TravelMode::Walking => "w",
                TravelMode::Bicycling => "b",
                TravelMode::Transit => "r",
                _ => "d",
            };
            let from = origin.as_deref().map(str::trim).filter(|o| !o.is_empty()).map(|o| format!("saddr={}&", enc(o))).unwrap_or_default();
            Some(format!("{base}?{from}daddr={}&dirflg={flag}&output=embed", enc(destination.trim())))
        }
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

/// Show a map in the NEXUS panel (keyless embed). If the embed is switched off (`mapsEmbed`) or cannot show the target,
/// the real Maps site opens as the page of a plain panel instead. Returns which one was used.
pub async fn show_map<R: Runtime>(app: &AppHandle<R>, target: MapsTarget) -> Result<&'static str, String> {
    if embed_enabled(app) {
        if let Some(src) = embed_url(&target) {
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
    fn embed_urls_per_target_need_no_key() {
        assert_eq!(embed_url(&MapsTarget::Place("Charminar, Hyderabad".into())).as_deref(), Some("https://www.google.com/maps?q=Charminar%2C+Hyderabad&output=embed"));
        assert!(embed_url(&MapsTarget::Search("pharmacy near me".into())).unwrap().contains("q=pharmacy+near+me"));
        let route = MapsTarget::Directions { origin: Some("Home".into()), destination: "T-Hub".into(), mode: TravelMode::Walking };
        assert_eq!(embed_url(&route).as_deref(), Some("https://www.google.com/maps?saddr=Home&daddr=T-Hub&dirflg=w&output=embed"));
        let no_origin = MapsTarget::Directions { origin: None, destination: "T-Hub".into(), mode: TravelMode::Driving };
        assert_eq!(embed_url(&no_origin).as_deref(), Some("https://www.google.com/maps?daddr=T-Hub&dirflg=d&output=embed"));
    }

    #[test]
    fn no_embed_for_empty_targets() {
        assert!(embed_url(&MapsTarget::Place("  ".into())).is_none());
        assert!(embed_url(&MapsTarget::Directions { origin: None, destination: " ".into(), mode: TravelMode::Driving }).is_none());
    }

    #[test]
    fn query_text_is_encoded_not_injected() {
        let u = embed_url(&MapsTarget::Place("a&output=EVIL#<script>".into())).unwrap();
        assert_eq!(u.matches("output=").count(), 1, "{u}");
        assert!(!u.contains('<') && !u.contains('#'), "{u}");
    }

    #[test]
    fn the_fallback_url_still_works_for_every_target() {
        assert!(legacy_url(&MapsTarget::Place("Charminar".into())).starts_with("https://www.google.com/maps/"));
        let d = legacy_url(&MapsTarget::Directions { origin: None, destination: "T-Hub".into(), mode: TravelMode::Driving });
        assert!(d.contains("T-Hub") && d.starts_with("https://"));
    }
}
