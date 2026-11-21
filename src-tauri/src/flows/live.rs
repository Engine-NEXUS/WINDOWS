//! Live execution of flows (plan 193).
//!
//! `try_run` is called by the orchestrator for every turn. A sentence that routes to a flow runs for real only
//! when the setting `flowsLive` is on; otherwise the router just logs what it would do (shadow mode).
//!
//! Confirmation reuses the existing machinery instead of adding a new IPC command: before the first side effect
//! the flow emits the normal `Confirm` event (sidebar card + spoken yes/no window). Its payload is
//! `{"kind":"mcp","server":"nexus-flow","tool":<capability>,"params":{"nonce":<one-time token>}}`. The paused
//! flow itself never leaves Rust: it waits in `PENDING` under that token. `orchestrator_mcp_confirm` hands a
//! `nexus-flow` payload to `resume`, which takes the flow out of the map (so a second "yes" finds nothing and
//! cannot send twice) and refuses anything older than ten minutes. A "no" discards it.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use tauri::{AppHandle, Manager, Runtime};

use super::{builtin, FlowRun, Next, Route, Value};
use crate::memcore::store::Trust;
use crate::orchestrator::{self, OrchestratorEvent, ProcessResult, Subsystem};
use crate::voice_profile::TurnOwnership;

/// The `server` name that marks a confirmation payload as a flow's.
pub const CONFIRM_SERVER: &str = "nexus-flow";

/// How long a read-back stays answerable. After that a "yes" does nothing and NEXUS says so.
const PENDING_TTL: Duration = Duration::from_secs(600);

// ─── Paused flows, kept in Rust ────────────────────────────────────────

struct Pending {
    run: FlowRun,
    created: Instant,
}

static PENDING: Lazy<Mutex<HashMap<String, Pending>>> = Lazy::new(|| Mutex::new(HashMap::new()));

enum Taken {
    Ready(FlowRun),
    Expired,
    /// Unknown, already used, or discarded: the caller stays silent (a double click must not talk over "Sent").
    Unknown,
}

fn new_nonce() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 16];
    if SystemRandom::new().fill(&mut bytes).is_err() {
        let n = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        bytes[..8].copy_from_slice(&n.to_le_bytes());
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn stash_at(run: FlowRun, created: Instant) -> String {
    let nonce = new_nonce();
    let mut g = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    g.retain(|_, p| p.created.elapsed() <= PENDING_TTL);
    g.insert(nonce.clone(), Pending { run, created });
    nonce
}

fn stash(run: FlowRun) -> String {
    stash_at(run, Instant::now())
}

/// Take a paused flow out of the map. A token works once.
fn take(nonce: &str) -> Taken {
    let mut g = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    match g.remove(nonce) {
        None => Taken::Unknown,
        Some(p) if p.created.elapsed() > PENDING_TTL => Taken::Expired,
        Some(p) => Taken::Ready(p.run),
    }
}

/// The user said no (or the card was dismissed): forget the paused flow.
pub(crate) fn discard(nonce: &str) {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).remove(nonce);
}

// ─── Entry points ──────────────────────────────────────────────────────

fn flag<R: Runtime>(app: &AppHandle<R>, key: &str, default: bool) -> bool {
    app.path().app_data_dir().map(|d| crate::memcore::flag(&d, key, default)).unwrap_or(default)
}

fn value(text: String, trust: Trust, origin: &str) -> Value {
    Value { text, trust, origin: origin.to_string() }
}

enum Outcome {
    /// Finished (or stopped): say this.
    Say(String),
    /// A confirmation was shown; the turn continues in `resume`.
    Asked,
}

fn missing_prompt(missing: &[String]) -> String {
    let what: Vec<&str> = missing
        .iter()
        .map(|m| match m.as_str() {
            "body" => "what the message should say",
            "contact" => "who to send it to",
            _ => "the missing detail",
        })
        .collect();
    format!("Tell me {} in the same sentence, sir.", what.join(" and "))
}

