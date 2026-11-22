//! Glue for the pure lifecycle. Receipts use their own sealed file/key, independent of memory settings/wipes.
use crate::agent_lifecycle::{Identity, Input, Kind, Phase, Registry};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{Emitter, Manager, Runtime};

struct State {
    registry: Registry,
    path: PathBuf,
    key: crate::memcore::crypto::Key,
    contexts: BTreeMap<String, Option<String>>,
    awake: Option<crate::keep_awake::KeepAwakeLease>,
    awake_since: u64,
    last_save: u64,
    last_encoding: String,
}
static STATE: OnceLock<Mutex<Result<State, String>>> = OnceLock::new();

fn now() -> u64 { crate::agent_hooks::now_ms() }
fn canonical(agent: &str) -> String {
    let lower=agent.trim().to_lowercase();
    crate::agent_hooks::Agent::from_name(&lower).or_else(||crate::agent_hooks::ALL_AGENTS.iter().copied().find(|a|a.label().eq_ignore_ascii_case(agent)))
        .map(|a| a.name().to_string()).unwrap_or_else(||lower.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(64).collect())
}
fn label(agent: &str) -> String {
    crate::agent_hooks::Agent::from_name(agent).map(|a| a.label().to_string()).unwrap_or_else(|| agent.to_string())
}

fn open<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<State, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let path = dir.join("agent_tasks.sealed");
    let entry = keyring::Entry::new(crate::app_dirs::APP_ID, "agent_task_receipts_v1").map_err(|e| e.to_string())?;
    let encoded = match entry.get_password() {
        Ok(key) => key,
        Err(keyring::Error::NoEntry) if !path.exists() => {
            let key = crate::memcore::crypto::generate_key().ok_or("could not generate receipt key")?;
            let text: String = key.iter().map(|v| format!("{v:02x}")).collect();
            entry.set_password(&text).map_err(|e| e.to_string())?;
            let fresh = keyring::Entry::new(crate::app_dirs::APP_ID, "agent_task_receipts_v1").map_err(|e| e.to_string())?;
            if fresh.get_password().map_err(|e| e.to_string())? != text { return Err("receipt key did not persist".into()); }
            text
        }
        Err(e) => return Err(format!("receipt key unavailable; existing state was preserved: {e}")),
    };
    if encoded.len() != 64 || !encoded.bytes().all(|c|c.is_ascii_hexdigit()) { return Err("invalid receipt key".into()); }
    let mut key = [0u8; 32];
    for (i, v) in key.iter_mut().enumerate() { *v = u8::from_str_radix(&encoded[i*2..i*2+2],16).map_err(|_| "invalid receipt key")?; }
    let registry = if path.exists() {
        if std::fs::metadata(&path).map_err(|e|e.to_string())?.len()>600*1024 {return Err("receipt state exceeds its size limit".into())}
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let clear = crate::memcore::crypto::open(&key, &bytes).ok_or("receipt state is corrupt or its key is unavailable; not overwriting it")?;
        Registry::restore(std::str::from_utf8(&clear).map_err(|_| "invalid receipt state")?, now())?
    } else { Registry::default() };
    Ok(State {registry,path,key,contexts:BTreeMap::new(),awake:None,awake_since:0,last_save:0,last_encoding:String::new()})
}

fn state<R: Runtime>(app: &tauri::AppHandle<R>) -> &'static Mutex<Result<State,String>> {
    STATE.get_or_init(|| {
        let result = open(app);
        if let Err(e) = &result {
            tracing::error!("agent lifecycle unavailable: {e}");
            let _ = app.emit("agent_watch:error", serde_json::json!({"reason":"receipt_store_unavailable"}));
        }
        Mutex::new(result)
    })
}
impl State {
    fn save(&mut self) -> bool {
        let Ok(text) = self.registry.encode() else { return false };
        if text==self.last_encoding {return true}
        let Some(sealed) = crate::memcore::crypto::seal(&self.key,text.as_bytes()) else { return false };
        if crate::memory::atomic_write(&self.path,&sealed) { self.last_save=now();self.last_encoding=text; true } else { false }
    }
    fn sync_awake(&mut self) {
        if self.registry.active() {
            // Overlapping bounded tasks can keep the coordinator active longer than one pin's lifetime.
            // Acquire the replacement before dropping the old lease so another task never loses its hold.
            if self.awake.is_none() || now().saturating_sub(self.awake_since)>=60*60_000 {
                if let Some(lease)=crate::keep_awake::acquire_pinned(crate::keep_awake::MAX_WATCH_HOLD) {
                    self.awake=Some(lease);self.awake_since=now();
                }
            }
        } else { self.awake=None; }
    }
}

pub fn record_hook<R: Runtime>(app: &tauri::AppHandle<R>, ev: &crate::agent_hooks::ParsedEvent) {
    let kind = match ev.kind {
        crate::agent_hooks::EventKind::Start => Kind::Start,
        crate::agent_hooks::EventKind::Notification => Kind::Wait,
        crate::agent_hooks::EventKind::Stop => Kind::Stop,
        crate::agent_hooks::EventKind::Message => Kind::Busy,
    };
    // Antigravity's invocation Stop is not its outer-task end unless fully_idle is explicit.
    let authoritative = ev.agent != crate::agent_hooks::Agent::Antigravity || ev.info.fully_idle == Some(true);
    record(app,ev.agent.name(),ev.cwd.as_deref(),&ev.identity,kind,ev.info.fully_idle,ev.info.ok,
        ev.tail.as_deref().unwrap_or(""),authoritative);
}

