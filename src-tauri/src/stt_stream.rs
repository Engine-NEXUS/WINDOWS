//! Live partial captions: the words the user is saying appear on screen while they are still speaking.
//!
//! The native microphone capture (`wakeword_oww.rs`) feeds the very samples the batch speech-to-text will later see into
//! `feed`; a single long-lived task here owns the WebSocket, and re-emits growing text as `stt:partial`.
//!
//! Rewritten 2026-11-24 after an audit found the old version (a) was never called (its only caller, the browser
//! recorder's `captureUntilSilence`, has no callers; the real microphone path is native), (b) streamed live audio to
//! Deepgram even when `localSttOnly` (privacy mode) was on, (c) let a late message from a closed stream reach the next
//! turn, (d) showed only the latest segment of a long sentence, and (e) never told Deepgram to close.
//!
//! Rules (each has a test):
//!   * OFF by default (`sttLiveCaptions`). Never while a meeting is on, in a Ghost hot-mic session (the directed gate
//!     decides what a hot mic hears) or while NEXUS itself is speaking (its own echo).
//!   * `localSttOnly` is a hard gate checked here in Rust: the audio goes only to the local server, never to the cloud.
//!   * Purely additive and never in the way: the audio callback only does an atomic read and a `try_send`; a full queue
//!     drops chunks (counted), it never waits. Every error is swallowed; a dead stream never touches the batch transcript.
//!   * One stream per turn, identified by a generation. A second start closes the first socket; a message that arrives
//!     for an older generation is dropped.
//!   * The text shown is every finished segment plus the current unfinished one, not only the latest segment.

use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

const LOCAL_URL: &str = "ws://127.0.0.1:39217/stream";
const DEEPGRAM_URL: &str = "wss://api.deepgram.com/v1/listen?model=nova-2&smart_format=true&encoding=linear16&sample_rate=16000&channels=1&interim_results=true";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(3000);
const SEND_TIMEOUT: Duration = Duration::from_millis(400);
const CLOSE_TIMEOUT: Duration = Duration::from_millis(300);
/// 80 ms chunks: 256 of them is about 20 s of audio held between the microphone and the socket.
const FEED_DEPTH: usize = 256;
const SESSION_DEPTH: usize = 256;

// ── Pure decisions ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Do not stream, and say why (logged once per turn).
    Off(&'static str),
    /// The local speech server's own stream: audio never leaves the machine.
    Local,
    /// Deepgram: audio goes to the cloud. Only with a key, and never in privacy mode.
    Cloud,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Gates {
    pub enabled: bool,
    pub local_only: bool,
    pub meeting: bool,
    pub ghost: bool,
    pub speaking: bool,
    pub has_cloud_key: bool,
}

pub fn plan_for(g: &Gates) -> Plan {
    if !g.enabled {
        return Plan::Off("live captions are off");
    }
    if g.meeting {
        return Plan::Off("a meeting is on");
    }
    if g.ghost {
        return Plan::Off("Ghost hot mic");
    }
    if g.speaking {
        return Plan::Off("NEXUS is speaking");
    }
    if g.local_only || !g.has_cloud_key {
        return Plan::Local;
    }
    Plan::Cloud
}

/// What the "this stream is finished" message looks like for each server.
pub fn close_message(plan: Plan) -> &'static str {
    match plan {
        Plan::Cloud => r#"{"type":"CloseStream"}"#,
        _ => r#"{"cmd":"stop"}"#,
    }
}

/// 80 ms of f32 samples as 16-bit little-endian mono PCM, exactly what the batch path uploads.
pub fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect()
}

/// Builds the shown text out of the server's messages.
#[derive(Debug, Default)]
pub struct Assembler {
    finals: String,
}

