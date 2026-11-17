//! site_probe — a safe website technology scanner (spec_p4b.md, P4B)
//!
//! The probe is a MEDIUM-level scanner: it fetches the first HTML document of
//! the user's URL (read-only GET, ≤1.5 MB, whole-probe ≤20 s), follows at most
//! 5 redirects, fetches the first 6 <script src> (same-site only) plus the
//! first 2 <link rel=stylesheet> sub-resources (read-only GETs, ≤400 KB each,
//! 8 s timeout), collects cookie NAMES across all redirect hops, and pins DNS
//! per hop (every resolved address must be public, or loopback in tests).
//!
//! The result is built by analyse — a pure function; nothing else touches
//! the network — using the shipped `site_fingerprints.json` table (read-only
//! include_str!; never network, never filesystem, never app-data).
//!
//! Statuses from a per-signal weighted product:
//! Detected ≥ 0.80, Likely ≥ 0.50, Inferred ≥ 0.30; below 0.30 dropped.

use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CONTENT_TYPE, HeaderMap, HeaderValue, SET_COOKIE,
};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

const MAX_HTML_BYTES: usize = 1_572_864;
const MAX_SUBRESOURCE_BYTES: usize = 409_600;
const MAX_SCRIPTS: usize = 6;
const MAX_STYLESHEETS: usize = 2;
const MAX_REDIRECTS: usize = 5;
const TOTAL_TIMEOUT_MS: u64 = 20_000;
const HOP_TIMEOUT_SECS: u64 = 10;
const CONNECT_TIMEOUT_SECS: u64 = 4;
const SUB_TIMEOUT_SECS: u64 = 8;
/// A plain desktop-browser identity. A custom bot token ("…StackScan/1.0") made
/// WAFs serve challenge pages instead of the site (audit 03 §3.5).
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const ACCEPT_LANGUAGE_VALUE: &str = "en-US,en;q=0.9";
const NOTE_TRUNCATED: &str = "page truncated at 1.5 MB";
/// Inline `<script>` bodies are searched too (most analytics snippets and
/// framework state live there); each is capped, and so is the total.
const MAX_INLINE_SCRIPT_BYTES: usize = 65_536;
const MAX_INLINE_TOTAL_BYTES: usize = 524_288;

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    Detected,
    Likely,
    Inferred,
}

impl Status {
    fn from_conf(c: f32) -> Status {
        if c >= 0.80 {
            Status::Detected
        } else if c >= 0.50 {
            Status::Likely
        } else {
            Status::Inferred
        }
    }
}

// ---------------------------------------------------------------------------
// Fingerprint table (compiled once from a read-only JSON in the installer)
// ---------------------------------------------------------------------------

const FINGERPRINT_JSON: &str = include_str!("site_fingerprints.json");

#[derive(Deserialize)]
struct SignalJson {
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    contains: Option<String>,
    #[serde(default)]
    regex: Option<String>,
    #[serde(default)]
    weight: f32,
    evidence: String,
    /// `dom` only: read this attribute of the matched element.
    #[serde(default)]
    attr: Option<String>,
    /// Capture a version from the matched value (js value, dom attribute, or
    /// the first regex group).
    #[serde(default)]
    version: bool,
}

#[derive(Deserialize)]
struct TechJson {
    id: String,
    name: String,
    category: String,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    signals: Vec<SignalJson>,
    #[serde(default)]
    implies: Vec<String>,
}

#[derive(Deserialize)]
struct TableJson {
    #[serde(rename = "version", default)]
    _version: u8,
    technologies: Vec<TechJson>,
}

struct CompiledSignal {
    kind: &'static str,
    /// Lowercased for header/meta/cookie; kept exactly for `js` (a global
    /// path) and `dom` (a CSS selector), which are case-sensitive.
    name: Option<String>,
    contains: Option<String>,
    regex: Option<Regex>,
    cookie_name: Option<String>,
    attr: Option<String>,
    version: bool,
    weight: f32,
    evidence: String,
    support_only: bool,
}

/// `js` signal names: a dotted global path ("next.version", "jQuery.fn.jquery")
/// or a built-in collector marker ("@react-root"). Validated so a table typo can
/// never become arbitrary text handed to the page.
pub(crate) fn valid_js_path(p: &str) -> bool {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(?:@[a-z][a-z0-9-]{1,30}|[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*){0,4})$").unwrap()
    });
    p.len() <= 80 && RE.is_match(p)
}

/// `dom` signal names: a short CSS selector with no braces or tag brackets.
pub(crate) fn valid_dom_selector(s: &str) -> bool {
    !s.is_empty() && s.len() <= 120 && !s.contains(['{', '}', '<', '>', '\\', '`'])
}

/// A version string is accepted only when it starts like one ("18", "4.2.1");
/// returns that numeric prefix.
pub(crate) fn clean_version(raw: &str) -> Option<String> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^v?(\d{1,4}(?:\.\d{1,5}){0,3})").unwrap());
    RE.captures(raw.trim()).map(|c| c[1].to_string())
}

struct CompiledTech {
    id: String,
    name: String,
    category: String,
    website: Option<String>,
    signals: Vec<CompiledSignal>,
    implies: Vec<String>,
}

fn normalize_kind(k: &str) -> Option<&'static str> {
    match k {
        "script_text" | "generic" => Some("script_text"),
        // Only the hidden render pass (site_render.rs) fills these inputs.
        "js" => Some("js"),
        "dom" => Some("dom"),
        "stylesheet" | "css_text" => Some("css_text"),
        "script_src" => Some("script_src"),
        "header" => Some("header"),
        "html" => Some("html"),
        "cookie" => Some("cookie"),
        "link_href" => Some("link_href"),
        "url" => Some("url"),
        "meta" => Some("meta"),
        _ => {
            tracing::warn!("site_probe: unknown signal kind in fingerprint table: {k}");
            None
        }
    }
}

static TABLE: Lazy<Vec<CompiledTech>> = Lazy::new(|| {
    match serde_json::from_str::<TableJson>(FINGERPRINT_JSON) {
        Ok(t) => t
            .technologies
            .into_iter()
            .map(|t| CompiledTech {
                id: t.id.clone(),
                name: t.name.clone(),
                category: t.category.clone(),
                website: t.website,
                signals: t
                    .signals
                    .iter()
                    .filter_map(|s| {
                        let kind = normalize_kind(&s.kind)?;
                        let exact_name = matches!(kind, "js" | "dom");
                        if kind == "js" && !s.name.as_deref().is_some_and(valid_js_path) {
                            tracing::warn!("site_probe: bad js path in table: {:?}", s.name);
                            return None;
                        }
                        if kind == "dom" && !s.name.as_deref().is_some_and(valid_dom_selector) {
                            tracing::warn!("site_probe: bad dom selector in table: {:?}", s.name);
                            return None;
                        }
                        let regex = s.regex.as_ref().and_then(|r| {
                            match Regex::new(r) {
                                Ok(re) => Some(re),
                                Err(e) => {
                                    tracing::warn!("site_probe: bad regex '{r}': {e}");
                                    None
                                }
                            }
                        });
                        Some(CompiledSignal {
                            kind,
                            name: s.name.as_ref().map(|n| {
                                if exact_name { n.clone() } else { n.to_ascii_lowercase() }
                            }),
                            contains: s.contains.as_ref().map(|c| c.to_ascii_lowercase()),
                            regex,
                            cookie_name: s.name.as_ref().map(|n| n.to_ascii_lowercase())
                                .filter(|_| s.kind == "cookie" && s.contains.is_none() && s.regex.is_none()),
                            attr: s.attr.clone().filter(|a| {
                                a.len() <= 40 && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                            }),
                            version: s.version,
                            weight: s.weight,
                            evidence: s.evidence.clone(),
                            support_only: s.weight < 0.30,
                        })
                    })
                    .collect(),
                implies: t.implies.clone(),
            })
            .collect(),
        Err(e) => {
            tracing::warn!("site_probe: failed to parse fingerprint table: {e}");
            Vec::new()
        }
    }
});

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Detection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) website: Option<String>,
    pub(crate) confidence: f32,
    pub(crate) status: Status,
    pub(crate) evidence: Vec<String>,
    /// Captured only when a signal asks for it and the text looks like a version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
}

/// Why a scan saw less than the real page (audit 03 §3.5). Spoken and shown so
/// a thin result is never mistaken for "this site uses nothing".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub(crate) enum ThinReason {
    /// A bot check (Cloudflare, Akamai, DataDome…) answered instead of the site.
    Challenge(String),
    /// The fetch landed on a sign-in page.
    LoginWall,
    /// The HTML is an empty shell that builds itself in the browser.
    SpaShell,
    /// The server answered with an error status.
    HttpError(u16),
}

