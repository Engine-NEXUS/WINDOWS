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
    /// No streaming server at all: while you speak, the audio so far is sent to Groq (the same service and key the
    /// final transcript uses, so no new party sees your voice) every second or so and the text is shown provisionally.
    /// Costs extra Groq requests; limited per turn and per minute. Never in privacy mode.
    Groq,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Gates {
    pub enabled: bool,
    pub local_only: bool,
    pub meeting: bool,
    pub ghost: bool,
    pub speaking: bool,
    pub has_cloud_key: bool,
    pub has_groq_key: bool,
    /// The local speech server is already running (so its model is loaded). Starting it just for captions would cost
    /// RAM, so this is only ever a free choice, never a reason to start it.
    pub local_warm: bool,
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
    if g.local_only {
        return Plan::Local;
    }
    if g.has_cloud_key {
        return Plan::Cloud; // the owner added a streaming key on purpose
    }
    // A warm local server beats Groq: real streaming (words in about half a second instead of 1.5 to 2.5 s), no request
    // quota, and the audio stays on this machine. Groq only when the local one is not already up.
    if g.local_warm {
        return Plan::Local;
    }
    if g.has_groq_key {
        return Plan::Groq;
    }
    Plan::Local
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

#[derive(Clone)]
pub struct StreamCfg {
    pub plan: Plan,
    pub url: String,
    pub authorization: Option<String>,
    /// Only for Plan::Groq: how to turn the audio so far into text.
    pub transcribe: Option<Transcribe>,
}

/// Audio so far (16 kHz mono PCM16) in, provisional text out.
pub type Transcribe = Arc<dyn Fn(Vec<i16>) -> futures_util::future::BoxFuture<'static, Result<String, String>> + Send + Sync>;

/// How often and how much the progressive (Groq) stream may ask. Every request spends the free Groq quota the final
/// transcript also needs, so it is rationed per turn and per minute.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Do not ask before this much audio exists.
    pub min_samples: usize,
    /// Do not ask again before this much NEW audio exists.
    pub new_samples: usize,
    /// Minimum time between two requests of one turn.
    pub min_gap: Duration,
    pub max_per_turn: usize,
    pub tick: Duration,
    /// Only the last this-many samples are sent (long monologues stay cheap and fast).
    pub window_samples: usize,
    /// Below this peak the new audio is silence or room noise: do not ask.
    pub min_peak: i16,
    /// If the last this-many samples are quiet the speaker has stopped and the final request is about to be made: an
    /// interim now would duplicate it (and could still be in flight when the turn ends, spent for nothing).
    pub tail_samples: usize,
}

impl Limits {
    pub const PRODUCTION: Limits = Limits {
        min_samples: 25_600,    // 1.6 s: a one-breath command ("open whatsapp") costs nothing
        new_samples: 12_800,    // 0.8 s
        min_gap: Duration::from_millis(1_500),
        max_per_turn: 4,
        tick: Duration::from_millis(150),
        window_samples: 16_000 * 12,
        min_peak: 700,
        tail_samples: 3_200,    // 200 ms
    };
}

/// Requests allowed across turns: at most `max` in any `per` window, and none for a while after an error (a 429 means
/// the free quota is nearly gone: the final transcript must have it).
#[derive(Debug)]
pub struct Budget {
    stamps: std::collections::VecDeque<std::time::Instant>,
    max: usize,
    per: Duration,
    blocked_until: Option<std::time::Instant>,
    /// At most this many caption requests a day (the free tier has about 2,000 requests a day in total, and the final
    /// transcripts need most of them).
    day_max: usize,
    day: u64,
    day_count: usize,
}

fn today_index() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

impl Budget {
    pub fn new(max: usize, per: Duration) -> Self {
        Self { stamps: Default::default(), max, per, blocked_until: None, day_max: usize::MAX, day: 0, day_count: 0 }
    }
    pub fn production() -> Self {
        let mut b = Self::new(8, Duration::from_secs(60));
        b.day_max = 600;
        b
    }
    /// Takes one request from the budget if there is one.
    pub fn take(&mut self, now: std::time::Instant) -> bool {
        self.take_on(now, today_index())
    }
    pub fn take_on(&mut self, now: std::time::Instant, day: u64) -> bool {
        if self.day != day {
            self.day = day;
            self.day_count = 0;
        }
        if self.day_count >= self.day_max {
            return false;
        }
        if self.blocked_until.is_some_and(|t| now < t) {
            return false;
        }
        while self.stamps.front().is_some_and(|t| now.duration_since(*t) > self.per) {
            self.stamps.pop_front();
        }
        if self.stamps.len() >= self.max {
            return false;
        }
        self.stamps.push_back(now);
        self.day_count += 1;
        true
    }
    /// An error came back: stop asking for `for_`.
    pub fn back_off(&mut self, now: std::time::Instant, for_: Duration) {
        self.blocked_until = Some(now + for_);
    }
}

