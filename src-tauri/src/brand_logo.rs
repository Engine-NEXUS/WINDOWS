//! Logo.dev brand logos: fetch, validate, cache, and expose as PNG data URLs.
//!
//! CDN URL (parameter names confirmed against https://docs.logo.dev, "Logo
//! Images"): `https://img.logo.dev/{domain}?token={publishable_key}&size=128
//! &format=png&retina=true&fallback=404`. `fallback=404` makes unknown brands
//! return HTTP 404 instead of a generated monogram, so a miss can be
//! negative-cached.
//!
//! Guarantees:
//! - No key (or a key that does not start with `pk_`) -> `None` immediately,
//!   with no network call.
//! - Disk cache under `<app data dir>\logo_cache\`: `<sha256(domain)>.png` is
//!   fresh for 30 days, `<sha256(domain)>.none` (a miss) is honoured for 24 h.
//!   A cache hit needs neither the key nor the network.
//! - In production only `img.logo.dev` over https is contacted, and only with
//!   a domain validated down to `[a-z0-9.-]`; the query is built with
//!   `reqwest::Url::query_pairs_mut`, never string concatenation.
//! - The publishable key never appears in logs, errors, panics, or returned
//!   strings (reqwest errors are stripped of their URL before formatting).
//! - Bodies are verified as PNG/JPEG/WebP by magic bytes AND decode-checked
//!   with the `image` crate before anything is cached; anything else is
//!   rejected.

use base64::Engine;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Production CDN base. Tests inject their own base into `fetch_logo`.
const DEFAULT_BASE: &str = "https://img.logo.dev";

/// How long a cached PNG stays fresh.
const PNG_FRESH_SECS: u64 = 30 * 24 * 3600;
/// How long a `.none` miss marker is honoured.
const NONE_FRESH_SECS: u64 = 24 * 3600;
/// A returned data URL must stay at or under this many bytes.
const MAX_DATA_URL_BYTES: usize = 40_000;
/// Resize target when the logo is larger than this on its longest side.
const MAX_SIDE_PX: u32 = 96;
/// Parallel fetch cap for `logos_for_domains`.
const MAX_PARALLEL: usize = 6;
/// Per-request timeout in seconds.
const TIMEOUT_SECS: u64 = 3;

/// Real network attempts (including the single retry), used by tests.
static NET_CALLS: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub(crate) fn network_calls() -> u64 {
    NET_CALLS.load(Ordering::Relaxed)
}

fn bump_net_calls() {
    NET_CALLS.fetch_add(1, Ordering::Relaxed);
}

static CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .connect_timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

// ─── Cache directory ────────────────────────────────────────────────────────

#[cfg(not(test))]
fn cache_dir() -> Option<std::path::PathBuf> {
    dirs_next::data_dir().map(|d| d.join("com.nexus.assistant").join("logo_cache"))
}

#[cfg(test)]
static TEST_CACHE_DIR: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
static TEST_SETTINGS_KEY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Serialises tests that touch the process-global test cache dir / settings key.
#[cfg(test)]
static TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn set_test_cache_dir(dir: Option<std::path::PathBuf>) {
    *TEST_CACHE_DIR.lock().unwrap() = dir;
}

#[cfg(test)]
pub(crate) fn set_test_settings_key(key: Option<String>) {
    *TEST_SETTINGS_KEY.lock().unwrap() = key;
}

#[cfg(test)]
fn cache_dir() -> Option<std::path::PathBuf> {
    TEST_CACHE_DIR.lock().unwrap().clone()
}

// ─── Key plumbing ───────────────────────────────────────────────────────────

/// Read the Logo.dev publishable key the way `places_api_key` is read at its
/// use site (`commands::read_api_key`): keychain first, then settings.json.
/// `None` when empty or when the value does not start with `pk_`.
pub(crate) fn logo_dev_key() -> Option<String> {
    let key = crate::auth_vault::get_api_key("logo_dev")
        .filter(|k| !k.trim().is_empty())
        .or_else(read_key_from_settings_file)?;
    let key = key.trim().to_string();
    if key.is_empty() || !key.starts_with("pk_") {
        return None;
    }
    Some(key)
}