impl ThinReason {
    pub(crate) fn spoken(&self) -> String {
        match self {
            ThinReason::Challenge(who) => {
                format!("The site showed me a {who} bot check, so I could only see its hosting.")
            }
            ThinReason::LoginWall => {
                "The site sent me to a sign-in page, so I only saw what's public.".into()
            }
            ThinReason::SpaShell => {
                "The page builds itself in the browser, so its first HTML said little.".into()
            }
            ThinReason::HttpError(code) => format!("The site answered with error {code}."),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SecurityHeaders {
    pub(crate) hsts: bool,
    pub(crate) csp: bool,
    pub(crate) xfo: bool,
    pub(crate) xcto: bool,
    pub(crate) referrer_policy: bool,
    pub(crate) permissions_policy: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct PageStats {
    pub(crate) html_bytes: usize,
    pub(crate) js_bytes: usize,
    pub(crate) css_bytes: usize,
    pub(crate) script_count: usize,
    pub(crate) inline_script_count: usize,
    pub(crate) stylesheet_count: usize,
    pub(crate) image_count: usize,
    pub(crate) third_party_hosts: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct SiteReport {
    pub(crate) url: String,
    pub(crate) final_url: String,
    pub(crate) status_code: u16,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) lang: String,
    pub(crate) detections: Vec<Detection>,
    pub(crate) security: SecurityHeaders,
    pub(crate) stats: PageStats,
    pub(crate) notes: Vec<String>,
    /// Empty when the page looked normal.
    pub(crate) thin: Vec<ThinReason>,
    /// Matching inputs kept so the hidden render can add to them and score
    /// again (site_render.rs). Never serialized.
    #[serde(skip)]
    pub(crate) ctx: Option<std::sync::Arc<MatchCtx>>,
}

// ---------------------------------------------------------------------------
// Probe refusals with a short spoken line
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum ProbeRefusal {
    BadUrl,
    NotHttp,
    PrivateAddress,
    Sensitive,
    NotHtml,
    TooManyRedirects,
    Timeout,
    Network(String),
}

impl ProbeRefusal {
    pub(crate) fn spoken(&self) -> String {
        match self {
            ProbeRefusal::BadUrl => "That does not look like a web address, sir.".into(),
            ProbeRefusal::NotHttp => "I can only open that over http or https, sir.".into(),
            ProbeRefusal::PrivateAddress => "I only scan sites on the public internet, sir.".into(),
            ProbeRefusal::Sensitive => "I do not scan private or banking sites, sir.".into(),
            ProbeRefusal::NotHtml => "That address did not serve an HTML page, sir.".into(),
            ProbeRefusal::TooManyRedirects => "That site redirected too many times, sir.".into(),
            ProbeRefusal::Timeout => "The scan took too long, sir.".into(),
            ProbeRefusal::Network(err) => format!("The site did not answer, sir — {err}"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProbeOpts {
    // Test-only: let 127.0.0.1 through so the mock server can be reached.
    pub(crate) allow_loopback: bool,
    pub(crate) total_timeout_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Confidence math
// ---------------------------------------------------------------------------

pub(crate) fn combine_confidence(weights: &[f32]) -> f32 {
    let prod: f32 = weights.iter().map(|w| (1.0 - *w).max(0.0)).product();
    (1.0 - prod).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// HTML regex helpers (lazy compiled regexes only — no DOM crate)
// ---------------------------------------------------------------------------

macro_rules! tag_regex {
    ($name:ident, $tag:literal) => {
        static $name: Lazy<Regex> =
            Lazy::new(|| Regex::new(concat!(r"(?is)<", $tag, r"\b[^>]*>")).unwrap());
    };
}

tag_regex!(RE_META_TAG, "meta");
tag_regex!(RE_SCRIPT_TAG, "script");
tag_regex!(RE_LINK_TAG, "link");
tag_regex!(RE_IMG_TAG, "img");
tag_regex!(RE_IFRAME_TAG, "iframe");
tag_regex!(RE_HTML_TAG, "html");

macro_rules! attr_regex {
    ($name:ident, $attr:literal) => {
        static $name: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r#"(?i)\b"#,
                $attr,
                r#"\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#
            ))
            .unwrap()
        });
    };
}

attr_regex!(RE_ATTR_SRC, "src");
attr_regex!(RE_ATTR_HREF, "href");
attr_regex!(RE_ATTR_REL, "rel");
attr_regex!(RE_ATTR_NAME, "name");
attr_regex!(RE_ATTR_CONTENT, "content");
attr_regex!(RE_ATTR_LANG, "lang");

static RE_TITLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<title\b[^>]*>(.*?)</title>").unwrap());

static RE_ENTITY_MATCH: Lazy<Regex> =
    Lazy::new(|| Regex::new("(?i)&(amp|lt|gt|quot|apos|#39|nbsp);").unwrap());

fn decode_entity(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" | "39" => "'",
        "nbsp" => " ",
        _ => "",
    }
}

fn decode_entities(text: &str) -> String {
    RE_ENTITY_MATCH
        .replace_all(text, |c: &Captures| decode_entity(&c[1]).to_string())
        .into_owned()
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::new();
    let mut in_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out.trim().to_string()
}

fn attr_value(tag: &str, re: &Regex) -> Option<String> {
    let c = re.captures(tag)?;
    for g in c.iter().skip(1).flatten() {
        return Some(decode_entities(g.as_str()));
    }
    None
}

/// Multi-label public suffixes (country second levels) and shared hosting
/// platforms where every customer gets a subdomain. A small curated list, not
/// the full public suffix list: it only decides third-party display and which
/// scripts count as "the site's own".
const MULTI_LABEL_SUFFIXES: &[&str] = &[
    // country second levels
    "co.uk", "org.uk", "ac.uk", "gov.uk", "me.uk", "ltd.uk", "plc.uk",
    "com.au", "net.au", "org.au", "edu.au", "gov.au",
    "co.nz", "org.nz", "co.jp", "ne.jp", "or.jp", "ac.jp",
    "co.in", "net.in", "org.in", "gov.in", "ac.in", "firm.in", "gen.in", "ind.in",
    "com.br", "net.br", "org.br", "com.mx", "com.ar", "com.co", "com.tr", "com.sg",
    "com.my", "com.hk", "com.tw", "com.cn", "net.cn", "org.cn", "co.kr", "or.kr",
    "co.za", "org.za", "co.id", "co.il", "com.pk", "com.ng", "com.eg", "com.sa",
    "com.ua", "com.vn", "com.ph", "co.th", "com.pe", "com.pl",
    // platforms that hand every customer a subdomain
    "github.io", "gitlab.io", "vercel.app", "now.sh", "netlify.app", "netlify.com",
    "pages.dev", "workers.dev", "web.app", "firebaseapp.com", "herokuapp.com",
    "azurewebsites.net", "azurestaticapps.net", "cloudfront.net", "appspot.com",
    "onrender.com", "fly.dev", "railway.app", "replit.app", "repl.co", "glitch.me",
    "surge.sh", "wixsite.com", "squarespace.com", "myshopify.com", "webflow.io",
    "framer.app", "framer.website", "blogspot.com", "wordpress.com", "substack.com",
    "notion.site", "carrd.co", "bubbleapps.io", "translate.goog",
];

/// The registrable part of a host ("www.bbc.co.uk" -> "bbc.co.uk",
/// "a.vercel.app" -> "a.vercel.app"). IP literals come back unchanged.
pub(crate) fn registrable_domain(host: &str) -> String {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    if h.parse::<IpAddr>().is_ok() || h.starts_with('[') {
        return h;
    }
    let labels: Vec<&str> = h.split('.').filter(|l| !l.is_empty()).collect();
    let n = labels.len();
    if n <= 2 {
        return labels.join(".");
    }
    let last_two = format!("{}.{}", labels[n - 2], labels[n - 1]);
    let last_three = format!("{}.{}.{}", labels[n - 3], labels[n - 2], labels[n - 1]);
    let keep = if MULTI_LABEL_SUFFIXES.contains(&last_three.as_str()) {
        4
    } else if MULTI_LABEL_SUFFIXES.contains(&last_two.as_str()) {
        3
    } else {
        2
    };
    labels[n.saturating_sub(keep)..].join(".")
}

fn same_site(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.parse::<IpAddr>().is_ok() || b.parse::<IpAddr>().is_ok() {
        return false;
    }
    registrable_domain(a) == registrable_domain(b)
}

// ---------------------------------------------------------------------------
// analyse — pure function of (url, status, headers, html, scripts, styles)
// ---------------------------------------------------------------------------

/// Everything the fingerprint signals are matched against. The static fetch
/// fills the first nine fields; the hidden render (site_render.rs) adds the
/// last two and more script URLs, then scores again.
#[derive(Debug, Clone, Default)]
pub(crate) struct MatchCtx {
    pub(crate) html_lower: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) cookie_names: Vec<String>,
    pub(crate) metas: Vec<(String, String)>,
    pub(crate) script_srcs: Vec<String>,
    pub(crate) link_hrefs: Vec<String>,
    pub(crate) scripts: Vec<String>,
    pub(crate) styles: Vec<String>,
    pub(crate) host: String,
    /// (global path or "@marker", primitive value if it had one)
    pub(crate) js_globals: Vec<(String, Option<String>)>,
    /// (selector, requested attribute, attribute value)
    pub(crate) dom_hits: Vec<(String, Option<String>, Option<String>)>,
}

/// Did the value match, and if the signal asks for a version, which one?
/// `None` = no match; `Some(v)` = matched with optional version.
fn value_match(sig: &CompiledSignal, raw: &str) -> Option<Option<String>> {
    if let Some(needle) = &sig.contains {
        return raw.to_ascii_lowercase().contains(needle).then_some(None);
    }
    if let Some(re) = &sig.regex {
        let caps = re.captures(raw)?;
        let version = if sig.version {
            caps.get(1).and_then(|m| clean_version(m.as_str()))
        } else {
            None
        };
        return Some(version);
    }
    Some(None)
}

fn signal_match(sig: &CompiledSignal, ctx: &MatchCtx) -> Option<Option<String>> {
    match sig.kind {
        "html" => {
            if sig.contains.is_none() && sig.regex.is_none() {
                return None;
            }
            value_match(sig, &ctx.html_lower)
        }
        "header" => {
            let want = sig.name.as_ref()?;
            ctx.headers
                .iter()
                .filter(|(h, _)| h.eq_ignore_ascii_case(want))
                .find_map(|(_, v)| value_match(sig, v))
        }
        "cookie" => ctx
            .cookie_names
            .iter()
            .any(|c| {
                if let Some(n) = &sig.cookie_name {
                    c.to_ascii_lowercase().starts_with(n)
                } else if let Some(sub) = &sig.contains {
                    c.to_ascii_lowercase().contains(sub)
                } else if let Some(re) = &sig.regex {
                    re.is_match(c)
                } else {
                    false
                }
            })
            .then_some(None),
        "meta" => {
            let want = sig.name.as_ref()?;
            ctx.metas
                .iter()
                .filter(|(m, _)| m == want)
                .find_map(|(_, content)| value_match(sig, content))
        }
        "script_src" => ctx.script_srcs.iter().find_map(|u| value_match(sig, u)),
        "link_href" => ctx.link_hrefs.iter().find_map(|u| value_match(sig, u)),
        "script_text" => ctx.scripts.iter().find_map(|t| value_match(sig, t)),
        "css_text" => ctx.styles.iter().find_map(|t| value_match(sig, t)),
        "url" => value_match(sig, &ctx.host),
        "js" => {
            let want = sig.name.as_ref()?;
            let (_, value) = ctx.js_globals.iter().find(|(p, _)| p == want)?;
            if sig.contains.is_some() || sig.regex.is_some() {
                let v = value.as_deref()?;
                let hit = value_match(sig, v)?;
                return Some(hit.or_else(|| sig.version.then(|| clean_version(v)).flatten()));
            }
            Some(if sig.version { value.as_deref().and_then(clean_version) } else { None })
        }
        "dom" => {
            let want = sig.name.as_ref()?;
            let (_, _, value) = ctx
                .dom_hits
                .iter()
                .find(|(s, a, _)| s == want && a.as_deref() == sig.attr.as_deref())?;
            if sig.contains.is_some() || sig.regex.is_some() {
                let v = value.as_deref()?;
                let hit = value_match(sig, v)?;
                return Some(hit.or_else(|| sig.version.then(|| clean_version(v)).flatten()));
            }
            Some(if sig.version { value.as_deref().and_then(clean_version) } else { None })
        }
        _ => None,
    }
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

fn push_unique_pair(list: &mut Vec<(String, String)>, name: String, value: String) {
    if !list.iter().any(|(n, v)| n == &name && v == &value) {
        list.push((name, value));
    }
}

/// Opening tag plus body of every `<script>` element.
static RE_SCRIPT_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script\b([^>]*)>(.*?)</script\s*>").unwrap());
static RE_STYLE_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>").unwrap());
static RE_ANY_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());

/// Inline script bodies (no `src`), each truncated, the total capped.
pub(crate) fn inline_script_bodies(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut total = 0usize;
    for c in RE_SCRIPT_BLOCK.captures_iter(html) {
        let open = c.get(1).map(|m| m.as_str()).unwrap_or("");
        if attr_value(&format!("<script {open}>"), &RE_ATTR_SRC).is_some_and(|s| !s.is_empty()) {
            continue;
        }
        let body = c.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        if body.is_empty() {
            continue;
        }
        let mut end = body.len().min(MAX_INLINE_SCRIPT_BYTES);
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        if total + end > MAX_INLINE_TOTAL_BYTES {
            break;
        }
        total += end;
        out.push(body[..end].to_string());
    }
    out
}

/// Characters of visible text: the HTML minus scripts, styles and tags.
pub(crate) fn visible_text_len(html: &str) -> usize {
    let no_scripts = RE_SCRIPT_BLOCK.replace_all(html, " ");
    let no_styles = RE_STYLE_BLOCK.replace_all(&no_scripts, " ");
    let text = RE_ANY_TAG.replace_all(&no_styles, " ");
    collapse_ws(&decode_entities(&text)).chars().count()
}

/// Page facts that are not fingerprints (title, counts, hosts).
pub(crate) struct PageFacts {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) lang: String,
    pub(crate) script_count: usize,
    pub(crate) inline_script_count: usize,
    pub(crate) stylesheet_count: usize,
    pub(crate) image_count: usize,
    pub(crate) extra_urls: Vec<String>,
    pub(crate) text_len: usize,
}

/// Build the matching inputs from a fetched page. Pure.
pub(crate) fn build_ctx(
    url: &Url,
    headers: &[(String, String)],
    html: &str,
    scripts: &[String],
    styles: &[String],
) -> (MatchCtx, PageFacts) {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();

    let mut cookie_names: Vec<String> = Vec::new();
    for (h, v) in headers {
        if h.eq_ignore_ascii_case("set-cookie") {
            let name = v.split(';').next().unwrap_or("");
            let name = name.split('=').next().unwrap_or("").trim().to_string();
            if !name.is_empty() {
                push_unique(&mut cookie_names, name);
            }
        }
    }

    let mut metas: Vec<(String, String)> = Vec::new();
    let mut description = String::new();
    for m in RE_META_TAG.find_iter(html) {
        let tag = m.as_str();
        let name = attr_value(tag, &RE_ATTR_NAME)
            .map(|n| n.to_ascii_lowercase())
            .unwrap_or_default();
        let content = attr_value(tag, &RE_ATTR_CONTENT).unwrap_or_default();
        if !name.is_empty() {
            if name == "description" && description.is_empty() {
                description = content.clone();
            }
            metas.push((name, content));
        }
    }
    let lang = RE_HTML_TAG
        .find(html)
        .and_then(|m| attr_value(m.as_str(), &RE_ATTR_LANG))
        .unwrap_or_default();
    let title = RE_TITLE
        .captures(html)
        .map(|c| collapse_ws(&decode_entities(c.get(1).map(|t| t.as_str()).unwrap_or(""))))
        .unwrap_or_default();

    let mut script_count = 0usize;
    let mut inline_script_count = 0usize;
    let mut stylesheet_count = 0usize;
    let mut image_count = 0usize;
    let mut script_srcs: Vec<String> = Vec::new();
    let mut link_hrefs: Vec<String> = Vec::new();
    let mut extra_urls: Vec<String> = Vec::new();
    for m in RE_SCRIPT_TAG.find_iter(html) {
        script_count += 1;
        match attr_value(m.as_str(), &RE_ATTR_SRC) {
            Some(src) if !src.is_empty() => {
                script_srcs.push(src.clone());
                extra_urls.push(src);
            }
            _ => inline_script_count += 1,
        }
    }
    for m in RE_LINK_TAG.find_iter(html) {
        let tag = m.as_str();
        let rel = attr_value(tag, &RE_ATTR_REL)
            .map(|r| r.to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(href) = attr_value(tag, &RE_ATTR_HREF) {
            if !href.is_empty() {
                link_hrefs.push(href.clone());
                extra_urls.push(href);
            }
        }
        if rel.contains("stylesheet") {
            stylesheet_count += 1;
        }
    }
    for m in RE_IMG_TAG.find_iter(html) {
        image_count += 1;
        if let Some(src) = attr_value(m.as_str(), &RE_ATTR_SRC) {
            if !src.is_empty() {
                extra_urls.push(src);
            }
        }
    }
    for m in RE_IFRAME_TAG.find_iter(html) {
        if let Some(src) = attr_value(m.as_str(), &RE_ATTR_SRC) {
            if !src.is_empty() {
                extra_urls.push(src);
            }
        }
    }

    // Fetched external scripts first, then inline bodies (audit 03 §3.1).
    let mut all_scripts: Vec<String> = scripts.to_vec();
    all_scripts.extend(inline_script_bodies(html));

    let ctx = MatchCtx {
        html_lower: html.to_ascii_lowercase(),
        headers: headers.to_vec(),
        cookie_names,
        metas,
        script_srcs,
        link_hrefs,
        scripts: all_scripts,
        styles: styles.to_vec(),
        host,
        js_globals: Vec::new(),
        dom_hits: Vec::new(),
    };
    let facts = PageFacts {
        title,
        description,
        lang,
        script_count,
        inline_script_count,
        stylesheet_count,
        image_count,
        extra_urls,
        text_len: visible_text_len(html),
    };
    (ctx, facts)
}

fn sort_detections(d: &mut [Detection]) {
    d.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Only a parent at least this sure passes confidence to what it implies.
const IMPLY_MIN_PARENT: f32 = 0.50;
const IMPLY_FACTOR: f32 = 0.8;

/// Score every technology against the inputs, then apply `implies`. Pure.
pub(crate) fn score_ctx(ctx: &MatchCtx) -> Vec<Detection> {
    let mut detections: Vec<Detection> = Vec::new();
    for tech in TABLE.iter() {
        let mut hits: Vec<(f32, &CompiledSignal)> = Vec::new();
        let mut support_evidence: Vec<String> = Vec::new();
        let mut version: Option<String> = None;
        for sig in &tech.signals {
            if let Some(v) = signal_match(sig, ctx) {
                if version.is_none() {
                    version = v;
                }
                if sig.support_only {
                    push_unique(&mut support_evidence, sig.evidence.clone());
                } else {
                    hits.push((sig.weight, sig));
                }
            }
        }
        let weights: Vec<f32> = hits.iter().map(|(w, _)| *w).collect();
        let confidence = combine_confidence(&weights);
        if confidence < 0.30 {
            continue;
        }
        let mut evidence: Vec<String> = Vec::new();
        for (_, s) in &hits {
            push_unique(&mut evidence, s.evidence.clone());
        }
        for ev in support_evidence {
            push_unique(&mut evidence, ev);
        }
        detections.push(Detection {
            id: tech.id.clone(),
            name: tech.name.clone(),
            category: tech.category.clone(),
            website: tech.website.clone(),
            confidence,
            status: Status::from_conf(confidence),
            evidence,
            version,
        });
    }
    apply_implies(&mut detections);
    detections
}

/// `implies`, two levels deep: only from parents at >= 0.50; an implied
/// technology that was already found is raised, never lowered. Sorts the result.
pub(crate) fn apply_implies(detections: &mut Vec<Detection>) {
    for _ in 0..2 {
        sort_detections(detections);
        let parents: Vec<Detection> = detections
            .iter()
            .filter(|d| d.confidence >= IMPLY_MIN_PARENT)
            .cloned()
            .collect();
        for parent in parents {
            let Some(tech) = TABLE.iter().find(|t| t.id == parent.id) else { continue };
            let implied_conf = parent.confidence * IMPLY_FACTOR;
            let ev = format!("implied by {}", tech.name);
            for imp_id in &tech.implies {
                if let Some(existing) = detections.iter_mut().find(|d| &d.id == imp_id) {
                    if implied_conf > existing.confidence {
                        existing.confidence = implied_conf;
                        existing.status = Status::from_conf(implied_conf);
                        push_unique(&mut existing.evidence, ev.clone());
                    }
                    continue;
                }
                if implied_conf < 0.30 {
                    continue;
                }
                let Some(imp_tech) = TABLE.iter().find(|t| &t.id == imp_id) else { continue };
                detections.push(Detection {
                    id: imp_id.clone(),
                    name: imp_tech.name.clone(),
                    category: imp_tech.category.clone(),
                    website: imp_tech.website.clone(),
                    confidence: implied_conf,
                    status: Status::from_conf(implied_conf),
                    evidence: vec![ev.clone()],
                    version: None,
                });
            }
        }
    }
    sort_detections(detections);
}

/// Hosts of sign-in services: a redirect through one means a login wall.
const LOGIN_HOSTS: &[&str] = &[
    "accounts.google.com",
    "login.microsoftonline.com",
    "login.live.com",
    "appleid.apple.com",
];
const LOGIN_HOST_SUFFIXES: &[&str] = &[".okta.com", ".auth0.com", ".onelogin.com"];

/// Why the fetched page may not be the real page. Pure.
/// `final_path` is the path of the last URL; `redirect_hosts` every host the
/// fetch passed through (including the last).
pub(crate) fn page_health(
    status: u16,
    headers: &[(String, String)],
    html_lower: &str,
    title: &str,
    text_len: usize,
    final_path: &str,
    redirect_hosts: &[String],
) -> Vec<ThinReason> {
    let header = |name: &str| -> Option<String> {
        headers
            .iter()
            .find(|(h, _)| h.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.to_ascii_lowercase())
    };
    let title_l = title.to_ascii_lowercase();
    let blocked_status = matches!(status, 401 | 403 | 429 | 503);
    let mut out = Vec::new();

    let challenge: Option<&str> = if header("cf-mitigated").is_some_and(|v| v.contains("challenge"))
        || title_l.starts_with("just a moment")
        || title_l == "attention required! | cloudflare"
        || (blocked_status && html_lower.contains("/cdn-cgi/challenge-platform/"))
    {
        Some("Cloudflare")
    } else if blocked_status && (html_lower.contains("_incapsula_resource") || header("x-iinfo").is_some()) {
        Some("Imperva")
    } else if blocked_status && (header("x-datadome").is_some() || html_lower.contains("captcha-delivery.com")) {
        Some("DataDome")
    } else if blocked_status && (html_lower.contains("px-captcha") || html_lower.contains("perimeterx")) {
        Some("PerimeterX")
    } else if blocked_status
        && title_l.contains("access denied")
        && header("server").is_some_and(|s| s.contains("akamai"))
    {
        Some("Akamai")
    } else {
        None
    };
    if let Some(who) = challenge {
        out.push(ThinReason::Challenge(who.to_string()));
    }

    let path_l = final_path.to_ascii_lowercase();
    let login_path = ["/login", "/signin", "/sign-in", "/sign_in", "/log-in", "/auth/", "/account/login", "/users/sign_in"]
        .iter()
        .any(|p| path_l.contains(p));
    let via_login_host = redirect_hosts.iter().any(|h| {
        let h = h.to_ascii_lowercase();
        LOGIN_HOSTS.iter().any(|l| h == *l) || LOGIN_HOST_SUFFIXES.iter().any(|s| h.ends_with(s))
    });
    if login_path || via_login_host {
        out.push(ThinReason::LoginWall);
    }

    let shell_root = ["id=\"root\"", "id=\"app\"", "id=\"__next\"", "id=\"__nuxt\"", "<app-root", "id='root'", "id='app'", "id=\"svelte\""]
        .iter()
        .any(|m| html_lower.contains(m));
    if challenge.is_none() && text_len < 200 && shell_root {
        out.push(ThinReason::SpaShell);
    }

    if challenge.is_none() && status >= 400 {
        out.push(ThinReason::HttpError(status));
    }
    out
}

/// The global paths and DOM probes the hidden render must read, taken from
/// the `js` / `dom` signals of the table ("@markers" are built into the
/// collector and not listed). Deduplicated, table order.
pub(crate) fn render_probes() -> (Vec<String>, Vec<(String, Option<String>)>) {
    let mut globals: Vec<String> = Vec::new();
    let mut dom: Vec<(String, Option<String>)> = Vec::new();
    for tech in TABLE.iter() {
        for sig in &tech.signals {
            match (sig.kind, sig.name.as_ref()) {
                ("js", Some(n)) if !n.starts_with('@') => push_unique(&mut globals, n.clone()),
                ("dom", Some(n)) => {
                    let probe = (n.clone(), sig.attr.clone());
                    if !dom.contains(&probe) {
                        dom.push(probe);
                    }
                }
                _ => {}
            }
        }
    }
    (globals, dom)
}

/// The table category of a technology id ("" when unknown).
pub(crate) fn category_of(id: &str) -> String {
    TABLE.iter().find(|t| t.id == id).map(|t| t.category.clone()).unwrap_or_default()
}

/// A bot-check interstitial title ("Just a moment...").
pub(crate) fn looks_like_challenge_title(title: &str) -> bool {
    let t = title.trim().to_ascii_lowercase();
    t.starts_with("just a moment") || t == "attention required! | cloudflare" || t.starts_with("access denied")
}

pub(crate) fn analyse(
    url: &Url,
    status_code: u16,
    headers: &[(String, String)],
    html: &str,
    scripts: &[String],
    styles: &[String],
) -> SiteReport {
    let (ctx, facts) = build_ctx(url, headers, html, scripts, styles);
    let detections = score_ctx(&ctx);
    let host = ctx.host.clone();

    let present = |name: &str| headers.iter().any(|(h, _)| h.eq_ignore_ascii_case(name));
    let security = SecurityHeaders {
        hsts: present("strict-transport-security"),
        csp: present("content-security-policy"),
        xfo: present("x-frame-options"),
        xcto: present("x-content-type-options"),
        referrer_policy: present("referrer-policy"),
        permissions_policy: present("permissions-policy"),
    };

    let stats = PageStats {
        html_bytes: html.len(),
        js_bytes: scripts.iter().map(|s| s.len()).sum(),
        css_bytes: styles.iter().map(|s| s.len()).sum(),
        script_count: facts.script_count,
        inline_script_count: facts.inline_script_count,
        stylesheet_count: facts.stylesheet_count,
        image_count: facts.image_count,
        third_party_hosts: third_party_hosts(url, &host, &facts.extra_urls),
    };

    let thin = page_health(
        status_code,
        headers,
        &ctx.html_lower,
        &facts.title,
        facts.text_len,
        url.path(),
        &[host.clone()],
    );

    SiteReport {
        url: sanitized_url(url),
        final_url: sanitized_url(url),
        status_code,
        title: facts.title,
        description: facts.description,
        lang: facts.lang,
        detections,
        security,
        stats,
        notes: Vec::new(),
        thin,
        ctx: Some(std::sync::Arc::new(ctx)),
    }
}

pub(crate) fn third_party_hosts(base: &Url, page_host: &str, raw_urls: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for u in raw_urls {
        let Ok(abs) = base.join(u) else { continue };
        let Some(h) = abs.host_str() else { continue };
        let hl = h.to_ascii_lowercase();
        if hl.is_empty() || hl == page_host || same_site(&hl, page_host) {
            continue;
        }
        if !out.contains(&hl) {
            out.push(hl);
        }
    }
    out.sort();
    out.truncate(30);
    out
}

fn sanitized_url(url: &Url) -> String {
    let mut out = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        out.push_str(&format!(":{port}"));
    }
    out.push_str(url.path());
    out
}

// ---------------------------------------------------------------------------
// group_by_category — detections grouped in the fixed panel order
// ---------------------------------------------------------------------------

const CATEGORY_ORDER: &[&str] = &[
    "meta-framework",
    "frontend-framework",
    "ui-library",
    "css-framework",
    "animation",
    "state-management",
    "js-library",
    "build-tool",
    "cms",
    "headless-cms",
    "ecommerce",
    "hosting",
    "cdn",
    "web-server",
    "backend-framework",
    "backend-language",
    "database-baas",
    "auth",
    "payments",
    "analytics",
    "tag-manager",
    "advertising",
    "search",
    "fonts",
    "maps",
    "video",
    "chat-support",
    "monitoring",
    "security",
    "ab-testing",
    "email-marketing",
    "cookie-consent",
    "icons",
    "programming-language",
];

fn category_label(category: &str) -> String {
    match category {
        "meta-framework" => "Meta-Framework".into(),
        "frontend-framework" => "Frontend Framework".into(),
        "ui-library" => "UI Library".into(),
        "css-framework" => "CSS Framework".into(),
        "animation" => "Animation".into(),
        "state-management" => "State Management".into(),
        "js-library" => "JavaScript Library".into(),
        "build-tool" => "Build Tool".into(),
        "cms" => "CMS".into(),
        "headless-cms" => "Headless CMS".into(),
        "ecommerce" => "E-commerce".into(),
        "hosting" => "Hosting".into(),
        "cdn" => "CDN".into(),
        "web-server" => "Web Server".into(),
        "backend-framework" => "Backend Framework".into(),
        "backend-language" => "Backend Language".into(),
        "database-baas" => "Database / BaaS".into(),
        "auth" => "Auth".into(),
        "payments" => "Payments".into(),
        "analytics" => "Analytics".into(),
        "tag-manager" => "Tag Manager".into(),
        "advertising" => "Advertising".into(),
        "search" => "Search".into(),
        "fonts" => "Fonts".into(),
        "maps" => "Maps".into(),
        "video" => "Video".into(),
        "chat-support" => "Chat & Support".into(),
        "monitoring" => "Monitoring".into(),
        "security" => "Security".into(),
        "ab-testing" => "A/B Testing".into(),
        "email-marketing" => "Email Marketing".into(),
        "cookie-consent" => "Cookie Consent".into(),
        "icons" => "Icons".into(),
        "programming-language" => "Programming Language".into(),
        other => {
            let mut out = String::new();
            for (i, ch) in other.chars().enumerate() {
                if ch == '_' {
                    out.push(' ');
                } else if i == 0 {
                    out.extend(ch.to_uppercase());
                } else {
                    out.push(ch);
                }
            }
            out
        }
    }
}

pub(crate) fn group_by_category(report: &SiteReport) -> Vec<(String, Vec<&Detection>)> {
    let rank = |cat: &str| -> usize {
        CATEGORY_ORDER
            .iter()
            .position(|c| c == &cat)
            .unwrap_or(usize::MAX)
    };
    let mut groups: Vec<(usize, String, Vec<&Detection>)> = Vec::new();
    for det in &report.detections {
        let r = rank(&det.category);
        match groups.iter_mut().find(|(rr, _, _)| *rr == r) {
            Some((_, _, dets)) => dets.push(det),
            None => groups.push((r, category_label(&det.category), vec![det])),
        }
    }
    groups.sort_by_key(|(r, _, _)| *r);
    groups
        .into_iter()
        .map(|(_, label, dets)| (label, dets))
        .collect()
}

// ---------------------------------------------------------------------------
// report_markdown
// ---------------------------------------------------------------------------

pub(crate) fn report_markdown(title: &str, report: &SiteReport) -> String {
    let head = if report.title.is_empty() {
        title.to_string()
    } else {
        format!("{} — {}", title, report.title)
    };
    let mut out = format!("## {head}\n\n");
    out.push_str(&format!(
        "### URL: {}\nStatus: {}\n\n",
        report.url, report.status_code
    ));
    if !report.thin.is_empty() {
        out.push_str("### Why this scan is limited\n");
        for t in &report.thin {
            out.push_str(&format!("- {}\n", t.spoken()));
        }
        out.push('\n');
    }
    if report.detections.is_empty() {
        out.push_str("Not detectable from outside.\n");
    } else {
        for (category, dets) in group_by_category(report) {
            out.push_str(&format!("### {category}\n"));
            for det in dets {
                let word = match det.status {
                    Status::Detected => "Detected",
                    Status::Likely => "Likely",
                    Status::Inferred => "Inferred",
                };
                let version = det.version.as_deref().map(|v| format!(" {v}")).unwrap_or_default();
                out.push_str(&format!(
                    "- **{}{}** ({}, {:.0}%)\n",
                    det.name, version, word, det.confidence * 100.0
                ));
                for ev in &det.evidence {
                    out.push_str(&format!("  - {ev}\n"));
                }
            }
        }
    }
    let sp = &report.stats;
    out.push_str(&format!(
        "\n### Page facts\n- script tags: {} ({} inline)\n- stylesheets: {}\n- images: {}\n- html: {}\n- js analysed: {}\n- css analysed: {}\n- third-party hosts: {}\n",
        sp.script_count,
        sp.inline_script_count,
        sp.stylesheet_count,
        sp.image_count,
        fmt_bytes(sp.html_bytes),
        fmt_bytes(sp.js_bytes),
        fmt_bytes(sp.css_bytes),
        if sp.third_party_hosts.is_empty() {
            "none".to_string()
        } else {
            sp.third_party_hosts.join(", ")
        }
    ));
    let sec = &report.security;
    let yes_no = |b: bool| if b { "yes" } else { "no" };
    out.push_str(&format!(
        "\n### Security headers\n- HSTS: {}\n- CSP: {}\n- X-Frame-Options: {}\n- X-Content-Type-Options: {}\n- Referrer-Policy: {}\n- Permissions-Policy: {}\n",
        yes_no(sec.hsts),
        yes_no(sec.csp),
        yes_no(sec.xfo),
        yes_no(sec.xcto),
        yes_no(sec.referrer_policy),
        yes_no(sec.permissions_policy),
    ));
    if !report.notes.is_empty() {
        out.push_str("\n### Notes\n");
        for n in &report.notes {
            out.push_str(&format!("- {n}\n"));
        }
    }
    out
}

fn fmt_bytes(n: usize) -> String {
    if n >= 1_048_576 {
        format!("{:.1} MB", n as f64 / 1_048_576.0)
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

// ---------------------------------------------------------------------------
// ip_is_public (v4 + v6; used by guard and DNS-pinning)
// ---------------------------------------------------------------------------

pub(crate) fn ip_is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_is_public(v4),
        IpAddr::V6(v6) => v6_is_public(v6),
    }
}

fn v4_is_public(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    !(v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_multicast()
        || v4.is_unspecified()
        // 0.0.0.0/8 this network
        || o[0] == 0
        // 100.64.0.0/10 CGNAT
        || (o[0] == 100 && (o[1] & 0xC0) == 64)
        // 192.0.0.0/24 IETF protocol assignments
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        // 198.18.0.0/15 benchmarking
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        // 240.0.0.0/4 reserved
        || o[0] >= 240)
}

fn v6_is_public(v6: Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() {
        return false;
    }
    let s = v6.segments();
    // ::ffff:x.y.z.w mapped and ::x.y.z.w compatible embedded IPv4
    if s[..5].iter().all(|x| *x == 0) && s[5] == 0xffff {
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return v4_is_public(v4);
    }
    if s[..6].iter().all(|x| *x == 0) {
        // ::a.b.c.d compatible form (excluding :: and ::1 refused above)
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return v4_is_public(v4);
    }
    // NAT64 64:ff9b::/96 embeds a plain IPv4 host
    if s[0] == 0x0064 && s[1] == 0xff9b && s[2..6].iter().all(|x| *x == 0) {
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return v4_is_public(v4);
    }
    // ff00::/8 multicast
    if (s[0] & 0xff00) == 0xff00 {
        return false;
    }
    // fc00::/7 unique-local
    if (s[0] & 0xfe00) == 0xfc00 {
        return false;
    }
    // fe80::/10 link-local
    if (s[0] & 0xffc0) == 0xfe80 {
        return false;
    }
    // 2001:db8::/32 documentation
    if s[0] == 0x2001 && s[1] == 0x0db8 {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// URL guard (spec table) — strict public helper, relaxed seam for probe/tests
// ---------------------------------------------------------------------------

fn guard_url_relaxed(raw: &str, allow_loopback: bool) -> Result<Url, ProbeRefusal> {
    let mut url = Url::parse(raw).map_err(|_| ProbeRefusal::BadUrl)?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(ProbeRefusal::NotHttp);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ProbeRefusal::BadUrl);
    }
    let host_raw = url.host_str().ok_or(ProbeRefusal::BadUrl)?.to_ascii_lowercase();
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        host_raw[1..host_raw.len() - 1].to_string()
    } else {
        host_raw
    };
    let banned = host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".lan")
        || host.ends_with(".home.arpa");
    if banned {
        return Err(ProbeRefusal::PrivateAddress);
    }
    let ip_check = |ip: IpAddr| -> bool {
        ip_is_public(ip) || (allow_loopback && ip.is_loopback())
    };
    if let Ok(v4) = host.parse::<Ipv4Addr>() {
        if !ip_check(IpAddr::V4(v4)) {
            return Err(ProbeRefusal::PrivateAddress);
        }
    } else if let Ok(v6) = host.parse::<Ipv6Addr>() {
        if !ip_check(IpAddr::V6(v6)) {
            return Err(ProbeRefusal::PrivateAddress);
        }
    }
    if crate::screen_context::is_sensitive("", "", Some(url.as_str())) {
        return Err(ProbeRefusal::Sensitive);
    }
    url.set_fragment(None);
    Ok(url)
}

/// The strict public guard (no loopback). Also used by the hidden render's
/// navigation policy (site_render.rs).
pub(crate) fn guard_url(raw: &str) -> Result<Url, ProbeRefusal> {
    guard_url_relaxed(raw, false)
}

// ---------------------------------------------------------------------------
// probe — DNS-pinned network fetch + analyse
// ---------------------------------------------------------------------------

async fn resolve_and_check(url: &Url, allow_loopback: bool) -> Result<IpAddr, ProbeRefusal> {
    let host_raw = url.host_str().ok_or(ProbeRefusal::BadUrl)?;
    let host = host_raw.trim_start_matches('[').trim_end_matches(']').to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host.clone(), port))
        .await
        .map_err(|e| ProbeRefusal::Network(format!("DNS lookup failed: {e}")))?;
    let mut chosen: Option<IpAddr> = None;
    for a in addrs {
        let ip = a.ip();
        if !(ip_is_public(ip) || (allow_loopback && ip.is_loopback())) {
            return Err(ProbeRefusal::PrivateAddress);
        }
        if chosen.is_none() {
            chosen = Some(ip);
        }
    }
    chosen.ok_or_else(|| ProbeRefusal::Network("DNS lookup returned no addresses".into()))
}

