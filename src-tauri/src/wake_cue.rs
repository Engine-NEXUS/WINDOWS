//! The "NEXUS heard you" sound: a short earcon that plays the instant NEXUS wakes, so you know it is
//! listening without looking at the screen.
//!
//! * Five synthesized candidates (no files, no licences); the user picks one in Command Hub > Audio.
//!   They sit in the 0.8-3 kHz band where hearing is most sensitive, are 130-230 ms long, start with a
//!   5 ms raised-cosine attack (no click) and peak at -6 dBFS (no startle).
//! * Plays on its own output stream. It deliberately does NOT use the TTS generation counter,
//!   `set_tts_playing`, the volume lease or keep-awake: each of those would cut the cue, suppress wake
//!   detection or move the system volume.
//! * The output stream is opened on demand and closed 20 s after the last cue, so an idle PC holds no
//!   audio device open (a permanently open stream wakes the CPU continuously, which the battery work
//!   removed). `prewarm()` opens it at the wake *candidate* so a verified wake does not pay the device
//!   open time.
//! * Echo guard: capture has no echo cancellation, so while the cue is audible (plus a short tail) the
//!   capture loop does not count chunks as voiced speech (`window_active_now`). Audio is still buffered,
//!   so a fast first word is never clipped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tauri::Manager;

pub const SAMPLE_RATE: u32 = 48_000;
/// Peak of every candidate before the user's volume (about -6 dBFS).
const PEAK: f32 = 0.5;
/// How long the capture loop keeps ignoring "voice" after the cue ends (speaker/room tail + DAC).
const ECHO_TAIL_MS: u64 = 120;
/// The output stream stays open this long after the last cue.
const STREAM_IDLE_SECS: u64 = 20;
/// ...and is replaced after this long, so a changed default device is picked up.
const STREAM_MAX_AGE_SECS: u64 = 300;
pub const DEFAULT_VOLUME: u8 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Rise,
    Glass,
    Pluck,
    Triplet,
    Sweep,
}

#[cfg(test)]
impl Style {
    pub const ALL: [Style; 5] = [Style::Rise, Style::Glass, Style::Pluck, Style::Triplet, Style::Sweep];

    pub fn key(self) -> &'static str {
        match self {
            Style::Rise => "rise",
            Style::Glass => "glass",
            Style::Pluck => "pluck",
            Style::Triplet => "triplet",
            Style::Sweep => "sweep",
        }
    }
}

impl Style {
    /// Unknown names fall back to `Rise` (never an error for a setting).
    pub fn parse(s: &str) -> Style {
        match s.trim().to_lowercase().as_str() {
            "glass" => Style::Glass,
            "pluck" => Style::Pluck,
            "triplet" => Style::Triplet,
            "sweep" => Style::Sweep,
            _ => Style::Rise,
        }
    }
}

// ---- Synthesis (pure) ----

/// One partial of a struck-bell note: (frequency ratio, amplitude, decay time constant in seconds).
type Partial = (f32, f32, f32);

fn add_note(buf: &mut Vec<f32>, rate: u32, start_s: f32, freq: f32, len_s: f32, partials: &[Partial]) {
    let r = rate as f32;
    let start = (start_s * r) as usize;
    let n = (len_s * r) as usize;
    if buf.len() < start + n {
        buf.resize(start + n, 0.0);
    }
    let attack = 0.005 * r; // 5 ms raised cosine
    let release = 0.006 * r; // last 6 ms fade to exactly zero
    for i in 0..n {
        let t = i as f32 / r;
        let mut v = 0.0;
        for &(ratio, amp, tau) in partials {
            v += amp * (std::f32::consts::TAU * freq * ratio * t).sin() * (-t / tau).exp();
        }
        let a = if (i as f32) < attack { 0.5 - 0.5 * (std::f32::consts::PI * i as f32 / attack).cos() } else { 1.0 };
        let left = (n - 1 - i) as f32;
        let d = if left < release { 0.5 - 0.5 * (std::f32::consts::PI * left / release).cos() } else { 1.0 };
        buf[start + i] += v * a * d;
    }
}

