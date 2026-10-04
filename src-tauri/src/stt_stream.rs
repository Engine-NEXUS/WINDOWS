//! Live partial-transcript stream client (plan Phase 4) — connects to the
//! local STT server's `/stream` WebSocket endpoint (see
//! `server/stt_server.py`) and re-emits Moonshine's growing partial lines
//! as `stt:partial` events for the on-screen live caption.
//!
//! Hard constraint: this is PURELY ADDITIVE and runs entirely parallel to
//! the existing batch STT pipeline (`stt.rs`'s `transcribe_audio` →
//! Groq/Moonshine → intent parsing/NLU/brain), which this module never
//! touches. Every operation here swallows its own errors — a dropped or
//! failed stream (server not running, connection refused, mid-stream
//! disconnect) must never affect command execution. The frontend degrades
//! silently to "no live caption, batch transcript arrives normally."

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use tauri::Emitter;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Same host/port the batch STT client (`stt.rs::STT_URL`) hardcodes —
/// both talk to the one local `stt_server.py` process.
const STREAM_URL: &str = "ws://127.0.0.1:39217/stream";

type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// The live connection's write half, if one is currently open. `None`
/// means "no stream" — every command below treats that as a silent no-op,
/// which is also what a connect failure leaves it as.
static SINK: Lazy<Mutex<Option<WsSink>>> = Lazy::new(|| Mutex::new(None));

/// IPC: open the live partial-transcript stream for this turn. Brackets
/// the same window as the existing batch capture (`startRecording()` in
/// `recorder.ts`). Best-effort: a connection failure (server not running,
/// `websockets` package missing, etc.) just leaves no stream open —
/// `stt_stream_push_chunk`/`stt_stream_stop` below silently no-op for the
/// rest of this turn, and the batch path is completely unaffected.
#[tauri::command]
pub async fn stt_stream_start<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    match tokio_tungstenite::connect_async(STREAM_URL).await {
        Ok((ws, _response)) => {
            let (sink, mut read) = ws.split();
            *SINK.lock().await = Some(sink);
            tauri::async_runtime::spawn(async move {
                while let Some(msg) = read.next().await {
                    let Ok(Message::Text(text)) = msg else { continue };
                    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    if let Some(partial) = payload.get("text").and_then(|v| v.as_str()) {
                        let _ = app.emit("stt:partial", serde_json::json!({ "text": partial }));
                    }
                }
                tracing::debug!("stt_stream: read loop ended");
            });
            tracing::debug!("stt_stream: connected");
        }
        Err(e) => {
            tracing::debug!("stt_stream: connect failed ({e}) — live caption disabled this turn");
            *SINK.lock().await = None;
        }
    }
    Ok(())
}

/// IPC: push one chunk of raw 16-bit LE mono PCM @ 16kHz (the same format
/// the batch path already downsamples to). Silently does nothing if no
/// stream is open (never started, or it died earlier this turn).
#[tauri::command]
pub async fn stt_stream_push_chunk(samples: Vec<i16>) -> Result<(), String> {
    let mut guard = SINK.lock().await;
    let Some(sink) = guard.as_mut() else {
        return Ok(());
    };
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in &samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    if sink.send(Message::from(bytes)).await.is_err() {
        // Connection died mid-turn — stop trying until the next start().
        *guard = None;
    }
    Ok(())
}

/// IPC: end the live stream (turn finished, aborted, or barge-in).
/// Best-effort — a failed close is not an error worth surfacing.
#[tauri::command]
pub async fn stt_stream_stop() -> Result<(), String> {
    let mut guard = SINK.lock().await;
    if let Some(mut sink) = guard.take() {
        let _ = sink.send(Message::from(r#"{"cmd":"stop"}"#.to_string())).await;
        let _ = sink.close().await;
    }
    Ok(())
}