fn client_for(url: &Url, pinned: IpAddr) -> Result<Client, ProbeRefusal> {
    let host_raw = url.host_str().ok_or(ProbeRefusal::BadUrl)?;
    let host = host_raw.trim_start_matches('[').trim_end_matches(']').to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(HOP_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .resolve(
            &host,
            SocketAddr::new(pinned, if port == 0 { 80 } else { port }),
        )
        .build()
        .map_err(|e| ProbeRefusal::Network(format!("client build failed: {e}")))
}

async fn send_get(url: &Url, allow_loopback: bool) -> Result<reqwest::Response, ProbeRefusal> {
    let ip = resolve_and_check(url, allow_loopback).await?;
    let client = client_for(url, ip)?;
    client
        .get(url.as_str())
        .header(ACCEPT, "text/html,application/xhtml+xml,*/*;q=0.8")
        .header(ACCEPT_LANGUAGE, ACCEPT_LANGUAGE_VALUE)
        .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
        .send()
        .await
        .map_err(|e| ProbeRefusal::Network(format!("request failed: {e}")))
}

fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect()
}

fn looks_like_html(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .any(|(h, v)| h.eq_ignore_ascii_case("content-type") && v.to_ascii_lowercase().contains("html"))
}

fn looks_like_subresource_type(headers: &HeaderMap, is_script: bool) -> bool {
    let ct = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ct.is_empty() {
        return true;
    }
    if is_script {
        ct.contains("javascript") || ct.contains("ecmascript")
    } else {
        ct.contains("css")
    }
}

