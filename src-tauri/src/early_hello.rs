//! The first spoken hello at logon, played as soon as the DESKTOP is shown (doc 158, phase P0).
//!
//! The full app needs 5-25 s before it can speak (window, tray, speech engines). The greeting itself is a cached clip in
//! the user's voice, so a small thread started at the very top of `run()` can play it the moment Windows has finished
//! showing the desktop: not while the logon animation or the lock screen is up, and before the heavy startup is done.
//! Only for the logon-task launch (`--background`), never for a restart from a terminal.
//!
//! The pure parts (which line for which time of day, no repeat, the script file, "is the desktop shown") are tested here;
//! the Windows glue and the playback are at the bottom.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Morning,
    Afternoon,
    /// From 18:00 it is night: "Welcome back", never "Good evening" ("Good night" is the farewell).
    Night,
}

/// 05:00-11:59 morning, 12:00-17:59 afternoon, 18:00-04:59 night.
pub fn period_for_hour(hour: u32) -> Period {
    match hour {
        5..=11 => Period::Morning,
        12..=17 => Period::Afternoon,
        _ => Period::Night,
    }
}

impl Period {
    pub fn key(self) -> &'static str {
        match self {
            Period::Morning => "morning",
            Period::Afternoon => "afternoon",
            Period::Night => "night",
        }
    }
}

/// No second early hello within this many seconds (a restart while developing must stay quiet).
pub const MIN_GAP_SECS: i64 = 2 * 3600;

/// Is a hello due? Nothing played yet, or long enough ago, or the clock went backwards.
pub fn due(last_played: Option<i64>, now: i64) -> bool {
    last_played.map_or(true, |t| now - t >= MIN_GAP_SECS || now < t)
}

pub const SCRIPT_FILE: &str = "hello_script.json";
pub const SCRIPT_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Script {
    pub version: u32,
    /// The voice the clips are rendered in (the edge voice id, e.g. "en-US-AvaNeural").
    pub voice: String,
    /// period key -> spoken lines, exactly as they are cached ("Good morning, sir").
    pub lines: BTreeMap<String, Vec<String>>,
    /// period key -> index of the variant played last time.
    pub last_index: BTreeMap<String, usize>,
    /// Unix seconds of the last early hello.
    pub last_played: Option<i64>,
}

/// The lines (their clips are rendered once by the voice cache warm-up, `tts::CACHED_PHRASES`).
pub fn default_script(voice: &str) -> Script {
    let mut lines = BTreeMap::new();
    lines.insert("morning".to_string(), vec!["Good morning, sir".to_string()]);
    lines.insert("afternoon".to_string(), vec!["Good afternoon, sir".to_string()]);
    lines.insert("night".to_string(), vec!["Welcome back, sir".to_string(), "Good to have you back, sir".to_string()]);
    Script { version: SCRIPT_VERSION, voice: voice.to_string(), lines, last_index: BTreeMap::new(), last_played: None }
}

/// The line for `period` and its index; with two or more variants never the one played last time.
pub fn pick(script: &Script, period: Period) -> Option<(String, usize)> {
    let lines = script.lines.get(period.key()).filter(|l| !l.is_empty())?;
    let idx = match script.last_index.get(period.key()) {
        Some(last) if lines.len() > 1 => (*last + 1) % lines.len(),
        _ => 0,
    };
    Some((lines[idx].clone(), idx))
}

pub fn after_play(script: &mut Script, period: Period, idx: usize, now: i64) {
    script.last_index.insert(period.key().to_string(), idx);
    script.last_played = Some(now);
}

/// Read the script. Anything unusable gives the default; a script of the same version keeps `last_played` even when the
/// voice changed (a voice change must not cause an extra hello).
pub fn load_script(dir: &Path, voice: &str) -> Script {
    let bytes = std::fs::read(dir.join(SCRIPT_FILE)).ok();
    let Some(read) = bytes.as_deref().and_then(|b| serde_json::from_slice::<Script>(b).ok()) else {
        // an older script has a different shape: keep only its `last_played` (a format change must not drop the 2 h guard)
        let mut fresh = default_script(voice);
        fresh.last_played = bytes
            .as_deref()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
            .and_then(|v| v.get("last_played").and_then(|x| x.as_i64()));
        return fresh;
    };
    if read.version != SCRIPT_VERSION {
        let mut fresh = default_script(voice);
        fresh.last_played = read.last_played;
        return fresh;
    }
    if read.voice != voice || read.lines.is_empty() {
        let mut fresh = default_script(voice);
        fresh.last_played = read.last_played;
        return fresh;
    }
    read
}

