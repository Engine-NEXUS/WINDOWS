//! Local Device Links Activity Manager. It samples foreground executable names
//! only after explicit opt-in. A separate browser opt-in can add site domains;
//! full URLs, page titles, paths and queries never enter this store. Sharing is deliberately absent until link-scoped consent
//! and encrypted transport are implemented.

use chrono::{Duration as ChronoDuration, Local, NaiveDate, Timelike};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

const FILE_NAME: &str = "device_activity.nxm";
const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
const ACTIVE_IDLE_LIMIT_SECS: u64 = 120;
const MAX_SITES_PER_DAY: usize = 1000;

#[derive(Default, Serialize, Deserialize)]
struct ActivityData {
    enabled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    sites_enabled: bool,
    /// Legacy minute counts kept for existing encrypted files.
    days: BTreeMap<String, BTreeMap<String, u32>>,
    /// New samples use seconds so short app switches are not rounded to a whole minute.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    precise_seconds: BTreeMap<String, BTreeMap<String, u32>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    site_seconds: BTreeMap<String, BTreeMap<String, u32>>,
}

fn is_false(value: &bool) -> bool { !*value }

struct ActivityRuntime {
    path: PathBuf,
    key: Option<crate::memcore::crypto::Key>,
    data: ActivityData,
    healthy: bool,
}

static STATE: Lazy<Mutex<Option<ActivityRuntime>>> = Lazy::new(|| Mutex::new(None));

#[derive(Serialize)]
pub struct AppMinutes {
    app: String,
    minutes: u32,
    seconds: u32,
}

#[derive(Serialize)]
pub struct DayActivity {
    day: String,
    active_minutes: u32,
    active_seconds: u32,
    apps: Vec<AppMinutes>,
}

#[derive(Serialize)]
pub struct SiteSeconds {
    domain: String,
    seconds: u32,
}

#[derive(Serialize)]
pub struct DaySites {
    day: String,
    sites: Vec<SiteSeconds>,
}

#[derive(Serialize)]
pub struct ActivityStatus {
    supported: bool,
    can_clear: bool,
    enabled: bool,
    sites_enabled: bool,
    days: Vec<DayActivity>,
    site_days: Vec<DaySites>,
    browser_connected: bool,
    /// Activity never leaves the laptop; sharing is not connected yet.
    sharing_available: bool,
}

fn supported() -> bool {
    cfg!(target_os = "windows")
}

/// Programs whose names are never recorded, matched anywhere in the process name so that variants
/// (`keepassxc`, `1password7`, `bitwarden-desktop`) are covered too. Password managers, one-time-code
/// apps, disk-encryption tools, and the Windows sign-in surfaces.
const PRIVATE_APPS: &[&str] = &[
    "nexus", "lockapp", "logonui", "consent", "credentialuibroker",
    "1password", "bitwarden", "keepass", "lastpass", "dashlane", "protonpass", "nordpass",
    "roboform", "enpass", "keeper", "authy", "authenticator", "veracrypt", "cryptomator",
];

fn is_private_app(stem: &str) -> bool {
    PRIVATE_APPS.iter().any(|p| stem.contains(p))
}

fn app_label(process: &str) -> Option<String> {
    let lower = process.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    if stem.is_empty() || stem.len() > 80
        || !stem.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || is_private_app(stem)
    {
        return None;
    }
    // These Windows hosts do not identify the app shown to the user. Recording
    // their generic process names would create misleading activity entries.
    if matches!(stem, "applicationframehost" | "msedgewebview2") {
        return None;
    }
    if stem.starts_with("powertoys.") || stem.starts_with("powertoys_") {
        return Some("PowerToys".into());
    }
    let installed_name = match stem {
        "studio64" => Some("Android Studio"),
        "antigravity" => Some("Antigravity"),
        "anydesk" => Some("AnyDesk"),
        "balenaetcher" => Some("balenaEtcher"),
        "blender" | "blender-launcher" => Some("Blender"),
        "chatgpt" => Some("ChatGPT"),
        "clipchamp" => Some("Clipchamp"),
        "codex" => Some("Codex"),
        "davinciresolve" | "resolve" => Some("DaVinci Resolve"),
        "figma" => Some("Figma"),
        "githubdesktop" => Some("GitHub Desktop"),
        "mongodbatlas" | "mongodbcompass" => Some("MongoDB Compass"),
        "olk" => Some("Outlook"),
        "powershell" | "pwsh" => Some("PowerShell"),
        "powertoys" => Some("PowerToys"),
        "qbittorrent" => Some("qBittorrent"),
        "rainmeter" => Some("Rainmeter"),
        "signal" => Some("Signal"),
        "steam" => Some("Steam"),
        "tailscale" | "tailscale-ipn" => Some("Tailscale"),
        "telegram" | "telegramdesktop" => Some("Telegram"),
        "trae" => Some("Trae"),
        "windsurf" => Some("Windsurf"),
        "zed" => Some("Zed"),
        _ => None,
    };
    if let Some(name) = installed_name {
        return Some(name.into());
    }
    Some(crate::memcore::resume::friendly_app(&lower).chars().take(80).collect())
}