#[allow(clippy::too_many_arguments)]
fn record<R: Runtime>(app: &tauri::AppHandle<R>, agent: &str, cwd: Option<&str>, identity: &Identity,
    kind: Kind, idle: Option<bool>, ok: Option<bool>, signature: &str, authoritative: bool) {
    let Ok(mut guard)=state(app).lock() else {return};
    let Ok(s)=guard.as_mut() else {return};
    let at=now();
    if let Some(key)=s.registry.observe(Input {agent,cwd,identity,kind,at_ms:at,fully_idle:idle,ok,signature,authoritative}) {
        // A sidecar poll without a cwd must not erase the hook's exact task context.
        if cwd.is_some() || !s.contexts.contains_key(&key) {s.contexts.insert(key,cwd.filter(|c|c.len()<=4096).map(str::to_string));}
        s.contexts.retain(|key,_|s.registry.contains(key));
        if matches!(kind,Kind::Stop|Kind::Wait) || at.saturating_sub(s.last_save)>30_000 { s.save(); }
    }
    s.sync_awake();
}

pub fn owned<R: Runtime>(app: &tauri::AppHandle<R>, agent: &str, cwd: Option<&str>, watch: &str, kind: Kind, failed: bool) {
    let id=Identity {owned_watch:Some(watch.to_string()),..Identity::default()};
    record(app,&canonical(agent),cwd,&id,kind,if failed {Some(true)} else {None},if failed {Some(false)} else {None},"owned watch",true);
}
pub fn owned_exit<R: Runtime>(app: &tauri::AppHandle<R>, agent: &str, watch: &str, failed: bool) {
    let id=Identity {owned_watch:Some(watch.to_string()),..Identity::default()};
    record(app,&canonical(agent),None,&id,Kind::Stop,Some(true),Some(!failed),"process exit",true);
}
pub fn cancel_owned(watch: &str) {
    let Some(shared)=STATE.get() else {return}; let Ok(mut g)=shared.lock() else {return}; let Ok(s)=g.as_mut() else {return};
    s.registry.cancel_owned(watch,now()); s.sync_awake(); s.save();
}
pub fn owned_phase(agent: &str, watch: &str) -> Option<Phase> {
    STATE.get()?.lock().ok()?.as_ref().ok()?.registry.owned_phase(&canonical(agent),watch)
}
pub fn busy_since(agent: &str, since: u64) -> bool {
    STATE.get().and_then(|s|s.lock().ok()).and_then(|s|s.as_ref().ok().map(|s|s.registry.busy_since(&canonical(agent),since))).unwrap_or(false)
}

/// One publisher for automatic completion, explicit-watch completion and return-from-away delivery.
/// The claim is sealed BEFORE handoff. A crash after handoff is "delivery unknown", never an automatic replay.
pub fn poll<R: Runtime>(app: &tauri::AppHandle<R>, away: bool) { deliver(app,away,None); }
pub fn deliver_labels<R: Runtime>(app: &tauri::AppHandle<R>, labels: &[String]) {
    deliver(app,!crate::proactive_policy::someone_is_here(),Some(labels));
}
fn deliver<R: Runtime>(app: &tauri::AppHandle<R>, away: bool, labels: Option<&[String]>) {
    let ready = {
        let Ok(mut g)=state(app).lock() else {return}; let Ok(s)=g.as_mut() else {return};
        s.registry.tick(now()); s.sync_awake();
        if away { s.save(); return; }
        let mut ready=Vec::new();
        for t in s.registry.ready() {
            if labels.is_some_and(|names| !names.contains(&label(&t.agent))) {continue}
            let Some(t)=s.registry.claim(&t.key) else {continue};
            if !s.save() { s.registry.unclaim(&t.key); tracing::error!("agent receipt persistence failed; completion handoff deferred");break; }
            let cwd=s.contexts.get(&t.key).cloned().flatten();
            ready.push((t,cwd));
        }
        ready
    };
    for (t,cwd) in ready {
        // a finish already told the moment it happened (agent_attention::finish_now) is not told again 15 s later
        if t.phase==Phase::Complete && t.stop_ms.is_some_and(|s| crate::agent_attention::fast_told_near(&t.agent, s)) { continue; }
        let name=label(&t.agent);
        let text=match t.phase {
            Phase::Complete=>format!("{name} finished, sir."),
            Phase::Failed=>format!("{name} stopped with an error, sir."),
            Phase::Expired=>format!("I stopped watching {name} after six hours, sir. Completion was not confirmed."),
            _=>continue,
        };
        let receipt=t.receipt();
        let alert_id=format!("agent_task_{receipt}");
        if let Some(event)=crate::notify_hub::publish_agent_task(app,&t.agent,&name,cwd.as_deref(),&receipt,&text) {
            crate::proactive_policy::link_alert(&alert_id,&event);
        }
        crate::proactive_policy::submit_event(app,alert_id,text,crate::google::types::AlertUrgency::High,Some(format!("agent_task:{receipt}")));
    }
}

/// A desktop watch can finish only from one unambiguous, real hook completion, never filesystem silence.
pub fn desktop_result(agent: &str, cwd: Option<&str>, since: u64) -> Option<Phase> {
    let shared=STATE.get()?;let g=shared.lock().ok()?;let s=g.as_ref().ok()?;
    let agent=canonical(agent);
    if s.registry.busy_since(&agent,since) {return None}
    let tasks:Vec<_>=s.registry.terminal_since(&agent,since).into_iter().filter(|t|
        cwd.is_none_or(|dir| s.contexts.get(&t.key).and_then(|c|c.as_deref())==Some(dir))).collect();
    if tasks.len()==1 {Some(tasks[0].phase)} else {None}
}
