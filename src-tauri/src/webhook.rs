//! Local webhook triggers (C5): external systems fire assistant actions.
//!
//! A minimal HTTP listener on 127.0.0.1:39220 (localhost ONLY — never
//! 0.0.0.0). `POST /trigger` with `Authorization: Bearer <token>` and
//! JSON `{"transcript": "..."}` routes through the normal orchestrator
//! pipeline. Use cases: Task Scheduler reminders, home automation,
//! scripts (`curl -X POST localhost:39220/trigger ...`).
//!
//! Security: localhost bind + bearer token (keychain-stored, rotatable)
//! + 8KB body cap + read timeouts. No new dependencies (std TcpListener).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

pub const WEBHOOK_PORT: u16 = 39220;
const MAX_BODY: usize = 8192;
/// A Stop hook posts the agent's final message, which can be long. Only `/agent-event` may send this much.
const MAX_AGENT_BODY: usize = 64 * 1024;
const TOKEN_SERVICE: &str = "webhook";

/// Get-or-create the webhook bearer token (keychain-stored).
pub fn webhook_token() -> String {
    if let Some(tok) = crate::auth_vault::get_api_key(TOKEN_SERVICE) {
        if !tok.is_empty() {
            return tok;
        }
    }
    let tok = generate_token();
    crate::auth_vault::set_api_key(TOKEN_SERVICE, &tok);
    tok
}

/// 32 random bytes from the OS CSPRNG, hex-encoded (64 chars). A predictable token would
/// let any local process (or a web page via DNS rebinding) fire assistant actions.
pub(crate) fn generate_token() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS random source unavailable");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Rotate the webhook token. Returns the new token.
pub fn rotate_webhook_token() -> String {
    crate::auth_vault::clear_api_key(TOKEN_SERVICE);
    webhook_token()
}

/// Parsed webhook request. Pure + unit-tested.
#[derive(Debug, PartialEq)]
pub struct WebhookRequest {
    pub query: String,
    pub method: String,
    pub path: String,
    pub auth: String,
    pub body: String,
}

/// Parse a raw HTTP request (headers + body already joined).
/// Returns None on malformed input or oversize body.
#[cfg(test)]
pub fn parse_request(raw: &str) -> Option<WebhookRequest> {
    parse_request_limited(raw, MAX_BODY)
}

/// Body limit for the request described by `head`: only `/agent-event` may be large.
fn body_limit(head: &str) -> usize {
    let target = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("");
    let path = target.split('?').next().unwrap_or("");
    if path == "/agent-event" {
        MAX_AGENT_BODY
    } else {
        MAX_BODY
    }
}

pub fn parse_request_limited(raw: &str, limit: usize) -> Option<WebhookRequest> {
    let (head, body) = raw.split_once("\r\n\r\n")?;
    let mut lines = head.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (path, query) = (path.to_string(), query.to_string());
    let mut auth = String::new();
    for line in lines {
        if let Some(v) = line.strip_prefix("Authorization:") {
            auth = v.trim().to_string();
        }
    }
    if body.len() > limit {
        return None;
    }
    Some(WebhookRequest {
        query,
        method,
        path,
        auth,
        body: body.to_string(),
    })
}

/// Constant-time bearer comparison (no timing oracle on localhost, but cheap).
fn bearer_ok(header: &str, token: &str) -> bool {
    let expect = format!("Bearer {}", token);
    if header.len() != expect.len() {
        return false;
    }
    header
        .bytes()
        .zip(expect.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    }
}

fn respond(stream: &mut std::net::TcpStream, code: u16, body: &str) {
    let resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        code,
        reason(code),
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
}