fn collect_cookie_names(headers: &HeaderMap, out: &mut Vec<String>) {
    for v in headers.get_all(SET_COOKIE) {
        let Ok(s) = v.to_str() else { continue };
        let name = s.split(';').next().unwrap_or("");
        let name = name.split('=').next().unwrap_or("").trim().to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
}

async fn stream_capped(
    mut resp: reqwest::Response,
    cap: usize,
) -> Result<(Vec<u8>, bool), ProbeRefusal> {
    let mut body: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ProbeRefusal::Network(format!("read body failed: {e}")))?
    {
        if body.len() + chunk.len() > cap {
            let room = cap.saturating_sub(body.len());
            body.extend_from_slice(&chunk[..room]);
            truncated = true;
            break;
        }
        body.extend_from_slice(&chunk);
        if body.len() >= cap {
            break;
        }
    }
    Ok((body, truncated))
}

async fn fetch_subresource(url: &Url, is_script: bool, allow_loopback: bool) -> Option<(String, String)> {
    let res = tokio::time::timeout(Duration::from_secs(SUB_TIMEOUT_SECS), async move {
        let ip = resolve_and_check(url, allow_loopback).await.ok()?;
        let client = client_for(url, ip).ok()?;
        let resp = client
            .get(url.as_str())
            .header(ACCEPT, "*/*")
            .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
            .send()
            .await
            .ok()?;
        if !looks_like_subresource_type(resp.headers(), is_script) {
            return None;
        }
        let ct = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let (body, _trunc) = stream_capped(resp, MAX_SUBRESOURCE_BYTES).await.ok()?;
        Some((ct, String::from_utf8_lossy(&body).into_owned()))
    })
    .await
    .ok()?;
    res
}