/// The saved contact for a spoken name: (name as saved, WhatsApp number). Several matches or none is an error
/// line to speak, never a guess: a message that cannot be undone goes only to a number NEXUS can name.
fn resolve_contact(dir: &std::path::Path, spoken: &str) -> Result<(String, String), String> {
    use crate::contacts_book::{self, Lookup};
    use crate::live::commands::wa_open;
    if let Some(number) = wa_open::learned_phone(dir, spoken) {
        return Ok((spoken.to_string(), number));
    }
    let no_number = || {
        format!("I don't have a number for {spoken}, sir. Say remember that wa phone {spoken} is the number, or import your contacts, then ask again.")
    };
    let book = contacts_book::load(dir);
    if book.is_empty() {
        return Err(no_number());
    }
    let learned = wa_open::learned_aliases(dir, spoken);
    match contacts_book::lookup(&book, &wa_open::name_candidates(spoken, &learned)) {
        Lookup::One { name, number } => wa_open::normalize_phone(&number).map(|n| (name, n)).ok_or_else(no_number),
        Lookup::Ambiguous(names) => Err(format!("I have more than one {spoken} in your contacts, sir: {}. Say the full name.", names.join(", "))),
        Lookup::None => Err(no_number()),
    }
}

/// Called once per turn. `None` = not a flow (or shadow mode): carry on with the normal pipeline.
pub(crate) async fn try_run<R: Runtime>(app: &AppHandle<R>, transcript: &str, ownership: TurnOwnership) -> Option<Result<ProcessResult, String>> {
    let split_and = flag(app, "flowsAndSplit", true);
    let Route::Flow { id, mut slots, missing } = super::route(transcript, builtin(), split_and) else { return None };
    if !flag(app, "flowsLive", false) {
        super::shadow(transcript, split_and);
        return None;
    }
    let flow = builtin().iter().find(|f| f.id == id)?.clone();
    let (request_id, cancel) = orchestrator::install_new_request(Subsystem::Mcp);
    tracing::info!("[FLOW] live route chosen={id} slots={:?} missing={missing:?} owner={}", slots.keys().collect::<Vec<_>>(), ownership.as_str());
    // A workflow can end in something that cannot be undone, so a voice that is not clearly the owner's
    // does not start one (the same rule the other side-effect paths use).
    if matches!(ownership, TurnOwnership::Uncertain | TurnOwnership::Rejected) {
        return Some(orchestrator::say_and_finish(app, request_id, "I couldn't be sure that was you, sir, so I haven't started that. Please ask again.".into()));
    }
    if !missing.is_empty() {
        return Some(orchestrator::say_and_finish(app, request_id, missing_prompt(&missing)));
    }
    if flow.slots.iter().any(|s| s.name == "contact_number") {
        let spoken = slots.get("contact").cloned().unwrap_or_default();
        let Ok(dir) = app.path().app_data_dir() else {
            return Some(orchestrator::say_and_finish(app, request_id, "I couldn't open my memory, sir.".into()));
        };
        match resolve_contact(&dir, &spoken) {
            Ok((name, number)) => {
                slots.insert("contact".into(), name);
                slots.insert("contact_number".into(), number);
            }
            Err(line) => return Some(orchestrator::say_and_finish(app, request_id, line)),
        }
    }
    match drive(app, FlowRun::new(flow, &slots), request_id.clone(), cancel, Vec::new()).await {
        Outcome::Say(text) => Some(orchestrator::say_and_finish(app, request_id, text)),
        Outcome::Asked => Some(Ok(ProcessResult { request_id, subsystem: Subsystem::Mcp, handled_locally: false })),
    }
}

fn finish<R: Runtime>(app: &AppHandle<R>, request_id: &str, text: String) {
    orchestrator::emit(app, &OrchestratorEvent::Result { text, request_id: request_id.to_string(), analysis: None, dialog_state: None });
    orchestrator::emit(app, &OrchestratorEvent::Done { request_id: request_id.to_string() });
    orchestrator::clear_active_request(request_id);
}