/// Only count a minute when the user was at the keyboard or mouse recently.
fn sampling_allowed(idle_secs: Option<u64>) -> bool {
    idle_secs.is_some_and(|seconds| seconds < ACTIVE_IDLE_LIMIT_SECS)
}

fn prune(data: &mut ActivityData, today: NaiveDate) -> bool {
    let oldest = (today - ChronoDuration::days(29)).format("%Y-%m-%d").to_string();
    let before = data.days.len() + data.precise_seconds.len() + data.site_seconds.len();
    data.days.retain(|day, _| day >= &oldest);
    data.precise_seconds.retain(|day, _| day >= &oldest);
    data.site_seconds.retain(|day, _| day >= &oldest);
    data.days.len() + data.precise_seconds.len() + data.site_seconds.len() != before
}

#[cfg(test)]
fn record(data: &mut ActivityData, today: NaiveDate, process: &str) -> bool {
    if !data.enabled { return false; }
    let Some(app) = app_label(process) else { return false; };
    prune(data, today);
    let day = today.format("%Y-%m-%d").to_string();
    let minutes = data.days.entry(day).or_default().entry(app).or_default();
    *minutes = minutes.saturating_add(1);
    true
}

fn record_seconds(data: &mut ActivityData, today: NaiveDate, process: &str, seconds: u32) -> bool {
    if !data.enabled || seconds == 0 { return false; }
    let Some(app) = app_label(process) else { return false; };
    prune(data, today);
    let day = today.format("%Y-%m-%d").to_string();
    let count = data.precise_seconds.entry(day).or_default().entry(app).or_default();
    *count = count.saturating_add(seconds);
    true
}

fn record_site_seconds(data: &mut ActivityData, today: NaiveDate, domain: &str, seconds: u32) -> bool {
    if !data.enabled || !data.sites_enabled || seconds == 0 { return false; }
    prune(data, today);
    let day = today.format("%Y-%m-%d").to_string();
    let sites = data.site_seconds.entry(day).or_default();
    if sites.len() >= MAX_SITES_PER_DAY && !sites.contains_key(domain) { return false; }
    let count = sites.entry(domain.into()).or_default();
    *count = count.saturating_add(seconds);
    true
}

fn record_split_site_seconds(data: &mut ActivityData, today: NaiveDate, seconds_since_midnight: u32, domain: &str, seconds: u32) -> bool {
    let today_seconds = seconds.min(seconds_since_midnight);
    let mut changed = false;
    if seconds > today_seconds {
        changed |= record_site_seconds(data, today - ChronoDuration::days(1), domain, seconds - today_seconds);
    }
    if today_seconds > 0 { changed |= record_site_seconds(data, today, domain, today_seconds); }
    changed
}

/// Keep a short sample on the correct side of local midnight. The foreground
/// app is still sampled, so an app switch within this interval is estimated.
fn record_split_seconds(
    data: &mut ActivityData, today: NaiveDate, seconds_since_midnight: u32,
    process: &str, seconds: u32,
) -> bool {
    let today_seconds = seconds.min(seconds_since_midnight);
    let yesterday_seconds = seconds - today_seconds;
    let mut changed = false;
    if yesterday_seconds > 0 {
        changed |= record_seconds(data, today - ChronoDuration::days(1), process, yesterday_seconds);
    }
    if today_seconds > 0 {
        changed |= record_seconds(data, today, process, today_seconds);
    }
    changed
}