fn pick_subresources(
    html: &str,
    base: &Url,
    allow_loopback: bool,
    skipped: &mut usize,
) -> Vec<(Url, bool)> {
    let page_host = base.host_str().unwrap_or_default().to_ascii_lowercase();
    let mut picks: Vec<(Url, bool)> = Vec::new();
    for m in RE_SCRIPT_TAG.find_iter(html) {
        if picks.len() >= MAX_SCRIPTS {
            break;
        }
        let Some(src) = attr_value(m.as_str(), &RE_ATTR_SRC) else { continue };
        if src.is_empty() {
            continue;
        }
        let Ok(abs) = base.join(&src) else { continue };
        let host = abs.host_str().unwrap_or_default().to_ascii_lowercase();
        if host.is_empty() || !(host == page_host || same_site(&host, &page_host)) {
            continue;
        }
        if guard_url_relaxed(abs.as_str(), allow_loopback).is_err() {
            *skipped += 1;
            continue;
        }
        picks.push((abs, true));
    }
    for m in RE_LINK_TAG.find_iter(html) {
        if picks.len() >= MAX_SCRIPTS + MAX_STYLESHEETS {
            break;
        }
        let tag = m.as_str();
        let rel = attr_value(tag, &RE_ATTR_REL)
            .map(|r| r.to_ascii_lowercase())
            .unwrap_or_default();
        if !rel.contains("stylesheet") {
            continue;
        }
        let Some(href) = attr_value(tag, &RE_ATTR_HREF) else { continue };
        if href.is_empty() {
            continue;
        }
        let Ok(abs) = base.join(&href) else { continue };
        if guard_url_relaxed(abs.as_str(), allow_loopback).is_err() {
            *skipped += 1;
            continue;
        }
        picks.push((abs, false));
    }
    picks
}

/// Everything one guarded fetch saw, before analysis. Also what the accuracy
/// harness saves to disk (site_eval.rs), so replayed scans run the exact same
/// analysis as live ones.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CapturedPage {
    /// The address the user asked for (guarded).
    pub(crate) initial_url: String,
    /// The address the last redirect landed on (guarded).
    pub(crate) final_url: String,
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) html: String,
    pub(crate) scripts: Vec<String>,
    pub(crate) styles: Vec<String>,
    /// Every host the fetch passed through, in order, the last one included.
    pub(crate) redirect_hosts: Vec<String>,
    pub(crate) truncated: bool,
    pub(crate) skipped: usize,
}

/// Fetch the page, follow guarded redirects, then fetch the chosen
/// sub-resources. Network only; no analysis.
pub(crate) async fn fetch_raw(raw: &str, opts: ProbeOpts) -> Result<CapturedPage, ProbeRefusal> {
    let initial = guard_url_relaxed(raw, opts.allow_loopback)?;
    let mut current = initial.clone();
    let mut redirects = 0usize;
    let mut hop_cookies: Vec<String> = Vec::new();
    let mut redirect_hosts: Vec<String> = Vec::new();

    let (status_code, resp_headers, body, truncated) = loop {
        if let Some(h) = current.host_str() {
            let h = h.to_ascii_lowercase();
            if !redirect_hosts.contains(&h) {
                redirect_hosts.push(h);
            }
        }
        let resp = send_get(&current, opts.allow_loopback).await?;
        collect_cookie_names(resp.headers(), &mut hop_cookies);
        let hvec = headers_to_vec(resp.headers());
        if resp.status().is_redirection() {
            if redirects >= MAX_REDIRECTS {
                return Err(ProbeRefusal::TooManyRedirects);
            }
            let loc = hvec
                .iter()
                .find(|(h, _)| h.eq_ignore_ascii_case("location"))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| ProbeRefusal::Network("redirect without location".into()))?;
            redirects += 1;
            let next = current
                .join(&loc)
                .map_err(|_| ProbeRefusal::Network("bad redirect location".into()))?;
            drop(resp);
            current = guard_url_relaxed(next.as_str(), opts.allow_loopback)?;
            continue;
        }
        if !looks_like_html(&hvec) {
            return Err(ProbeRefusal::NotHtml);
        }
        let status = resp.status().as_u16();
        let (body, truncated) = stream_capped(resp, MAX_HTML_BYTES).await?;
        break (status, hvec, body, truncated);
    };

    let html = String::from_utf8_lossy(&body).into_owned();
    let mut skipped = 0usize;
    let picks = pick_subresources(&html, &current, opts.allow_loopback, &mut skipped);

    let fetched: Vec<Option<(String, String)>> = futures_util::future::join_all(
        picks
            .iter()
            .map(|(url, is_script)| fetch_subresource(url, *is_script, opts.allow_loopback)),
    )
    .await;

    let mut scripts = Vec::new();
    let mut styles = Vec::new();
    for (res, (_url, is_script)) in fetched.into_iter().zip(picks.iter()) {
        match res {
            Some((ct, text)) => {
                let ok = if *is_script {
                    ct.is_empty() || ct.contains("javascript") || ct.contains("ecmascript")
                } else {
                    ct.is_empty() || ct.contains("css")
                };
                if ok {
                    if *is_script {
                        scripts.push(text);
                    } else {
                        styles.push(text);
                    }
                } else {
                    skipped += 1;
                }
            }
            None => skipped += 1,
        }
    }

    let mut merged = resp_headers;
    for name in &hop_cookies {
        push_unique_pair(&mut merged, "set-cookie".into(), format!("{name}="));
    }

    Ok(CapturedPage {
        initial_url: initial.to_string(),
        final_url: current.to_string(),
        status: status_code,
        headers: merged,
        html,
        scripts,
        styles,
        redirect_hosts,
        truncated,
        skipped,
    })
}

/// Analyse a captured page (live or replayed from disk). Pure.
pub(crate) fn analyse_captured(c: &CapturedPage) -> Result<SiteReport, ProbeRefusal> {
    let current = Url::parse(&c.final_url).map_err(|_| ProbeRefusal::BadUrl)?;
    let initial = Url::parse(&c.initial_url).unwrap_or_else(|_| current.clone());
    let mut report = analyse(&current, c.status, &c.headers, &c.html, &c.scripts, &c.styles);
    // Redirects seen during the fetch also reveal sign-in walls.
    let text_len = visible_text_len(&c.html);
    report.thin = page_health(
        c.status,
        &c.headers,
        &c.html.to_ascii_lowercase(),
        &report.title,
        text_len,
        current.path(),
        &c.redirect_hosts,
    );
    if c.truncated {
        report.notes.push(NOTE_TRUNCATED.into());
    }
    if c.skipped > 0 {
        report.notes.push(format!("{} sub-resource(s) skipped", c.skipped));
    }
    report.url = sanitized_url(&initial);
    Ok(report)
}

async fn fetch_and_analyse(raw: &str, opts: ProbeOpts) -> Result<SiteReport, ProbeRefusal> {
    let captured = fetch_raw(raw, opts).await?;
    analyse_captured(&captured)
}

pub(crate) async fn probe_with(raw_url: &str, opts: ProbeOpts) -> Result<SiteReport, ProbeRefusal> {
    let total = Duration::from_millis(opts.total_timeout_ms.unwrap_or(TOTAL_TIMEOUT_MS));
    match tokio::time::timeout(total, fetch_and_analyse(raw_url, opts)).await {
        Ok(inner) => inner,
        Err(_) => Err(ProbeRefusal::Timeout),
    }
}