/// The user said yes (`orchestrator_mcp_confirm` hands the payload over). Only the one-time token comes back
/// from the frontend; everything else was kept here.
pub(crate) async fn resume<R: Runtime>(app: &AppHandle<R>, request_id: String, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let nonce = params.get("nonce").and_then(|v| v.as_str()).unwrap_or("");
    match take(nonce) {
        Taken::Unknown => {
            tracing::info!("[FLOW] confirmation ignored: nothing is waiting for it (already used, discarded or unknown)");
            Ok(serde_json::json!({ "ok": false, "unknown": true }))
        }
        Taken::Expired => {
            tracing::info!("[FLOW] confirmation expired, nothing was done");
            finish(app, &request_id, "That was a while ago, sir, so I didn't do it. Ask me again.".into());
            Ok(serde_json::json!({ "ok": false, "expired": true }))
        }
        Taken::Ready(mut run) => {
            run.approve();
            tracing::info!("[FLOW] confirmed, resuming");
            // A fresh cancel flag under the same request id: "stop" from here on reaches the remaining steps.
            let cancel = orchestrator::install_new_request_with_id(request_id.clone(), Subsystem::Mcp);
            if let Outcome::Say(text) = drive(app, run, request_id.clone(), cancel, Vec::new()).await {
                finish(app, &request_id, text);
            }
            Ok(serde_json::json!({ "ok": true }))
        }
    }
}

async fn drive<R: Runtime>(app: &AppHandle<R>, mut run: FlowRun, request_id: String, cancel: Arc<AtomicBool>, mut said: Vec<String>) -> Outcome {
    loop {
        if cancel.load(Ordering::Relaxed) {
            said.push(run.stopped_line());
            return Outcome::Say(said.join(" "));
        }
        match run.next_action() {
            Next::Done => {
                if said.is_empty() {
                    said.push("Done, sir.".into());
                }
                return Outcome::Say(said.join(" "));
            }
            Next::Failed { spoken } => {
                said.push(spoken);
                return Outcome::Say(said.join(" "));
            }
            Next::Confirm { cap, read_back } => {
                if let Err(e) = precheck(cap).await {
                    said.push(e);
                    return Outcome::Say(said.join(" "));
                }
                let prompt = if said.is_empty() { read_back } else { format!("{} {}", said.join(" "), read_back) };
                let nonce = stash(run);
                let pending = serde_json::json!({
                    "kind": "mcp",
                    "server": CONFIRM_SERVER,
                    "tool": cap,
                    "params": { "nonce": nonce },
                    "transcript": "",
                });
                orchestrator::emit(app, &OrchestratorEvent::Confirm { prompt: prompt.clone(), request_id: request_id.clone(), command: pending.clone() });
                let payload = serde_json::json!({ "requestId": request_id, "prompt": prompt, "command": pending });
                if let Err(e) = crate::commands::show_sidebar_with_confirmation(app.clone(), "Action Confirmation".to_string(), prompt, payload).await {
                    tracing::warn!("[FLOW] confirmation sidebar failed to open: {e}");
                }
                return Outcome::Asked;
            }
            Next::Run { cap, inputs } => match run_cap(app, cap, &inputs).await {
                Ok((out, line)) => {
                    said.extend(line);
                    run.record(out);
                }
                Err(e) => {
                    said.push(run.failure_line(&e));
                    return Outcome::Say(said.join(" "));
                }
            },
        }
    }
}

/// Checks made BEFORE asking the user to approve, so they are never asked to approve something that cannot run.
async fn precheck(cap: &str) -> Result<(), String> {
    if cap == "whatsapp.send" {
        use crate::mcp_client::PairingState;
        let client = crate::stt::shared_client();
        let state = tokio::time::timeout(Duration::from_secs(3), crate::mcp_client::query_pairing_status(&client)).await;
        return match state {
            Ok(PairingState::Ready) => Ok(()),
            Ok(PairingState::AwaitingQr { .. }) => Err("WhatsApp isn't linked yet, sir, so I can't send that. Scan the QR code in the Command Hub first.".into()),
            _ => Err("WhatsApp isn't reachable right now, sir, so I can't send that. Start the WhatsApp bridge and ask again.".into()),
        };
    }
    Ok(())
}

// ─── Real adapters ─────────────────────────────────────────────────────

type CapOut = (BTreeMap<String, Value>, Option<String>);

async fn fetch_full(email: Option<String>, id: String) -> Option<serde_json::Value> {
    use crate::memcore::google_io;
    google_io::with_client(email.as_deref(), move |c| {
        let id = id.clone();
        async move { google_io::gmail_full(&c, &id).await }
    })
    .await
    .ok()
}