/// Handle one connection. Returns a transcript to process, if triggered.
fn handle_conn(
    mut stream: std::net::TcpStream,
    token: &str,
    agent_token: &str,
) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok()?;
    // Read headers first, then the exact body per Content-Length.
    let mut buf = vec![0u8; 4096];
    let mut raw = Vec::new();
    let mut header_end = None;
    while header_end.is_none() {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_BODY + 2048 {
            respond(&mut stream, 400, r#"{"error":"too large"}"#);
            return None;
        }
        header_end = find_header_end(&raw);
    }
    let hlen = header_end?;
    let head = String::from_utf8_lossy(&raw[..hlen]).to_string();
    let content_len: usize = head
        .lines()
        .find_map(|l| {
            l.strip_prefix("Content-Length:")
                .or_else(|| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse().ok())
        })
        .unwrap_or(0);
    if content_len > body_limit(&head) {
        respond(&mut stream, 400, r#"{"error":"too large"}"#);
        return None;
    }
    while raw.len() < hlen + content_len {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..n]);
    }
    let body = String::from_utf8_lossy(&raw[hlen..]).to_string();
    let req = parse_request_limited(&format!("{}\r\n\r\n{}", head, body), body_limit(&head))?;

    if req.method == "GET" && req.path == "/health" {
        respond(&mut stream, 200, r#"{"ok":true,"service":"nexus-webhook"}"#);
        return None;
    }
    if req.path == "/agent-event" {
        // An agent's Stop hook says "claude stopped". Its own token (never the /trigger one), a body of two
        // words, and nothing that can run a command: see agent_hooks.
        let ok = req.method == "POST" && bearer_ok(&req.auth, agent_token);
        let (code, reply, event) = crate::agent_hooks::route_event(ok, &req.query, &req.body);
        if let Some(mut ev) = event {
            // Antigravity and Aider name a file the final text is read from (path-checked, size-capped)
            crate::agent_hooks::resolve_tail(&mut ev);
            println!(
                "[WATCH-POWER] agent event: {} {:?} ok={:?} fully_idle={:?} text={}B",
                ev.agent.name(),
                ev.kind,
                ev.info.ok,
                ev.info.fully_idle,
                ev.tail.as_ref().map_or(0, |t| t.len())
            );
            crate::agent_hooks::record_event_at(ev.agent, ev.kind, crate::agent_hooks::now_ms(), ev.tail, ev.info);
        }
        respond(&mut stream, code, reply);
        return None;
    }
    if req.method != "POST" || req.path != "/trigger" {
        respond(&mut stream, 404, r#"{"error":"not found"}"#);
        return None;
    }
    if !bearer_ok(&req.auth, token) {
        respond(&mut stream, 401, r#"{"error":"unauthorized"}"#);
        return None;
    }
    let json: serde_json::Value = serde_json::from_str(&req.body).ok()?;
    let transcript = json.get("transcript")?.as_str()?.trim().to_string();
    if transcript.is_empty() || transcript.len() > 500 {
        respond(&mut stream, 400, r#"{"error":"bad transcript"}"#);
        return None;
    }
    respond(&mut stream, 202, r#"{"ok":true,"queued":true}"#);
    Some(transcript)
}

fn find_header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
}

/// Spawn the localhost webhook listener. Fire-and-forget thread; logs only.
pub fn spawn_listener<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    std::thread::Builder::new()
        .name("webhook-rx".into())
        .spawn(move || {
            let listener = match TcpListener::bind(("127.0.0.1", WEBHOOK_PORT)) {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("webhook: bind failed (port busy?): {}", e);
                    return;
                }
            };
            let token = webhook_token();
            let agent_token = crate::agent_hooks::agent_event_token();
            tracing::info!("webhook: listening on 127.0.0.1:{} (localhost only)", WEBHOOK_PORT);
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let peer_local = stream
                    .peer_addr()
                    .map(|a| a.ip().is_loopback())
                    .unwrap_or(false);
                if !peer_local {
                    continue;
                }
                if let Some(transcript) = handle_conn(stream, &token, &agent_token) {
                    tracing::info!(
                        "webhook: trigger {:?}...",
                        transcript.chars().take(60).collect::<String>()
                    );
                    if let Ok(dir) = app.path().app_data_dir() {
                        crate::diary::log_event(&dir, "webhook", &transcript);
                    }
                    let app_c = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::orchestrator::process_transcript(
                            app_c,
                            transcript,
                            None,
                            None,
                        )
                        .await;
                    });
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_64_hex_chars_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn test_parse_trigger() {
        let raw = "POST /trigger HTTP/1.1\r\nAuthorization: Bearer abc\r\nContent-Length: 3\r\n\r\n{\"a\":1}";
        let r = parse_request(raw).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.path, "/trigger");
        assert_eq!(r.auth, "Bearer abc");
    }

    #[test]
    fn test_parse_health() {
        let raw = "GET /health HTTP/1.1\r\nHost: x\r\n\r\n";
        let r = parse_request(raw).unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.path, "/health");
    }

    #[test]
    fn test_parse_strips_query() {
        let raw = "GET /health?x=1 HTTP/1.1\r\n\r\n";
        assert_eq!(parse_request(raw).unwrap().path, "/health");
    }

    #[test]
    fn test_parse_malformed_none() {
        assert!(parse_request("garbage").is_none());
        assert!(parse_request("GET").is_none());
    }

    #[test]
    fn test_bearer_ok() {
        assert!(bearer_ok("Bearer abc123", "abc123"));
        assert!(!bearer_ok("Bearer abc123", "abc124"));
        assert!(!bearer_ok("Bearer short", "much-longer-token"));
        assert!(!bearer_ok("", "abc"));
    }

    #[test]
    fn test_find_header_end() {
        let raw = b"GET / HTTP/1.1\r\n\r\nbody";
        assert_eq!(find_header_end(raw), Some(18));
        assert_eq!(find_header_end(b"incomplete"), None);
    }
}