fn normalize(buf: &mut [f32]) {
    let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        let k = PEAK / peak;
        for s in buf.iter_mut() {
            *s *= k;
        }
    }
}

/// The samples of one candidate (mono, `rate` Hz, peak 0.5).
pub fn cue_samples(style: Style, rate: u32) -> Vec<f32> {
    let mut b: Vec<f32> = Vec::new();
    match style {
        // G5 then D6: a rising fifth of two soft bell notes. "Ready."
        Style::Rise => {
            let p: &[Partial] = &[(1.0, 1.0, 0.07), (2.0, 0.25, 0.04)];
            add_note(&mut b, rate, 0.0, 784.0, 0.12, p);
            add_note(&mut b, rate, 0.075, 1175.0, 0.115, p);
        }
        // One clean bell with a quickly fading inharmonic partial.
        Style::Glass => {
            add_note(&mut b, rate, 0.0, 1568.0, 0.23, &[(1.0, 1.0, 0.09), (2.76, 0.35, 0.03), (5.4, 0.12, 0.015)]);
        }
        // A short warm marimba-like note.
        Style::Pluck => {
            add_note(&mut b, rate, 0.0, 880.0, 0.13, &[(1.0, 1.0, 0.04), (2.0, 0.5, 0.025), (3.0, 0.25, 0.015)]);
        }
        // C6 E6 G6 in quick succession: the most noticeable from across the room.
        Style::Triplet => {
            let p: &[Partial] = &[(1.0, 1.0, 0.035), (2.0, 0.2, 0.02)];
            add_note(&mut b, rate, 0.0, 1047.0, 0.075, p);
            add_note(&mut b, rate, 0.05, 1319.0, 0.075, p);
            add_note(&mut b, rate, 0.10, 1568.0, 0.07, p);
        }
        // An upward glide 600 -> 1800 Hz that lands on a bell.
        Style::Sweep => {
            let r = rate as f32;
            let glide = 0.14f32;
            let n = (glide * r) as usize;
            let mut phase = 0.0f32;
            let mut g = Vec::with_capacity(n);
            for i in 0..n {
                let t = i as f32 / r;
                let f = 600.0 * (1800.0f32 / 600.0).powf(t / glide);
                phase += std::f32::consts::TAU * f / r;
                let env = (t / glide).powf(0.7); // grows into the bell
                let attack = (i as f32 / (0.005 * r)).min(1.0);
                g.push((phase.sin() + 0.2 * (2.0 * phase).sin()) * env * attack * 0.6);
            }
            b = g;
            add_note(&mut b, rate, glide - 0.01, 1800.0, 0.08, &[(1.0, 1.0, 0.05), (2.0, 0.2, 0.03)]);
        }
    }
    normalize(&mut b);
    b
}

// ---- Echo guard ----

static CUE_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

fn mono_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Is the cue (plus its tail) still audible at `now_ms`? Pure, so it is unit-tested.
pub fn cue_window_active(now_ms: u64, until_ms: u64) -> bool {
    now_ms < until_ms
}

/// When the echo-guard window ends for a cue of `len_ms` that starts at `now_ms`.
pub fn window_end(now_ms: u64, len_ms: u64) -> u64 {
    now_ms + len_ms + ECHO_TAIL_MS
}

/// Called by the capture loop for every chunk.
pub fn window_active_now() -> bool {
    cue_window_active(mono_ms(), CUE_UNTIL_MS.load(Ordering::Relaxed))
}

// ---- Decision and settings ----

/// Pure rule for "should the wake cue play". Meeting mute is the same switch that mutes speech.
pub fn should_cue(enabled: bool, meeting_suppressed: bool) -> bool {
    enabled && !meeting_suppressed
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub style: Style,
    pub volume: u8,
}