async fn recent_ids(email: Option<&str>) -> Vec<String> {
    use crate::memcore::google_io;
    google_io::with_client(email, |c| async move { google_io::gmail_recent_ids(&c).await }).await.unwrap_or_default()
}

/// Where the text came from, for the read-back: "an email from Acme".
fn sender_label(msg: &serde_json::Value) -> String {
    let from = crate::memcore::explain::header_value(msg, "From").unwrap_or_default();
    let name = from.split('<').next().unwrap_or("").trim().trim_matches('"').trim().to_string();
    let name: String = name.chars().take(40).collect();
    if name.is_empty() {
        "an email".into()
    } else {
        format!("an email from {name}")
    }
}

/// The place in the mail just read (whichever connected account holds it), else in the newest mail of every
/// connected account: the first one that carries a Maps link or a location line. Returns (place, origin label).
async fn find_place(mail_id: &str) -> Result<(String, String), String> {
    use crate::memcore::{explain, google_io};
    let accounts = google_io::list_accounts(google_io::can_read_mail);
    if accounts.is_empty() {
        return Err("Gmail isn't connected".into());
    }
    if !mail_id.is_empty() {
        for acc in &accounts {
            if let Some(msg) = fetch_full(acc.email.clone(), mail_id.to_string()).await {
                if let Some(p) = explain::extract_place(&msg) {
                    return Ok((p, sender_label(&msg)));
                }
                break;
            }
        }
    }
    for acc in &accounts {
        for id in recent_ids(acc.email.as_deref()).await.into_iter().filter(|m| m != mail_id).take(5) {
            if let Some(msg) = fetch_full(acc.email.clone(), id).await {
                if let Some(p) = explain::extract_place(&msg) {
                    return Ok((p, sender_label(&msg)));
                }
            }
        }
    }
    Err("I couldn't find a place or a Maps link in your latest emails".into())
}