pub enum Cmd {
    Audio(Vec<i16>),
    Stop,
}

pub enum Feed {
    Begin,
    Chunk(Vec<i16>),
    End,
}

/// State shared between the microphone thread and the stream task. One per dispatcher (tests make their own).
/// Which words of a caption can be trusted. A streaming recogniser guesses the newest words first and corrects them a
/// moment later ("link in" becomes "best pizza"), which a reader sees as flicker. Showing only the words two hypotheses in a
/// row agree on removes that but doubles the delay (measured on real speech: median 0.67 s to 1.4 s), so the text is shown
/// at once and the screen draws the words that are not yet confirmed dimmer. `step` returns how many leading words of
/// `text` the previous hypothesis of the SAME turn also had, comparing without case or punctuation.
#[derive(Default)]
pub struct Stability {
    gen: u64,
    prev: Vec<String>,
}

impl Stability {
    pub fn step(&mut self, gen: u64, text: &str) -> usize {
        let words: Vec<String> = text
            .split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().to_lowercase())
            .collect();
        if gen != self.gen {
            self.gen = gen;
            self.prev.clear(); // a new turn starts with nothing confirmed
        }
        let n = words.iter().zip(&self.prev).take_while(|(a, b)| a == b).count();
        self.prev = words;
        n
    }
}

pub struct Shared {
    /// 0 = nothing is streaming; otherwise the generation allowed to emit.
    pub active_gen: AtomicU64,
    next_gen: AtomicU64,
    /// Cheap flag the audio callback reads; true only while a stream is wanted and allowed.
    pub armed: AtomicBool,
    pub dropped: AtomicU64,
    /// A turn has begun and not ended. Until the dispatcher has decided the plan (it reads settings, so it takes a moment) the
    /// audio is kept in `preroll` instead of being dropped: the first words of a command must not be missing from the caption.
    wanted: AtomicBool,
    preroll: std::sync::Mutex<Vec<i16>>,
}

/// The most audio kept while the plan is being decided (4 s at 16 kHz).
const PREROLL_CAP: usize = 16_000 * 4;

