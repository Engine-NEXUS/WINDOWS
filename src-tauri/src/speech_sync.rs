//! One playback timeline for the stage's orb and captions.
//!
//! Control messages are sparse; the active playback cursor is sent through a
//! Tauri channel. The stage acknowledges `prepared` after its next paint before
//! the player is allowed to append audible samples.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::Serialize;
use tauri::ipc::Channel;

use crate::tts::CaptionTrack;

/// Emergency rollback without a rebuild: set NEXUS_SPEECH_SYNC_V2=0 before
/// launching NEXUS. The existing `tts:caption`/`tts-started` route remains.
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NEXUS_SPEECH_SYNC_V2").ok().as_deref() != Some("0"))
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechKind {
    Ack,
    Result,
    Prompt,
    Alert,
    Narration,
    Preview,
}

impl SpeechKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ack => "ack",
            Self::Result => "result",
            Self::Prompt => "prompt",
            Self::Alert => "alert",
            Self::Narration => "narration",
            Self::Preview => "preview",
        }
    }
}

struct SpeechIntent {
    utterance_id: u64,
    request_id: Option<String>,
    source_center: String,
    kind: SpeechKind,
}

#[derive(Clone, Serialize)]
pub struct SpeechFrame {
    pub utterance_id: u64,
    pub seq: u64,
    pub emitted_at_unix_ms: u64,
    pub phase: &'static str,
    pub position_ms: u64,
    pub caption: Option<CaptionTrack>,
    pub request_id: Option<String>,
    pub source_center: String,
    pub kind: String,
}

struct SpeechState {
    channel: Option<Channel<SpeechFrame>>,
    channel_revision: u64,
    current: Option<SpeechFrame>,
    tracks: Vec<CaptionTrack>,
    intent: Option<SpeechIntent>,
    acknowledged: u64,
    next_seq: u64,
}

static STATE: Lazy<(Mutex<SpeechState>, Condvar)> = Lazy::new(|| {
    (
        Mutex::new(SpeechState {
            channel: None,
            channel_revision: 0,
            current: None,
            tracks: Vec::new(),
            intent: None,
            acknowledged: 0,
            next_seq: 0,
        }),
        Condvar::new(),
    )
});

/// Re-registering replaces a dead stage channel and immediately replays the
/// current frame, allowing a remounted stage to recover without stale speech.
#[derive(Clone, Serialize)]
pub struct SpeechSnapshot {
    pub frame: Option<SpeechFrame>,
    pub tracks: Vec<CaptionTrack>,
}

#[tauri::command]
pub fn speech_stage_register(
    window: tauri::WebviewWindow,
    on_frame: Channel<SpeechFrame>,
) -> Result<SpeechSnapshot, String> {
    if !enabled() {
        return Err("Speech timeline disabled by rollback flag".into());
    }
    if window.label() != "stage" {
        return Err("Only the NEXUS stage may register speech playback".into());
    }
    let (lock, _) = &*STATE;
    let mut state = lock.lock().unwrap();
    state.channel_revision = state.channel_revision.wrapping_add(1);
    state.channel = Some(on_frame);
    Ok(SpeechSnapshot {
        frame: state.current.clone(),
        tracks: state.tracks.clone(),
    })
}

#[tauri::command]
pub fn speech_stage_ack(window: tauri::WebviewWindow, utterance_id: u64) {
    if !enabled() {
        return;
    }
    if window.label() != "stage" {
        return;
    }
    let (lock, cv) = &*STATE;
    let mut state = lock.lock().unwrap();
    if state
        .current
        .as_ref()
        .is_some_and(|f| f.utterance_id == utterance_id && f.phase == "prepared")
    {
        state.acknowledged = utterance_id;
        cv.notify_all();
    }
}

#[tauri::command]
pub fn speech_stage_snapshot(window: tauri::WebviewWindow) -> Result<SpeechSnapshot, String> {
    if !enabled() {
        return Err("Speech timeline disabled by rollback flag".into());
    }
    if window.label() != "stage" {
        return Err("Only the NEXUS stage may read speech playback".into());
    }
    let state = STATE.0.lock().unwrap();
    Ok(SpeechSnapshot {
        frame: state.current.clone(),
        tracks: state.tracks.clone(),
    })
}