/// Suspend and clock changes must not become foreground usage. Carry only the
/// fractional part of valid ticks so scheduler jitter does not lose seconds.
fn accountable_seconds(elapsed: Duration, wall_elapsed: ChronoDuration, remainder: &mut Duration) -> u32 {
    let wall_ms = wall_elapsed.num_milliseconds();
    if elapsed > Duration::from_secs(5)
        || wall_ms < 0
        || wall_ms.abs_diff(elapsed.as_millis() as i64) > 2_000
    {
        *remainder = Duration::ZERO;
        return 0;
    }
    *remainder += elapsed;
    let whole = remainder.as_secs() as u32;
    *remainder -= Duration::from_secs(u64::from(whole));
    whole
}

fn persist(runtime: &ActivityRuntime) -> bool {
    let Some(key) = runtime.key else { return false; };
    let Ok(json) = serde_json::to_vec(&runtime.data) else { return false; };
    let Some(sealed) = crate::memcore::crypto::seal(&key, &json) else { return false; };
    crate::memory::atomic_write(&runtime.path, &sealed)
}

fn load(dir: &Path) -> ActivityRuntime {
    let path = dir.join(FILE_NAME);
    let key = crate::memcore::crypto::process_key();
    let healthy = supported() && key.is_some();
    let data = if path.exists() {
        key.and_then(|k| std::fs::read(&path).ok()
            .and_then(|bytes| crate::memcore::crypto::open(&k, &bytes))
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()))
    } else { Some(ActivityData::default()) };
    let mut runtime = ActivityRuntime {
        path, key, healthy: healthy && data.is_some(),
        data: data.unwrap_or_default(),
    };
    if runtime.healthy && prune(&mut runtime.data, Local::now().date_naive()) && !persist(&runtime) {
        runtime.healthy = false;
        runtime.data.enabled = false;
    }
    runtime
}

fn ensure_started(dir: &Path) {
    let mut state = STATE.lock();
    if state.is_some() { return; }
    *state = Some(load(dir));
    if !supported() { return; }
    std::thread::spawn(|| {
      let mut last_tick = std::time::Instant::now();
      let mut last_wall = Local::now();
      let mut last_save = std::time::Instant::now();
      let mut remainder = Duration::ZERO;
      let mut dirty = false;
      loop {
        std::thread::sleep(SAMPLE_INTERVAL);
        if crate::focus_mode::paused() {
            crate::focus_mode::defer_log("device_activity");
            last_tick = std::time::Instant::now();
            last_wall = Local::now();
            remainder = Duration::ZERO;
            continue;
        }
        let now = Local::now();
        let elapsed = last_tick.elapsed();
        last_tick = std::time::Instant::now();
        let seconds = accountable_seconds(elapsed, now.signed_duration_since(last_wall), &mut remainder);
        last_wall = now;
        {
            let mut state = STATE.lock();
            if let Some(runtime) = state.as_mut() {
                if runtime.healthy && prune(&mut runtime.data, now.date_naive())
                    && !persist(runtime)
                {
                    runtime.healthy = false;
                    runtime.data.enabled = false;
                    tracing::warn!("device activity: expired-history cleanup failed; sampling stopped");
                }
            }
        }
        let enabled = STATE.lock().as_ref().is_some_and(|s| s.healthy && s.data.enabled);
        if !enabled {
            remainder = Duration::ZERO;
            continue;
        }
        #[cfg(target_os = "windows")]
        let process = if sampling_allowed(crate::memcore::resume::user_idle_secs()) {
            crate::browser_url::foreground_process_name()
        } else { None };
        #[cfg(not(target_os = "windows"))]
        let process: Option<String> = None;
        if process.is_none() { remainder = Duration::ZERO; }
        let domain = process.as_deref().and_then(crate::activity_sites::current_domain);
        let mut state = STATE.lock();
        if let Some(runtime) = state.as_mut() {
            if runtime.healthy {
                if let Some(process) = process.as_deref() {
                    dirty |= record_split_seconds(
                        &mut runtime.data, now.date_naive(), now.time().num_seconds_from_midnight(), process, seconds,
                    );
                    if let Some(domain) = domain.as_deref() {
                        dirty |= record_split_site_seconds(
                            &mut runtime.data, now.date_naive(), now.time().num_seconds_from_midnight(), domain, seconds,
                        );
                    }
                }
            }
            if runtime.healthy && dirty
                && (last_save.elapsed() >= Duration::from_secs(10) || process.is_none()) {
                if persist(runtime) { last_save = std::time::Instant::now(); }
                else {
                    runtime.healthy = false;
                    runtime.data.enabled = false;
                    tracing::warn!("device activity: encrypted local save failed; sampling stopped");
                }
                dirty = false;
            }
        }
      }
    });
}