/// Each adapter wraps an existing center function and reports provenance on what it returns.
async fn run_cap<R: Runtime>(app: &AppHandle<R>, cap: &str, inputs: &BTreeMap<String, Value>) -> Result<CapOut, String> {
    let text = |k: &str| inputs.get(k).map(|v| v.text.clone()).unwrap_or_default();
    match cap {
        "mail.check" => {
            use crate::memcore::{briefing, google_io};
            if google_io::list_accounts(google_io::can_read_mail).is_empty() {
                return Err("Gmail isn't connected".into());
            }
            let dir = app.path().app_data_dir().map_err(|_| "I couldn't open my memory".to_string())?;
            let items = briefing::important_mail(&dir, chrono::Utc::now().timestamp());
            let mut out = BTreeMap::new();
            let Some(first) = items.first() else {
                out.insert("summary".into(), value("Nothing important".into(), Trust::Untrusted, "your email"));
                out.insert("ref".into(), value(String::new(), Trust::Derived, "your email"));
                return Ok((out, Some("Nothing important in your inbox, sir.".into())));
            };
            let line = briefing::mail_line(first);
            let id = first.key.as_deref().and_then(|k| k.strip_prefix("mail:")).unwrap_or("").to_string();
            out.insert("summary".into(), value(line.clone(), Trust::Untrusted, "your email"));
            out.insert("ref".into(), value(id, Trust::Derived, "your email"));
            let said = if items.len() == 1 {
                format!("You have one important email, sir: {line}.")
            } else {
                format!("You have {} important emails, sir. The most urgent is {line}.", items.len())
            };
            Ok((out, Some(said)))
        }
        "mail.extract_place" => {
            let (place, origin) = find_place(&text("mail")).await?;
            let mut out = BTreeMap::new();
            out.insert("place".into(), value(place, Trust::Untrusted, &origin));
            Ok((out, None))
        }
        "whatsapp.send" => {
            use crate::mcp_client::{call_tool, McpServer};
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .map_err(|e| format!("http client: {e}"))?;
            let token = crate::auth_vault::resolve_server_token(McpServer::WhatsApp).await;
            // The number from the contacts book, never the raw spoken name.
            let result = call_tool(
                McpServer::WhatsApp,
                "send_message",
                serde_json::json!({ "recipient": text("number"), "message": text("body") }),
                &client,
                token.as_deref(),
            )
            .await;
            if result.ok {
                Ok((BTreeMap::new(), Some("Sent, sir.".into())))
            } else {
                Err(format!("WhatsApp did not accept it ({})", result.error.unwrap_or_else(|| "unknown error".into())))
            }
        }
        "maps.open_route" => {
            let dest = text("destination");
            let url = if dest.starts_with("https://") {
                dest
            } else {
                format!("https://www.google.com/maps/dir/?api=1&destination={}", crate::memcore::mailtriage::pct_encode(&dest))
            };
            crate::commands::open_maps_url(app, &url)?;
            Ok((BTreeMap::new(), Some("Opening directions, sir.".into())))
        }
        other => Err(format!("{other} is not available")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The store is process-global and `stash` sweeps expired entries, so these tests run one at a time.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: Mutex<()> = Mutex::new(());
        GUARD.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn paused() -> FlowRun {
        let flow = builtin().iter().find(|f| f.id == "mail_then_maps").cloned().unwrap();
        FlowRun::new(flow, &BTreeMap::new())
    }

    #[test]
    fn a_confirmation_token_works_once() {
        let _g = serial();
        let nonce = stash(paused());
        assert!(matches!(take(&nonce), Taken::Ready(_)));
        assert!(matches!(take(&nonce), Taken::Unknown), "a second yes (card click plus spoken yes) finds nothing");
    }

    #[test]
    fn an_unknown_or_forged_token_finds_nothing() {
        let _g = serial();
        assert!(matches!(take("not-a-token"), Taken::Unknown));
        assert!(matches!(take(""), Taken::Unknown));
        let a = stash(paused());
        let b = stash(paused());
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn an_old_confirmation_expires_and_a_no_discards_it() {
        let _g = serial();
        let old = stash_at(paused(), Instant::now() - PENDING_TTL - Duration::from_secs(5));
        assert!(matches!(take(&old), Taken::Expired));
        assert!(matches!(take(&old), Taken::Unknown), "an expired token is gone too");
        let nonce = stash(paused());
        discard(&nonce);
        assert!(matches!(take(&nonce), Taken::Unknown));
    }

    #[test]
    fn expired_entries_are_swept_when_a_new_one_is_stashed() {
        let _g = serial();
        let old = stash_at(paused(), Instant::now() - PENDING_TTL - Duration::from_secs(5));
        let _fresh = stash(paused());
        assert!(!PENDING.lock().unwrap().contains_key(&old));
    }

    #[test]
    fn contacts_resolve_to_a_number_or_say_why_not() {
        let dir = std::env::temp_dir().join(format!("nexus_flow_contacts_{}", new_nonce()));
        std::fs::create_dir_all(&dir).unwrap();
        // no book at all: asks for a number instead of guessing
        let err = resolve_contact(&dir, "ravi").unwrap_err();
        assert!(err.contains("don't have a number for ravi"), "{err}");
        std::fs::write(dir.join("contacts.json"), r#"{"Ravi Kumar":"+91 98765 43210","Ravi Teja":"+91 90000 11111","Asha":"+91 91111 22222"}"#).unwrap();
        let err = resolve_contact(&dir, "ravi").unwrap_err();
        assert!(err.contains("more than one ravi") && err.contains("Ravi Kumar") && err.contains("Ravi Teja"), "{err}");
        let (name, number) = resolve_contact(&dir, "Asha").unwrap();
        assert_eq!(name, "Asha");
        assert!(number.ends_with("9111122222"), "{number}");
        assert!(resolve_contact(&dir, "Ravi Kumar").is_ok());
        assert!(resolve_contact(&dir, "Zed").unwrap_err().contains("don't have a number for Zed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_read_back_label_names_the_sender() {
        let msg = serde_json::json!({"payload": {"headers": [{"name": "From", "value": "\"Acme Events\" <hello@acme.example>"}]}});
        assert_eq!(sender_label(&msg), "an email from Acme Events");
        assert_eq!(sender_label(&serde_json::json!({})), "an email");
    }
}