impl Assembler {
    /// A Deepgram `Results` message: `is_final` segments are kept, the current unfinished segment is appended.
    pub fn feed_deepgram(&mut self, json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let text = v
            .get("channel")?
            .get("alternatives")?
            .as_array()?
            .first()?
            .get("transcript")?
            .as_str()?
            .trim()
            .to_string();
        let is_final = v.get("is_final").and_then(|b| b.as_bool()).unwrap_or(false);
        if is_final {
            if !text.is_empty() {
                if !self.finals.is_empty() {
                    self.finals.push(' ');
                }
                self.finals.push_str(&text);
            }
            return (!self.finals.is_empty()).then(|| self.finals.clone());
        }
        match (self.finals.is_empty(), text.is_empty()) {
            (_, true) => (!self.finals.is_empty()).then(|| self.finals.clone()),
            (true, false) => Some(text),
            (false, false) => Some(format!("{} {}", self.finals, text)),
        }
    }

    /// The local server already sends the whole growing line as `{"text": ...}`.
    pub fn feed_local(&mut self, json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let t = v.get("text")?.as_str()?.trim();
        (!t.is_empty()).then(|| t.to_string())
    }
}

// ── The stream ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StreamCfg {
    pub plan: Plan,
    pub url: String,
    pub authorization: Option<String>,
}

enum Cmd {
    Audio(Vec<i16>),
    Stop,
}

pub enum Feed {
    Begin,
    Chunk(Vec<i16>),
    End,
}

/// State shared between the microphone thread and the stream task. One per dispatcher (tests make their own).
pub struct Shared {
    /// 0 = nothing is streaming; otherwise the generation allowed to emit.
    pub active_gen: AtomicU64,
    next_gen: AtomicU64,
    /// Cheap flag the audio callback reads; true only while a stream is wanted and allowed.
    pub armed: AtomicBool,
    pub dropped: AtomicU64,
}

impl Shared {
    pub fn new() -> Self {
        Self { active_gen: AtomicU64::new(0), next_gen: AtomicU64::new(1), armed: AtomicBool::new(false), dropped: AtomicU64::new(0) }
    }
    fn new_gen(&self) -> u64 {
        self.next_gen.fetch_add(1, Ordering::SeqCst)
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

type Emit = Arc<dyn Fn(u64, String) + Send + Sync>;

/// One server message for generation `my_gen`. Returns false when the turn is no longer current (stop reading).
/// An older turn's late message must never reach a newer one: checked before and again right before emitting.
fn deliver(plan: Plan, asm: &mut Assembler, text: &str, shared: &Shared, my_gen: u64, emit: &Emit) -> bool {
    if shared.active_gen.load(Ordering::SeqCst) != my_gen {
        return false;
    }
    let out = match plan {
        Plan::Cloud => asm.feed_deepgram(text),
        _ => asm.feed_local(text),
    };
    if let Some(text) = out {
        if shared.active_gen.load(Ordering::SeqCst) == my_gen {
            emit(my_gen, text);
        }
    }
    true
}

async fn run_session(cfg: StreamCfg, mut rx: mpsc::Receiver<Cmd>, my_gen: u64, shared: Arc<Shared>, emit: Emit) {
    let Ok(mut req) = cfg.url.as_str().into_client_request() else { return };
    if let Some(auth) = &cfg.authorization {
        if let Ok(h) = auth.parse() {
            req.headers_mut().insert("Authorization", h);
        }
    }
    let conn = match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(req)).await {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => {
            tracing::debug!("stt_stream: connect failed ({e}) — no live caption this turn");
            return;
        }
        Err(_) => {
            tracing::debug!("stt_stream: connect timed out — no live caption this turn");
            return;
        }
    };
    let (mut sink, mut read) = conn.split();
    let plan = cfg.plan;
    let rshared = shared.clone();
    let reader = tokio::spawn(async move {
        let mut asm = Assembler::default();
        while let Some(msg) = read.next().await {
            let Ok(Message::Text(t)) = msg else { continue };
            if !deliver(plan, &mut asm, t.as_ref(), &rshared, my_gen, &emit) {
                break;
            }
        }
    });
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Audio(samples) => {
                let mut bytes = Vec::with_capacity(samples.len() * 2);
                for s in &samples {
                    bytes.extend_from_slice(&s.to_le_bytes());
                }
                match tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::from(bytes))).await {
                    Ok(Ok(())) => {}
                    _ => break, // the connection died or stalled: stop trying this turn
                }
            }
            Cmd::Stop => break,
        }
    }
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, sink.send(Message::from(close_message(plan).to_string()))).await;
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, sink.close()).await;
    reader.abort();
}