/// Loose reader (camelCase or snake_case, defaults when absent or unreadable): on, `rise`, volume 70.
pub fn read_config(dir: &std::path::Path) -> Config {
    let json = std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .unwrap_or(serde_json::Value::Null);
    let pick = |a: &str, b: &str| json.get(a).or_else(|| json.get(b)).cloned();
    Config {
        enabled: pick("wakeSound", "wake_sound").and_then(|v| v.as_bool()).unwrap_or(true),
        style: pick("wakeSoundStyle", "wake_sound_style")
            .and_then(|v| v.as_str().map(Style::parse))
            .unwrap_or(Style::Rise),
        volume: pick("wakeSoundVolume", "wake_sound_volume")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(100) as u8)
            .unwrap_or(DEFAULT_VOLUME),
    }
}

// ---- Player thread ----

enum Cmd {
    Play { samples: Vec<f32>, gain: f32, queued: Instant },
    Warm,
}

fn sender() -> &'static Sender<Cmd> {
    static TX: OnceLock<Sender<Cmd>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::Builder::new()
            .name("wake-cue".into())
            .spawn(move || player_loop(rx))
            .expect("spawn wake-cue thread");
        tx
    })
}

struct Open {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    opened: Instant,
    last_use: Instant,
}

fn open_stream() -> Option<Open> {
    let t0 = Instant::now();
    match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => {
            tracing::info!("[CUE] output opened in {} ms", t0.elapsed().as_millis());
            Some(Open { _stream: stream, handle, opened: Instant::now(), last_use: Instant::now() })
        }
        Err(e) => {
            tracing::warn!("[CUE] cannot open audio output: {e}");
            None
        }
    }
}

fn player_loop(rx: mpsc::Receiver<Cmd>) {
    let mut open: Option<Open> = None;
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Cmd::Warm) => {
                if open.is_none() {
                    open = open_stream();
                }
            }
            Ok(Cmd::Play { samples, gain, queued }) => {
                if open.is_none() {
                    open = open_stream();
                }
                let mut played = false;
                if let Some(o) = open.as_mut() {
                    match rodio::Sink::try_new(&o.handle) {
                        Ok(sink) => {
                            sink.set_volume(gain);
                            sink.append(rodio::buffer::SamplesBuffer::new(1, SAMPLE_RATE, samples));
                            sink.detach();
                            o.last_use = Instant::now();
                            played = true;
                            tracing::info!("[CUE] queued -> audio handed to device in {} ms", queued.elapsed().as_millis());
                        }
                        Err(e) => tracing::warn!("[CUE] sink failed ({e}); reopening the output"),
                    }
                }
                if !played {
                    open = None; // the device may have changed; the next cue reopens it
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if let Some(o) = &open {
            let idle = o.last_use.elapsed() > Duration::from_secs(STREAM_IDLE_SECS);
            let stale = o.opened.elapsed() > Duration::from_secs(STREAM_MAX_AGE_SECS) && o.last_use.elapsed() > Duration::from_secs(2);
            if idle || stale {
                open = None; // closes the device so an idle PC holds none open
            }
        }
    }
}

/// Open the output now (call at the wake candidate, before verification finishes).
pub fn prewarm() {
    let _ = sender().send(Cmd::Warm);
}

fn gain_for(volume: u8) -> f32 {
    f32::from(volume.min(100)) / 100.0
}

/// Play one style at one volume (the Command Hub preview). Does not touch the echo guard.
pub fn play_preview(style: &str, volume: u8) {
    let st = Style::parse(style);
    let _ = sender().send(Cmd::Play { samples: cue_samples(st, SAMPLE_RATE), gain: gain_for(volume), queued: Instant::now() });
}

