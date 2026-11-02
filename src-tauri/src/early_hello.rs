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
    Evening,
    Night,
}

/// 05:00-11:59 morning, 12:00-17:59 afternoon, 18:00-21:59 evening, otherwise (22:00-04:59) night.
pub fn period_for_hour(hour: u32) -> Period {
    match hour {
        5..=11 => Period::Morning,
        12..=17 => Period::Afternoon,
        18..=21 => Period::Evening,
        _ => Period::Night,
    }
}

impl Period {
    pub fn key(self) -> &'static str {
        match self {
            Period::Morning => "morning",
            Period::Afternoon => "afternoon",
            Period::Evening => "evening",
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
pub const SCRIPT_VERSION: u32 = 1;

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

/// The lines whose clips every install already has (the boot hello renders them).
pub fn default_script(voice: &str) -> Script {
    let mut lines = BTreeMap::new();
    lines.insert("morning".to_string(), vec!["Good morning, sir".to_string()]);
    lines.insert("afternoon".to_string(), vec!["Good afternoon, sir".to_string()]);
    lines.insert("evening".to_string(), vec!["Good evening, sir".to_string()]);
    lines.insert("night".to_string(), vec!["Good evening, sir".to_string()]);
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
    let Some(read) = std::fs::read(dir.join(SCRIPT_FILE)).ok().and_then(|b| serde_json::from_slice::<Script>(&b).ok()) else {
        return default_script(voice);
    };
    if read.version != SCRIPT_VERSION {
        return default_script(voice);
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

/// Settings read straight from settings.json (the app's own settings are not loaded yet this early).
fn read_settings(dir: &Path) -> (bool, String) {
    let v: serde_json::Value =
        std::fs::read(dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::Value::Null);
    let enabled = v.get("earlyHello").and_then(|x| x.as_bool()).unwrap_or(true);
    let voice = v.get("edgeTtsVoice").and_then(|x| x.as_str()).unwrap_or("en-US-AvaNeural").to_string();
    (enabled, voice)
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
    let (enabled, voice) = read_settings(&dir);
    if !should_start(std::env::args().any(|a| a == "--background"), enabled) {
        return;
    }
    let mut script = load_script(&dir, &voice);
    if !due(script.last_played, now_secs()) {
        return;
    }
    STATE.store(PENDING, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name("early-hello".into()).spawn(move || {
        let played = run(&dir, &voice, &mut script);
        STATE.store(if played { PLAYED } else { GAVE_UP }, Ordering::SeqCst);
    });
    if spawned.is_err() {
        STATE.store(GAVE_UP, Ordering::SeqCst);
    }
}

/// Wait for the desktop, then play. `true` when the greeting was spoken.
fn run(dir: &Path, voice: &str, script: &mut Script) -> bool {
    use chrono::Timelike;
    let started = std::time::Instant::now();
    while !desktop_shown() {
        if started.elapsed().as_secs() >= DESKTOP_WAIT_SECS {
            println!("[EARLY-HELLO] the desktop never appeared within {DESKTOP_WAIT_SECS}s: leaving the hello to the app");
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
    }
    crate::boot::mark("early_hello_desktop_shown");
    std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));
    let period = period_for_hour(chrono::Local::now().hour());
    let Some((text, idx)) = pick(script, period) else { return false };
    let cache = crate::tts_cache_disk::dir_for(dir);
    let Some(clip) = crate::tts_cache_disk::load(&cache, voice, &text) else {
        println!("[EARLY-HELLO] no cached clip for '{text}' in voice {voice}: leaving the hello to the app");
        return false;
    };
    // The audio device can be late at logon: keep trying for a few seconds.
    let (stream, handle) = loop {
        match rodio::OutputStream::try_default() {
            Ok(pair) => break pair,
            Err(e) => {
                if started.elapsed().as_secs() >= DESKTOP_WAIT_SECS + AUDIO_WAIT_SECS {
                    println!("[EARLY-HELLO] no audio device: {e}");
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
    };
    let Ok(sink) = rodio::Sink::try_new(&handle) else { return false };
    sink.set_volume(0.85);
    // From here the normal boot hello must stay silent, whatever happens next.
    STATE.store(PLAYED, Ordering::SeqCst);
    crate::boot::mark("early_hello_audio");
    println!("[EARLY-HELLO] '{text}' ({voice}) {} ms after the thread started", started.elapsed().as_millis());
    sink.append(rodio::buffer::SamplesBuffer::new(1, clip.sample_rate, clip.samples));
    after_play(script, period, idx, now_secs());
    save_script(dir, script);
    sink.sleep_until_end();
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
    fn the_four_parts_of_the_day() {
        for (h, p) in [
            (4, Period::Night),
            (5, Period::Morning),
            (11, Period::Morning),
            (12, Period::Afternoon),
            (17, Period::Afternoon),
            (18, Period::Evening),
            (21, Period::Evening),
            (22, Period::Night),
            (0, Period::Night),
        ] {
            assert_eq!(period_for_hour(h), p, "hour {h}");
        }
        assert_eq!(Period::Evening.key(), "evening");
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
        // a different version
        let mut old = default_script("v");
        old.version = 99;
        old.last_played = Some(5);
        save_script(&dir, &old);
        assert_eq!(load_script(&dir, "v"), default_script("v"));
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
