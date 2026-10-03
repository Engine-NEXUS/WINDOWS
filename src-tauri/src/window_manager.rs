//! Window management: transparent frameless always-on-top overlay with click-through control.
//!
//! The overlay starts hidden and click-through. On wake, Rust shows the window and
//! disables click-through. When the assistant goes idle, the frontend re-enables
//! click-through and eventually hides the window.

use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

const WIN: &str = "main";

/// Read orb position + size from settings.json.
/// Falls back to defaults (center-bottom, 200px) if the file is missing,
/// can't be parsed, or doesn't contain the orb fields.
pub fn read_orb_settings<R: Runtime>(app: &AppHandle<R>) -> (f64, f64, u32) {
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return (0.5, 1.0, 200),
    };
    let path = dir.join("settings.json");
    if !path.exists() {
        return (0.5, 1.0, 200);
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (0.5, 1.0, 200),
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return (0.5, 1.0, 200),
    };
    let h = json.get("orbHorizontalPct").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let v = json.get("orbVerticalPct").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let size = json.get("orbSize").and_then(|v| v.as_u64()).unwrap_or(200) as u32;
    // Clamp to safe ranges
    let h = h.max(0.0).min(1.0);
    let v = v.max(0.0).min(1.0);
    let size = size.max(100).min(300);
    (h, v, size)
}

/// Loose settings reader used by both the orb and the waves/loading paths:
/// `(key) -> f64` with clamped fallback, tolerant of missing/corrupt files.
fn read_pct_key(json: &serde_json::Value, key: &str, fallback: f64) -> f64 {
    json.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| v.max(0.0).min(1.0))
        .unwrap_or(fallback)
}

fn read_size_key(json: &serde_json::Value, key: &str, fallback: u32, min: u32, max: u32) -> u32 {
    json.get(key)
        .and_then(|v| v.as_u64())
        .map(|v| (v as u32).max(min).min(max))
        .unwrap_or(fallback)
}

/// Read waves placement from settings.json (ghost-session rect for the
/// waves visual inside the orb window). Defaults = the wakeup defaults
/// (user invariant: waves live where the wakeup orb lives).
pub fn read_waves_settings<R: Runtime>(app: &AppHandle<R>) -> (f64, f64, u32) {
    let fallback = (0.5f64, 1.0f64, 200u32);
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return fallback,
    };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else {
        return fallback;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return fallback;
    };
    (
        read_pct_key(&json, "wavesHorizontalPct", 0.5),
        read_pct_key(&json, "wavesVerticalPct", 1.0),
        read_size_key(&json, "wavesSize", 200, 100, 300),
    )
}

/// Read loading-indicator placement from settings.json. Defaults ≈ the
/// historical hardcoded top-right corner (center-anchored 0.95/0.05, 80px).
pub fn read_loading_settings<R: Runtime>(app: &AppHandle<R>) -> (f64, f64, u32) {
    let fallback = (0.95f64, 0.05f64, 80u32);
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return fallback,
    };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else {
        return fallback;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return fallback;
    };
    (
        read_pct_key(&json, "loadingHorizontalPct", 0.95),
        read_pct_key(&json, "loadingVerticalPct", 0.05),
        read_size_key(&json, "loadingSize", 80, 40, 160),
    )
}

/// Position the loading-indicator window from saved settings. Used by both
/// show paths (orchestrator show_loading + commands show_loading_indicator)
/// so the user-calibrated placement is what actually shows at runtime.
pub fn position_loading<R: Runtime>(win: &WebviewWindow<R>) -> Result<(), String> {
    if let Ok(Some(monitor)) = win.current_monitor() {
        let (h, v, size) = read_loading_settings(win.app_handle());
        let (x, y) = overlay_xy(
            h,
            v,
            size,
            monitor.size().width as i32,
            monitor.size().height as i32,
            monitor.scale_factor(),
        );
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = win.set_size(tauri::PhysicalSize::new(size, size));
    }
    Ok(())
}

/// Pure overlay placement math (unit-tested): center-anchored pct →
/// clamped physical px. Single conversion choke point shared by the orb,
/// waves (ghost sessions), loading indicator, and the calibration previews.
pub fn overlay_xy(
    h_pct: f64,
    v_pct: f64,
    size: u32,
    screen_w: i32,
    screen_h: i32,
    scale: f64,
) -> (i32, i32) {
    let h = h_pct.max(0.0).min(1.0);
    let v = v_pct.max(0.0).min(1.0);
    let phys = (size as f64 * scale) as i32;
    let raw_x = (screen_w as f64 * h) as i32 - phys / 2;
    let raw_y = (screen_h as f64 * v) as i32 - phys / 2;
    let x = raw_x.max(0).min(screen_w - phys);
    let y = raw_y.max(0).min(screen_h - phys);
    (x, y)
}