/// Play the user's chosen cue for a real wake, if the settings and the meeting state allow it. Sets the
/// echo-guard window BEFORE returning, so the capture that starts next already ignores the cue.
pub fn play_for_wake<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let cfg = read_config(&dir);
    let meeting = app
        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
        .map(|s| s.should_suppress_tts())
        .unwrap_or(false);
    if !should_cue(cfg.enabled, meeting) {
        return;
    }
    let samples = cue_samples(cfg.style, SAMPLE_RATE);
    let len_ms = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
    CUE_UNTIL_MS.store(window_end(mono_ms(), len_ms), Ordering::Relaxed);
    let _ = sender().send(Cmd::Play { samples, gain: gain_for(cfg.volume), queued: Instant::now() });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dft_energy(x: &[f32], rate: u32, f: f32) -> f64 {
        let w = std::f64::consts::TAU * f as f64 / rate as f64;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (n, s) in x.iter().enumerate() {
            re += *s as f64 * (w * n as f64).cos();
            im -= *s as f64 * (w * n as f64).sin();
        }
        re * re + im * im
    }

    #[test]
    fn every_style_is_short_quiet_and_click_free() {
        for st in Style::ALL {
            let s = cue_samples(st, SAMPLE_RATE);
            let ms = s.len() as u64 * 1000 / SAMPLE_RATE as u64;
            assert!((100..=260).contains(&ms), "{st:?} is {ms} ms");
            let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            // -6 dBFS: loud enough to hear from across a room, quiet enough not to startle (literal on purpose).
            assert!((0.49..=0.51).contains(&peak), "{st:?} peak {peak}");
            assert!(s.iter().all(|v| v.is_finite()));
            assert!(s[0].abs() < 0.01, "{st:?} starts with a click: {}", s[0]);
            assert!(s[s.len() - 1].abs() < 0.01, "{st:?} ends with a click: {}", s[s.len() - 1]);
            // the first 1 ms is still ramping up, not at full level
            let head = s[..(SAMPLE_RATE / 1000) as usize].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(head < 0.125, "{st:?} attack too abrupt: {head}");
        }
    }

    #[test]
    fn the_energy_sits_where_hearing_is_most_sensitive() {
        for st in Style::ALL {
            let s = cue_samples(st, SAMPLE_RATE);
            let (mut inband, mut total) = (0.0f64, 0.0f64);
            let mut f = 100.0f32;
            while f <= 9000.0 {
                let e = dft_energy(&s, SAMPLE_RATE, f);
                total += e;
                if (600.0..=4000.0).contains(&f) {
                    inband += e;
                }
                f += 50.0;
            }
            let share = inband / total;
            assert!(share > 0.8, "{st:?}: only {:.0}% of the energy is in 0.6-4 kHz", share * 100.0);
        }
    }

    #[test]
    fn the_five_are_distinct() {
        let all: Vec<Vec<f32>> = Style::ALL.iter().map(|s| cue_samples(*s, SAMPLE_RATE)).collect();
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let n = all[i].len().min(all[j].len());
                let (mut dot, mut a, mut b) = (0.0f64, 0.0f64, 0.0f64);
                for k in 0..n {
                    dot += all[i][k] as f64 * all[j][k] as f64;
                    a += (all[i][k] as f64).powi(2);
                    b += (all[j][k] as f64).powi(2);
                }
                let corr = dot / (a.sqrt() * b.sqrt());
                assert!(corr.abs() < 0.6, "{:?} and {:?} are too alike (corr {corr:.2})", Style::ALL[i], Style::ALL[j]);
                let (la, lb) = (all[i].len() as i64, all[j].len() as i64);
                assert!((la - lb).abs() > (SAMPLE_RATE / 100) as i64, "{:?} and {:?} are the same length", Style::ALL[i], Style::ALL[j]);
            }
        }
    }

    #[test]
    fn the_wav_round_trips() {
        let s = cue_samples(Style::Rise, SAMPLE_RATE);
        let bytes = crate::tts_wav::encode_wav_i16(&s, SAMPLE_RATE);
        let (back, rate) = crate::tts_wav::decode_wav(&bytes).unwrap();
        assert_eq!(rate, SAMPLE_RATE);
        assert_eq!(back.len(), s.len());
        let worst = s.iter().zip(&back).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1.0 / 16000.0, "{worst}");
    }

    #[test]
    fn style_names_parse_and_unknown_falls_back() {
        for st in Style::ALL {
            assert_eq!(Style::parse(st.key()), st);
            assert_eq!(Style::parse(&st.key().to_uppercase()), st);
        }
        assert_eq!(Style::parse("nonsense"), Style::Rise);
        assert_eq!(Style::parse(""), Style::Rise);
    }

    #[test]
    fn the_echo_window_covers_the_cue_and_then_ends() {
        assert!(cue_window_active(1000, 1300));
        assert!(!cue_window_active(1300, 1300), "ends exactly at the boundary");
        assert!(!cue_window_active(5000, 1300));
        assert!(!cue_window_active(0, 0), "never armed");
    }

    #[test]
    fn the_echo_window_outlasts_the_cue_by_a_speaker_tail() {
        let end = window_end(1000, 190);
        assert!(end >= 1000 + 190 + 100, "needs at least ~100 ms of room/DAC tail, got {}", end - 1190);
        assert!(end <= 1000 + 190 + 300, "a long tail would swallow the first words");
        assert!(cue_window_active(1000 + 190 + 50, end), "still muted just after the cue ends");
    }

    #[test]
    fn the_cue_obeys_the_switch_and_the_meeting_mute() {
        assert!(should_cue(true, false));
        assert!(!should_cue(false, false), "setting off");
        assert!(!should_cue(true, true), "meeting active");
    }

    #[test]
    fn settings_default_on_rise_and_read_both_spellings() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("nexus_cue_cfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_config(&dir), Config { enabled: true, style: Style::Rise, volume: DEFAULT_VOLUME }, "no file");
        std::fs::write(dir.join("settings.json"), r#"{"wakeSound":false,"wakeSoundStyle":"glass","wakeSoundVolume":40}"#).unwrap();
        assert_eq!(read_config(&dir), Config { enabled: false, style: Style::Glass, volume: 40 });
        std::fs::write(dir.join("settings.json"), r#"{"wake_sound_style":"sweep","wake_sound_volume":250}"#).unwrap();
        assert_eq!(read_config(&dir), Config { enabled: true, style: Style::Sweep, volume: 100 }, "volume is clamped");
        std::fs::write(dir.join("settings.json"), "not json").unwrap();
        assert_eq!(read_config(&dir).style, Style::Rise);
    }

    /// The wiring is what makes the sound useful and safe, and it lives in hot paths that are hard to run in a
    /// test: pin it so a refactor cannot silently drop it.
    #[test]
    fn the_cue_is_wired_into_both_wake_paths_and_the_echo_guard() {
        let wake = include_str!("wakeword_oww.rs");
        let hotkey = include_str!("hotkey.rs");
        assert!(wake.contains("crate::wake_cue::play_for_wake(&app)"), "wake-word fire must play the cue");
        assert!(wake.contains("crate::wake_cue::prewarm()"), "candidate must open the output early");
        assert!(wake.contains("crate::wake_cue::window_active_now()"), "capture loop must ignore the cue as speech");
        assert!(hotkey.contains("crate::wake_cue::play_for_wake(&handle)"), "Ctrl+Space must play the cue");
        // the cue must come before capture starts, or the echo window would not be armed in time
        let cue = wake.find("crate::wake_cue::play_for_wake(&app)").unwrap();
        let start = wake[cue..].find("start_stt_capture();").map(|i| i + cue).unwrap();
        assert!(cue < start);
        // and it must NOT be in the paths that re-listen without a wake
        for quiet in [include_str!("../../frontend/src/net/ghostHotMic.ts"), include_str!("../../frontend/src/audio/recorder.ts")] {
            assert!(!quiet.contains("preview_wake_sound") && !quiet.contains("wake_cue"));
        }
    }

    /// Dev helper: write the five candidates to target/wake_cue_candidates/ so they can be played in any
    /// player. `cargo test --lib write_candidate_wavs -- --ignored`
    #[test]
    #[ignore]
    fn write_candidate_wavs() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("wake_cue_candidates");
        std::fs::create_dir_all(&dir).unwrap();
        for st in Style::ALL {
            let bytes = crate::tts_wav::encode_wav_i16(&cue_samples(st, SAMPLE_RATE), SAMPLE_RATE);
            std::fs::write(dir.join(format!("{}.wav", st.key())), bytes).unwrap();
        }
        println!("wrote to {}", dir.display());
    }
}