#[cfg(not(test))]
fn read_key_from_settings_file() -> Option<String> {
    let dir = dirs_next::data_dir()?.join("com.nexus.assistant");
    let content = std::fs::read_to_string(dir.join("settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let val = json
        .get("logoDevPublishableKey")
        .or_else(|| json.get("logo_dev_publishable_key"))
        .and_then(|v| v.as_str())?;
    let val = val.trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

#[cfg(test)]
fn read_key_from_settings_file() -> Option<String> {
    TEST_SETTINGS_KEY.lock().unwrap().clone()
}

// ─── Domain validation ──────────────────────────────────────────────────────

/// Validate and normalise a domain for the Logo.dev CDN: lowercase, only
/// `[a-z0-9.-]`, 1..=253 chars, has a dot, no leading/trailing dot, no empty
/// or hyphen-edged labels, no "..". Strips a leading `https://` / `http://`,
/// a leading `www.`, and any path/query/fragment. Rejects userinfo, ports,
/// IP literals, and `localhost`.
pub(crate) fn valid_domain(d: &str) -> Option<String> {
    let s = d.trim().to_lowercase();
    if s.is_empty() || s.contains("..") {
        return None;
    }
    let mut rest = s.as_str();
    for prefix in ["https://", "http://"] {
        if let Some(r) = rest.strip_prefix(prefix) {
            rest = r;
            break;
        }
    }
    if let Some(r) = rest.strip_prefix("//") {
        rest = r;
    }
    // Authority ends at the first path/query/fragment separator.
    let end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    let authority = &rest[..end];
    // Userinfo is rejected, not parsed.
    if authority.contains('@') {
        return None;
    }
    // Ports (and any ':' — IPv6 literals) are rejected.
    if authority.contains(':') {
        return None;
    }
    // Only a leading "www." is stripped (registrable-ish, per the spec).
    let host = authority.strip_prefix("www.").unwrap_or(authority);
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    if host.starts_with('.') || host.ends_with('.') || host.len() > 253 {
        return None;
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    for label in &labels {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return None;
        }
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return None;
    }
    // IPv4 literal: four all-numeric labels.
    if labels.len() == 4
        && labels
            .iter()
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    Some(host.to_string())
}

/// Extract the domain from a website URL or bare domain:
/// `https://www.nextjs.org/docs` -> `nextjs.org`. `None` when invalid.
pub(crate) fn domain_from_website(url: &str) -> Option<String> {
    valid_domain(url)
}

// ─── Fetch, validate, cache ─────────────────────────────────────────────────

/// Cached Logo.dev logo for `domain` as a `data:image/png;base64,...` URL
/// (<= 40 KB). `None` when the domain is invalid, the key is missing, the
/// lookup misses (negative-cached for 24 h), or anything fails. Never panics
/// and never blocks longer than the request timeout.
pub(crate) async fn logo_data_url(domain: &str) -> Option<String> {
    let domain = valid_domain(domain)?;
    let Some(dir) = cache_dir() else { return None };
    let digest = sha256_hex(&domain);
    let png_path = dir.join(format!("{digest}.png"));
    let none_path = dir.join(format!("{digest}.none"));
    if fresh(&png_path, PNG_FRESH_SECS) {
        if let Some(url) = png_file_to_data_url(&png_path) {
            return Some(url);
        }
        // Corrupt cache entry: drop it and fall through to the network path.
        let _ = std::fs::remove_file(&png_path);
    }
    if fresh(&none_path, NONE_FRESH_SECS) {
        return None;
    }
    let key = logo_dev_key()?;
    match fetch_logo(DEFAULT_BASE, &domain, &key).await {
        Ok(url) => url,
        Err(reason) => {
            tracing::debug!("brand_logo: lookup failed for {domain}: {reason}");
            None
        }
    }
}

/// Fetch, validate, and cache one logo. `base` is injectable so tests can
/// point at a local mock server. Returns:
/// - `Ok(Some(data_url))` — a valid logo (also written to the PNG cache);
/// - `Ok(None)` — the CDN reported no logo (404; a `.none` marker is written);
/// - `Err(reason)` — network failure (one retry) or a body that is not a real
///   image (nothing is cached). Error strings never contain the key or URL.
pub(crate) async fn fetch_logo(base: &str, domain: &str, key: &str) -> Result<Option<String>, String> {
    let domain = valid_domain(domain).ok_or_else(|| "invalid domain".to_string())?;
    let mut url = reqwest::Url::parse(base).map_err(|_| "invalid base url".to_string())?;
    let loopback = base.starts_with("http://127.0.0.1") || base.starts_with("http://localhost");
    if url.scheme() != "https" && !loopback {
        return Err("base url must be https".to_string());
    }
    url.set_path(&domain);
    url.query_pairs_mut()
        .append_pair("token", key)
        .append_pair("size", "128")
        .append_pair("format", "png")
        .append_pair("retina", "true")
        .append_pair("fallback", "404");

    let mut body: Option<Vec<u8>> = None;
    let mut last_err = String::from("request failed");
    for attempt in 0..2 {
        bump_net_calls();
        match CLIENT.get(url.clone()).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::NOT_FOUND {
                    write_none_marker(&domain);
                    tracing::debug!("brand_logo: {domain} -> 404 (no logo)");
                    return Ok(None);
                }
                if !status.is_success() {
                    tracing::debug!("brand_logo: {domain} -> http {status}");
                    return Err(format!("cdn returned http {status}"));
                }
                match resp.bytes().await {
                    Ok(bytes) => {
                        body = Some(bytes.to_vec());
                        break;
                    }
                    Err(e) => last_err = format!("body read failed ({})", e.without_url()),
                }
            }
            Err(e) => last_err = format!("network error ({})", e.without_url()),
        }
        if attempt == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }
    let Some(bytes) = body else { return Err(last_err) };
    if !looks_like_image(&bytes) {
        tracing::debug!("brand_logo: {domain} -> body was not a recognised image");
        return Err("response was not a recognised image".to_string());
    }
    let png = normalise_to_png(&bytes)?;
    write_png_cache(&domain, &png);
    Ok(Some(to_data_url(&png)))
}

/// Magic-byte check: PNG, JPEG, or WebP.
fn looks_like_image(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return true;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return true;
    }
    bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
}