/// Pure nudge math (unit-tested): shift the CURRENT top-left by logical
/// px, then re-solve the center-anchored pct. The inverse of overlay_xy
/// up to the edge clamp — drag, wheel, and keyboard share one path.
pub fn overlay_nudge(
    h_pct: f64,
    v_pct: f64,
    size: u32,
    dx_logical: i32,
    dy_logical: i32,
    screen_w: i32,
    screen_h: i32,
    scale: f64,
) -> (f64, f64) {
    let phys = (size as f64 * scale) as i32;
    let (x, y) = overlay_xy(h_pct, v_pct, size, screen_w, screen_h, scale);
    let nx = x + (dx_logical as f64 * scale) as i32;
    let ny = y + (dy_logical as f64 * scale) as i32;
    let h = ((nx + phys / 2) as f64 / screen_w.max(1) as f64).max(0.0).min(1.0);
    let v = ((ny + phys / 2) as f64 / screen_h.max(1) as f64).max(0.0).min(1.0);
    (h, v)
}

/// Position the orb based on saved settings (orbHorizontalPct, orbVerticalPct, orbSize).
/// Falls back to center-bottom, 200px if settings are missing or invalid.
/// Called at startup, on every wake, on every hotkey press, and on show_overlay.
/// During a ghost session the WAVES placement is applied instead (the waves
/// own the visual then — same window, different saved rect), and during
/// animation calibration repositioning is suppressed entirely (the drag
/// owns the window).
pub fn position_orb<R: Runtime>(win: &WebviewWindow<R>) -> Result<(), String> {
    use tauri::PhysicalPosition;
    if crate::calibration::is_active() {
        tracing::debug!("position_orb: calibration active — leaving preview placement alone");
        return Ok(());
    }
    if let Ok(Some(monitor)) = win.current_monitor() {
        let scale = monitor.scale_factor();
        let screen = monitor.size();

        let (h_pct, v_pct, orb_size) = if crate::ghost::session_active() {
            read_waves_settings(win.app_handle())
        } else {
            read_orb_settings(win.app_handle())
        };
        let (x, y) = overlay_xy(
            h_pct,
            v_pct,
            orb_size,
            screen.width as i32,
            screen.height as i32,
            scale,
        );

        let _ = win.set_position(PhysicalPosition::new(x, y));
        let _ = win.set_size(tauri::PhysicalSize::new(orb_size, orb_size));
        tracing::debug!("orb positioned at ({}, {}) size {}px [h={}, v={}, scale={}]",
            x, y, orb_size, h_pct, v_pct, scale);
    }
    Ok(())
}

/// Configure window as a non-activating floating overlay (does not steal keyboard focus from active apps)
pub fn configure_non_activating_overlay<R: Runtime>(win: &WebviewWindow<R>) -> Result<(), String> {
    let _ = position_orb(win);
    win.set_always_on_top(true).map_err(|e| e.to_string())?;
    let _ = win.set_focusable(false);
    Ok(())
}

pub fn init<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let win = app
        .get_webview_window(WIN)
        .ok_or_else(|| "main window not found".to_string())?;

    configure_non_activating_overlay(&win)?;
    // Start with click-through OFF so the user can interact with the window.
    win.set_ignore_cursor_events(false).map_err(|e| e.to_string())?;
    Ok(())
}

/// IPC: `invoke('set_click_through', { ignore: bool })`.
#[tauri::command]
pub fn set_click_through<R: Runtime>(
    app: AppHandle<R>,
    ignore: bool,
) -> Result<(), String> {
    let win = app
        .get_webview_window(WIN)
        .ok_or_else(|| "main window not found".to_string())?;
    win.set_ignore_cursor_events(ignore).map_err(|e| e.to_string())?;
    if !ignore {
        let _ = win.set_always_on_top(true);
    }
    Ok(())
}

/// Convenience: re-apply overlay state (called after show).
#[allow(dead_code)]
pub fn refresh_overlay<R: Runtime>(win: &WebviewWindow<R>) -> Result<(), String> {
    let _ = position_orb(win);
    win.set_always_on_top(true).map_err(|e| e.to_string())?;
    win.set_ignore_cursor_events(true).map_err(|e| e.to_string())
}

/// IPC: `invoke('show_overlay')`.
/// Shows the native overlay window. Used by the frontend when `visible` becomes true.
/// CSS opacity/transform alone can't reliably hide WebView2 transparent windows after
/// content has been rendered (GPU compositing caches the last frame), so we use
/// native show/hide for reliable visibility control.
#[tauri::command]
pub fn show_overlay<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let win = app
        .get_webview_window(WIN)
        .ok_or_else(|| "main window not found".to_string())?;
    win.show().map_err(|e| e.to_string())?;
    configure_non_activating_overlay(&win)?;
    win.set_ignore_cursor_events(false).map_err(|e| e.to_string())?;
    Ok(())
}