fn publish(mut frame: SpeechFrame) -> bool {
    let (lock, cv) = &*STATE;
    let mut state = lock.lock().unwrap();
    // A superseded playback must never publish over its successor.
    if state
        .current
        .as_ref()
        .is_some_and(|cur| cur.utterance_id > frame.utterance_id)
    {
        return false;
    }
    if state.current.as_ref().is_some_and(|cur| {
        cur.utterance_id == frame.utterance_id
            && matches!(cur.phase, "finished" | "cancelled" | "failed")
    }) {
        return false;
    }
    if let Some(intent) = state
        .intent
        .as_ref()
        .filter(|intent| intent.utterance_id == frame.utterance_id)
    {
        frame.request_id = intent.request_id.clone();
        frame.source_center = intent.source_center.clone();
        frame.kind = intent.kind.as_str().into();
    }
    state.next_seq += 1;
    frame.seq = state.next_seq;
    frame.emitted_at_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if frame.phase == "prepared" {
        state.tracks.clear();
    }
    if let Some(caption) = &frame.caption {
        if state.tracks.len() == 128 {
            state.tracks.remove(0);
        }
        state.tracks.push(caption.clone());
    }
    state.current = Some(frame.clone());
    cv.notify_all();
    let channel = state.channel.clone();
    let channel_revision = state.channel_revision;
    drop(state);
    // Channel IPC can stall behind a busy WebView. Never hold the timeline
    // mutex while sending: cancellation and stage re-registration need it.
    if let Some(channel) = channel {
        if channel.send(frame).is_ok() {
            return true;
        }
        let mut state = lock.lock().unwrap();
        if state.channel_revision == channel_revision {
            state.channel = None;
        }
    }
    false
}

pub fn register_intent(
    utterance_id: u64,
    request_id: Option<String>,
    source_center: Option<String>,
    kind: Option<SpeechKind>,
) {
    if !enabled() {
        return;
    }
    let source = source_center.unwrap_or_else(|| "main".into());
    let source = if source.len() <= 64
        && source
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        source
    } else {
        "main".into()
    };
    let (lock, _) = &*STATE;
    let mut state = lock.lock().unwrap();
    state.intent = Some(SpeechIntent {
        utterance_id,
        request_id: request_id.filter(|s| s.len() <= 128),
        source_center: source,
        kind: kind.unwrap_or(SpeechKind::Result),
    });
}

pub fn prepare(utterance_id: u64, caption: CaptionTrack) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    let frame = SpeechFrame {
        utterance_id,
        seq: 0,
        emitted_at_unix_ms: 0,
        phase: "prepared",
        position_ms: 0,
        caption: Some(caption),
        request_id: None,
        source_center: "main".into(),
        kind: "result".into(),
    };
    // A missing or stale stage channel can recover through its watchdog.
    // Keep the prepared snapshot available for the newly registered stage.
    let _ = publish(frame);
    let (lock, cv) = &*STATE;
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut state = lock.lock().unwrap();
    loop {
        if state.acknowledged == utterance_id {
            return Ok(());
        }
        if !state
            .current
            .as_ref()
            .is_some_and(|f| f.utterance_id == utterance_id && f.phase == "prepared")
        {
            return Err("Speech superseded before stage was ready".into());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            drop(state);
            finish(utterance_id, "failed", 0);
            return Err("Speech stage did not acknowledge the orb and caption".into());
        }
        let (next, _) = cv.wait_timeout(state, remaining).unwrap();
        state = next;
    }
}

pub fn chunk(utterance_id: u64, caption: CaptionTrack) {
    if !enabled() {
        return;
    }
    let _ = publish(SpeechFrame {
        utterance_id,
        seq: 0,
        emitted_at_unix_ms: 0,
        phase: "chunk",
        position_ms: 0,
        caption: Some(caption),
        request_id: None,
        source_center: "main".into(),
        kind: "result".into(),
    });
}

pub fn playing(utterance_id: u64, position_ms: u64) {
    if !enabled() {
        return;
    }
    let _ = publish(SpeechFrame {
        utterance_id,
        seq: 0,
        emitted_at_unix_ms: 0,
        phase: "playing",
        position_ms,
        caption: None,
        request_id: None,
        source_center: "main".into(),
        kind: "result".into(),
    });
}

pub fn finish(utterance_id: u64, phase: &'static str, position_ms: u64) {
    if !enabled() {
        return;
    }
    let _ = publish(SpeechFrame {
        utterance_id,
        seq: 0,
        emitted_at_unix_ms: 0,
        phase,
        position_ms,
        caption: None,
        request_id: None,
        source_center: "main".into(),
        kind: "result".into(),
    });
}

pub fn cancel_current() {
    if !enabled() {
        return;
    }
    let frame = STATE.0.lock().unwrap().current.clone();
    if let Some(frame) = frame {
        if matches!(frame.phase, "prepared" | "chunk" | "playing") {
            finish(frame.utterance_id, "cancelled", frame.position_ms);
        }
    }
}