/// Write the script atomically (best effort).
pub fn save_script(dir: &Path, script: &Script) {
    let Ok(json) = serde_json::to_vec_pretty(script) else { return };
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join(format!("{SCRIPT_FILE}.tmp"));
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(SCRIPT_FILE));
    }
}

/// The desktop is shown only when the input desktop is the normal one (not the logon / lock screen), the taskbar is
/// visible and the desktop window is visible.
pub fn desktop_ready(input_desktop_is_default: bool, tray_visible: bool, progman_visible: bool) -> bool {
    input_desktop_is_default && tray_visible && progman_visible
}

/// Only for the logon-task launch (`--background`), and only when the setting is on.
pub fn should_start(is_background: bool, enabled: bool) -> bool {
    is_background && enabled
}

// ─── state shared with the normal boot hello ─────────────────────────

use std::sync::atomic::{AtomicU8, Ordering};

const NOT_STARTED: u8 = 0;
const PENDING: u8 = 1;
const PLAYED: u8 = 2;
const GAVE_UP: u8 = 3;
static STATE: AtomicU8 = AtomicU8::new(NOT_STARTED);

/// True while the early hello is waiting for the desktop or has played: the normal hello must not speak a second one.
pub fn owns_the_hello() -> bool {
    matches!(STATE.load(Ordering::SeqCst), PENDING | PLAYED)
}

/// True once the early hello has actually been spoken.
pub fn played_this_run() -> bool {
    STATE.load(Ordering::SeqCst) == PLAYED
}

// ─── Windows glue ────────────────────────────────────────────────────

/// How long the thread waits for the desktop before giving up (the normal hello then speaks).
const DESKTOP_WAIT_SECS: u64 = 90;
const POLL_MS: u64 = 100;
/// A short settle after the desktop appears so the sound does not start on the very first frame of the shell.
const SETTLE_MS: u64 = 250;
/// How long to keep trying to open the audio device (it can be late at logon).
const AUDIO_WAIT_SECS: u64 = 10;

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The settings the early hello needs, read straight from settings.json (the app's own settings are not loaded yet).
#[derive(Debug, Clone, PartialEq)]
pub struct EarlySettings {
    pub enabled: bool,
    pub voice: String,
    /// The speech volume the app sets while it talks (percent). 0 = leave the system volume alone.
    pub volume_pct: u8,
    /// The capsule style chosen in the Command Hub (`orbCapsuleStyle`): "pebble" (default) or "classic".
    pub classic: bool,
    /// The pebble's faint rim line (`orbBaseRimOpacity` / 100).
    pub rim_alpha: f32,
}

/// Pure: the settings out of the parsed settings.json (`Null` = defaults).
pub fn parse_settings(v: &serde_json::Value) -> EarlySettings {
    let num = |k: &str| v.get(k).and_then(|x| x.as_f64());
    let classic = v.get("orbCapsuleStyle").and_then(|x| x.as_str()).map_or(false, |s| s.eq_ignore_ascii_case("classic"));
    EarlySettings {
        enabled: v.get("earlyHello").and_then(|x| x.as_bool()).unwrap_or(true),
        voice: v.get("edgeTtsVoice").and_then(|x| x.as_str()).unwrap_or("en-US-AvaNeural").to_string(),
        volume_pct: num("ttsVolume").map_or(75, |n| n.clamp(0.0, 100.0) as u8),
        classic,
        rim_alpha: num("orbBaseRimOpacity").map_or(0.25, |n| (n / 100.0).clamp(0.0, 1.0) as f32),
    }
}

fn read_settings(dir: &Path) -> EarlySettings {
    let v: serde_json::Value =
        std::fs::read(dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::Value::Null);
    parse_settings(&v)
}

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;

    #[link(name = "user32")]
    extern "system" {
        fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> *mut c_void;
        fn CloseDesktop(desktop: *mut c_void) -> i32;
        fn GetUserObjectInformationW(obj: *mut c_void, index: i32, info: *mut c_void, len: u32, needed: *mut u32) -> i32;
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut c_void;
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
    }

    const DESKTOP_READOBJECTS: u32 = 0x0001;
    const UOI_NAME: i32 = 2;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Is the desktop that receives input the normal "Default" one? The logon animation and the lock screen run on
    /// "Winlogon"; if it cannot be opened at all, that is not the normal desktop either.
    pub fn input_desktop_is_default() -> bool {
        // SAFETY: plain user32 calls; the handle is closed before returning, the buffer is sized and owned here.
        unsafe {
            let d = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
            if d.is_null() {
                return false;
            }
            let mut buf = [0u16; 64];
            let mut needed = 0u32;
            let ok = GetUserObjectInformationW(d, UOI_NAME, buf.as_mut_ptr() as *mut c_void, (buf.len() * 2) as u32, &mut needed);
            CloseDesktop(d);
            if ok == 0 {
                return false;
            }
            let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..len]).eq_ignore_ascii_case("Default")
        }
    }

    fn class_visible(class: &str) -> bool {
        let c = wide(class);
        // SAFETY: `c` is a NUL-terminated UTF-16 string that outlives the calls.
        unsafe {
            let h = FindWindowW(c.as_ptr(), std::ptr::null());
            !h.is_null() && IsWindowVisible(h) != 0
        }
    }

    pub fn tray_visible() -> bool {
        class_visible("Shell_TrayWnd")
    }

    pub fn progman_visible() -> bool {
        class_visible("Progman")
    }
}

