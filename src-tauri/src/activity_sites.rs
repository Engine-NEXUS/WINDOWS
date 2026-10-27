//! Optional Chromium extension bridge for foreground website domains.
//! The extension sends only a browser identifier and hostname. No page URL,
//! title, path, query, or browser history crosses this local connection.

use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const PORT: u16 = 39221;
const TOKEN_SERVICE: &str = "activity_browser";
const SIGNAL_TTL: Duration = Duration::from_millis(3500);

#[derive(Clone)]
struct Signal {
    domain: String,
    received: Instant,
}

static SIGNALS: Lazy<Mutex<HashMap<String, Signal>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static LAST_SEEN: Lazy<Mutex<HashMap<String, Instant>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize)]
struct BrowserMessage {
    token: String,
    browser: String,
    domain: Option<String>,
}

pub fn token() -> String {
    if let Some(existing) = crate::auth_vault::get_api_key(TOKEN_SERVICE) {
        if existing.len() == 64 && existing.bytes().all(|b| b.is_ascii_hexdigit()) {
            return existing;
        }
    }
    let generated = crate::webhook::generate_token();
    crate::auth_vault::set_api_key(TOKEN_SERVICE, &generated);
    generated
}

pub fn clear_signals() {
    SIGNALS.lock().clear();
    LAST_SEEN.lock().clear();
}

fn browser_process(browser: &str) -> Option<&'static str> {
    match browser {
        "chrome" => Some("chrome.exe"),
        "brave" => Some("brave.exe"),
        "edge" => Some("msedge.exe"),
        _ => None,
    }
}

fn clean_domain(input: &str) -> Option<String> {
    let domain = input.trim_end_matches('.').to_ascii_lowercase();
    if domain.len() > 253 || !domain.contains('.') || domain.parse::<std::net::IpAddr>().is_ok()
        || domain.ends_with(".local") || domain.ends_with(".internal")
        || domain.ends_with(".localhost")
    {
        return None;
    }
    let labels: Vec<_> = domain.split('.').collect();
    if labels.iter().any(|label| label.is_empty() || label.len() > 63
        || label.starts_with('-') || label.ends_with('-')
        || !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    {
        return None;
    }
    Some(domain)
}

fn receive(message: &BrowserMessage, expected_token: &str, now: Instant) -> bool {
    if !constant_time_eq(message.token.as_bytes(), expected_token.as_bytes()) {
        return false;
    }
    let Some(process) = browser_process(&message.browser) else { return false; };
    LAST_SEEN.lock().insert(process.into(), now);
    let mut signals = SIGNALS.lock();
    if let Some(domain) = message.domain.as_deref().and_then(clean_domain) {
        signals.insert(process.into(), Signal { domain, received: now });
    } else {
        signals.remove(process);
    }
    true
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for (x, y) in a.iter().zip(b.iter()) { diff |= usize::from(x ^ y); }
    diff == 0
}

pub fn current_domain(process: &str) -> Option<String> {
    let lower = process.to_ascii_lowercase();
    let signal = SIGNALS.lock().get(&lower).cloned()?;
    (signal.received.elapsed() <= SIGNAL_TTL).then_some(signal.domain)
}

pub fn connected() -> bool {
    LAST_SEEN.lock().values().any(|seen| seen.elapsed() <= SIGNAL_TTL)
}

pub fn spawn_listener() {
    if !cfg!(target_os = "windows") { return; }
    std::thread::Builder::new().name("activity-sites".into()).spawn(|| {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => { tracing::warn!("activity sites: runtime: {error}"); return; }
        };
        runtime.block_on(async {
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
                Ok(listener) => listener,
                Err(error) => { tracing::warn!("activity sites: bind: {error}"); return; }
            };
            let limit = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
            loop {
                let Ok((stream, peer)) = listener.accept().await else { continue; };
                if !peer.ip().is_loopback() { continue; }
                let Ok(permit) = limit.clone().try_acquire_owned() else { continue; };
                tokio::spawn(async move {
                    let _permit = permit;
                    let Ok(Ok(mut socket)) = tokio::time::timeout(
                        Duration::from_secs(5), tokio_tungstenite::accept_async(stream),
                    ).await else { return; };
                    loop {
                        let next = tokio::time::timeout(Duration::from_secs(8), socket.next()).await;
                        let Ok(Some(Ok(message))) = next else { break; };
                        if !message.is_text() || message.len() > 1024 { break; }
                        let Ok(payload) = serde_json::from_slice::<BrowserMessage>(&message.into_data()) else { break; };
                        if !crate::device_activity::sites_consent_active() {
                            let _ = socket.send(tokio_tungstenite::tungstenite::Message::Text(
                                r#"{"enabled":false}"#.into(),
                            )).await;
                            break;
                        }
                        if !receive(&payload, &token(), Instant::now()) { break; }
                        let _ = socket.send(tokio_tungstenite::tungstenite::Message::Text(
                            r#"{"enabled":true}"#.into(),
                        )).await;
                    }
                });
            }
        });
    }).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn accepts_domains_only_with_a_matching_token_and_browser() {
        let _guard = TEST_LOCK.lock();
        clear_signals();
        let now = Instant::now();
        let good = BrowserMessage { token: "correct".into(), browser: "brave".into(), domain: Some("YouTube.COM".into()) };
        assert!(!receive(&good, "wrong", now));
        assert_eq!(current_domain("brave.exe"), None);
        assert!(receive(&good, "correct", now));
        assert_eq!(current_domain("brave.exe").as_deref(), Some("youtube.com"));
        assert_eq!(current_domain("chrome.exe"), None);
        let empty = BrowserMessage { domain: None, ..good };
        assert!(receive(&empty, "correct", now));
        assert_eq!(current_domain("brave.exe"), None);
    }

    #[test]
    fn rejects_urls_addresses_and_local_hosts() {
        let _guard = TEST_LOCK.lock();
        for bad in ["https://example.com/path", "example.com/search?q=x", "localhost", "127.0.0.1", "foo.local", "-bad.com", "bad..com", "a_b.com"] {
            assert_eq!(clean_domain(bad), None, "{bad}");
        }
        assert_eq!(clean_domain("WWW.Example.COM."), Some("www.example.com".into()));
    }

    #[test]
    fn old_foreground_signals_expire() {
        let _guard = TEST_LOCK.lock();
        clear_signals();
        let old = Instant::now() - Duration::from_secs(5);
        let message = BrowserMessage { token: "x".into(), browser: "chrome".into(), domain: Some("example.com".into()) };
        assert!(receive(&message, "x", old));
        assert_eq!(current_domain("chrome.exe"), None);
    }
}