/// IPC: `invoke('hide_overlay')`.
/// Hides the native overlay window. Used by the frontend when `visible` becomes false.
#[tauri::command]
pub fn hide_overlay<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let win = app
        .get_webview_window(WIN)
        .ok_or_else(|| "main window not found".to_string())?;
    win.hide().map_err(|e| e.to_string())?;
    Ok(())
}

/// IPC: `invoke('set_orb_position', { horizontalPct, verticalPct, size })`.
/// Live-updates the orb position and size without restarting.
/// Does NOT save to settings — the frontend should call save_settings separately.
/// Used by the settings sidebar sliders for real-time preview.
#[tauri::command]
pub fn set_orb_position<R: Runtime>(
    app: AppHandle<R>,
    horizontal_pct: f64,
    vertical_pct: f64,
    size: u32,
) -> Result<(), String> {
    let win = app
        .get_webview_window(WIN)
        .ok_or_else(|| "main window not found".to_string())?;

    // Clamp inputs to safe ranges
    let h = horizontal_pct.max(0.0).min(1.0);
    let v = vertical_pct.max(0.0).min(1.0);
    let orb = size.max(100).min(300) as i32;

    if let Ok(Some(monitor)) = win.current_monitor() {
        let scale = monitor.scale_factor();
        let screen = monitor.size();

        let (x, y) = overlay_xy(h, v, size, screen.width as i32, screen.height as i32, scale);
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = win.set_size(tauri::PhysicalSize::new(orb, orb));
        tracing::debug!("set_orb_position: ({}, {}) size {}px [h={}, v={}]",
            x, y, orb, h, v);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1920x1080 @ 1x, 200px window.
    const SW: i32 = 1920;
    const SH: i32 = 1080;

    #[test]
    fn test_overlay_xy_center_anchor() {
        // h=0.5 centers horizontally: x = 960 - 100 = 860.
        assert_eq!(overlay_xy(0.5, 0.5, 200, SW, SH, 1.0), (860, 440));
        // v=1.0 parks the bottom edge exactly on screen.
        assert_eq!(overlay_xy(0.5, 1.0, 200, SW, SH, 1.0), (860, 880));
        // Corners clamp fully on-screen.
        assert_eq!(overlay_xy(0.0, 0.0, 200, SW, SH, 1.0), (0, 0));
        assert_eq!(overlay_xy(1.0, 1.0, 200, SW, SH, 1.0), (1720, 880));
    }

    #[test]
    fn test_overlay_xy_dpi_scales_physical() {
        // 200% scale: 400 physical px window, same fractions.
        let (x, _y) = overlay_xy(0.5, 1.0, 200, SW, SH, 2.0);
        assert_eq!(x, 960 - 200);
    }

    #[test]
    fn test_overlay_nudge_moves_one_logical_px() {
        // From center, +1px right / +1px down at 1x.
        let (h, v) = overlay_nudge(0.5, 0.5, 200, 1, 1, SW, SH, 1.0);
        let (x, y) = overlay_xy(h, v, 200, SW, SH, 1.0);
        assert_eq!((x, y), (861, 441));
    }

    #[test]
    fn test_overlay_nudge_scales_with_dpi() {
        // +1 logical px at 2x = +2 physical px.
        let (h, _v) = overlay_nudge(0.5, 0.5, 200, 1, 0, SW, SH, 2.0);
        let (x, _y) = overlay_xy(h, 0.5, 200, SW, SH, 2.0);
        assert_eq!(x, 960 - 200 + 2);
    }

    #[test]
    fn test_overlay_nudge_clamps_at_edges() {
        // Nudging far past the right edge pins at h=1.0.
        let (h, _v) = overlay_nudge(1.0, 1.0, 200, 5000, 5000, SW, SH, 1.0);
        assert_eq!((h, 1.0), (1.0, 1.0));
        // And far past top-left pins at 0.0.
        let (h2, v2) = overlay_nudge(0.0, 0.0, 200, -5000, -5000, SW, SH, 1.0);
        assert_eq!((h2, v2), (0.0, 0.0));
    }

    #[test]
    fn test_overlay_nudge_roundtrip_stable() {
        // Nudge right then left by the same amount returns home.
        let (h1, v1) = overlay_nudge(0.5, 0.5, 200, 37, -12, SW, SH, 1.0);
        let (h2, v2) = overlay_nudge(h1, v1, 200, -37, 12, SW, SH, 1.0);
        assert!((h2 - 0.5).abs() < 1e-9);
        assert!((v2 - 0.5).abs() < 1e-9);
    }
}