#[cfg(target_os = "windows")]
fn desktop_shown() -> bool {
    desktop_ready(win::input_desktop_is_default(), win::tray_visible(), win::progman_visible())
}

#[cfg(not(target_os = "windows"))]
fn desktop_shown() -> bool {
    false
}

/// Start the thread when this is the logon launch. Cheap and never blocks `run()`.
pub fn start_if_logon() {
    let Some(dir) = crate::app_dirs::nexus_data_dir() else { return };
    let cfg = read_settings(&dir);
    let is_background = std::env::args().any(|a| a == "--background");
    if !should_start(is_background, cfg.enabled) {
        if is_background {
            gave_up("the earlyHello setting is off");
        }
        return;
    }
    let mut script = load_script(&dir, &cfg.voice);
    if !due(script.last_played, now_secs()) {
        gave_up("a hello was already played less than 2 hours ago");
        return;
    }
    STATE.store(PENDING, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name("early-hello".into()).spawn(move || {
        let played = run(&dir, &cfg, &mut script);
        STATE.store(if played { PLAYED } else { GAVE_UP }, Ordering::SeqCst);
    });
    if spawned.is_err() {
        STATE.store(GAVE_UP, Ordering::SeqCst);
    }
}

/// A line that is always cached (the stock `Hello, sir`), used when the clip for the part of the day is not on disk yet
/// (first logon after an update that added a line, or a freshly chosen voice). A plain hello beats silence.
const FALLBACK_LINE: &str = "Hello, sir";

/// The clip to play: the picked line, else another variant of the same part of the day, else the stock hello.
fn clip_for(dir: &Path, cfg: &EarlySettings, script: &Script, period: Period) -> Option<(String, usize, crate::tts_cache_disk::Loaded)> {
    let cache = crate::tts_cache_disk::dir_for(dir);
    let (text, idx) = pick(script, period)?;
    let mut candidates: Vec<String> = vec![text];
    candidates.extend(script.lines.get(period.key()).into_iter().flatten().cloned());
    candidates.push(FALLBACK_LINE.to_string());
    for (n, line) in candidates.iter().enumerate() {
        if let Some(clip) = crate::tts_cache_disk::load(&cache, &cfg.voice, line) {
            if n > 0 {
                println!("[EARLY-HELLO] the clip for '{}' is not cached yet: playing '{line}' instead", candidates[0]);
            }
            return Some((line.clone(), idx, clip));
        }
    }
    None
}

/// Every way the early hello can end without speaking leaves a marker in the boot timeline and a log line.
fn gave_up(reason: &str) {
    println!("[EARLY-HELLO] not played: {reason}");
    crate::boot::mark_with("early_hello_gave_up", reason);
}

/// While waiting for the desktop: open the audio device (it can be late at logon) so that the sound can start the moment
/// the desktop appears, instead of paying the device start-up then (measured 2026-11-02: 3.7 s from desktop to sound).
fn open_audio() -> Option<(rodio::OutputStream, rodio::OutputStreamHandle)> {
    rodio::OutputStream::try_default().ok()
}

/// Wait for the desktop, then play. `true` when the greeting was spoken.
fn run(dir: &Path, cfg: &EarlySettings, script: &mut Script) -> bool {
    use chrono::Timelike;
    let started = std::time::Instant::now();
    let mut period = period_for_hour(chrono::Local::now().hour());
    // Everything slow is done BEFORE the desktop appears: read the clip, open the audio device.
    let mut clip = clip_for(dir, cfg, script, period);
    if clip.is_none() {
        gave_up(&format!("no cached clip at all for the {} hello in voice {}", period.key(), cfg.voice));
        return false;
    }
    let mut audio: Option<(rodio::OutputStream, rodio::OutputStreamHandle)> = None;
    while !desktop_shown() {
        if started.elapsed().as_secs() >= DESKTOP_WAIT_SECS {
            gave_up(&format!("the desktop never appeared within {DESKTOP_WAIT_SECS}s"));
            return false;
        }
        if audio.is_none() {
            audio = open_audio();
            if audio.is_some() {
                crate::boot::mark("early_hello_audio_ready");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
    }
    crate::boot::mark("early_hello_desktop_shown");
    std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));
    // The part of the day can have changed while waiting.
    let now_period = period_for_hour(chrono::Local::now().hour());
    if now_period != period {
        period = now_period;
        clip = clip_for(dir, cfg, script, period);
    }
    let Some((text, idx, clip)) = clip else {
        gave_up("no clip for the part of the day after the wait");
        return false;
    };
    // The audio device can still be late: keep trying for a few more seconds.
    let (stream, handle) = loop {
        if let Some(pair) = audio.take().or_else(open_audio) {
            break pair;
        }
        if started.elapsed().as_secs() >= DESKTOP_WAIT_SECS + AUDIO_WAIT_SECS {
            gave_up("no audio device");
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    };
    let Ok(sink) = rodio::Sink::try_new(&handle) else {
        gave_up("could not open an audio sink");
        return false;
    };
    // Same rule as every spoken line: the system volume is set to the speech volume while NEXUS talks and restored after.
    let volume_lease = if cfg.volume_pct > 0 { crate::volume::acquire_volume_lease(cfg.volume_pct as f32 / 100.0) } else { None };
    // From here the normal boot hello must stay silent, whatever happens next.
    STATE.store(PLAYED, Ordering::SeqCst);
    // The orb and the caption, drawn natively because the app's own window does not exist yet.
    let env = crate::tts::compute_envelope(&clip.samples, clip.sample_rate, 20);
    crate::early_orb::show(crate::early_orb::Show {
        style: if cfg.classic { crate::early_orb::Style::Classic } else { crate::early_orb::Style::Pebble },
        rim_alpha: cfg.rim_alpha,
        text: text.clone(),
        words: clip.words.clone(),
        env,
        env_frame_ms: 20,
        clip_ms: clip.total_ms,
    });
    crate::boot::mark("early_hello_audio");
    println!("[EARLY-HELLO] '{text}' ({}) volume {}% {} ms after the thread started", cfg.voice, cfg.volume_pct, started.elapsed().as_millis());
    crate::boot::mark_with("early_hello_playing", &text);
    sink.append(rodio::buffer::SamplesBuffer::new(1, clip.sample_rate, clip.samples));
    after_play(script, period, idx, now_secs());
    save_script(dir, script);
    sink.sleep_until_end();
    drop(volume_lease); // restores the user's own volume (after the short grace the volume module keeps)
    drop(stream);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("nexus_early_hello_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn the_three_parts_of_the_day_and_night_starts_at_six() {
        for (h, p) in [
            (4, Period::Night),
            (5, Period::Morning),
            (11, Period::Morning),
            (12, Period::Afternoon),
            (17, Period::Afternoon),
            (18, Period::Night),
            (19, Period::Night),
            (21, Period::Night),
            (23, Period::Night),
            (0, Period::Night),
        ] {
            assert_eq!(period_for_hour(h), p, "hour {h}");
        }
        assert_eq!(Period::Night.key(), "night");
    }

    #[test]
    fn after_six_it_says_welcome_back_never_good_evening_and_the_variants_alternate() {
        let mut s = default_script("v");
        for p in [Period::Morning, Period::Afternoon, Period::Night] {
            let lines = &s.lines[p.key()];
            assert!(lines.iter().all(|l| !l.to_lowercase().contains("evening") && !l.to_lowercase().contains("good night")), "{lines:?}");
        }
        assert_eq!(s.lines["night"], vec!["Welcome back, sir".to_string(), "Good to have you back, sir".to_string()]);
        let (first, i0) = pick(&s, Period::Night).unwrap();
        assert_eq!(first, "Welcome back, sir");
        after_play(&mut s, Period::Night, i0, 1);
        assert_eq!(pick(&s, Period::Night).unwrap().0, "Good to have you back, sir", "not the same line twice in a row");
    }

    #[test]
    fn an_older_script_with_an_evening_key_is_replaced_by_the_new_lines() {
        let dir = tmp("v1");
        let mut old = default_script("v");
        old.version = 1;
        old.lines.insert("evening".into(), vec!["Good evening, sir".into()]);
        save_script(&dir, &old);
        let got = load_script(&dir, "v");
        assert_eq!(got.lines, default_script("v").lines, "the old lines are replaced by the new ones");
        assert!(!got.lines.contains_key("evening"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hello_is_due_once_per_two_hours() {
        assert!(due(None, 1_000));
        assert!(!due(Some(1_000), 1_000 + 3_600));
        assert!(due(Some(1_000), 1_000 + MIN_GAP_SECS));
        assert!(due(Some(5_000), 1_000), "the clock went backwards");
    }

    #[test]
    fn variants_never_repeat_back_to_back() {
        let mut s = default_script("v");
        assert_eq!(pick(&s, Period::Morning), Some(("Good morning, sir".to_string(), 0)));
        s.lines.insert("morning".into(), vec!["Good morning, sir".into(), "Morning, sir".into()]);
        let (t, i) = pick(&s, Period::Morning).unwrap();
        assert_eq!((t.as_str(), i), ("Good morning, sir", 0), "nothing played yet");
        after_play(&mut s, Period::Morning, i, 10);
        assert_eq!(pick(&s, Period::Morning).unwrap().1, 1);
        after_play(&mut s, Period::Morning, 1, 20);
        assert_eq!(pick(&s, Period::Morning).unwrap().1, 0, "wraps");
        assert_eq!(s.last_played, Some(20));
        s.lines.insert("night".into(), vec![]);
        assert_eq!(pick(&s, Period::Night), None, "an empty period has nothing to say");
    }

    #[test]
    fn the_script_survives_a_round_trip_and_bad_files_give_the_default() {
        let dir = tmp("script");
        let mut s = default_script("en-US-AvaNeural");
        after_play(&mut s, Period::Afternoon, 0, 777);
        save_script(&dir, &s);
        assert_eq!(load_script(&dir, "en-US-AvaNeural"), s);
        // a different voice: new lines, but last_played kept (no extra hello after a voice change)
        let other = load_script(&dir, "en-GB-RyanNeural");
        assert_eq!(other.voice, "en-GB-RyanNeural");
        assert_eq!(other.last_played, Some(777));
        assert!(other.last_index.is_empty());
        // corrupt file
        std::fs::write(dir.join(SCRIPT_FILE), b"not json").unwrap();
        assert_eq!(load_script(&dir, "v"), default_script("v"));
        // a different version: the lines are replaced, `last_played` is kept
        let mut old = default_script("v");
        old.version = 99;
        old.last_played = Some(5);
        save_script(&dir, &old);
        let reloaded = load_script(&dir, "v");
        assert_eq!(reloaded.lines, default_script("v").lines);
        assert_eq!(reloaded.last_played, Some(5));
        // missing file
        let empty = tmp("script_missing");
        assert_eq!(load_script(&empty, "v"), default_script("v"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn the_desktop_counts_as_shown_only_when_everything_is_up() {
        for bits in 0..8u8 {
            let (a, b, c) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            assert_eq!(desktop_ready(a, b, c), a && b && c, "{a} {b} {c}");
        }
    }

    #[test]
    fn only_the_logon_launch_with_the_setting_on_starts_it() {
        for (bg, on) in [(false, false), (false, true), (true, false), (true, true)] {
            assert_eq!(should_start(bg, on), bg && on);
        }
    }

    #[test]
    fn a_missing_clip_falls_back_to_the_same_period_then_to_the_stock_hello_and_never_to_silence() {
        let dir = tmp("clips");
        let cache = crate::tts_cache_disk::dir_for(&dir);
        let cfg = parse_settings(&serde_json::json!({"edgeTtsVoice": "v"}));
        let script = default_script("v");
        let save = |text: &str| {
            crate::tts_cache_disk::save(&cache, "v", text, crate::tts_cache_disk::Origin::Edge, &[0.1; 800], 16_000, 50, &[]).unwrap();
        };
        // nothing cached: nothing to play
        assert!(clip_for(&dir, &cfg, &script, Period::Night).is_none());
        // only the stock hello is cached: it is used
        save(FALLBACK_LINE);
        assert_eq!(clip_for(&dir, &cfg, &script, Period::Night).unwrap().0, FALLBACK_LINE);
        // the other night variant is cached: preferred over the stock hello
        save("Good to have you back, sir");
        assert_eq!(clip_for(&dir, &cfg, &script, Period::Night).unwrap().0, "Good to have you back, sir");
        // the picked line is cached: it wins
        save("Welcome back, sir");
        assert_eq!(clip_for(&dir, &cfg, &script, Period::Night).unwrap().0, "Welcome back, sir");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_settings_come_from_settings_json_with_safe_defaults() {
        let d = parse_settings(&serde_json::Value::Null);
        assert_eq!((d.enabled, d.voice.as_str(), d.volume_pct, d.classic, d.rim_alpha), (true, "en-US-AvaNeural", 75, false, 0.25));
        let v: serde_json::Value = serde_json::from_str(
            r#"{"earlyHello":false,"edgeTtsVoice":"en-GB-RyanNeural","ttsVolume":70,"orbCapsuleStyle":"classic","orbBaseRimOpacity":40}"#,
        )
        .unwrap();
        let s = parse_settings(&v);
        assert_eq!((s.enabled, s.voice.as_str(), s.volume_pct, s.classic, s.rim_alpha), (false, "en-GB-RyanNeural", 70, true, 0.4));
        // nonsense is clamped, not trusted
        let bad: serde_json::Value = serde_json::from_str(r#"{"ttsVolume":900,"orbBaseRimOpacity":999}"#).unwrap();
        let b = parse_settings(&bad);
        assert_eq!((b.volume_pct, b.rim_alpha), (100, 1.0));
        // a speech volume of 0 means "leave the system volume alone"
        assert_eq!(parse_settings(&serde_json::json!({"ttsVolume": 0})).volume_pct, 0);
    }

    #[test]
    fn the_normal_hello_stays_silent_while_the_early_one_is_waiting_or_has_played() {
        STATE.store(NOT_STARTED, Ordering::SeqCst);
        assert!(!owns_the_hello());
        STATE.store(PENDING, Ordering::SeqCst);
        assert!(owns_the_hello() && !played_this_run());
        STATE.store(PLAYED, Ordering::SeqCst);
        assert!(owns_the_hello() && played_this_run());
        STATE.store(GAVE_UP, Ordering::SeqCst);
        assert!(!owns_the_hello(), "when the early hello gave up the app speaks as before");
        STATE.store(NOT_STARTED, Ordering::SeqCst);
    }

    /// Run by hand on a signed-in desktop: the real Windows checks must say "shown".
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "needs a signed-in interactive desktop"]
    fn on_a_live_desktop_the_windows_checks_say_shown() {
        assert!(win::input_desktop_is_default(), "the input desktop should be Default");
        assert!(win::tray_visible(), "the taskbar should be visible");
        assert!(win::progman_visible(), "the desktop window should be visible");
        assert!(desktop_shown());
    }
}