pub fn start(dir: &Path) {
    ensure_started(dir);
    crate::activity_sites::spawn_listener();
}

pub(crate) fn sites_consent_active() -> bool {
    STATE.lock().as_ref().is_some_and(|runtime| runtime.healthy
        && runtime.data.enabled && runtime.data.sites_enabled)
}

fn current_status() -> ActivityStatus {
    let state = STATE.lock();
    let Some(runtime) = state.as_ref() else {
        return ActivityStatus {
            supported: false, can_clear: false, enabled: false, sites_enabled: false,
            days: vec![], site_days: vec![], browser_connected: false, sharing_available: false,
        };
    };
    status_of(runtime)
}

fn status_of(runtime: &ActivityRuntime) -> ActivityStatus {
    let mut totals: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    for (day, apps) in &runtime.data.days {
        for (app, minutes) in apps {
            let count = totals.entry(day.clone()).or_default().entry(app.clone()).or_default();
            *count = count.saturating_add(minutes.saturating_mul(60));
        }
    }
    for (day, apps) in &runtime.data.precise_seconds {
        for (app, seconds) in apps {
            let count = totals.entry(day.clone()).or_default().entry(app.clone()).or_default();
            *count = count.saturating_add(*seconds);
        }
    }
    let mut days: Vec<DayActivity> = totals.iter().rev().map(|(day, apps)| {
        let mut apps: Vec<AppMinutes> = apps.iter().map(|(app, seconds)| AppMinutes {
            app: app.clone(), minutes: seconds / 60, seconds: *seconds,
        }).collect();
        apps.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.app.cmp(&b.app)));
        let active_seconds = apps.iter().fold(0u32, |sum, app| sum.saturating_add(app.seconds));
        DayActivity {
            day: day.clone(),
            active_minutes: active_seconds / 60,
            active_seconds,
            apps,
        }
    }).collect();
    days.truncate(30);
    let mut site_days: Vec<DaySites> = runtime.data.site_seconds.iter().rev().map(|(day, sites)| {
        let mut sites: Vec<SiteSeconds> = sites.iter().map(|(domain, seconds)| SiteSeconds {
            domain: domain.clone(), seconds: *seconds,
        }).collect();
        sites.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.domain.cmp(&b.domain)));
        DaySites { day: day.clone(), sites }
    }).collect();
    site_days.truncate(30);
    ActivityStatus {
        supported: runtime.healthy, enabled: runtime.healthy && runtime.data.enabled,
        sites_enabled: runtime.healthy && runtime.data.sites_enabled,
        can_clear: supported() && runtime.key.is_some(),
        days, site_days,
        browser_connected: runtime.healthy && runtime.data.sites_enabled && crate::activity_sites::connected(),
        sharing_available: false,
    }
}

#[tauri::command]
pub fn device_activity_status<R: Runtime>(app: AppHandle<R>) -> Result<ActivityStatus, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    ensure_started(&dir);
    Ok(current_status())
}

#[tauri::command]
pub fn device_activity_set_enabled<R: Runtime>(
    app: AppHandle<R>, enabled: bool,
) -> Result<ActivityStatus, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    ensure_started(&dir);
    let mut state = STATE.lock();
    let runtime = state.as_mut().ok_or("activity_unavailable")?;
    apply_enabled(runtime, enabled)?;
    drop(state);
    Ok(current_status())
}

/// Turn sampling on or off. The change is kept only if it could be saved.
fn apply_enabled(runtime: &mut ActivityRuntime, enabled: bool) -> Result<(), String> {
    if !runtime.healthy { return Err("activity_unavailable".into()); }
    let previous = runtime.data.enabled;
    let previous_sites = runtime.data.sites_enabled;
    runtime.data.enabled = enabled;
    if !enabled { runtime.data.sites_enabled = false; crate::activity_sites::clear_signals(); }
    if !persist(runtime) {
        runtime.data.enabled = previous;
        runtime.data.sites_enabled = previous_sites;
        return Err("activity_save_failed".into());
    }
    Ok(())
}