pub(crate) async fn probe(raw_url: &str) -> Result<SiteReport, ProbeRefusal> {
    probe_with(raw_url, ProbeOpts::default()).await
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(r: &'a SiteReport, id: &str) -> Option<&'a Detection> {
        r.detections.iter().find(|d| d.id == id)
    }

    fn det(id: &str, name: &str, category: &str, conf: f32) -> Detection {
        Detection {
            id: id.into(),
            name: name.into(),
            category: category.into(),
            website: None,
            confidence: conf,
            status: Status::from_conf(conf),
            evidence: vec!["test evidence".into()],
            version: None,
        }
    }

    // ----------------------------------------------------------------
    // combine_confidence
    // ----------------------------------------------------------------

    #[test]
    fn combine_confidence_edge_cases() {
        assert_eq!(combine_confidence(&[]), 0.0);
        let s = combine_confidence(&[0.5]);
        assert!((s - 0.5).abs() < 0.005);
        let t = combine_confidence(&[0.5, 0.5]);
        assert!((t - 0.75).abs() < 0.005);
        let c = combine_confidence(&[1.1]);
        assert!((c - 1.0).abs() < 1e-2, "saturated product must clamp: {c}");
    }

    // ----------------------------------------------------------------
    // ip_is_public
    // ----------------------------------------------------------------

    #[test]
    fn ip_is_public_v4_v6_tables() {
        let refused_v4 = [
            "127.0.0.1", "10.0.0.5", "172.16.0.9", "172.31.255.1", "192.168.1.1",
            "169.254.1.2", "255.255.255.255", "192.0.2.1", "198.51.100.7", "203.0.113.9",
            "224.0.0.5", "100.64.0.1", "100.127.255.1", "192.0.0.8",
            "198.18.0.1", "198.19.255.4", "240.0.0.1", "250.1.2.3", "0.0.0.0", "0.1.2.3",
        ];
        for a in refused_v4 {
            let ip: IpAddr = a.parse().unwrap();
            assert!(!ip_is_public(ip), "must refuse v4 {a}");
        }
        for a in ["8.8.8.8", "1.1.1.1", "151.101.1.140", "9.9.9.9", "100.128.0.1"] {
            let ip: IpAddr = a.parse().unwrap();
            assert!(ip_is_public(ip), "must allow v4 {a}");
        }
        let refused_v6 = [
            "::1", "::", "fe80::a1", "febf::1", "fc00::1", "fd12:3456::a",
            "ff02::1", "2001:db8::5", "::ffff:127.0.0.1",
        ];
        for a in refused_v6 {
            let ip: IpAddr = a.parse().unwrap();
            assert!(!ip_is_public(ip), "must refuse v6 {a}");
        }
        for a in ["2606:4700::1111", "2001:4860:4860::8888"] {
            let ip: IpAddr = a.parse().unwrap();
            assert!(ip_is_public(ip), "must allow v6 {a}");
        }
    }

    #[test]
    fn ip_mapped_zero_and_documentation_refused() {
        let ip: IpAddr = "::ffff:0.0.0.0".parse().unwrap();
        assert!(!ip_is_public(ip));
        let ip: IpAddr = "::ffff:192.0.2.9".parse().unwrap();
        assert!(!ip_is_public(ip), "documentation v4 host via a mapped v6 socket");
    }

    // ----------------------------------------------------------------
    // guard_url (spec refusals table)
    // ----------------------------------------------------------------

    #[test]
    fn guard_url_spec_table() {
        assert!(matches!(guard_url("file:///c:/x"), Err(ProbeRefusal::NotHttp)));
        assert!(matches!(guard_url("ftp://example.com"), Err(ProbeRefusal::NotHttp)));
        assert!(matches!(guard_url("not a url"), Err(ProbeRefusal::BadUrl)));
        assert!(matches!(guard_url("http://user:pw@example.com/x"), Err(ProbeRefusal::BadUrl)));
        assert!(matches!(guard_url("http://localhost:3000/"), Err(ProbeRefusal::PrivateAddress)));
        assert!(matches!(guard_url("http://127.0.0.1:8080/admin"), Err(ProbeRefusal::PrivateAddress)));
        assert!(matches!(guard_url("http://192.168.1.14/topology"), Err(ProbeRefusal::PrivateAddress)));
        assert!(matches!(guard_url("http://router.lan/"), Err(ProbeRefusal::PrivateAddress)));
        assert!(matches!(guard_url("https://www.paypal.com/signin"), Err(ProbeRefusal::Sensitive)));
        let guarded = guard_url("https://example.com/a?token=SECRET#frag");
        assert!(guarded.is_ok());
        let guarded = guarded.unwrap();
        assert_eq!(guarded.fragment(), None, "fragment must be stripped");
        assert_eq!(guarded.query(), Some("token=SECRET"), "query must be kept");
    }

    #[test]
    fn guard_url_whatwg_ip_normalization() {
        assert!(matches!(guard_url("http://2130706433/"), Err(ProbeRefusal::PrivateAddress)));
        assert!(matches!(guard_url("http://0x7f.1/"), Err(ProbeRefusal::PrivateAddress)));
    }

    #[test]
    fn sanitized_url_shape() {
        let u = Url::parse("http://2130706433/README.md?tab=readme#license").unwrap();
        assert_eq!(sanitized_url(&u), "http://127.0.0.1/README.md");
        let u2 = Url::parse("https://example.com/").unwrap();
        assert_eq!(sanitized_url(&u2), "https://example.com/");
        let u3 = Url::parse("http://127.0.0.1:8080/admin?x=1").unwrap();
        assert_eq!(sanitized_url(&u3), "http://127.0.0.1:8080/admin");
    }

    #[test]
    fn disguised_ipv4_inside_ipv6_is_judged_by_the_embedded_address() {
        use std::net::IpAddr;
        // The SSRF hole that the first draft had: ::ffff:a.b.c.d must be treated as a.b.c.d.
        for bad in [
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.5",
            "::ffff:192.168.1.1",
            "::ffff:169.254.169.254",
            "::ffff:100.64.0.1",
            "::127.0.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b::a9fe:a9fe",
        ] {
            let ip: IpAddr = bad.parse().unwrap();
            assert!(!ip_is_public(ip), "{bad} must NOT be public");
        }
        for good in ["::ffff:8.8.8.8", "::ffff:1.1.1.1", "64:ff9b::808:808", "2606:4700:4700::1111"] {
            let ip: IpAddr = good.parse().unwrap();
            assert!(ip_is_public(ip), "{good} should be public");
        }
        assert!(guard_url("http://[::ffff:127.0.0.1]/").is_err());
        assert!(guard_url("http://[::ffff:169.254.169.254]/latest/meta-data").is_err());
    }

    #[test]
    fn spoken_lines_are_short_and_sir_shaped() {
        assert!(ProbeRefusal::PrivateAddress.spoken().contains("public internet"));
        assert!(ProbeRefusal::Sensitive.spoken().contains("banking"));
    }

    // ----------------------------------------------------------------
    // header / cookie matching
    // ----------------------------------------------------------------

    #[test]
    fn header_signal_via_x_powered_by() {
        let url = Url::parse("https://example.com/case-a").unwrap();
        let headers = vec![
            ("x-powered-by".to_string(), "Next.js 14".to_string()),
        ];
        let r = analyse(&url, 200, &headers, "", &[], &[]);
        let d = find(&r, "nextjs").expect("Next.js must be detected");
        assert!(d.confidence >= 0.80, "conf {}", d.confidence);
        assert!(
            d.evidence.iter().any(|e| e == "X-Powered-By: Next.js"),
            "evidence must be the exact string {:?}",
            d.evidence
        );
    }

    #[test]
    fn cookie_signal_shopify_y_and_ga() {
        let url = Url::parse("https://shop.example.com/case-b").unwrap();
        let headers = vec![(
            "set-cookie".to_string(),
            "_shopify_y=deadbeef; path=/".to_string(),
        )];
        let r = analyse(&url, 200, &headers, "", &[], &[]);
        assert!(find(&r, "shopify").is_some(), "shopify via cookie prefix");

        let url = Url::parse("https://shop.example.com/case-c").unwrap();
        let headers = vec![("set-cookie".to_string(), "_ga=GA1.2.x; path=/".to_string())];
        let r = analyse(&url, 200, &headers, "", &[], &[]);
        let d = find(&r, "google-analytics").expect("GA via _ga cookie");
        assert!(d.confidence >= 0.80);
    }

    // ----------------------------------------------------------------
    // implies
    // ----------------------------------------------------------------

    #[test]
    fn implies_boosts_and_attaches_evidence() {
        let url = Url::parse("https://example.com/implies").unwrap();
        let headers = vec![("x-powered-by".to_string(), "Next.js".to_string())];
        let r = analyse(&url, 200, &headers, "", &[], &[]);
        let react = find(&r, "react").expect("react implied by nextjs");
        assert!(react.confidence >= 0.50, "0.95 * 0.8 ~= 0.7998, got {}", react.confidence);
        assert!(
            react.evidence.iter().any(|e| e.starts_with("implied by ")),
            "implied evidence missing"
        );
        let node = find(&r, "nodejs").expect("nodejs implied");
        assert!(node.confidence >= 0.50);
    }

    #[test]
    fn below_threshold_is_dropped() {
        let url = Url::parse("https://example.com/drop").unwrap();
        let r = analyse(&url, 200, &Vec::new(), "<div class=\"x\">nothing</div>", &[], &[]);
        assert!(find(&r, "shadcn-ui").is_none());
        assert!(r.detections.is_empty(), "no detections expected: {:?}", r.detections);
    }

    // ----------------------------------------------------------------
    // evidence dedupe
    // ----------------------------------------------------------------

    #[test]
    fn evidence_deduped_for_repeated_signals() {
        let url = Url::parse("https://example.com/dedup").unwrap();
        let html = concat!(
            "<link rel=\"stylesheet\" href=\"https://example.com/css/bootstrap.min.css\">",
            "<link rel=\"stylesheet\" href=\"https://example.com/css/bootstrap-theme.css\">"
        );
        let styles = [String::from(".btn-primary { color: red }")];
        let r = analyse(&url, 200, &Vec::new(), html, &[], &styles);
        let d = find(&r, "bootstrap").expect("bootstrap present");
        // two link-href hits dedupe to one entry; the css_text signal keeps its own
        assert_eq!(d.evidence.len(), 2, "different signals keep distinct evidence: {:?}", d.evidence);
        let hrefs = d.evidence.iter().filter(|e| e.contains("stylesheet href")).count();
        assert_eq!(hrefs, 1, "same-kind evidences must dedupe: {:?}", d.evidence);
        assert!(
            d.evidence.iter().any(|e| e.contains("Bootstrap button classes in CSS")),
            "css evidence expected: {:?}", d.evidence
        );
    }

    // ----------------------------------------------------------------
    // group_by_category ordering
    // ----------------------------------------------------------------

    #[test]
    fn group_by_category_is_fixed_order_with_unknown_last() {
        let report = SiteReport {
            url: String::new(),
            final_url: String::new(),
            status_code: 200,
            title: String::new(),
            description: String::new(),
            lang: String::new(),
            detections: vec![
                det("mystery", "Mystery", "zzz", 0.3),
                det("paypal", "PayPal", "payments", 0.3),
                det("cloudflare", "Cloudflare", "security", 0.3),
                det("vercel", "Vercel", "hosting", 0.3),
            ],
            security: Default::default(),
            stats: Default::default(),
            notes: Vec::new(),
            thin: Vec::new(),
            ctx: None,
        };
        let order: Vec<String> = group_by_category(&report)
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        let pos = |k: &str| {
            order
                .iter()
                .position(|x| *x == k)
                .unwrap_or_else(|| panic!("{k} not in {order:?}"))
        };
        assert!(pos("Hosting") < pos("Payments"), "hosting first");
        assert!(pos("Payments") < pos("Security"), "payments before security");
        assert_eq!(*order.last().unwrap(), "Zzz", "unknown category last");
    }

    // ----------------------------------------------------------------
    // report_markdown
    // ----------------------------------------------------------------

    #[test]
    fn report_markdown_header_and_fallback_line() {
        let report = SiteReport {
            url: String::new(),
            final_url: String::new(),
            status_code: 200,
            title: "Docs Home".into(),
            description: String::new(),
            lang: String::new(),
            detections: vec![det("nextjs", "Next.js", "meta-framework", 0.9)],
            security: Default::default(),
            stats: Default::default(),
            notes: Vec::new(),
            thin: Vec::new(),
            ctx: None,
        };
        let md = report_markdown("example.com", &report);
        assert!(md.starts_with("## example.com"), "title line: {md}");

        let empty = SiteReport {
            url: String::new(),
            final_url: String::new(),
            status_code: 200,
            title: String::new(),
            description: String::new(),
            lang: String::new(),
            detections: vec![],
            security: Default::default(),
            stats: Default::default(),
            notes: Vec::new(),
            thin: Vec::new(),
            ctx: None,
        };
        let md2 = report_markdown("example.com", &empty);
        assert!(md2.contains("Not detectable from outside"), "fallback missing: {md2}");
    }

    // ----------------------------------------------------------------
    // analyse tests from real fixtures
    // ----------------------------------------------------------------

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/site/{}", env!("CARGO_MANIFEST_DIR"), name);
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture missing: {path}"))
    }

    fn headers_of(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(h, v)| (h.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn analyse_fixture_nextjs_vercel() {
        let url = Url::parse("https://example.com/f1").unwrap();
        let headers = headers_of(&[("x-powered-by", "Next.js"), ("x-vercel-id", "iad1::abc")]);
        let r = analyse(&url, 200, &headers, &fixture("nextjs_vercel.html"), &[], &[]);
        let n = find(&r, "nextjs").expect("nextjs");
        assert!(n.confidence >= 0.99, "{}", n.confidence);
        assert!(matches!(n.status, Status::Detected));
        let v = find(&r, "vercel").expect("vercel via X-Vercel-Id header");
        assert!(v.confidence >= 0.80, "{}", v.confidence);
        let react = find(&r, "react").expect("react implied");
        assert!(react.confidence >= 0.50);
        let node = find(&r, "nodejs").expect("nodejs implied");
        assert!(node.confidence >= 0.50);
    }

    #[test]
    fn analyse_fixture_wordpress_woocommerce_hidden() {
        let url = Url::parse("https://example.com/f2").unwrap();
        let headers = vec![(
            "set-cookie".to_string(),
            "woocommerce_cart_hash=abc; path=/".to_string(),
        )];
        // the Woocommerce script text is only in the fetched subresource
        let scripts = [String::from(
            "var wc_add_to_cart_params = {\"ajax_url\":\"\\\\/wp-admin\\\\/admin-ajax.php\"};",
        )];
        let r = analyse(&url, 200, &headers, &fixture("wordpress_woocommerce.html"), &scripts, &[]);
        let w = find(&r, "wordpress").expect("wordpress");
        assert!(w.confidence >= 0.99, "{}", w.confidence);
        let wc = find(&r, "woocommerce").expect("woocommerce must be present");
        assert!(wc.confidence >= 0.80, "{}", wc.confidence);
        assert!(find(&r, "shopify").is_none(), "shopify must be absent");
    }

    #[test]
    fn analyse_fixture_shopify() {
        let url = Url::parse("https://example.com/f3").unwrap();
        let headers = vec![(
            "set-cookie".to_string(),
            "_shopify_y=abc; path=/".to_string(),
        )];
        let r = analyse(&url, 200, &headers, &fixture("shopify.html"), &[], &[]);
        let s = find(&r, "shopify").expect("shopify");
        assert!(s.confidence >= 0.99, "{}", s.confidence);
        assert!(find(&r, "woocommerce").is_none(), "woocommerce must be absent");
    }

    #[test]
    fn analyse_fixture_nuxt_cloudflare_pages() {
        let url = Url::parse("https://nh.pages.dev/f4").unwrap();
        let headers = vec![("cf-ray".to_string(), "abc123".to_string())];
        let styles = [String::from(".nuxt-link-active { font-weight: bold }")];
        let r = analyse(&url, 200, &headers, &fixture("nuxt_cloudflare.html"), &[], &styles);
        let nu = find(&r, "nuxt").expect("nuxt");
        assert!(nu.confidence >= 0.99, "{}", nu.confidence);
        let vue = find(&r, "vue").expect("vue implied by nuxt");
        assert!(vue.confidence >= 0.50);
        let cf = find(&r, "cloudflare-cdn").expect("cloudflare-cdn");
        assert!(cf.confidence >= 0.80, "{}", cf.confidence);
        assert!(find(&r, "vercel").is_none(), "vercel must be absent");
    }

    #[test]
    fn analyse_fixture_static_tailwind_ga() {
        let url = Url::parse("https://example.com/f5").unwrap();
        let styles = [String::from(".md\\:shadow-lg { --tw-translate-x: 0 }")];
        let scripts = [String::from(
            "window.dataLayer = window.dataLayer || []; gtag('js', new Date());",
        )];
        let r = analyse(&url, 200, &Vec::new(), &fixture("static_tailwind_ga.html"), &scripts, &styles);
        let tw = find(&r, "tailwind").expect("tailwind");
        assert!(tw.confidence >= 0.80, "{}", tw.confidence);
        let ga = find(&r, "google-analytics").expect("GA");
        assert!(ga.confidence >= 0.80, "{}", ga.confidence);
        assert!(find(&r, "nextjs").is_none(), "nextjs must be absent");
    }

    #[test]
    fn analyse_fixture_angular() {
        let url = Url::parse("https://example.com/f6").unwrap();
        let r = analyse(&url, 200, &Vec::new(), &fixture("angular.html"), &[], &[]);
        let a = find(&r, "angular").expect("angular");
        assert!(a.confidence >= 0.80, "{}", a.confidence);
        assert!(find(&r, "angularjs").is_none(), "AngularJS must be absent");
        assert!(find(&r, "nextjs").is_none());
    }

    // ----------------------------------------------------------------
    // probe without a server
    // ----------------------------------------------------------------

    #[tokio::test]
    async fn probe_refuses_loopback_without_permission() {
        match probe("http://127.0.0.1:1/").await {
            Err(ProbeRefusal::PrivateAddress) => {}
            other => panic!("expected PrivateAddress, got {other:?}"),
        }
    }

    // ----------------------------------------------------------------
    // mock server + network tests
    // ----------------------------------------------------------------

    type Handled = (u16, Vec<(String, String)>, Vec<u8>, Option<String>);

    fn response_bytes(status: u16, headers: &[(String, String)], body: &[u8]) -> Vec<u8> {
        let reason = match status {
            200 => "OK",
            404 => "Not Found",
            _ => "Found",
        };
        let mut out = format!("HTTP/1.1 {status} {reason}\r\n").into_bytes();
        for (h, v) in headers {
            out.extend_from_slice(format!("{h}: {v}\r\n").as_bytes());
        }
        out.extend_from_slice(format!("content-length: {}\r\n", body.len()).as_bytes());
        out.extend_from_slice(b"connection: close\r\n\r\n");
        out.extend_from_slice(body);
        out
    }

    async fn spawn_mock(handles: Vec<Handled>) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for (status, headers, body, location) in handles {
                let (stream, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 65536];
                let _ = tokio::time::timeout(std::time::Duration::from_secs(10), stream.readable()).await;
                let n = stream.try_read(&mut buf).unwrap_or(0);
                let _req = String::from_utf8_lossy(&buf[..n]).to_string();
                let mut resp_headers = Vec::new();
                if let Some(loc) = location {
                    resp_headers.push(("location".to_string(), loc));
                }
                resp_headers.extend(headers);
                let out = response_bytes(status, &resp_headers, &body);
                let _ = stream.try_write(&out);
                drop(stream);
            }
        });
        addr
    }

    #[tokio::test]
    async fn probe_200_html_parses_and_collects_third_party() {
        let body = concat!(
            "<html>",
            "<meta name=\"generator\" content=\"WordPress 6.5\">",
            "<script src=\"/app.js\"></script>",
            "<img src=\"https://stats.example.net/pixel?id=1\">",
            "<iframe src=\"https://payments.example.org/checkout\"></iframe>",
            "</html>",
        );
        let body2 = "regeneratorRuntime = function(){};";
        let addr = spawn_mock(vec![
            (
                200,
                vec![("content-type".into(), "text/html".into())],
                body.as_bytes().to_vec(),
                None,
            ),
            (
                200,
                vec![("content-type".into(), "application/javascript".into())],
                body2.as_bytes().to_vec(),
                None,
            ),
        ])
        .await;
        let opts = ProbeOpts { allow_loopback: true, total_timeout_ms: Some(3_000) };
        let report = probe_with(&format!("http://{addr}/"), opts).await.expect("loopback probe");
        let wp = find(&report, "wordpress").expect("wordpress via generator meta");
        assert!(wp.confidence >= 0.80);
        let regen = find(&report, "regenerator-runtime").expect("regenerator via fetched subresource");
        assert!(regen.confidence >= 0.50, "{}", regen.confidence);
        let hosts = &report.stats.third_party_hosts;
        assert!(hosts.iter().any(|h| h == "stats.example.net"), "hosts: {hosts:?}");
        assert!(hosts.iter().any(|h| h == "payments.example.org"), "hosts: {hosts:?}");
    }

    #[tokio::test]
    async fn probe_rejects_non_html_content_type() {
        let addr = spawn_mock(vec![(
            200,
            vec![("content-type".into(), "application/json".into())],
            b"{\"ok\":true}".to_vec(),
            None,
        )])
        .await;
        let opts = ProbeOpts { allow_loopback: true, total_timeout_ms: Some(3_000) };
        match probe_with(&format!("http://{addr}/"), opts).await {
            Err(ProbeRefusal::NotHtml) => {}
            other => panic!("expected NotHtml, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_truncates_oversized_page() {
        let addr = spawn_mock(vec![(
            200,
            vec![("content-type".into(), "text/html".into())],
            vec![b'x'; 5 * 1024 * 1024],
            None,
        )])
        .await;
        let opts = ProbeOpts { allow_loopback: true, total_timeout_ms: Some(3_000) };
        let report = probe_with(&format!("http://{addr}/"), opts).await.expect("probe");
        assert!(
            report.notes.iter().any(|n| n == NOTE_TRUNCATED),
            "truncation note missing: {:?}",
            report.notes
        );
    }

    #[tokio::test]
    async fn probe_rejects_private_redirect_hop() {
        let addr = spawn_mock(vec![(
            302,
            vec![],
            Vec::new(),
            Some("http://169.254.169.254/latest/meta-data/".into()),
        )])
        .await;
        let opts = ProbeOpts { allow_loopback: true, total_timeout_ms: Some(3_000) };
        match probe_with(&format!("http://{addr}/"), opts).await {
            Err(ProbeRefusal::PrivateAddress) => {}
            other => panic!("expected PrivateAddress, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_rejects_too_many_redirects() {
        let mut handles = Vec::new();
        for _ in 0..6 {
            handles.push((
                302u16,
                vec![("content-type".into(), "text/html".into())],
                Vec::new(),
                Some("/".to_string()),
            ));
        }
        let addr = spawn_mock(handles).await;
        let opts = ProbeOpts { allow_loopback: true, total_timeout_ms: Some(4_000) };
        match probe_with(&format!("http://{addr}/"), opts).await {
            Err(ProbeRefusal::TooManyRedirects) => {}
            other => panic!("expected TooManyRedirects, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_reports_timeout_when_server_hangs() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(5), stream.readable()).await;
                let _ = stream.try_read(&mut buf);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
        let opts = ProbeOpts { allow_loopback: true, total_timeout_ms: Some(1_500) };
        match probe_with(&format!("http://{addr}/"), opts).await {
            Err(ProbeRefusal::Timeout) => {}
            other => panic!("expected Timeout, got {other:?}"),
        }
    }

    // ----------------------------------------------------------------
    // 2026-11-14 detection fixes (audit 03 §3, §6)
    // ----------------------------------------------------------------

    #[test]
    fn inline_scripts_are_searched() {
        // An analytics snippet that only exists inline used to be invisible.
        let url = Url::parse("https://example.com/inline").unwrap();
        let html = "<html><head><script>window.dataLayer=window.dataLayer||[];function gtag(){dataLayer.push(arguments)}gtag('js',new Date());</script></head><body>hi</body></html>";
        let r = analyse(&url, 200, &[], html, &[], &[]);
        assert!(find(&r, "google-analytics").is_some(), "{:?}", r.detections);
        // external script tags are not double-counted as inline bodies
        assert!(inline_script_bodies("<script src=\"/a.js\"></script>").is_empty());
    }

    #[test]
    fn inline_script_bodies_are_capped() {
        let big = "x".repeat(MAX_INLINE_SCRIPT_BYTES * 3);
        let html = format!("<script>{big}</script>").repeat(20);
        let bodies = inline_script_bodies(&html);
        assert!(bodies.iter().all(|b| b.len() <= MAX_INLINE_SCRIPT_BYTES));
        assert!(bodies.iter().map(|b| b.len()).sum::<usize>() <= MAX_INLINE_TOTAL_BYTES);
    }

    #[test]
    fn registrable_domain_table() {
        for (host, want) in [
            ("www.example.com", "example.com"),
            ("a.b.example.com", "example.com"),
            ("www.bbc.co.uk", "bbc.co.uk"),
            ("shop.example.com.au", "example.com.au"),
            ("my-app.vercel.app", "my-app.vercel.app"),
            ("docs.my-app.vercel.app", "my-app.vercel.app"),
            ("user.github.io", "user.github.io"),
            ("example.com", "example.com"),
            ("8.8.8.8", "8.8.8.8"),
        ] {
            assert_eq!(registrable_domain(host), want, "{host}");
        }
        assert!(same_site("cdn.example.com", "www.example.com"));
        assert!(!same_site("a.co.uk", "b.co.uk"), "different .co.uk sites are not one site");
        assert!(!same_site("alice.vercel.app", "bob.vercel.app"), "platform customers are separate");
    }

    #[test]
    fn implies_needs_a_sure_parent_and_raises_existing_children() {
        let url = Url::parse("https://example.com/imp").unwrap();
        // Next.js at 0.95 raises React's own weak hint and adds evidence.
        let headers = vec![("x-powered-by".to_string(), "Next.js".to_string())];
        let r = analyse(&url, 200, &headers, "<div data-reactroot></div>", &[], &[]);
        let react = find(&r, "react").unwrap();
        assert!(react.confidence >= 0.84, "own signal kept: {}", react.confidence);
        // A parent below 0.50 implies nothing; at or above it, the child arrives at 0.8x.
        let mut weak = vec![det("nuxt", "Nuxt", "meta-framework", 0.45)];
        apply_implies(&mut weak);
        assert!(find_in(&weak, "vue").is_none(), "weak parent must not imply children: {weak:?}");
        let mut sure = vec![det("nuxt", "Nuxt", "meta-framework", 0.6)];
        apply_implies(&mut sure);
        let vue = find_in(&sure, "vue").expect("sure parent implies vue");
        assert!((vue.confidence - 0.48).abs() < 1e-4, "{}", vue.confidence);
        assert!(vue.evidence.iter().any(|e| e == "implied by Nuxt"));
    }

    fn find_in<'a>(d: &'a [Detection], id: &str) -> Option<&'a Detection> {
        d.iter().find(|x| x.id == id)
    }

    /// False positives seen on real sites (nextjs.org, angular.dev, allbirds.com,
    /// github.com, nuxt.com …, 2026-11-14): ordinary markup and minified code must
    /// not name a technology.
    #[test]
    fn ordinary_markup_and_minified_code_name_no_technology() {
        let url = Url::parse("https://example.com/").unwrap();
        let html = concat!(
            "<html><head><script src=\"/_next/static/chunks/turbopack-0a1b2c.js\"></script>",
            "<link rel=\"stylesheet\" href=\"/build/app.css\"></head>",
            "<body class=\"ui dark is-active\"><div class=\"important-note ghost-button\">",
            "<a href=\"/docs/readme.md\">Readme</a><p>Docs mention ng-app and L.map here.</p>",
            "<i data-icon=\"x\"></i></div></body></html>"
        );
        let scripts = [String::from("var a=b.map(c);function cn(x){return x}var s=create(t);hj(1);L.map(q);")];
        let headers = vec![
            ("x-request-id".to_string(), "abc".to_string()),
            ("x-frame-options".to_string(), "SAMEORIGIN".to_string()),
        ];
        let styles = [String::from(".btn-primary{color:red}")];
        let r = analyse(&url, 200, &headers, html, &scripts, &styles);
        for id in [
            "semantic-ui", "bulma", "ant-design", "angularjs", "ghost", "leaflet", "remix", "turbo",
            "hotjar", "zustand", "shadcn-ui", "django", "heroku", "ruby-on-rails", "rails", "bootstrap", "iconify",
        ] {
            assert!(find(&r, id).is_none(), "{id} must not be detected: {:?}", find(&r, id));
        }
        // The real markers still work.
        let real = analyse(
            &url,
            200,
            &[],
            "<html ng-app=\"shop\"><body data-turbo=\"true\"></body></html>",
            &[],
            &[],
        );
        assert!(find(&real, "angularjs").is_some(), "ng-app attribute");
        assert!(find(&real, "turbo").is_some(), "data-turbo attribute");
    }

    #[test]
    fn current_sveltekit_markers_are_detected() {
        // As served by svelte.dev on 2026-11-14 (trimmed).
        let url = Url::parse("https://svelte.example/").unwrap();
        let html = concat!(
            "<html><head><style>svg.svelte-hfc14b{vertical-align:middle}</style>",
            "<link href=\"./_app/immutable/assets/font.DSJrtJSV.woff2\" rel=\"preload\"></head>",
            "<body data-sveltekit-preload-data=\"hover\"><script>{ __sveltekit_1qlx2lx = { base: \"\" }; }</script></body></html>"
        );
        let r = analyse(&url, 200, &[], html, &[], &[]);
        let kit = find(&r, "sveltekit").expect("sveltekit");
        assert!(matches!(kit.status, Status::Detected), "{}", kit.confidence);
        assert!(find(&r, "svelte").is_some_and(|s| s.confidence >= 0.8));
    }

    #[test]
    fn page_health_names_why_a_scan_is_thin() {
        let cf = page_health(
            403,
            &[("cf-mitigated".into(), "challenge".into())],
            "<html><title>just a moment...</title></html>",
            "Just a moment...",
            40,
            "/",
            &["example.com".into()],
        );
        assert_eq!(cf, vec![ThinReason::Challenge("Cloudflare".into())]);
        // A normal Cloudflare page carries the challenge script too: not a challenge.
        let normal = page_health(
            200,
            &[("cf-ray".into(), "abc".into())],
            "<script src=\"/cdn-cgi/challenge-platform/scripts/jsd/main.js\"></script><p>welcome</p>",
            "Home",
            5000,
            "/",
            &["example.com".into()],
        );
        assert!(normal.is_empty(), "{normal:?}");
        let login = page_health(200, &[], "<form>", "Sign in", 300, "/", &["app.example.com".into(), "accounts.google.com".into()]);
        assert!(login.contains(&ThinReason::LoginWall));
        let login_path = page_health(200, &[], "<form>", "Log in", 300, "/users/sign_in", &["x.com".into()]);
        assert!(login_path.contains(&ThinReason::LoginWall));
        let shell = page_health(200, &[], "<div id=\"root\"></div>", "App", 0, "/", &["x.com".into()]);
        assert_eq!(shell, vec![ThinReason::SpaShell]);
        let err = page_health(500, &[], "<p>oops</p>", "Error", 400, "/", &["x.com".into()]);
        assert_eq!(err, vec![ThinReason::HttpError(500)]);
        // every reason speaks a full sentence
        for t in [cf[0].clone(), ThinReason::LoginWall, ThinReason::SpaShell, ThinReason::HttpError(404)] {
            assert!(t.spoken().ends_with('.'));
        }
    }

    #[test]
    fn analyse_flags_an_empty_spa_shell() {
        let url = Url::parse("https://app.example.com/").unwrap();
        let html = "<html><head><title>App</title></head><body><div id=\"root\"></div><script type=\"module\" src=\"/assets/index-a1b2.js\"></script></body></html>";
        let r = analyse(&url, 200, &[], html, &[], &[]);
        assert!(r.thin.contains(&ThinReason::SpaShell), "{:?}", r.thin);
        assert!(report_markdown("app", &r).contains("Why this scan is limited"));
    }

    #[test]
    fn analyse_captured_sees_a_redirect_through_a_sign_in_service() {
        let page = CapturedPage {
            initial_url: "https://app.example.com/".into(),
            final_url: "https://app.example.com/welcome".into(),
            status: 200,
            headers: vec![],
            html: "<html><title>Welcome</title><body><p>Please continue</p></body></html>".into(),
            redirect_hosts: vec!["app.example.com".into(), "login.microsoftonline.com".into()],
            ..Default::default()
        };
        let r = analyse_captured(&page).unwrap();
        assert!(r.thin.contains(&ThinReason::LoginWall));
    }

    #[test]
    fn user_agent_is_a_plain_browser() {
        assert!(!USER_AGENT.contains("NEXUS"), "a bot token invites WAF challenges");
        assert!(USER_AGENT.starts_with("Mozilla/5.0"));
        assert!(!USER_AGENT.to_ascii_lowercase().contains("bot"));
    }

    #[test]
    fn versions_are_captured_only_when_they_look_like_versions() {
        assert_eq!(clean_version("14.2.3"), Some("14.2.3".into()));
        assert_eq!(clean_version("v3.4.21-beta"), Some("3.4.21".into()));
        assert_eq!(clean_version("18"), Some("18".into()));
        assert_eq!(clean_version("latest"), None);
        assert_eq!(clean_version("<script>"), None);
    }

    #[test]
    fn js_and_dom_signals_match_only_rendered_inputs() {
        let ctx = MatchCtx {
            js_globals: vec![("jQuery.fn.jquery".into(), Some("3.7.1".into()))],
            dom_hits: vec![("[ng-version]".into(), Some("ng-version".into()), Some("17.3.0".into()))],
            ..Default::default()
        };
        let d = score_ctx(&ctx);
        let jq = d.iter().find(|x| x.id == "jquery").expect("jquery via global");
        assert_eq!(jq.version.as_deref(), Some("3.7.1"));
        let ng = d.iter().find(|x| x.id == "angular").expect("angular via ng-version");
        assert_eq!(ng.version.as_deref(), Some("17.3.0"));
        // The static path never fills these inputs.
        let url = Url::parse("https://example.com/").unwrap();
        let (static_ctx, _) = build_ctx(&url, &[], "<p>jQuery.fn.jquery</p>", &[], &[]);
        assert!(static_ctx.js_globals.is_empty() && static_ctx.dom_hits.is_empty());
    }

    #[test]
    fn js_path_and_selector_validation() {
        for ok in ["next.version", "jQuery.fn.jquery", "__NUXT__", "$nuxt", "@react-root", "_.VERSION"] {
            assert!(valid_js_path(ok), "{ok}");
        }
        for bad in ["", "a b", "x();", "a..b", "a.b.c.d.e.f.g", "@", "@X", "window['x']", "x\"});"] {
            assert!(!valid_js_path(bad), "{bad}");
        }
        assert!(valid_dom_selector("[class*=\"svelte-\"]"));
        for bad in ["", "a{b}", "<img>", "x\\y"] {
            assert!(!valid_dom_selector(bad), "{bad}");
        }
    }

    // ----------------------------------------------------------------
    // fingerprint table lints
    // ----------------------------------------------------------------

    fn raw_table() -> serde_json::Value {
        serde_json::from_str(FINGERPRINT_JSON).expect("table parses")
    }

    #[test]
    fn table_has_no_duplicate_ids_or_names() {
        let t = raw_table();
        let techs = t["technologies"].as_array().unwrap();
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for x in techs {
            assert!(ids.insert(x["id"].as_str().unwrap().to_string()), "duplicate id {}", x["id"]);
            assert!(names.insert(x["name"].as_str().unwrap().to_string()), "duplicate name {}", x["name"]);
        }
        assert_eq!(TABLE.len(), techs.len(), "every entry compiles");
    }

    #[test]
    fn every_table_signal_compiles_with_a_known_kind() {
        let t = raw_table();
        let mut raw = 0usize;
        for x in t["technologies"].as_array().unwrap() {
            for s in x["signals"].as_array().unwrap() {
                raw += 1;
                let kind = s["kind"].as_str().unwrap();
                assert!(normalize_kind(kind).is_some(), "{}: unknown kind {kind}", x["id"]);
                if let Some(re) = s["regex"].as_str() {
                    assert!(Regex::new(re).is_ok(), "{}: bad regex {re}", x["id"]);
                    assert!(!re.contains('\u{8}'), "{}: backspace character in regex", x["id"]);
                }
            }
        }
        let compiled: usize = TABLE.iter().map(|t| t.signals.len()).sum();
        assert_eq!(compiled, raw, "a signal was dropped while compiling (bad js path or selector?)");
    }

    #[test]
    fn every_technology_can_be_detected() {
        let t = raw_table();
        let techs = t["technologies"].as_array().unwrap();
        let implied: std::collections::HashSet<String> = techs
            .iter()
            .flat_map(|x| x["implies"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        for x in techs {
            let id = x["id"].as_str().unwrap();
            let strong = x["signals"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["weight"].as_f64().unwrap_or(0.0) >= 0.30);
            assert!(strong || implied.contains(id), "{id} has only support-only signals and is never implied");
        }
        for imp in &implied {
            assert!(techs.iter().any(|x| x["id"] == imp.as_str()), "implies unknown id {imp}");
        }
    }

    #[test]
    fn text_signals_are_not_loose_short_substrings() {
        // A 3-letter `contains` at weight >= 0.5 fires on unrelated words
        // ("lit" in "split", "aos" in "chaos"); use a word-bounded regex instead.
        let t = raw_table();
        for x in t["technologies"].as_array().unwrap() {
            for s in x["signals"].as_array().unwrap() {
                let kind = s["kind"].as_str().unwrap();
                if !matches!(kind, "html" | "script_text" | "css_text" | "script_src" | "link_href") {
                    continue;
                }
                if let Some(c) = s["contains"].as_str() {
                    let w = s["weight"].as_f64().unwrap_or(0.0);
                    assert!(c.len() >= 5 || w < 0.5, "{}: loose contains {c:?} at {w}", x["id"]);
                }
            }
        }
    }
}