/// Decode-check with the `image` crate, then produce PNG bytes at or under
/// `MAX_DATA_URL_BYTES`, resizing to `MAX_SIDE_PX` on the longest side when
/// the source is larger.
fn normalise_to_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() <= MAX_DATA_URL_BYTES
        && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
    {
        return Ok(bytes.to_vec());
    }
    let img = image::load_from_memory(bytes).map_err(|_| "image failed to decode".to_string())?;
    let img = if img.width() > MAX_SIDE_PX || img.height() > MAX_SIDE_PX {
        img.resize(MAX_SIDE_PX, MAX_SIDE_PX, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Vec::with_capacity(16 * 1024);
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|_| "png re-encode failed".to_string())?;
    if out.len() > MAX_DATA_URL_BYTES {
        return Err("logo too large after re-encode".to_string());
    }
    Ok(out)
}

fn to_data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

fn png_file_to_data_url(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > MAX_DATA_URL_BYTES || !looks_like_image(&bytes) {
        return None;
    }
    Some(to_data_url(&bytes))
}

fn write_png_cache(domain: &str, png: &[u8]) {
    if let Some(dir) = cache_dir() {
        if std::fs::create_dir_all(&dir).is_ok() {
            let path = dir.join(format!("{}.png", sha256_hex(domain)));
            let _ = std::fs::write(&path, png);
        }
    }
}

fn write_none_marker(domain: &str) {
    if let Some(dir) = cache_dir() {
        if std::fs::create_dir_all(&dir).is_ok() {
            let path = dir.join(format!("{}.none", sha256_hex(domain)));
            let _ = std::fs::write(&path, b"");
        }
    }
}

/// A file exists and was modified within `max_age_secs`.
fn fresh(path: &std::path::Path, max_age_secs: u64) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(elapsed) = SystemTime::now().duration_since(modified) else {
        return false;
    };
    elapsed.as_secs() < max_age_secs
}

fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    let out = h.finalize();
    use std::fmt::Write as _;
    let mut s = String::with_capacity(64);
    for b in out {
        let _ = write!(s, "{b:02x}");
    }
    s
}

// ─── Batch fetch ────────────────────────────────────────────────────────────

/// In-memory copy of every logo fetched or read from disk this run, so the
/// payload builders can attach logos synchronously and repeat scans are free.
static MEMORY: Lazy<std::sync::Mutex<std::collections::HashMap<String, String>>> =
    Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// A logo already resolved this run (never touches the network or the disk).
pub(crate) fn memory_logo(domain: &str) -> Option<String> {
    let d = valid_domain(domain)?;
    MEMORY.lock().ok()?.get(&d).cloned()
}

fn remember(domain: &str, data_url: &str) {
    if let Ok(mut m) = MEMORY.lock() {
        if m.len() < 512 {
            m.insert(domain.to_string(), data_url.to_string());
        }
    }
}

#[cfg(test)]
pub(crate) fn remember_for_test(domain: &str, data_url: &str) {
    remember(domain, data_url);
}

