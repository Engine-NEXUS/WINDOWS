//! Foreground app/site review evidence and its local encrypted reference.
//! The orchestrator owns capture and turn lifecycle. This module only accepts
//! a frozen target, inspects eligible public pages, and stores one review.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, Runtime};

const MAX_STORED_BYTES: usize = 9_000_000;
const MAX_HTML_BYTES: usize = 1_000_000;
const MAX_PAGES: usize = 6;
static SESSION_REVIEW: Lazy<Mutex<Option<ReviewRecord>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnologyClaim {
    pub name: String,
    pub status: String,
    pub evidence: String,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPage {
    pub url: String,
    pub title: String,
    pub captured_at: String,
    pub image_data_url: Option<String>,
    pub method: String,
    pub completeness: String,
    pub rea_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewMeta {
    pub id: String,
    pub created_at: String,
    pub target_app: String,
    pub target_title: String,
    pub target_url: Option<String>,
    pub completeness: String,
    pub persistence: String,
    pub pages: Vec<ReviewPage>,
    pub technologies: Vec<TechnologyClaim>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub meta: ReviewMeta,
    pub spatial: crate::vision::SpatialAnalysisPayload,
}

fn review_path<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("foreground-review.nxm"))
}

/// Save only a sealed blob. If the OS-backed key is unavailable, the review
/// remains in memory for this process and is explicitly marked temporary.
pub fn save<R: Runtime>(app: &tauri::AppHandle<R>, mut record: ReviewRecord) -> Result<ReviewRecord, String> {
    record.meta.persistence = "session-only".into();
    if let (Some(key), Some(path)) = (crate::memcore::crypto::process_key(), review_path(app)) {
        record.meta.persistence = "encrypted-local".into();
        let plain = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        if plain.len() > MAX_STORED_BYTES {
            return Err("Review evidence exceeds the local size limit".into());
        }
        let sealed = crate::memcore::crypto::seal(&key, &plain)
            .ok_or("Could not encrypt the review")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if !crate::memory::atomic_write(&path, &sealed) {
            return Err("Could not save the encrypted review".into());
        }
    }
    *SESSION_REVIEW.lock().map_err(|e| e.to_string())? = Some(record.clone());
    Ok(record)
}

pub fn last<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<ReviewRecord> {
    if let Some(hit) = SESSION_REVIEW.lock().ok()?.clone() {
        return Some(hit);
    }
    let (key, path) = (crate::memcore::crypto::process_key()?, review_path(app)?);
    let sealed = std::fs::read(path).ok()?;
    if sealed.len() > MAX_STORED_BYTES + 64 {
        return None;
    }
    let plain = crate::memcore::crypto::open(&key, &sealed)?;
    let record: ReviewRecord = serde_json::from_slice(&plain).ok()?;
    *SESSION_REVIEW.lock().ok()? = Some(record.clone());
    Some(record)
}

pub fn delete<R: Runtime>(app: &tauri::AppHandle<R>, id: &str) -> Result<(), String> {
    let Some(record) = last(app) else { return Err("No saved review".into()) };
    if record.meta.id != id { return Err("Review ID does not match".into()) }
    if let Some(path) = review_path(app) {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    *SESSION_REVIEW.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

#[tauri::command]
pub fn get_last_foreground_review<R: Runtime>(app: tauri::AppHandle<R>) -> Option<ReviewRecord> {
    last(&app)
}

#[tauri::command]
pub fn delete_foreground_review<R: Runtime>(app: tauri::AppHandle<R>, id: String) -> Result<(), String> {
    delete(&app, &id)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private() || v.is_loopback() || v.is_link_local()
                || v.is_multicast() || v.is_broadcast() || v.is_unspecified()
                || v.is_documentation() || o[0] == 0 || o[0] >= 224
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19)))
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            !(v.is_loopback() || v.is_unspecified() || v.is_multicast()
                || v.is_unique_local() || v.is_unicast_link_local())
        }
    }
}

pub fn eligible_public_url(raw: &str) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(raw).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if !host.contains('.') || host == "localhost" || host.ends_with(".localhost")
        || host.ends_with(".local") || host.ends_with(".internal")
        || host.ends_with(".test") || host.ends_with(".invalid") {
        return None;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !public_ip(ip) { return None; }
    }
    // Public inspection does not carry session or tracking parameters out of
    // the foreground tab. The page path remains enough to find main links.
    url.set_query(None);
    url.set_fragment(None);
    Some(url)
}

/// Resolve and pin a public address so a DNS change cannot redirect the
/// subsequent HTTP request to a private address.
pub(crate) fn public_address(url: &reqwest::Url) -> Result<SocketAddr, String> {
    let host = url.host_str().ok_or("No host")?;
    let addrs: Vec<SocketAddr> = (host, 443)
        .to_socket_addrs().map_err(|e| format!("DNS lookup failed: {e}"))?
        .collect();
    if addrs.is_empty() || addrs.iter().any(|a| !public_ip(a.ip())) {
        return Err("The page resolves to a private or unsupported address".into());
    }
    Ok(addrs.iter().copied().find(SocketAddr::is_ipv4).unwrap_or(addrs[0]))
}