#[tauri::command]
pub fn device_activity_set_sites_enabled<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<ActivityStatus, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    ensure_started(&dir);
    let mut state = STATE.lock();
    let runtime = state.as_mut().ok_or("activity_unavailable")?;
    if !runtime.healthy || (enabled && !runtime.data.enabled) { return Err("activity_unavailable".into()); }
    let previous = runtime.data.sites_enabled;
    runtime.data.sites_enabled = enabled;
    if !persist(runtime) {
        runtime.data.sites_enabled = previous;
        return Err("activity_save_failed".into());
    }
    if !enabled { crate::activity_sites::clear_signals(); }
    drop(state);
    Ok(current_status())
}

#[tauri::command]
pub fn device_activity_browser_token<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    ensure_started(&dir);
    let state = STATE.lock();
    if !state.as_ref().is_some_and(|s| s.healthy && s.data.enabled && s.data.sites_enabled) {
        return Err("website_tracking_disabled".into());
    }
    Ok(crate::activity_sites::token())
}

#[tauri::command]
pub fn device_activity_open_extension_folder<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    let candidates = [
        resource_dir.join("activity-manager-extension"),
        resource_dir.join("resources").join("activity-manager-extension"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("activity-manager-extension"),
    ];
    let folder = candidates.into_iter().find(|path| path.join("manifest.json").is_file())
        .ok_or("activity_extension_not_found")?;
    open::that(folder).map_err(|e| e.to_string())
}

/// Erase all recorded days (the on/off choice is kept). Restored if the save fails.
fn apply_clear(runtime: &mut ActivityRuntime) -> Result<(), String> {
    if !supported() || runtime.key.is_none() { return Err("activity_unavailable".into()); }
    let previous = std::mem::take(&mut runtime.data.days);
    let previous_seconds = std::mem::take(&mut runtime.data.precise_seconds);
    let previous_sites = std::mem::take(&mut runtime.data.site_seconds);
    if !persist(runtime) {
        runtime.data.days = previous;
        runtime.data.precise_seconds = previous_seconds;
        runtime.data.site_seconds = previous_sites;
        return Err("activity_save_failed".into());
    }
    runtime.healthy = true;
    Ok(())
}