/// The one task that owns the current stream. `plan_fn` is asked at every Begin (settings can change between turns).
pub async fn dispatch(
    mut rx: mpsc::Receiver<Feed>,
    shared: Arc<Shared>,
    plan_fn: Box<dyn Fn() -> Plan + Send + Sync>,
    cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync>,
    emit: Emit,
) {
    let mut current: Option<mpsc::Sender<Cmd>> = None;
    while let Some(feed) = rx.recv().await {
        match feed {
            Feed::Begin => {
                // a second start closes the first stream instead of leaking it
                shared.active_gen.store(0, Ordering::SeqCst);
                if let Some(old) = current.take() {
                    let _ = old.try_send(Cmd::Stop);
                }
                let plan = plan_fn();
                let Some(cfg) = (!matches!(plan, Plan::Off(_))).then(|| cfg_fn(plan)).flatten() else {
                    if let Plan::Off(why) = plan {
                        tracing::debug!("stt_stream: not streaming this turn ({why})");
                    }
                    shared.armed.store(false, Ordering::SeqCst);
                    continue;
                };
                let my_gen = shared.new_gen();
                shared.active_gen.store(my_gen, Ordering::SeqCst);
                let (tx, srx) = mpsc::channel(SESSION_DEPTH);
                current = Some(tx);
                shared.armed.store(true, Ordering::SeqCst);
                tokio::spawn(run_session(cfg, srx, my_gen, shared.clone(), emit.clone()));
            }
            Feed::Chunk(v) => {
                if let Some(s) = &current {
                    if s.try_send(Cmd::Audio(v)).is_err() {
                        shared.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            Feed::End => {
                shared.armed.store(false, Ordering::SeqCst);
                shared.active_gen.store(0, Ordering::SeqCst);
                if let Some(s) = current.take() {
                    let _ = s.try_send(Cmd::Stop);
                }
            }
        }
    }
}

// ── The microphone side: never blocks, never allocates unless a stream is wanted ────────────────

pub struct Handle {
    shared: Arc<Shared>,
    tx: mpsc::Sender<Feed>,
}

impl Handle {
    pub fn new(shared: Arc<Shared>) -> (Self, mpsc::Receiver<Feed>) {
        let (tx, rx) = mpsc::channel(FEED_DEPTH);
        (Self { shared, tx }, rx)
    }
    pub fn begin(&self) {
        // armed is set by the dispatcher once the plan allows a stream; chunks before that are simply not sent
        let _ = self.tx.try_send(Feed::Begin);
    }
    pub fn feed(&self, samples: &[f32]) {
        if !self.shared.armed.load(Ordering::Relaxed) {
            return;
        }
        if self.tx.try_send(Feed::Chunk(to_i16(samples))).is_err() {
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn end(&self) {
        // stop emitting at once even if the queue is full and the End message cannot be queued
        self.shared.armed.store(false, Ordering::SeqCst);
        self.shared.active_gen.store(0, Ordering::SeqCst);
        let _ = self.tx.try_send(Feed::End);
    }
}

static HANDLE: OnceLock<Handle> = OnceLock::new();
static SHARED: Lazy<Arc<Shared>> = Lazy::new(|| Arc::new(Shared::new()));

/// Called once at start-up. Spawns the dispatcher; until then every call below is a no-op.
pub fn init<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let (handle, rx) = Handle::new(SHARED.clone());
    if HANDLE.set(handle).is_err() {
        return;
    }
    let plan_app = app.clone();
    let plan_fn: Box<dyn Fn() -> Plan + Send + Sync> = Box::new(move || plan_for(&current_gates(&plan_app)));
    let key_app = app.clone();
    let cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync> = Box::new(move |plan| match plan {
        Plan::Local => Some(StreamCfg { plan, url: LOCAL_URL.to_string(), authorization: None }),
        Plan::Cloud => {
            let key = crate::commands::read_api_key(&key_app, "deepgram");
            (!key.is_empty()).then(|| StreamCfg {
                plan,
                url: DEEPGRAM_URL.to_string(),
                authorization: Some(format!("Token {key}")),
            })
        }
        Plan::Off(_) => None,
    });
    let emit_app = app.clone();
    let emit: Emit = Arc::new(move |gen, text| {
        let _ = emit_app.emit("stt:partial", serde_json::json!({ "text": text, "session": gen }));
    });
    tauri::async_runtime::spawn(dispatch(rx, SHARED.clone(), plan_fn, cfg_fn, emit));
}

fn current_gates<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Gates {
    use tauri::Manager;
    let meeting = app
        .try_state::<Arc<crate::meeting_detect::MeetingState>>()
        .map(|m| m.is_meeting_active())
        .unwrap_or(false);
    let speaking = app
        .try_state::<Arc<crate::meeting_detect::MeetingState>>()
        .map(|m| m.tts_playing.load(Ordering::Relaxed))
        .unwrap_or(false);
    Gates {
        enabled: crate::commands::read_stt_live_captions(app),
        local_only: crate::commands::read_local_stt_only(app),
        meeting,
        ghost: crate::ghost::session_active(),
        speaking,
        has_cloud_key: !crate::commands::read_api_key(app, "deepgram").is_empty(),
    }
}

/// A capture turn has started.
pub fn begin() {
    if let Some(h) = HANDLE.get() {
        h.begin();
    }
}

/// One chunk of the capture, exactly as the batch path will transcribe it. Safe to call from the audio callback.
pub fn feed(samples: &[f32]) {
    if let Some(h) = HANDLE.get() {
        h.feed(samples);
    }
}

/// The capture turn is over (endpoint, abort, barge-in, Ghost stop).
pub fn end() {
    if let Some(h) = HANDLE.get() {
        h.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::net::TcpListener;

    // ── pure ──

    #[test]
    fn privacy_mode_never_reaches_the_cloud_even_with_a_key() {
        let g = Gates { enabled: true, local_only: true, has_cloud_key: true, ..Default::default() };
        assert_eq!(plan_for(&g), Plan::Local, "localSttOnly means the audio stays on this machine");
        let g = Gates { enabled: true, local_only: false, has_cloud_key: true, ..Default::default() };
        assert_eq!(plan_for(&g), Plan::Cloud);
        let g = Gates { enabled: true, ..Default::default() };
        assert_eq!(plan_for(&g), Plan::Local, "no key: the local server's own stream");
    }

    #[test]
    fn it_is_off_unless_enabled_and_never_in_a_meeting_ghost_hot_mic_or_over_its_own_voice() {
        let on = Gates { enabled: true, has_cloud_key: true, ..Default::default() };
        assert!(matches!(plan_for(&Gates { enabled: false, ..on }), Plan::Off(_)), "off by default");
        assert!(matches!(plan_for(&Gates { meeting: true, ..on }), Plan::Off(_)));
        assert!(matches!(plan_for(&Gates { ghost: true, ..on }), Plan::Off(_)));
        assert!(matches!(plan_for(&Gates { speaking: true, ..on }), Plan::Off(_)));
        assert_eq!(Gates::default().enabled, false);
    }

    #[test]
    fn each_server_gets_its_own_close_message() {
        assert_eq!(close_message(Plan::Cloud), r#"{"type":"CloseStream"}"#);
        assert_eq!(close_message(Plan::Local), r#"{"cmd":"stop"}"#);
    }

    #[test]
    fn samples_convert_to_pcm16_and_clip() {
        assert_eq!(to_i16(&[0.0, 1.0, -1.0, 2.0, -3.0, 0.5]), vec![0, 32767, -32767, 32767, -32767, 16383]);
    }

    #[test]
    fn a_long_sentence_shows_every_finished_segment_plus_the_current_one() {
        let mut a = Assembler::default();
        let msg = |t: &str, fin: bool| format!(r#"{{"type":"Results","is_final":{fin},"channel":{{"alternatives":[{{"transcript":"{t}"}}]}}}}"#);
        assert_eq!(a.feed_deepgram(&msg("open", false)).as_deref(), Some("open"));
        assert_eq!(a.feed_deepgram(&msg("open what", false)).as_deref(), Some("open what"));
        assert_eq!(a.feed_deepgram(&msg("open whatsapp", true)).as_deref(), Some("open whatsapp"));
        assert_eq!(a.feed_deepgram(&msg("and", false)).as_deref(), Some("open whatsapp and"), "the first segment is not lost");
        assert_eq!(a.feed_deepgram(&msg("and send hi", true)).as_deref(), Some("open whatsapp and send hi"));
        assert_eq!(a.feed_deepgram(&msg("", false)).as_deref(), Some("open whatsapp and send hi"), "an empty interim changes nothing");
        assert!(a.feed_deepgram(r#"{"type":"Metadata","request_id":"x"}"#).is_none(), "messages with no transcript are ignored");
        assert!(a.feed_deepgram("not json").is_none());
    }

    #[test]
    fn a_message_for_an_older_turn_is_dropped_and_ends_the_reader() {
        let shared = Shared::new();
        let seen: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let emit: Emit = Arc::new(move |g, t| sink.lock().unwrap().push((g, t)));
        let mut asm = Assembler::default();
        let msg = dg("hello", false);
        // turn 7 is current: its message is shown
        shared.active_gen.store(7, Ordering::SeqCst);
        assert!(deliver(Plan::Cloud, &mut asm, &msg, &shared, 7, &emit));
        // a newer turn (8) starts: the same stream's next message is stale and ends its reader
        shared.active_gen.store(8, Ordering::SeqCst);
        assert!(!deliver(Plan::Cloud, &mut asm, &dg("LATE", false), &shared, 7, &emit));
        // the turn ended (0): nothing is shown either
        shared.active_gen.store(0, Ordering::SeqCst);
        assert!(!deliver(Plan::Cloud, &mut asm, &dg("LATER", false), &shared, 8, &emit));
        assert_eq!(*seen.lock().unwrap(), vec![(7, "hello".to_string())]);
    }

    #[test]
    fn the_local_servers_growing_line_is_passed_through() {
        let mut a = Assembler::default();
        assert_eq!(a.feed_local(r#"{"text":" open what "}"#).as_deref(), Some("open what"));
        assert!(a.feed_local(r#"{"text":""}"#).is_none());
        assert!(a.feed_local(r#"{"nope":1}"#).is_none());
    }

    // ── a real WebSocket server on localhost ──

    #[derive(Debug, Clone, PartialEq)]
    enum Seen {
        Connected(u32),
        Audio(usize),
        Text(String),
    }

    /// Accepts connections; after each binary frame it replies with the next scripted text for that connection.
    async fn mock_server(scripts: Vec<Vec<String>>) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            let mut n = 0u32;
            let scripts = Arc::new(scripts);
            loop {
                let Ok((sock, _)) = listener.accept().await else { return };
                n += 1;
                let idx = n;
                let log = log.clone();
                let scripts = scripts.clone();
                tokio::spawn(async move {
                    let Ok(ws) = tokio_tungstenite::accept_async(sock).await else { return };
                    log.lock().unwrap().push(Seen::Connected(idx));
                    let (mut sink, mut read) = ws.split();
                    let mut script = scripts.get(idx as usize - 1).cloned().unwrap_or_default().into_iter();
                    while let Some(Ok(msg)) = read.next().await {
                        match msg {
                            Message::Binary(b) => {
                                log.lock().unwrap().push(Seen::Audio(b.len()));
                                if let Some(reply) = script.next() {
                                    let _ = sink.send(Message::from(reply)).await;
                                }
                            }
                            Message::Text(t) => log.lock().unwrap().push(Seen::Text(t.to_string())),
                            Message::Close(_) => return,
                            _ => {}
                        }
                    }
                });
            }
        });
        (url, seen)
    }

    fn dg(t: &str, fin: bool) -> String {
        format!(r#"{{"type":"Results","is_final":{fin},"channel":{{"alternatives":[{{"transcript":"{t}"}}]}}}}"#)
    }

    async fn wait_for(mut f: impl FnMut() -> bool) {
        for _ in 0..100 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    struct Rig {
        handle: Handle,
        shared: Arc<Shared>,
        emitted: Arc<Mutex<Vec<(u64, String)>>>,
    }

    fn rig(plan: Plan, url: String) -> Rig {
        let shared = Arc::new(Shared::new());
        let (handle, rx) = Handle::new(shared.clone());
        let emitted: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let e = emitted.clone();
        let emit: Emit = Arc::new(move |g, t| e.lock().unwrap().push((g, t)));
        let cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync> =
            Box::new(move |p| Some(StreamCfg { plan: p, url: url.clone(), authorization: None }));
        tokio::spawn(dispatch(rx, shared.clone(), Box::new(move || plan), cfg_fn, emit));
        Rig { handle, shared, emitted }
    }

    #[tokio::test]
    async fn a_turn_streams_audio_shows_growing_text_and_closes_with_closestream() {
        let (url, seen) = mock_server(vec![vec![dg("open", false), dg("open what", false), dg("open whatsapp", true)]]).await;
        let r = rig(Plan::Cloud, url);
        r.handle.begin();
        wait_for(|| r.shared.armed.load(Ordering::SeqCst)).await;
        for _ in 0..3 {
            r.handle.feed(&[0.1f32; 1280]);
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        wait_for(|| r.emitted.lock().unwrap().len() >= 3).await;
        let texts: Vec<String> = r.emitted.lock().unwrap().iter().map(|(_, t)| t.clone()).collect();
        assert_eq!(texts, vec!["open", "open what", "open whatsapp"]);
        r.handle.end();
        wait_for(|| seen.lock().unwrap().iter().any(|s| matches!(s, Seen::Text(_)))).await;
        let s = seen.lock().unwrap().clone();
        assert_eq!(s.iter().filter(|x| matches!(x, Seen::Audio(2560))).count(), 3, "three 80 ms chunks of 16-bit audio: {s:?}");
        assert!(s.contains(&Seen::Text(r#"{"type":"CloseStream"}"#.into())), "Deepgram is told the stream is over: {s:?}");
    }

    #[tokio::test]
    async fn nothing_is_connected_or_sent_when_the_plan_is_off() {
        let (url, seen) = mock_server(vec![]).await;
        let r = rig(Plan::Off("a meeting is on"), url);
        r.handle.begin();
        tokio::time::sleep(Duration::from_millis(150)).await;
        r.handle.feed(&[0.1f32; 1280]);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(!r.shared.armed.load(Ordering::SeqCst), "never armed");
        assert!(seen.lock().unwrap().is_empty(), "no connection, no audio: {:?}", seen.lock().unwrap());
        assert!(r.emitted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_cloud_configuration_is_never_even_built_in_privacy_mode() {
        // plan_for says Local in privacy mode, and the config for Cloud is a trap that fails the test if asked
        let gates = Gates { enabled: true, local_only: true, has_cloud_key: true, ..Default::default() };
        let (url, _seen) = mock_server(vec![]).await;
        let shared = Arc::new(Shared::new());
        let (handle, rx) = Handle::new(shared.clone());
        let asked_cloud = Arc::new(AtomicBool::new(false));
        let flag = asked_cloud.clone();
        let cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync> = Box::new(move |p| {
            if p == Plan::Cloud {
                flag.store(true, Ordering::SeqCst);
            }
            Some(StreamCfg { plan: p, url: url.clone(), authorization: None })
        });
        let emit: Emit = Arc::new(|_, _| {});
        tokio::spawn(dispatch(rx, shared.clone(), Box::new(move || plan_for(&gates)), cfg_fn, emit));
        handle.begin();
        wait_for(|| shared.armed.load(Ordering::SeqCst)).await;
        assert!(!asked_cloud.load(Ordering::SeqCst), "privacy mode must never ask for the cloud configuration");
    }

    #[tokio::test]
    async fn a_second_start_closes_the_first_stream_and_the_old_turns_late_text_is_dropped() {
        // connection 1 replies late (on its 2nd audio frame); by then connection 2 is the active turn
        let (url, seen) = mock_server(vec![vec![dg("old turn", false), dg("LATE old text", false)], vec![dg("new turn", false)]]).await;
        let r = rig(Plan::Cloud, url);
        r.handle.begin();
        wait_for(|| r.shared.armed.load(Ordering::SeqCst)).await;
        r.handle.feed(&[0.1f32; 1280]);
        wait_for(|| r.emitted.lock().unwrap().len() == 1).await;
        r.handle.begin(); // a new turn starts without the old one ending
        wait_for(|| seen.lock().unwrap().iter().filter(|s| matches!(s, Seen::Connected(_))).count() == 2).await;
        r.handle.feed(&[0.1f32; 1280]);
        wait_for(|| r.emitted.lock().unwrap().iter().any(|(_, t)| t == "new turn")).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        let s = seen.lock().unwrap().clone();
        assert!(s.contains(&Seen::Text(r#"{"type":"CloseStream"}"#.into())), "the first socket was closed properly: {s:?}");
        let texts: Vec<(u64, String)> = r.emitted.lock().unwrap().clone();
        assert!(!texts.iter().any(|(_, t)| t.contains("LATE")), "late text from the old turn never shows: {texts:?}");
        let gens: std::collections::HashSet<u64> = texts.iter().map(|(g, _)| *g).collect();
        assert_eq!(gens.len(), 2, "two different generations were used: {texts:?}");
    }

    #[tokio::test]
    async fn nothing_is_emitted_after_the_turn_ends() {
        let (url, _seen) = mock_server(vec![vec![dg("hello", false), dg("hello there", false)]]).await;
        let r = rig(Plan::Cloud, url);
        r.handle.begin();
        wait_for(|| r.shared.armed.load(Ordering::SeqCst)).await;
        r.handle.feed(&[0.1f32; 1280]);
        wait_for(|| r.emitted.lock().unwrap().len() == 1).await;
        r.handle.end();
        r.handle.feed(&[0.1f32; 1280]); // not armed any more: this chunk is not even queued
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(r.emitted.lock().unwrap().len(), 1, "the second server message arrives after the end and is dropped");
    }

    #[test]
    fn a_full_queue_drops_chunks_and_never_blocks_the_audio_thread() {
        let shared = Arc::new(Shared::new());
        let (handle, _rx) = Handle::new(shared.clone()); // nobody drains it
        shared.armed.store(true, Ordering::SeqCst);
        let t0 = std::time::Instant::now();
        for _ in 0..(FEED_DEPTH + 50) {
            handle.feed(&[0.2f32; 1280]);
        }
        assert!(t0.elapsed() < Duration::from_millis(500), "feed must never wait");
        assert_eq!(shared.dropped.load(Ordering::Relaxed), 50, "everything past the queue depth is dropped and counted");
        handle.end();
        assert!(!shared.armed.load(Ordering::SeqCst) && shared.active_gen.load(Ordering::SeqCst) == 0, "end() stops emission even with a full queue");
    }

    #[test]
    fn the_capture_hooks_exist_in_the_native_microphone_path() {
        let src = include_str!("wakeword_oww.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        for hook in ["crate::stt_stream::begin()", "crate::stt_stream::feed(", "crate::stt_stream::end()"] {
            assert!(prod.contains(hook), "wakeword_oww.rs must call {hook}");
        }
        // the chunk streamed is the same chunk the batch buffer receives
        let a = prod.find("cap.extend(chunk.iter().copied())").expect("buffer append");
        let b = prod.find("crate::stt_stream::feed(").expect("feed");
        assert!(b > a && b - a < 400, "feed sits right after the batch buffer append");
    }
}