impl Shared {
    pub fn new() -> Self {
        Self {
            active_gen: AtomicU64::new(0),
            next_gen: AtomicU64::new(1),
            armed: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            wanted: AtomicBool::new(false),
            preroll: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn clear_preroll(&self) {
        if let Ok(mut p) = self.preroll.lock() {
            p.clear();
        }
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

static BUDGET: Lazy<Arc<std::sync::Mutex<Budget>>> = Lazy::new(|| Arc::new(std::sync::Mutex::new(Budget::production())));

fn peak(samples: &[i16]) -> i16 {
    samples.iter().map(|s| s.saturating_abs()).max().unwrap_or(0)
}

/// Provisional captions without a streaming server: while the turn runs, send the audio so far to the transcriber at
/// most every `min_gap`, one request at a time, and show what comes back. The final transcript is untouched.
pub async fn run_progressive(
    transcribe: Transcribe,
    mut rx: mpsc::Receiver<Cmd>,
    my_gen: u64,
    shared: Arc<Shared>,
    emit: Emit,
    limits: Limits,
    budget: Arc<std::sync::Mutex<Budget>>,
) {
    let mut audio: Vec<i16> = Vec::new();
    let mut asked_at_len = 0usize;
    let mut asked = 0usize;
    let mut last_ask: Option<std::time::Instant> = None;
    let mut in_flight: Option<tokio::task::JoinHandle<Result<String, String>>> = None;
    let mut failed = false;
    let mut tick = tokio::time::interval(limits.tick);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(Cmd::Audio(v)) => audio.extend_from_slice(&v),
                Some(Cmd::Stop) | None => {
                    if let Some(h) = in_flight.take() { h.abort(); }
                    return;
                }
            },
            res = async { match in_flight.as_mut() { Some(h) => h.await, None => std::future::pending().await } } => {
                in_flight = None;
                match res {
                    Ok(Ok(text)) => {
                        if let Some(t) = crate::stt::interim_transcript(&text) {
                            if shared.active_gen.load(Ordering::SeqCst) == my_gen {
                                emit(my_gen, t);
                            }
                        }
                    }
                    _ => {
                        // a failed or rate-limited request: stop asking this turn and for a minute after
                        failed = true;
                        if let Ok(mut b) = budget.lock() { b.back_off(std::time::Instant::now(), Duration::from_secs(120)); }
                    }
                }
            },
            _ = tick.tick() => {}
        }
        if failed || in_flight.is_some() || asked >= limits.max_per_turn {
            continue;
        }
        if audio.len() < limits.min_samples || audio.len() < asked_at_len + limits.new_samples {
            continue;
        }
        let now = std::time::Instant::now();
        if last_ask.is_some_and(|t| now.duration_since(t) < limits.min_gap) {
            continue;
        }
        if peak(&audio[asked_at_len..]) < limits.min_peak {
            continue; // the new audio is silence or room noise
        }
        if limits.tail_samples > 0 && peak(&audio[audio.len().saturating_sub(limits.tail_samples)..]) < limits.min_peak {
            continue; // the speaker has just stopped: the final request is coming, an interim would duplicate it
        }
        if !budget.lock().map(|mut b| b.take(now)).unwrap_or(false) {
            continue;
        }
        let from = audio.len().saturating_sub(limits.window_samples);
        let snapshot = audio[from..].to_vec();
        asked_at_len = audio.len();
        asked += 1;
        last_ask = Some(now);
        let f = transcribe(snapshot);
        in_flight = Some(tokio::spawn(f));
    }
}

/// The one task that owns the current stream. `plan_fn` is asked at every Begin (settings can change between turns).
pub async fn dispatch(
    rx: mpsc::Receiver<Feed>,
    shared: Arc<Shared>,
    plan_fn: Box<dyn Fn() -> Plan + Send + Sync>,
    cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync>,
    emit: Emit,
) {
    dispatch_with(rx, shared, plan_fn, cfg_fn, emit, Limits::PRODUCTION, BUDGET.clone()).await
}

/// `dispatch` with the progressive limits and budget supplied (tests use small ones).
pub async fn dispatch_with(
    mut rx: mpsc::Receiver<Feed>,
    shared: Arc<Shared>,
    plan_fn: Box<dyn Fn() -> Plan + Send + Sync>,
    cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync>,
    emit: Emit,
    limits: Limits,
    budget: Arc<std::sync::Mutex<Budget>>,
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
                    shared.wanted.store(false, Ordering::SeqCst); // no stream this turn: stop keeping its audio
                    shared.clear_preroll();
                    continue;
                };
                tracing::info!("stt_stream: this turn's live captions come from {:?}", cfg.plan);
                let my_gen = shared.new_gen();
                shared.active_gen.store(my_gen, Ordering::SeqCst);
                let (tx, srx) = mpsc::channel(SESSION_DEPTH);
                // The audio that came in while the plan was being decided goes first, in order, and the stream is armed
                // under the same lock, so a chunk can never overtake it.
                {
                    let mut pre = shared.preroll.lock().unwrap_or_else(|e| e.into_inner());
                    for c in std::mem::take(&mut *pre).chunks(1280) {
                        let _ = tx.try_send(Cmd::Audio(c.to_vec()));
                    }
                    shared.armed.store(true, Ordering::SeqCst);
                }
                current = Some(tx);
                if cfg.plan == Plan::Groq {
                    let Some(transcribe) = cfg.transcribe.clone() else { continue };
                    tokio::spawn(run_progressive(
                        transcribe, srx, my_gen, shared.clone(), emit.clone(), limits, budget.clone(),
                    ));
                } else {
                    tokio::spawn(run_session(cfg, srx, my_gen, shared.clone(), emit.clone()));
                }
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
                shared.wanted.store(false, Ordering::SeqCst);
                shared.clear_preroll();
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
        // armed is set by the dispatcher once the plan allows a stream; audio before that waits in the pre-roll
        self.shared.clear_preroll();
        self.shared.wanted.store(true, Ordering::SeqCst);
        let _ = self.tx.try_send(Feed::Begin);
    }
    pub fn feed(&self, samples: &[f32]) {
        if !self.shared.armed.load(Ordering::Relaxed) {
            if !self.shared.wanted.load(Ordering::Relaxed) {
                return; // nothing is streaming and no turn is waiting for a plan: an atomic read and out
            }
            // a turn has begun but the plan is not decided yet: keep the audio (never wait for the dispatcher)
            let Ok(mut pre) = self.shared.preroll.try_lock() else {
                self.shared.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            };
            if !self.shared.armed.load(Ordering::SeqCst) {
                if self.shared.wanted.load(Ordering::SeqCst) && pre.len() < PREROLL_CAP {
                    pre.extend(to_i16(samples));
                }
                return;
            }
            // armed while we took the lock: the dispatcher has already moved the pre-roll on, send this one normally
        }
        if self.tx.try_send(Feed::Chunk(to_i16(samples))).is_err() {
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn end(&self) {
        // stop emitting at once even if the queue is full and the End message cannot be queued
        self.shared.armed.store(false, Ordering::SeqCst);
        self.shared.wanted.store(false, Ordering::SeqCst);
        self.shared.active_gen.store(0, Ordering::SeqCst);
        self.shared.clear_preroll();
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
        Plan::Local => Some(StreamCfg { plan, url: LOCAL_URL.to_string(), authorization: None, transcribe: None }),
        Plan::Cloud => {
            let key = crate::commands::read_api_key(&key_app, "deepgram");
            (!key.is_empty()).then(|| StreamCfg {
                plan,
                url: DEEPGRAM_URL.to_string(),
                authorization: Some(format!("Token {key}")),
                transcribe: None,
            })
        }
        Plan::Groq => {
            let key = crate::commands::read_groq_api_key(&key_app);
            (!key.is_empty()).then(|| StreamCfg {
                plan,
                url: String::new(),
                authorization: None,
                transcribe: Some(Arc::new(move |samples| {
                    let key = key.clone();
                    Box::pin(async move {
                        crate::stt_groq::transcribe_interim_with_groq(&samples, &key, crate::stt::shared_client()).await
                    })
                })),
            })
        }
        Plan::Off(_) => None,
    });
    let emit_app = app.clone();
    let stability = std::sync::Mutex::new(Stability::default());
    let emit: Emit = Arc::new(move |gen, text| {
        let stable = stability.lock().map(|mut s| s.step(gen, &text)).unwrap_or(0);
        let _ = emit_app.emit("stt:partial", serde_json::json!({ "text": text, "session": gen, "stable_words": stable }));
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
        has_groq_key: !crate::commands::read_groq_api_key(app).is_empty(),
        local_warm: crate::lazy_stt::is_stt_responsive(),
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
    fn a_warm_local_server_is_preferred_over_groq_but_never_over_a_streaming_key_and_is_never_started_for_captions() {
        let base = Gates { enabled: true, ..Default::default() };
        assert_eq!(plan_for(&Gates { has_groq_key: true, ..base }), Plan::Groq, "a cold local server: Groq, as before");
        assert_eq!(plan_for(&Gates { has_groq_key: true, local_warm: true, ..base }), Plan::Local, "warm: free, private, true streaming");
        assert_eq!(plan_for(&Gates { has_groq_key: true, has_cloud_key: true, local_warm: true, ..base }), Plan::Cloud, "a streaming key the owner added wins");
        assert!(matches!(plan_for(&Gates { enabled: false, local_warm: true, ..base }), Plan::Off(_)), "off stays off");
        assert!(matches!(plan_for(&Gates { meeting: true, local_warm: true, ..base }), Plan::Off(_)));
        assert!(matches!(plan_for(&Gates { speaking: true, local_warm: true, ..base }), Plan::Off(_)));
        assert!(matches!(plan_for(&Gates { ghost: true, local_warm: true, ..base }), Plan::Off(_)));
        // asking for the plan must never start the server: the gate is a plain connection check
        let src = include_str!("stt_stream.rs").split("mod tests {").next().unwrap().to_string();
        assert!(src.contains("local_warm: crate::lazy_stt::is_stt_responsive()"));
        assert!(!src.contains("ensure_stt_running"), "captions never start the local server by themselves");
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
            Box::new(move |p| Some(StreamCfg { plan: p, url: url.clone(), authorization: None, transcribe: None }));
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
            Some(StreamCfg { plan: p, url: url.clone(), authorization: None, transcribe: None })
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

    #[test]
    fn stability_counts_only_the_words_two_hypotheses_in_a_row_agree_on() {
        let mut s = Stability::default();
        assert_eq!(s.step(1, "Search for the link in"), 0, "the first hypothesis of a turn confirms nothing");
        assert_eq!(s.step(1, "Search for the best pizza"), 3, "search for the: the same as before, the rest was revised");
        assert_eq!(s.step(1, "search, for the best pizza places"), 5, "case and punctuation do not count as a change");
        assert_eq!(s.step(1, "Search for the best pizza places near me"), 6);
        assert_eq!(s.step(1, "Something else entirely"), 0, "a rewrite from the start confirms nothing");
        assert_eq!(s.step(2, "Something else entirely now"), 0, "a new turn forgets the old one even with the same words");
        assert_eq!(s.step(2, "Something else entirely now"), 4);
        assert_eq!(s.step(2, ""), 0, "empty text confirms nothing and does not panic");
    }

    #[tokio::test]
    async fn audio_fed_before_the_plan_is_decided_is_not_lost_and_arrives_first_and_in_order() {
        let (url, seen) = mock_server(vec![vec![dg("hello", false)]]).await;
        let r = rig(Plan::Cloud, url);
        // begin and feed with no pause: the dispatcher has not armed the stream yet (the real start-up feeds the seed this way)
        r.handle.begin();
        r.handle.feed(&[0.1f32; 1280]);
        r.handle.feed(&[0.1f32; 640]);
        wait_for(|| r.shared.armed.load(Ordering::SeqCst)).await;
        r.handle.feed(&[0.1f32; 1280]); // after arming: goes the normal way
        wait_for(|| seen.lock().unwrap().iter().filter(|x| matches!(x, Seen::Audio(_))).count() >= 2).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let lens: Vec<usize> = seen.lock().unwrap().iter().filter_map(|x| if let Seen::Audio(n) = x { Some(*n) } else { None }).collect();
        let total: usize = lens.iter().sum();
        assert_eq!(total, (1280 + 640 + 1280) * 2, "every sample fed reached the server: {lens:?}");
        assert_eq!(lens.last(), Some(&2560), "the chunk fed after arming is the last one: {lens:?}");
    }

    #[test]
    fn the_pre_roll_is_bounded_and_nothing_is_kept_when_no_turn_is_waiting() {
        let shared = Arc::new(Shared::new());
        let (handle, _rx) = Handle::new(shared.clone());
        handle.feed(&[0.2f32; 1280]);
        assert!(shared.preroll.lock().unwrap().is_empty(), "no turn has begun: not even a copy is made");
        handle.begin();
        for _ in 0..200 {
            handle.feed(&[0.2f32; 1280]);
        }
        assert_eq!(shared.preroll.lock().unwrap().len(), PREROLL_CAP, "200 chunks fed, only 4 seconds are kept");
        handle.end();
        assert!(shared.preroll.lock().unwrap().is_empty(), "the end of the turn frees it");
        handle.feed(&[0.2f32; 1280]);
        assert!(shared.preroll.lock().unwrap().is_empty(), "and nothing is kept after the end");
    }

    #[tokio::test]
    async fn when_no_stream_is_wanted_this_turn_the_waiting_audio_is_freed() {
        let r = rig(Plan::Off("live captions are off"), String::new());
        r.handle.begin();
        r.handle.feed(&[0.1f32; 1280]);
        wait_for(|| !r.shared.wanted.load(Ordering::SeqCst)).await;
        assert!(r.shared.preroll.lock().unwrap().is_empty());
        r.handle.feed(&[0.1f32; 1280]);
        assert!(r.shared.preroll.lock().unwrap().is_empty(), "an off turn keeps nothing");
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

    // ── progressive (Groq) captions ──

    #[test]
    fn with_a_groq_key_and_no_streaming_server_the_progressive_plan_is_used_but_never_in_privacy_mode() {
        let g = Gates { enabled: true, has_groq_key: true, ..Default::default() };
        assert_eq!(plan_for(&g), Plan::Groq);
        assert_eq!(plan_for(&Gates { local_only: true, ..g }), Plan::Local, "privacy mode: never Groq");
        assert_eq!(plan_for(&Gates { has_cloud_key: true, ..g }), Plan::Cloud, "a real streaming server beats the progressive one");
        assert_eq!(plan_for(&Gates { has_groq_key: false, ..g }), Plan::Local);
        assert!(matches!(plan_for(&Gates { meeting: true, ..g }), Plan::Off(_)));
    }

    #[test]
    fn the_budget_rations_requests_per_minute_and_backs_off_after_an_error() {
        let t0 = std::time::Instant::now();
        let mut b = Budget::new(3, Duration::from_secs(60));
        assert!(b.take(t0) && b.take(t0) && b.take(t0));
        assert!(!b.take(t0), "the fourth in a minute is refused");
        assert!(b.take(t0 + Duration::from_secs(61)), "a minute later there is room again");
        b.back_off(t0 + Duration::from_secs(61), Duration::from_secs(60));
        assert!(!b.take(t0 + Duration::from_secs(90)), "blocked after an error");
        assert!(b.take(t0 + Duration::from_secs(130)), "and free again afterwards");
    }

    #[test]
    fn production_limits_leave_room_for_the_final_transcript_on_the_free_tier() {
        let l = Limits::PRODUCTION;
        assert!(l.min_samples >= 25_600, "no request before 1.6 s of audio: a one-breath command costs nothing");
        assert!(l.max_per_turn <= 4);
        assert!(l.min_gap >= Duration::from_millis(1_500));
        assert!(l.tail_samples >= 3_200, "no request while the speaker has just stopped");
        let b = Budget::production();
        assert!(b.max <= 8, "Groq's free tier is 20 requests a minute; the finals need the other half");
        assert!(b.day_max <= 600, "captions may not eat the day's quota");
    }

    fn test_limits() -> Limits {
        Limits { min_samples: 1_600, new_samples: 800, min_gap: Duration::from_millis(30), max_per_turn: 3, tick: Duration::from_millis(10), window_samples: 100_000, min_peak: 700, tail_samples: 400 }
    }

    /// A fake transcriber: records the length of each request, and answers from a script.
    #[derive(Clone)]
    struct Fake {
        calls: Arc<Mutex<Vec<usize>>>,
        replies: Arc<Mutex<Vec<Result<String, String>>>>,
        delay: Duration,
        running: Arc<std::sync::atomic::AtomicUsize>,
        max_running: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Fake {
        fn new(replies: Vec<Result<String, String>>, delay_ms: u64) -> Self {
            Self {
                calls: Default::default(),
                replies: Arc::new(Mutex::new(replies)),
                delay: Duration::from_millis(delay_ms),
                running: Default::default(),
                max_running: Default::default(),
            }
        }
        fn transcribe(&self) -> Transcribe {
            let me = self.clone();
            Arc::new(move |samples: Vec<i16>| {
                let me = me.clone();
                Box::pin(async move {
                    me.calls.lock().unwrap().push(samples.len());
                    let now = me.running.fetch_add(1, Ordering::SeqCst) + 1;
                    me.max_running.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(me.delay).await;
                    me.running.fetch_sub(1, Ordering::SeqCst);
                    let mut r = me.replies.lock().unwrap();
                    if r.is_empty() { Ok(String::new()) } else { r.remove(0) }
                })
            })
        }
    }

    struct Prog {
        tx: mpsc::Sender<Cmd>,
        shared: Arc<Shared>,
        emitted: Arc<Mutex<Vec<(u64, String)>>>,
        task: tokio::task::JoinHandle<()>,
    }

    fn start_progressive(fake: &Fake, limits: Limits, budget: Budget) -> Prog {
        let shared = Arc::new(Shared::new());
        shared.active_gen.store(5, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel(256);
        let emitted: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let e = emitted.clone();
        let emit: Emit = Arc::new(move |g, t| e.lock().unwrap().push((g, t)));
        let task = tokio::spawn(run_progressive(fake.transcribe(), rx, 5, shared.clone(), emit, limits, Arc::new(Mutex::new(budget))));
        Prog { tx, shared, emitted, task }
    }

    fn loud(n: usize) -> Cmd {
        Cmd::Audio(vec![5000i16; n])
    }

    #[tokio::test]
    async fn it_waits_for_enough_audio_then_sends_everything_so_far_and_shows_the_text() {
        let fake = Fake::new(vec![Ok("hello".into()), Ok("hello there".into())], 5);
        let p = start_progressive(&fake, test_limits(), Budget::new(100, Duration::from_secs(60)));
        p.tx.send(loud(1_000)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(fake.calls.lock().unwrap().is_empty(), "1000 samples is too little to ask");
        p.tx.send(loud(1_000)).await.unwrap();
        wait_for(|| p.emitted.lock().unwrap().len() == 1).await;
        assert_eq!(*fake.calls.lock().unwrap(), vec![2_000], "the first request carries everything said so far");
        // less than 800 new samples: no second request yet
        p.tx.send(loud(400)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(fake.calls.lock().unwrap().len(), 1, "not enough new audio");
        p.tx.send(loud(1_000)).await.unwrap();
        wait_for(|| p.emitted.lock().unwrap().len() == 2).await;
        assert_eq!(*fake.calls.lock().unwrap(), vec![2_000, 3_400], "the second request is cumulative, not just the new part");
        let texts: Vec<String> = p.emitted.lock().unwrap().iter().map(|(_, t)| t.clone()).collect();
        assert_eq!(texts, vec!["hello", "hello there"], "each answer replaces the last on screen");
        let _ = p.tx.send(Cmd::Stop).await;
        let _ = p.task.await;
    }

    #[tokio::test]
    async fn only_one_request_at_a_time_and_never_more_than_the_turn_limit() {
        let fake = Fake::new(vec![Ok("a".into()), Ok("b".into()), Ok("c".into()), Ok("d".into())], 60);
        let p = start_progressive(&fake, test_limits(), Budget::new(100, Duration::from_secs(60)));
        for _ in 0..20 {
            p.tx.send(loud(1_000)).await.unwrap();
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(fake.max_running.load(Ordering::SeqCst), 1, "never two requests in flight");
        assert_eq!(fake.calls.lock().unwrap().len(), 3, "at most max_per_turn requests");
        let _ = p.tx.send(Cmd::Stop).await;
    }

    #[tokio::test]
    async fn silence_and_room_noise_are_never_sent() {
        let fake = Fake::new(vec![Ok("you".into())], 5);
        let p = start_progressive(&fake, test_limits(), Budget::new(100, Duration::from_secs(60)));
        p.tx.send(Cmd::Audio(vec![120i16; 6_000])).await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(fake.calls.lock().unwrap().is_empty(), "a peak of 120 is room noise: no request, no quota spent");
        let _ = p.tx.send(Cmd::Stop).await;
    }

    #[tokio::test]
    async fn a_quiet_tail_means_the_speaker_has_stopped_and_no_request_is_made() {
        let fake = Fake::new(vec![Ok("open".into()), Ok("open whatsapp".into())], 5);
        let p = start_progressive(&fake, test_limits(), Budget::new(100, Duration::from_secs(60)));
        // ONE block that is long enough, with plenty of loud audio in it, but whose last 400+ samples are quiet: the
        // speaker has just stopped, the final request is about to be made, so no interim must be sent.
        let mut block = vec![5000i16; 1_000];
        block.extend(vec![50i16; 1_000]);
        p.tx.send(Cmd::Audio(block)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(fake.calls.lock().unwrap().is_empty(), "a quiet tail: nothing is asked");
        // the speaker was only pausing: more speech arrives and the request goes out, carrying everything
        p.tx.send(loud(1_000)).await.unwrap();
        wait_for(|| fake.calls.lock().unwrap().len() == 1).await;
        assert_eq!(*fake.calls.lock().unwrap(), vec![3_000]);
        let _ = p.tx.send(Cmd::Stop).await;
    }

    #[test]
    fn the_daily_cap_stops_captions_before_they_eat_the_days_quota() {
        let t0 = std::time::Instant::now();
        let mut b = Budget::new(1_000, Duration::from_secs(60));
        b.day_max = 3;
        assert!(b.take_on(t0, 100) && b.take_on(t0, 100) && b.take_on(t0, 100));
        assert!(!b.take_on(t0, 100), "the fourth of the day is refused");
        assert!(b.take_on(t0, 101), "a new day starts again");
    }

    #[test]
    fn caption_requests_do_not_inflate_the_requests_today_figure() {
        let src = include_str!("stt_groq.rs");
        let prod = src.split("mod tests {").next().unwrap(); // the production part (it has a test-only URL override of its own above the test module)
        assert!(prod.contains("transcribe_plain(samples, api_key, client, None, GROQ_MODEL_LARGE, \"stt_interim\")"));
        assert!(prod.contains("transcribe_plain(samples, api_key, client, prompt, GROQ_MODEL, \"stt\")"), "the final transcript is unchanged");
        let cfg = include_str!("stt_stream.rs").split("#[cfg(test)]").next().unwrap().to_string();
        assert!(cfg.contains("transcribe_interim_with_groq"), "the progressive source uses the interim request");
    }

    #[tokio::test]
    async fn an_error_stops_the_requests_for_this_turn_and_blocks_the_budget() {
        let fake = Fake::new(vec![Err("Groq rate limit hit".into())], 5);
        let budget = Arc::new(Mutex::new(Budget::new(100, Duration::from_secs(60))));
        let shared = Arc::new(Shared::new());
        shared.active_gen.store(5, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel(256);
        let emit: Emit = Arc::new(|_, _| {});
        let task = tokio::spawn(run_progressive(fake.transcribe(), rx, 5, shared, emit, test_limits(), budget.clone()));
        for _ in 0..10 {
            tx.send(loud(1_000)).await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        assert_eq!(fake.calls.lock().unwrap().len(), 1, "after the failure nothing more is asked this turn");
        assert!(!budget.lock().unwrap().take(std::time::Instant::now()), "and the budget is blocked for the next turns too");
        let _ = tx.send(Cmd::Stop).await;
        let _ = task.await;
    }

    #[tokio::test]
    async fn an_exhausted_budget_means_no_request_at_all() {
        let fake = Fake::new(vec![Ok("x".into())], 5);
        let p = start_progressive(&fake, test_limits(), Budget::new(0, Duration::from_secs(60)));
        p.tx.send(loud(5_000)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(fake.calls.lock().unwrap().is_empty());
        let _ = p.tx.send(Cmd::Stop).await;
    }

    #[tokio::test]
    async fn junk_the_recogniser_invents_on_silence_is_not_shown_and_a_stale_turn_shows_nothing() {
        let fake = Fake::new(vec![Ok("Thanks for watching!".into()), Ok("open whatsapp".into())], 120);
        let p = start_progressive(&fake, test_limits(), Budget::new(100, Duration::from_secs(60)));
        p.tx.send(loud(2_000)).await.unwrap();
        wait_for(|| fake.calls.lock().unwrap().len() == 1).await;
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(p.emitted.lock().unwrap().is_empty(), "a known silence hallucination is filtered");
        // the turn ends (generation 0) while the second request is in flight: its answer must not show
        p.tx.send(loud(2_000)).await.unwrap();
        wait_for(|| fake.calls.lock().unwrap().len() == 2).await;
        p.shared.active_gen.store(0, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(p.emitted.lock().unwrap().is_empty(), "nothing shows after the turn ended");
        let _ = p.tx.send(Cmd::Stop).await;
    }

    #[tokio::test]
    async fn stop_ends_the_task_at_once_and_a_long_turn_sends_only_the_recent_window() {
        let fake = Fake::new(vec![Ok("x".into())], 5);
        let mut l = test_limits();
        l.window_samples = 1_000;
        let p = start_progressive(&fake, l, Budget::new(100, Duration::from_secs(60)));
        p.tx.send(loud(3_000)).await.unwrap();
        wait_for(|| fake.calls.lock().unwrap().len() == 1).await;
        assert_eq!(*fake.calls.lock().unwrap(), vec![1_000], "only the last window is sent");
        p.tx.send(Cmd::Stop).await.unwrap();
        let done = tokio::time::timeout(Duration::from_millis(500), p.task).await;
        assert!(done.is_ok(), "Stop ends the task");
    }

    /// Reads the 16 kHz mono PCM16 test WAV ("open whatsapp") the project already ships.
    fn fixture_wav() -> Vec<f32> {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sapi_open_whatsapp_16k.wav")).expect("fixture wav");
        bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect()
    }

    /// The whole chain a real turn takes, with a real recording and a fake recogniser: the microphone side
    /// (`Handle`: begin, 80 ms chunks, end) -> dispatcher -> plan -> progressive session -> emitted caption.
    #[tokio::test]
    async fn a_real_recording_goes_through_the_whole_chain_and_ends_cleanly() {
        let wav = fixture_wav();
        assert!(wav.len() > 8_000, "the recording is not empty");
        let fake = Fake::new((0..6).map(|i| Ok(format!("open whatsapp {i}"))).collect(), 10);
        let shared = Arc::new(Shared::new());
        let (handle, rx) = Handle::new(shared.clone());
        let emitted: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let e = emitted.clone();
        let emit: Emit = Arc::new(move |g, t| e.lock().unwrap().push((g, t)));
        let tr = fake.transcribe();
        let cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync> = Box::new(move |p| {
            Some(StreamCfg { plan: p, url: String::new(), authorization: None, transcribe: Some(tr.clone()) })
        });
        let mut l = test_limits();
        l.min_samples = 4_000;
        l.new_samples = 2_000;
        l.min_gap = Duration::from_millis(40);
        tokio::spawn(dispatch_with(rx, shared.clone(), Box::new(|| Plan::Groq), cfg_fn, emit, l, Arc::new(Mutex::new(Budget::new(100, Duration::from_secs(60))))));
        handle.begin();
        wait_for(|| shared.armed.load(Ordering::SeqCst)).await;
        for chunk in wav.chunks(1_280) {
            handle.feed(chunk);
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
        wait_for(|| !emitted.lock().unwrap().is_empty()).await;
        assert!(!emitted.lock().unwrap().is_empty(), "a caption appeared while the recording was still being fed");
        let calls = fake.calls.lock().unwrap().clone();
        assert!(calls.windows(2).all(|w| w[1] > w[0]), "each request carries more audio than the last: {calls:?}");
        assert!(calls.len() <= l.max_per_turn);
        handle.end();
        let n = emitted.lock().unwrap().len();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(emitted.lock().unwrap().len(), n, "nothing appears after the turn has ended");
        assert!(!shared.armed.load(Ordering::SeqCst) && shared.active_gen.load(Ordering::SeqCst) == 0);
        // a second turn right after works the same way, with a new generation
        handle.begin();
        wait_for(|| shared.armed.load(Ordering::SeqCst)).await;
        assert!(shared.active_gen.load(Ordering::SeqCst) > 0);
        handle.end();
    }

    /// MEASUREMENT, not a pass/fail test: real-time replay of utterances of different lengths through the real scheduling
    /// code with a recogniser that takes 300 ms (Groq's measured median is about 300 ms). Prints requests, audio-seconds
    /// sent and caption timing, so the cost and lag can be read off instead of guessed.
    ///   cargo test --lib stt_stream::tests::measure_cost_and_lag -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn measure_cost_and_lag() {
        println!("\nspoken  | requests | audio sent | first caption | last caption after end of speech");
        for secs in [1.0f64, 1.5, 2.0, 3.0, 5.0, 8.0] {
            let fake = Fake::new((0..20).map(|i| Ok(format!("open whatsapp {i}"))).collect(), 300);
            let shared = Arc::new(Shared::new());
            let (handle, rx) = Handle::new(shared.clone());
            let times: Arc<Mutex<Vec<std::time::Instant>>> = Arc::new(Mutex::new(Vec::new()));
            let t = times.clone();
            let emit: Emit = Arc::new(move |_, _| t.lock().unwrap().push(std::time::Instant::now()));
            let tr = fake.transcribe();
            let cfg_fn: Box<dyn Fn(Plan) -> Option<StreamCfg> + Send + Sync> =
                Box::new(move |p| Some(StreamCfg { plan: p, url: String::new(), authorization: None, transcribe: Some(tr.clone()) }));
            tokio::spawn(dispatch_with(rx, shared.clone(), Box::new(|| Plan::Groq), cfg_fn, emit, Limits::PRODUCTION, Arc::new(Mutex::new(Budget::production()))));
            handle.begin();
            wait_for(|| shared.armed.load(Ordering::SeqCst)).await;
            let started = std::time::Instant::now();
            let chunks = (secs * 1000.0 / 80.0).round() as usize;
            for _ in 0..chunks {
                handle.feed(&[0.2f32; 1280]);
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
            let ended = std::time::Instant::now();
            handle.end();
            tokio::time::sleep(Duration::from_millis(400)).await;
            let calls = fake.calls.lock().unwrap().clone();
            let audio_s: f64 = calls.iter().map(|n| *n as f64 / 16_000.0).sum();
            let ts = times.lock().unwrap().clone();
            let first = ts.first().map(|t| format!("{:.1}s", t.duration_since(started).as_secs_f64())).unwrap_or("none".into());
            let last = ts.last().map(|t| format!("{:+.1}s", t.duration_since(ended).as_secs_f64())).unwrap_or("none".into());
            println!("{secs:>5.1}s | {:>8} | {audio_s:>8.1} s | {first:>13} | {last}", calls.len());
        }
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