#[tauri::command]
pub fn device_activity_clear<R: Runtime>(app: AppHandle<R>) -> Result<ActivityStatus, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    ensure_started(&dir);
    let mut state = STATE.lock();
    let runtime = state.as_mut().ok_or("activity_unavailable")?;
    apply_clear(runtime)?;
    drop(state);
    Ok(current_status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_in_app_only_and_thirty_day_boundary() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let mut data = ActivityData::default();
        assert!(!record(&mut data, day, "code.exe"));
        data.enabled = true;
        assert!(!record(&mut data, day, "nexus.exe"));
        assert!(!record(&mut data, day, "bitwarden.exe"));
        assert!(!record(&mut data, day, "Chrome - secret.example"));
        assert!(record(&mut data, day - ChronoDuration::days(29), "code.exe"));
        assert!(record(&mut data, day, "chrome.exe"));
        assert!(record(&mut data, day, "chrome.exe"));
        assert!(record(&mut data, day + ChronoDuration::days(1), "code.exe"));
        assert_eq!(data.days["2026-10-19"]["Chrome"], 2);
        assert!(!data.days.contains_key("2026-09-20"));
        assert!(data.days.contains_key("2026-10-20"));
        data.enabled = false;
        assert!(prune(&mut data, day + ChronoDuration::days(31)));
        assert!(data.days.is_empty());
    }

    #[test]
    fn website_counts_require_both_consents_and_are_separate_from_browser_app_time() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let mut data = ActivityData::default();
        assert!(!record_site_seconds(&mut data, day, "example.com", 2));
        data.enabled = true;
        assert!(!record_site_seconds(&mut data, day, "example.com", 2));
        data.sites_enabled = true;
        assert!(record_seconds(&mut data, day, "brave.exe", 2));
        assert!(record_site_seconds(&mut data, day, "example.com", 2));
        assert_eq!(data.precise_seconds["2026-10-22"]["Brave"], 2);
        assert_eq!(data.site_seconds["2026-10-22"]["example.com"], 2);
        assert!(record_split_site_seconds(&mut data, day, 1, "example.com", 3));
        assert_eq!(data.site_seconds["2026-10-21"]["example.com"], 2);
        assert_eq!(data.site_seconds["2026-10-22"]["example.com"], 3);
        assert!(prune(&mut data, day + ChronoDuration::days(31)));
        assert!(data.site_seconds.is_empty());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nexus_activity_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Windows-only checks use `supported()`; on other systems these assertions are skipped.
    fn windows() -> bool { supported() }

    #[test]
    fn password_managers_authenticators_and_sign_in_screens_are_never_recorded_in_any_variant() {
        for p in [
            "keepassxc.exe", "KeePass.exe", "1Password.exe", "1password7.exe", "Bitwarden.exe", "bitwarden-desktop.exe",
            "LastPass.exe", "dashlane.exe", "ProtonPass.exe", "NordPass.exe", "RoboForm.exe", "Enpass.exe",
            "Keeper.exe", "Authy Desktop.exe", "WinAuth-authenticator.exe", "VeraCrypt.exe", "Cryptomator.exe",
            "LockApp.exe", "LogonUI.exe", "consent.exe", "CredentialUIBroker.exe", "nexus.exe", "NEXUS.EXE",
        ] {
            assert_eq!(app_label(p), None, "{p} must not be recorded");
        }
    }

    #[test]
    fn app_labels_are_short_clean_names_and_anything_odd_is_dropped() {
        assert_eq!(app_label("CODE.EXE").as_deref(), Some("VS Code"));
        assert_eq!(app_label("chrome.exe").as_deref(), Some("Chrome"));
        assert!(app_label("some-tool_2.exe").is_some());
        for bad in ["", ".exe", "a b.exe", "Chrome - secret.example", "evil\\path.exe", "x/y.exe", "caf\u{e9}.exe"] {
            assert_eq!(app_label(bad), None, "{bad:?}");
        }
        assert_eq!(app_label(&format!("{}.exe", "a".repeat(81))), None);
        assert!(app_label(&format!("{}.exe", "a".repeat(80))).is_some());
    }

    #[test]
    fn installed_app_names_are_distinct_and_generic_windows_hosts_are_skipped() {
        for (process, name) in [
            ("studio64.exe", "Android Studio"),
            ("Resolve.exe", "DaVinci Resolve"),
            ("GitHubDesktop.exe", "GitHub Desktop"),
            ("pwsh.exe", "PowerShell"),
            ("qBittorrent.exe", "qBittorrent"),
            ("PowerToys.Run.exe", "PowerToys"),
            ("WhatsApp.exe", "WhatsApp"),
        ] {
            assert_eq!(app_label(process).as_deref(), Some(name), "{process}");
        }
        for process in ["ApplicationFrameHost.exe", "msedgewebview2.exe"] {
            assert_eq!(app_label(process), None, "{process}");
        }
    }

    #[test]
    fn only_minutes_with_recent_input_are_counted() {
        assert!(sampling_allowed(Some(0)));
        assert!(sampling_allowed(Some(ACTIVE_IDLE_LIMIT_SECS - 1)));
        assert!(!sampling_allowed(Some(ACTIVE_IDLE_LIMIT_SECS)));
        assert!(!sampling_allowed(Some(3600)));
        assert!(!sampling_allowed(None), "unknown idle time is not counted");
    }

    #[test]
    fn nothing_but_an_app_name_and_a_minute_count_is_ever_stored() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let mut data = ActivityData { enabled: true, ..Default::default() };
        assert!(record(&mut data, day, "chrome.exe"));
        let json = serde_json::to_string(&data).unwrap();
        assert_eq!(json, r#"{"enabled":true,"days":{"2026-10-20":{"Chrome":1}}}"#);
    }

    #[test]
    fn counting_never_overflows_and_a_disabled_store_records_nothing() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let mut data = ActivityData { enabled: true, ..Default::default() };
        data.days.entry("2026-10-20".into()).or_default().insert("Chrome".into(), u32::MAX);
        assert!(record(&mut data, day, "chrome.exe"));
        assert_eq!(data.days["2026-10-20"]["Chrome"], u32::MAX);
        data.enabled = false;
        assert!(!record(&mut data, day, "code.exe"));
    }

    #[test]
    fn precise_samples_preserve_short_sessions_and_existing_minute_history() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let mut data = ActivityData { enabled: true, ..Default::default() };
        assert!(record(&mut data, day, "chrome.exe"));
        assert!(record_seconds(&mut data, day, "chrome.exe", 17));
        assert!(record_seconds(&mut data, day, "code.exe", 8));
        assert!(!record_seconds(&mut data, day, "bitwarden.exe", 10));
        let runtime = ActivityRuntime { path: PathBuf::new(), key: None, data, healthy: true };
        let status = status_of(&runtime);
        assert_eq!(status.days[0].active_seconds, 85);
        assert_eq!(status.days[0].active_minutes, 1);
        assert_eq!(status.days[0].apps[0].app, "Chrome");
        assert_eq!(status.days[0].apps[0].seconds, 77);
        assert_eq!(status.days[0].apps[1].seconds, 8);
    }

    #[test]
    fn a_sample_crossing_midnight_is_split_between_calendar_days() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let mut data = ActivityData { enabled: true, ..Default::default() };
        assert!(record_split_seconds(&mut data, today, 1, "chrome.exe", 3));
        assert_eq!(data.precise_seconds["2026-10-21"]["Chrome"], 2);
        assert_eq!(data.precise_seconds["2026-10-22"]["Chrome"], 1);
        assert_eq!(status_of(&ActivityRuntime {
            path: PathBuf::new(), key: None, data, healthy: true,
        }).days.iter().map(|day| day.active_seconds).sum::<u32>(), 3);
    }

    #[test]
    fn suspend_and_wall_clock_jumps_do_not_count_as_app_time() {
        let mut remainder = Duration::ZERO;
        assert_eq!(accountable_seconds(Duration::from_millis(1_900), ChronoDuration::milliseconds(1_900), &mut remainder), 1);
        assert_eq!(accountable_seconds(Duration::from_millis(2_100), ChronoDuration::milliseconds(2_100), &mut remainder), 3);
        assert_eq!(accountable_seconds(Duration::from_secs(10), ChronoDuration::seconds(10), &mut remainder), 0);
        assert_eq!(remainder, Duration::ZERO);
        assert_eq!(accountable_seconds(Duration::from_secs(2), ChronoDuration::hours(1), &mut remainder), 0);
        assert_eq!(accountable_seconds(Duration::from_secs(2), ChronoDuration::seconds(-1), &mut remainder), 0);
        assert_eq!(remainder, Duration::ZERO);
    }

    #[test]
    fn the_sealed_file_round_trips_and_holds_no_plain_text() {
        if !windows() { return; }
        let dir = temp_dir("roundtrip");
        let mut rt = load(&dir);
        assert!(rt.healthy);
        rt.data.enabled = true;
        rt.data.sites_enabled = true;
        record(&mut rt.data, Local::now().date_naive(), "code.exe");
        record_site_seconds(&mut rt.data, Local::now().date_naive(), "example.com", 8);
        assert!(persist(&rt));
        let bytes = std::fs::read(dir.join(FILE_NAME)).unwrap();
        assert!(crate::memcore::crypto::is_sealed(&bytes));
        assert!(!String::from_utf8_lossy(&bytes).contains("VS Code"));
        assert!(!String::from_utf8_lossy(&bytes).contains("example.com"));
        let again = load(&dir);
        assert!(again.healthy && again.data.enabled);
        assert_eq!(again.data.days.values().next().unwrap()["VS Code"], 1);
        assert_eq!(again.data.site_seconds.values().next().unwrap()["example.com"], 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_or_foreign_file_stops_sampling_and_is_not_overwritten() {
        if !windows() { return; }
        let dir = temp_dir("tamper");
        std::fs::write(dir.join(FILE_NAME), b"not a sealed file at all").unwrap();
        let rt = load(&dir);
        assert!(!rt.healthy, "an unreadable store must stop sampling");
        assert!(rt.data.days.is_empty() && !status_of(&rt).enabled);
        assert_eq!(std::fs::read(dir.join(FILE_NAME)).unwrap(), b"not a sealed file at all", "the file is left alone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expired_days_are_removed_from_disk_at_load_not_only_in_memory() {
        if !windows() { return; }
        let dir = temp_dir("expire");
        let mut rt = load(&dir);
        let today = Local::now().date_naive();
        rt.data.enabled = true;
        rt.data.days.entry((today - ChronoDuration::days(45)).format("%Y-%m-%d").to_string()).or_default().insert("Old".into(), 5);
        rt.data.days.entry(today.format("%Y-%m-%d").to_string()).or_default().insert("New".into(), 1);
        assert!(persist(&rt));
        let again = load(&dir);
        assert_eq!(again.data.days.len(), 1);
        let on_disk = crate::memcore::crypto::open(&again.key.unwrap(), &std::fs::read(dir.join(FILE_NAME)).unwrap()).unwrap();
        assert!(!String::from_utf8_lossy(&on_disk).contains("Old"), "the expired day must be gone from the file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn turning_it_on_or_off_is_kept_only_if_it_could_be_saved() {
        if !windows() { return; }
        let dir = temp_dir("toggle");
        let mut rt = load(&dir);
        assert!(apply_enabled(&mut rt, true).is_ok() && rt.data.enabled);
        rt.data.sites_enabled = true;
        assert!(apply_enabled(&mut rt, false).is_ok() && !rt.data.enabled);
        assert!(!rt.data.sites_enabled, "app opt-out also ends website consent");
        // a save that cannot succeed (the target path is a directory) rolls the choice back
        rt.path = dir.clone();
        assert_eq!(apply_enabled(&mut rt, true), Err("activity_save_failed".to_string()));
        assert!(!rt.data.enabled, "the choice must roll back when it cannot be saved");
        rt.healthy = false;
        assert_eq!(apply_enabled(&mut rt, true), Err("activity_unavailable".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_erases_every_day_keeps_the_choice_and_restores_on_failure() {
        if !windows() { return; }
        let dir = temp_dir("clear");
        let mut rt = load(&dir);
        rt.data.enabled = true;
        rt.data.sites_enabled = true;
        record(&mut rt.data, Local::now().date_naive(), "code.exe");
        record_seconds(&mut rt.data, Local::now().date_naive(), "chrome.exe", 9);
        record_site_seconds(&mut rt.data, Local::now().date_naive(), "example.com", 9);
        assert!(persist(&rt));
        assert!(apply_clear(&mut rt).is_ok());
        assert!(rt.data.days.is_empty() && rt.data.precise_seconds.is_empty() && rt.data.site_seconds.is_empty() && rt.data.enabled);
        assert!(load(&dir).data.days.is_empty() && load(&dir).data.precise_seconds.is_empty(), "the erase reached the disk");
        record(&mut rt.data, Local::now().date_naive(), "code.exe");
        record_seconds(&mut rt.data, Local::now().date_naive(), "chrome.exe", 7);
        record_site_seconds(&mut rt.data, Local::now().date_naive(), "example.com", 7);
        rt.path = dir.clone();
        assert_eq!(apply_clear(&mut rt), Err("activity_save_failed".to_string()));
        assert_eq!(rt.data.days.len(), 1, "data comes back when the erase could not be saved");
        assert_eq!(rt.data.precise_seconds.len(), 1, "short sessions come back too");
        assert_eq!(rt.data.site_seconds.len(), 1, "site time comes back too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live check on the real machine (run with `--ignored`): the foreground program and the idle clock can
    /// be read, and what would be stored is only a clean app name.
    #[test]
    #[ignore = "reads the real foreground window; run by hand"]
    fn live_foreground_sample_is_a_clean_app_name() {
        if !windows() { return; }
        let process = crate::browser_url::foreground_process_name().expect("a foreground process");
        let idle = crate::memcore::resume::user_idle_secs();
        println!("[PROOF] foreground={process:?} idle={idle:?} label={:?}", app_label(&process));
        assert!(idle.is_some(), "the idle clock must be readable");
        if let Some(label) = app_label(&process) {
            assert!(label.len() <= 80 && !label.contains('\\') && !label.contains('/'));
        }
    }

    #[test]
    fn the_status_lists_newest_day_first_biggest_app_first_at_most_thirty_days_and_never_offers_sharing() {
        if !windows() { return; }
        let dir = temp_dir("status");
        let mut rt = load(&dir);
        rt.data.enabled = true;
        let base = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        for i in 0..35 {
            let d = (base + ChronoDuration::days(i)).format("%Y-%m-%d").to_string();
            let e = rt.data.days.entry(d).or_default();
            e.insert("B".into(), 5);
            e.insert("A".into(), 5);
            e.insert("Z".into(), 9);
        }
        let s = status_of(&rt);
        assert_eq!(s.days.len(), 30);
        assert_eq!(s.days[0].day, "2026-09-04");
        let apps: Vec<_> = s.days[0].apps.iter().map(|a| a.app.as_str()).collect();
        assert_eq!(apps, ["Z", "A", "B"], "most minutes first, ties by name");
        assert_eq!(s.days[0].active_minutes, 19);
        assert!(!s.sharing_available && s.enabled && s.supported && s.can_clear);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