fn pinned_client(url: &reqwest::Url) -> Result<reqwest::Client, String> {
    let host = url.host_str().ok_or("No host")?;
    let addr = public_address(url)?;
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(host, addr)
        .build().map_err(|e| e.to_string())
}

async fn fetch_public_html(url: &reqwest::Url) -> Result<String, String> {
    let client = pinned_client(url)?;
    let mut resp = client.get(url.clone()).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let content_type = resp.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    if !content_type.contains("text/html") {
        return Err("The page did not return HTML".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_HTML_BYTES {
            return Err("The page HTML exceeded the inspection limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn main_page_links(base: &reqwest::Url, html: &str) -> Vec<reqwest::Url> {
    static HREF: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?is)<a\b[^>]*?\bhref\s*=\s*["']([^"']{1,500})["']"#).unwrap());
    let mut out = Vec::new();
    for cap in HREF.captures_iter(html).take(300) {
        let Some(mut candidate) = base.join(&cap[1]).ok() else { continue };
        candidate.set_fragment(None);
        candidate.set_query(None);
        if candidate.scheme() != "https" || candidate.origin() != base.origin()
            || eligible_public_url(candidate.as_str()).is_none() { continue }
        let path = candidate.path().to_ascii_lowercase();
        if !["about", "feature", "product", "service", "pricing", "contact", "docs"]
            .iter().any(|word| path.contains(word)) { continue }
        if candidate == *base || out.contains(&candidate) { continue }
        out.push(candidate);
        if out.len() >= MAX_PAGES - 1 { break }
    }
    out
}

fn claims_from_html(url: &reqwest::Url, html: &str) -> Vec<TechnologyClaim> {
    let lower = html.to_ascii_lowercase();
    let signals = [
        ("Next.js", "/_next/"),
        ("WordPress", "/wp-content/"),
        ("Angular", "ng-version="),
        ("Vue", "data-v-"),
        ("Bootstrap", "bootstrap.min.css"),
        ("Vite", "/@vite/client"),
    ];
    signals.iter().filter(|(_, marker)| lower.contains(marker))
        .map(|(name, marker)| TechnologyClaim {
            name: (*name).into(), status: "observed-marker".into(),
            evidence: format!("HTML contains {marker}"), source_url: url.to_string(),
        }).collect()
}

pub struct PublicSiteEvidence {
    pub pages: Vec<ReviewPage>,
    pub technologies: Vec<TechnologyClaim>,
    pub warnings: Vec<String>,
}

/// Discover same-origin main pages without touching the user's browser.
/// REA observation and screenshots are layered onto these URLs separately.
pub async fn discover_public_pages(url: &str) -> PublicSiteEvidence {
    let mut result = PublicSiteEvidence { pages: Vec::new(), technologies: Vec::new(), warnings: Vec::new() };
    let Some(base) = eligible_public_url(url) else {
        result.warnings.push("Private, signed-in, or unsupported URL: public-page inspection skipped.".into());
        return result;
    };
    let html = match fetch_public_html(&base).await {
        Ok(html) => html,
        Err(e) => {
            result.warnings.push(format!("Public-page discovery unavailable: {e}"));
            return result;
        }
    };
    result.technologies = claims_from_html(&base, &html);
    let mut urls = vec![base.clone()];
    urls.extend(main_page_links(&base, &html));
    for url in urls {
        result.pages.push(ReviewPage {
            url: url.to_string(), title: url.path().trim_matches('/').replace('-', " "),
            captured_at: chrono::Utc::now().to_rfc3339(), image_data_url: None,
            method: "public HTML discovery".into(), completeness: "not captured".into(),
            rea_summary: None,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_private_and_credential_urls() {
        for s in ["http://example.com", "https://localhost/a", "https://192.168.1.2/",
            "https://user:pass@example.com/", "https://intranet.local/", "file:///etc/passwd"] {
            assert!(eligible_public_url(s).is_none(), "{s}");
        }
        assert!(eligible_public_url("https://example.com/about").is_some());
    }

    #[test]
    fn picks_bounded_same_origin_main_pages() {
        let base = reqwest::Url::parse("https://example.com/").unwrap();
        let html = r#"<a href="/about">About</a><a href="https://evil.example/contact">Other</a>
          <a href="/pricing?x=1">Pricing</a><a href="/about">Dup</a><a href="http://example.com/docs">HTTP</a>"#;
        let links = main_page_links(&base, html);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].as_str(), "https://example.com/about");
        assert_eq!(links[1].as_str(), "https://example.com/pricing");
    }

    #[test]
    fn html_claims_have_exact_evidence() {
        let url = reqwest::Url::parse("https://example.com/").unwrap();
        let claims = claims_from_html(&url, "<script src='/_next/app.js'></script>");
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].name, "Next.js");
        assert!(claims[0].evidence.contains("/_next/"));
    }
}