#[cfg(test)]
mod agent_event_http_tests {
    use super::*;
    use std::io::{Read, Write};

    /// Send one raw HTTP request to a real `handle_conn` on a loopback socket; return (status, body).
    fn call(request: String) -> (u16, String) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_conn(stream, "trigger-token", "agent-token")
        });
        let mut c = std::net::TcpStream::connect(addr).unwrap();
        c.write_all(request.as_bytes()).unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        let transcript = server.join().unwrap();
        assert_eq!(transcript, None, "the agent endpoint can never queue a spoken command");
        let status = out.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        (status, out.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
    }

    fn post(path: &str, token: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn the_agent_event_endpoint_records_only_authorised_known_events() {
        use crate::agent_hooks::{last_event_ms, Agent, EventKind};
        // other tests share the event list: compare with what was there before, never with "nothing"
        let before = last_event_ms(Agent::Claude, EventKind::Notification);

        // the /trigger token does not open this door
        let (code, _) = call(post("/agent-event", "trigger-token", r#"{"agent":"claude","event":"notification"}"#));
        assert_eq!(code, 401);
        // no token at all, and a wrong method
        let (code, _) = call("POST /agent-event HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}".to_string());
        assert_eq!(code, 401);
        let (code, _) = call("GET /agent-event HTTP/1.1\r\nAuthorization: Bearer agent-token\r\n\r\n".to_string());
        assert_eq!(code, 401);
        // an unknown agent or event is refused
        let (code, _) = call(post("/agent-event", "agent-token", r#"{"agent":"evil","event":"notification"}"#));
        assert_eq!(code, 400);
        let (code, _) = call(post("/agent-event", "agent-token", r#"{"agent":"claude","event":"shutdown"}"#));
        assert_eq!(code, 400);
        assert_eq!(last_event_ms(Agent::Claude, EventKind::Notification), before, "nothing refused was recorded");

        // the real thing
        let (code, body) = call(post("/agent-event", "agent-token", r#"{"agent":"claude","event":"notification"}"#));
        assert_eq!((code, body.as_str()), (202, r#"{"ok":true}"#));
        assert_ne!(last_event_ms(Agent::Claude, EventKind::Notification), before, "the accepted one was recorded");

        // and the agent token does not open /trigger
        let (code, _) = call(post("/trigger", "agent-token", r#"{"transcript":"shut down my laptop"}"#));
        assert_eq!(code, 401);
    }
}

#[cfg(all(test, windows))]
mod agent_script_tests {
    use crate::agent_hooks::{parse_event, write_script, Agent, EventKind, TailSource};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::process::Stdio;

    /// Runs the REAL generated script through cmd and curl against a local socket and returns the exact
    /// request an agent's hook would make. `stdin` is what the agent's hook would pipe in. `None` when
    /// curl.exe is not on this machine.
    fn run_script(args: &[&str], stdin: Option<&str>) -> Option<String> {
        run_script_full(args, stdin, None).map(|(request, _)| request)
    }

    /// Like `run_script`, in a chosen working directory, also returning what the script printed to stdout.
    fn run_script_full(args: &[&str], stdin: Option<&str>, cwd: Option<&std::path::Path>) -> Option<(String, String)> {
        static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if std::process::Command::new("curl.exe").arg("--version").output().is_err() {
            eprintln!("curl.exe not available: skipped");
            return None;
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
            let mut raw = Vec::new();
            let mut buf = [0u8; 2048];
            loop {
                let n = s.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let want: usize = head
                        .lines()
                        .find_map(|l| l.to_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok()))
                        .unwrap_or(0);
                    if body.len() >= want {
                        break;
                    }
                }
            }
            let _ = s.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}");
            String::from_utf8_lossy(&raw).to_string()
        });
        let dir = std::env::temp_dir().join(format!("nexus_script_run_{}_{}", std::process::id(), RUNS.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&dir);
        let script = write_script(&dir, "agent-token", port).unwrap();
        let mut cmd = std::process::Command::new(&script);
        cmd.args(args)
            .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(c) = cwd {
            cmd.current_dir(c);
        }
        let mut child = cmd.spawn().unwrap();
        if let Some(text) = stdin {
            let mut pipe = child.stdin.take().unwrap();
            pipe.write_all(text.as_bytes()).unwrap();
            drop(pipe);
        }
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        let request = server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        Some((request, String::from_utf8_lossy(&out.stdout).to_string()))
    }

    fn body_of(request: &str) -> &str {
        request.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("")
    }

    fn query_of(request: &str) -> &str {
        let target = request.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("");
        target.split_once('?').map(|(_, q)| q).unwrap_or("")
    }

    #[test]
    fn the_generated_script_posts_the_right_request_through_cmd_and_curl() {
        let Some(request) = run_script(&["claude", "notification"], None) else { return };
        assert!(request.starts_with("POST /agent-event?agent=claude&event=notification HTTP/1.1"), "{request}");
        assert!(request.to_lowercase().contains("authorization: bearer agent-token"), "{request}");
        let ev = parse_event(query_of(&request), body_of(&request)).expect("a valid event");
        assert_eq!((ev.agent, ev.kind, ev.tail), (Agent::Claude, EventKind::Notification, None), "{request}");
    }

    #[test]
    fn a_claude_stop_carries_the_final_message_the_hook_received_on_stdin() {
        let stdin = r#"{"hook_event_name":"Stop","session_id":"s1","transcript_path":"C:/x/t.jsonl","last_assistant_message":"Implemented it.\nAll 12 tests passed."}"#;
        let Some(request) = run_script(&["claude", "stop"], Some(stdin)) else { return };
        assert_eq!(body_of(&request), stdin, "the body is exactly what the hook piped in");
        let ev = parse_event(query_of(&request), body_of(&request)).expect("a valid event");
        assert_eq!((ev.agent, ev.kind), (Agent::Claude, EventKind::Stop));
        assert_eq!(ev.tail.as_deref(), Some("Implemented it.\nAll 12 tests passed."), "{request}");
    }

    #[test]
    fn codex_style_calls_default_to_stop_and_ignore_the_json_codex_appends() {
        let payload = r#"{"type":"agent-turn-complete","turn-id":"12345","input-messages":["rm -rf /"]}"#;
        let Some(request) = run_script(&["codex", "stop", payload], None) else { return };
        let ev = parse_event(query_of(&request), body_of(&request)).expect("a valid event");
        assert_eq!((ev.agent, ev.kind), (Agent::Codex, EventKind::Stop), "{request}");
        assert!(!request.contains("rm -rf"), "what the agent passes along as an argument is never forwarded");
        // no event argument at all: still a stop
        let Some(request) = run_script(&["claude"], None) else { return };
        let ev = parse_event(query_of(&request), body_of(&request)).expect("a valid event");
        assert_eq!((ev.agent, ev.kind), (Agent::Claude, EventKind::Stop), "{request}");
    }

    #[test]
    fn the_script_prints_neutral_json_so_no_agent_is_ever_blocked() {
        let Some((_, out)) = run_script_full(&["claude", "stop"], Some("{}"), None) else { return };
        assert_eq!(out.trim(), "{}");
        let Some((_, out)) = run_script_full(&["gemini", "stop"], Some("{}"), None) else { return };
        assert_eq!(out.trim(), "{}");
        // Cursor's prompt hook blocks the prompt unless it answers continue
        let Some((_, out)) = run_script_full(&["cursor", "start"], Some("{}"), None) else { return };
        assert_eq!(out.trim(), r#"{"continue":true}"#);
    }

    #[test]
    fn aiders_notification_command_posts_its_working_directory_without_any_stdin() {
        // spaces and parentheses in the path are cmd's classic trap
        let cwd = std::env::temp_dir().join(format!("nexus aider (x86) {}", std::process::id()));
        std::fs::create_dir_all(&cwd).unwrap();
        let Some((request, out)) = run_script_full(&["aider", "stop"], None, Some(&cwd)) else { return };
        assert!(request.starts_with("POST /agent-event?agent=aider&event=stop HTTP/1.1"), "{request}");
        assert_eq!(out.trim(), "{}");
        let ev = parse_event(query_of(&request), body_of(&request)).expect("a valid event");
        assert_eq!((ev.agent, ev.kind), (Agent::Aider, EventKind::Stop));
        let Some(TailSource::AiderCwd(posted)) = ev.source else { panic!("no working directory in {request}") };
        assert_eq!(std::fs::canonicalize(&posted).unwrap(), std::fs::canonicalize(&cwd).unwrap(), "{request}");
        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn a_cursor_reply_and_an_antigravity_stop_pass_through_the_script_intact() {
        let reply = r#"{"text":"Fixed the bug.","conversation_id":"c1"}"#;
        let Some(request) = run_script(&["cursor", "message"], Some(reply)) else { return };
        let ev = parse_event(query_of(&request), body_of(&request)).expect("a valid event");
        assert_eq!((ev.agent, ev.kind, ev.tail.as_deref()), (Agent::Cursor, EventKind::Message, Some("Fixed the bug.")), "{request}");
        let stop = r#"{"executionNum":1,"terminationReason":"model_stop","error":"","fullyIdle":false,"transcriptPath":"C:/x/t.jsonl"}"#;
        let Some(request) = run_script(&["antigravity", "stop"], Some(stop)) else { return };
        let ev = parse_event(query_of(&request), body_of(&request)).expect("a valid event");
        assert_eq!((ev.agent, ev.kind, ev.info.fully_idle), (Agent::Antigravity, EventKind::Stop, Some(false)), "{request}");
    }
}