/// Logos for many domains, fetched in parallel (max 6 at a time),
/// de-duplicated, keyed by the validated domain. Misses and failures are
/// simply absent from the map.
pub(crate) async fn logos_for_domains(
    domains: &[String],
) -> std::collections::HashMap<String, String> {
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_PARALLEL));
    let mut seen = std::collections::HashSet::new();
    let mut handles = Vec::new();
    for d in domains {
        let Some(vd) = valid_domain(d) else { continue };
        if !seen.insert(vd.clone()) {
            continue;
        }
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let url = logo_data_url(&vd).await;
            (vd, url)
        }));
    }
    let mut out = std::collections::HashMap::new();
    for h in handles {
        if let Ok((domain, Some(url))) = h.await {
            remember(&domain, &url);
            out.insert(domain, url);
        }
    }
    out
}

// ─── Tests (no real network) ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tiny_png() -> Vec<u8> {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([10, 20, 30, 255]),
        ));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn valid_domain_accepts_and_rejects() {
        assert_eq!(valid_domain("nextjs.org").as_deref(), Some("nextjs.org"));
        assert_eq!(
            valid_domain("https://www.Vercel.com/path?x=1").as_deref(),
            Some("vercel.com")
        );
        assert_eq!(valid_domain("HTTPS://EXAMPLE.COM/A").as_deref(), Some("example.com"));
        assert_eq!(valid_domain("www.github.com").as_deref(), Some("github.com"));
        // Rejects.
        assert_eq!(valid_domain("localhost"), None);
        assert_eq!(valid_domain("127.0.0.1"), None);
        assert_eq!(valid_domain("a"), None);
        assert_eq!(valid_domain("a..b"), None);
        assert_eq!(valid_domain("-a.com"), None);
        assert_eq!(valid_domain("a-.com"), None);
        assert_eq!(valid_domain("a.com:8080"), None);
        assert_eq!(valid_domain("user@a.com"), None);
        assert_eq!(valid_domain("a b.com"), None);
        assert_eq!(valid_domain("evil.com/../x"), None);
        assert_eq!(valid_domain(""), None);
        assert_eq!(valid_domain(".a.com"), None);
        assert_eq!(valid_domain("a.com."), None);
        let long = format!("{a}.com", a = "x".repeat(300));
        assert_eq!(valid_domain(&long), None);
    }

    #[test]
    fn domain_from_website_extracts() {
        assert_eq!(
            domain_from_website("https://www.nextjs.org/docs").as_deref(),
            Some("nextjs.org")
        );
        assert_eq!(
            domain_from_website("https://stripe.com").as_deref(),
            Some("stripe.com")
        );
        assert_eq!(domain_from_website("not a domain"), None);
        assert_eq!(domain_from_website("localhost"), None);
    }

    #[tokio::test]
    async fn cache_hit_needs_no_key_or_network() {
        let _guard = TEST_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        set_test_cache_dir(Some(tmp.path().to_path_buf()));
        set_test_settings_key(None);
        let png = tiny_png();
        std::fs::write(tmp.path().join(format!("{}.png", sha256_hex("nextjs.org"))), &png).unwrap();
        let before = network_calls();
        let url = logo_data_url("nextjs.org").await;
        assert_eq!(network_calls(), before, "cache hit must not touch the network");
        let url = url.expect("cached PNG must be returned");
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(url, to_data_url(&png));
        set_test_cache_dir(None);
    }

    #[tokio::test]
    async fn none_marker_is_honoured() {
        let _guard = TEST_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        set_test_cache_dir(Some(tmp.path().to_path_buf()));
        set_test_settings_key(None);
        std::fs::write(tmp.path().join(format!("{}.none", sha256_hex("vercel.com"))), b"").unwrap();
        let before = network_calls();
        assert_eq!(logo_data_url("vercel.com").await, None);
        assert_eq!(network_calls(), before, "a fresh miss marker must not hit the network");
        set_test_cache_dir(None);
    }

    #[tokio::test]
    async fn missing_key_returns_none_without_network() {
        let _guard = TEST_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        set_test_cache_dir(Some(tmp.path().to_path_buf()));
        set_test_settings_key(None);
        let before = network_calls();
        assert_eq!(logo_data_url("vercel.com").await, None);
        assert_eq!(network_calls(), before, "missing key must mean zero network calls");
        set_test_cache_dir(None);
    }

    #[tokio::test]
    async fn invalid_domain_returns_none_without_network() {
        let before = network_calls();
        assert_eq!(logo_data_url("localhost").await, None);
        assert_eq!(logo_data_url("a b.com").await, None);
        assert_eq!(network_calls(), before);
    }

    #[test]
    fn key_gating_requires_pk_prefix() {
        let _guard = TEST_GUARD.lock().unwrap();
        set_test_settings_key(None);
        // Keychain mock is empty under test, so the settings fallback decides.
        set_test_settings_key(Some("sk_secret_server_key".to_string()));
        assert_eq!(logo_dev_key(), None, "secret keys are never accepted");
        set_test_settings_key(Some("not_a_key".to_string()));
        assert_eq!(logo_dev_key(), None);
        set_test_settings_key(Some(String::new()));
        assert_eq!(logo_dev_key(), None);
        set_test_settings_key(Some("  pk_live_abc123  ".to_string()));
        assert_eq!(logo_dev_key().as_deref(), Some("pk_live_abc123"));
        set_test_settings_key(None);
    }

    /// One-shot HTTP mock on 127.0.0.1 (std TcpListener on a thread, so no
    /// tokio "net" feature is needed). Returns the base URL.
    fn spawn_mock_server(status: &'static str, content_type: &'static str, body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = std::io::Read::read(&mut sock, &mut buf);
                let mut resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                resp.extend_from_slice(&body);
                let _ = sock.write_all(&resp);
                let _ = sock.shutdown(std::net::Shutdown::Both);
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn mock_png_is_accepted_and_cached() {
        let _guard = TEST_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        set_test_cache_dir(Some(tmp.path().to_path_buf()));
        let png = tiny_png();
        let base = spawn_mock_server("200 OK", "image/png", png.clone());
        let key = "pk_supersecret_test_key";
        let result = fetch_logo(&base, "nextjs.org", key).await;
        let url = result
            .expect("valid PNG must be accepted")
            .expect("a 200 response must yield a data url");
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(url, to_data_url(&png));
        let cached = tmp.path().join(format!("{}.png", sha256_hex("nextjs.org")));
        assert_eq!(std::fs::read(&cached).unwrap(), png, "the PNG must be cached");
        set_test_cache_dir(None);
    }

    #[tokio::test]
    async fn mock_404_writes_none_marker() {
        let _guard = TEST_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        set_test_cache_dir(Some(tmp.path().to_path_buf()));
        let base = spawn_mock_server("404 Not Found", "text/plain", b"not found".to_vec());
        let result = fetch_logo(&base, "vercel.com", "pk_test_key_123456").await;
        assert_eq!(result.unwrap(), None, "404 must map to a miss");
        let marker = tmp.path().join(format!("{}.none", sha256_hex("vercel.com")));
        assert!(marker.exists(), "a 404 must write the .none marker");
        set_test_cache_dir(None);
    }

    #[tokio::test]
    async fn mock_html_body_is_rejected_and_key_never_leaks() {
        let _guard = TEST_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        set_test_cache_dir(Some(tmp.path().to_path_buf()));
        let base = spawn_mock_server("200 OK", "text/html", b"<html>not an image</html>".to_vec());
        let key = "pk_supersecret_test_key";
        let result = fetch_logo(&base, "nextjs.org", key).await;
        let err = result.expect_err("an HTML body must be rejected");
        assert!(!err.contains(key), "error strings must never contain the key");
        assert!(!err.contains("http"), "error strings must never contain the URL");
        // Nothing may be cached for a rejected body.
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
        set_test_cache_dir(None);
    }

    #[tokio::test]
    async fn network_error_never_leaks_the_key_or_url() {
        let _guard = TEST_GUARD.lock().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // a closed port: every connect fails
        let base = format!("http://{addr}");
        let key = "pk_supersecret_test_key";
        let result = fetch_logo(&base, "nextjs.org", key).await;
        let err = result.expect_err("a dead endpoint must be an error");
        assert!(!err.contains(key), "error strings must never contain the key");
        assert!(!err.contains(&addr.to_string()), "error strings must never contain the URL");
    }

    #[tokio::test]
    async fn logos_for_domains_dedups_and_skips_invalid() {
        let _guard = TEST_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        set_test_cache_dir(Some(tmp.path().to_path_buf()));
        set_test_settings_key(None);
        let png = tiny_png();
        std::fs::write(tmp.path().join(format!("{}.png", sha256_hex("nextjs.org"))), &png).unwrap();
        let before = network_calls();
        let domains = vec![
            "https://www.nextjs.org/docs".to_string(),
            "nextjs.org".to_string(), // duplicate after validation
            "localhost".to_string(),  // invalid, skipped
            "a..b".to_string(),       // invalid, skipped
        ];
        let map = logos_for_domains(&domains).await;
        assert_eq!(network_calls(), before, "cache hits must not touch the network");
        assert_eq!(map.len(), 1, "duplicates collapse and invalid domains are skipped");
        assert_eq!(map.get("nextjs.org").map(String::as_str), Some(to_data_url(&png).as_str()));
        set_test_cache_dir(None);
    }
}
