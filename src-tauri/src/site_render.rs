//! site_render — the hidden render pass behind "Nexus, reverse engineer this website".
//!
//! The static probe (`site_probe.rs`) reads one anonymous copy of the page's HTML.
//! Client-rendered apps (React/Vue/Vite) ship an empty `<div id="root">`, so the
//! static copy says little. This pass loads the SAME guarded URL in NEXUS's own
//! invisible WebView2 window, lets it hydrate, runs one read-only collector
//! script, then destroys the window. The collector reads framework globals,
//! DOM markers, the list of files the page loaded and a few page facts; the
//! result is merged into the static report and scored again.
//!
//! Safety (each rule has a unit test):
//! - Only an `https` URL that passed `site_probe::guard_url` (public host, not a
//!   sensitive/banking page) is rendered; every navigation is re-checked and a
//!   redirect to a sign-in service is refused (`render_nav_allowed`).
//! - Requests to private, loopback or `.local`-style hosts are answered with a
//!   403 before they leave (`render_request_allowed`).
//! - New windows are denied, downloads cancelled, every permission denied,
//!   audio muted, and the session is InPrivate (no cookies, no history).
//! - The window label is in no capability file, so the page has no Tauri IPC.
//! - The collector only reads: data properties (no getters run), `querySelector`,
//!   resource timing, script URLs without query strings. A static test bans
//!   network, navigation, eval and DOM-writing APIs from it. Output is capped.
//!
//! DNS rebinding: WebView2 resolves names itself, so the static probe's pin does not reach it. The window is
//! therefore started with `--proxy-server` pointing at `render_proxy` (loopback, CONNECT 443 only, every resolved
//! address must be public), and the request filter above stays as a second layer. Untested against a real
//! WebView2 until the isolated live run (see docs/changes/209-*).

use crate::site_probe::{self, SiteReport, ThinReason};
use serde::Deserialize;
use std::net::IpAddr;

/// The collector, with `__NEXUS_PROBES__` replaced by `{"g":[…],"d":[[sel,attr]…]}`.
/// Must stay read-only: see `collector_uses_no_banned_api`.
pub(crate) const COLLECTOR_TEMPLATE: &str = r##"(function () {
  "use strict";
  var P = __NEXUS_PROBES__;
  var out = { v: 1, globals: [], dom: [], markers: [], resources: [], scripts: [], inline: [],
              generator: null, text_len: 0, elements: 0, title: "", final_url: "" };
  function own(o, k) {
    try {
      if (o === null || o === undefined) return null;
      var t = typeof o;
      if (t !== "object" && t !== "function") return null;
      var d = Object.getOwnPropertyDescriptor(o, k);
      if (!d || !("value" in d)) return null;
      return { v: d.value };
    } catch (e) { return null; }
  }
  function prim(v) {
    var t = typeof v;
    if (t === "string") return v.slice(0, 64);
    if (t === "number" || t === "boolean") return String(v);
    return null;
  }
  function strip(u) {
    try {
      var x = new URL(u, document.baseURI);
      return (x.protocol === "http:" || x.protocol === "https:") ? x.origin + x.pathname : null;
    } catch (e) { return null; }
  }
  try { out.title = String(document.title || "").slice(0, 200); } catch (e) {}
  try { out.final_url = strip(document.URL) || ""; } catch (e) {}
  for (var i = 0; i < P.g.length && i < 300; i++) {
    try {
      var parts = String(P.g[i]).split("."), cur = window, ok = true;
      for (var j = 0; j < parts.length; j++) {
        var r = own(cur, parts[j]);
        if (!r) { ok = false; break; }
        cur = r.v;
      }
      if (ok && cur !== undefined && cur !== null) out.globals.push([P.g[i], prim(cur)]);
    } catch (e) {}
  }
  for (var k = 0; k < P.d.length && k < 300; k++) {
    try {
      var sel = P.d[k][0], attr = P.d[k][1];
      var el = document.querySelector(sel);
      if (el) out.dom.push([sel, attr, attr ? String(el.getAttribute(attr) || "").slice(0, 64) : null]);
    } catch (e) {}
  }
  try {
    var roots = document.querySelectorAll("#root,#app,#__next,#__nuxt,#___gatsby,[data-reactroot],[data-v-app],body > div");
    var seen = {};
    for (var m = 0; m < roots.length && m < 25; m++) {
      var keys = Object.keys(roots[m]);
      for (var n = 0; n < keys.length && n < 50; n++) {
        var key = keys[n];
        if (!seen.react && (key.indexOf("__reactContainer") === 0 || key.indexOf("__reactFiber") === 0 || key === "_reactRootContainer")) {
          seen.react = 1; out.markers.push(["@react-root", null]);
        }
        if (!seen.vue && key === "__vue_app__") {
          seen.vue = 1;
          var app = own(roots[m], "__vue_app__");
          var ver = app ? own(app.v, "version") : null;
          out.markers.push(["@vue-app", ver ? prim(ver.v) : null]);
        }
        if (!seen.vue2 && key === "__vue__") { seen.vue2 = 1; out.markers.push(["@vue2", null]); }
      }
    }
  } catch (e) {}
  try {
    var ents = performance.getEntriesByType("resource"), rs = {};
    for (var a = 0; a < ents.length && out.resources.length < 300; a++) {
      var s1 = strip(ents[a].name);
      if (s1 && !rs[s1]) { rs[s1] = 1; out.resources.push(s1); }
    }
  } catch (e) {}
  try {
    var sc = document.scripts;
    for (var b = 0; b < sc.length && b < 200; b++) {
      var src = sc[b].getAttribute("src");
      if (src) {
        var s2 = strip(src);
        if (s2 && out.scripts.length < 150) out.scripts.push(s2);
      } else if (out.inline.length < 20) {
        var tx = String(sc[b].textContent || "").slice(0, 2048);
        if (tx) out.inline.push(tx);
      }
    }
  } catch (e) {}
  try {
    var g = document.querySelector('meta[name="generator"]');
    if (g) out.generator = String(g.getAttribute("content") || "").slice(0, 120);
  } catch (e) {}
  try { out.elements = document.getElementsByTagName("*").length; } catch (e) {}
  try { out.text_len = String((document.body && document.body.innerText) || "").length; } catch (e) {}
  var json = JSON.stringify(out);
  if (json.length > 262144) { out.inline = []; json = JSON.stringify(out); }
  if (json.length > 262144) { out.resources = out.resources.slice(0, 50); json = JSON.stringify(out); }
  if (json.length > 262144) { out.scripts = out.scripts.slice(0, 50); out.dom = []; json = JSON.stringify(out); }
  return json;
})()"##;

/// Upper bound on the raw ExecuteScript answer (a JSON string holding JSON).
const MAX_RAW_BYTES: usize = 600_000;
const MAX_GLOBALS: usize = 300;
const MAX_DOM: usize = 300;
const MAX_MARKERS: usize = 10;
const MAX_RESOURCES: usize = 300;
const MAX_SCRIPTS: usize = 150;
const MAX_INLINE: usize = 20;
const MAX_INLINE_CHARS: usize = 2048;
const MAX_VALUE_CHARS: usize = 64;
/// Visible text at or above this means the page really rendered content.
const HYDRATED_TEXT_LEN: usize = 200;

/// What the collector read from the rendered page.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub(crate) struct RenderedPage {
    pub(crate) v: u32,
    #[serde(default)]
    pub(crate) globals: Vec<(String, Option<String>)>,
    #[serde(default)]
    pub(crate) dom: Vec<(String, Option<String>, Option<String>)>,
    #[serde(default)]
    pub(crate) markers: Vec<(String, Option<String>)>,
    #[serde(default)]
    pub(crate) resources: Vec<String>,
    #[serde(default)]
    pub(crate) scripts: Vec<String>,
    #[serde(default)]
    pub(crate) inline: Vec<String>,
    #[serde(default)]
    pub(crate) generator: Option<String>,
    #[serde(default)]
    pub(crate) text_len: usize,
    #[serde(default)]
    pub(crate) elements: usize,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) final_url: String,
}

/// Insert the probe lists into the collector. The lists come from our own
/// table and are JSON-encoded, so nothing in them can break out of the literal.
pub(crate) fn build_collector(globals: &[String], dom: &[(String, Option<String>)]) -> String {
    let g: Vec<&String> = globals.iter().filter(|p| site_probe::valid_js_path(p)).collect();
    let d: Vec<(&String, &Option<String>)> = dom
        .iter()
        .filter(|(s, _)| site_probe::valid_dom_selector(s))
        .map(|(s, a)| (s, a))
        .collect();
    let probes = serde_json::json!({ "g": g, "d": d });
    COLLECTOR_TEMPLATE.replace("__NEXUS_PROBES__", &probes.to_string())
}

fn cut(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Keep `scheme://host/path` of an http(s) URL, nothing else.
fn strip_query(u: &str) -> Option<String> {
    let lower = u.trim().to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return None;
    }
    let end = u.find(['?', '#']).unwrap_or(u.len());
    Some(cut(&u[..end], 300))
}

/// Decode WebView2's ExecuteScript answer: the script returns a JSON string, and
/// WebView2 JSON-encodes that again. Re-applies every cap so a hostile page
/// cannot hand back more than the collector would.
pub(crate) fn parse_rendered(raw: &str) -> Result<RenderedPage, String> {
    if raw.len() > MAX_RAW_BYTES {
        return Err(format!("render answer too large ({} bytes)", raw.len()));
    }
    let inner: String = serde_json::from_str(raw).map_err(|e| format!("render answer is not a string: {e}"))?;
    let mut page: RenderedPage =
        serde_json::from_str(&inner).map_err(|e| format!("render answer has a bad shape: {e}"))?;
    if page.v != 1 {
        return Err(format!("unknown collector version {}", page.v));
    }
    page.globals.truncate(MAX_GLOBALS);
    for (p, v) in page.globals.iter_mut() {
        *p = cut(p, 80);
        *v = v.as_deref().map(|x| cut(x, MAX_VALUE_CHARS));
    }
    page.globals.retain(|(p, _)| site_probe::valid_js_path(p));
    page.dom.truncate(MAX_DOM);
    for (s, a, v) in page.dom.iter_mut() {
        *s = cut(s, 120);
        *a = a.as_deref().map(|x| cut(x, 40));
        *v = v.as_deref().map(|x| cut(x, MAX_VALUE_CHARS));
    }
    page.markers.truncate(MAX_MARKERS);
    page.markers.retain(|(m, _)| m.starts_with('@') && site_probe::valid_js_path(m));
    for (_, v) in page.markers.iter_mut() {
        *v = v.as_deref().map(|x| cut(x, MAX_VALUE_CHARS));
    }
    page.resources = page.resources.iter().filter_map(|u| strip_query(u)).take(MAX_RESOURCES).collect();
    page.scripts = page.scripts.iter().filter_map(|u| strip_query(u)).take(MAX_SCRIPTS).collect();
    page.inline.truncate(MAX_INLINE);
    for s in page.inline.iter_mut() {
        *s = cut(s, MAX_INLINE_CHARS);
    }
    page.generator = page.generator.as_deref().map(|g| cut(g, 120));
    page.title = cut(&page.title, 200);
    page.final_url = strip_query(&page.final_url).unwrap_or_default();
    Ok(page)
}

fn is_js_like(u: &str) -> bool {
    let path = u.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    path.ends_with(".js") || path.ends_with(".mjs") || !path.contains('.')
}

/// Add what the render saw to the static inputs and score again. Rendered
/// inputs only ever add evidence, so a confidence can rise but never fall. Pure.
pub(crate) fn merge_rendered_signals(report: &SiteReport, page: &RenderedPage) -> SiteReport {
    let mut out = report.clone();
    let mut ctx = report.ctx.as_deref().cloned().unwrap_or_default();
    for (p, v) in page.globals.iter().chain(page.markers.iter()) {
        if !ctx.js_globals.iter().any(|(q, _)| q == p) {
            ctx.js_globals.push((p.clone(), v.clone()));
        }
    }
    for hit in &page.dom {
        if !ctx.dom_hits.contains(hit) {
            ctx.dom_hits.push(hit.clone());
        }
    }
    for u in page.scripts.iter().chain(page.resources.iter().filter(|u| is_js_like(u))) {
        if !ctx.script_srcs.contains(u) {
            ctx.script_srcs.push(u.clone());
        }
    }
    for u in page.resources.iter().filter(|u| u.to_ascii_lowercase().ends_with(".css")) {
        if !ctx.link_hrefs.contains(u) {
            ctx.link_hrefs.push(u.clone());
        }
    }
    ctx.scripts.extend(page.inline.iter().cloned());
    if let Some(g) = page.generator.as_ref().filter(|g| !g.is_empty()) {
        if !ctx.metas.iter().any(|(n, _)| n == "generator") {
            ctx.metas.push(("generator".into(), g.clone()));
        }
    }

    let before: Vec<String> = report.detections.iter().map(|d| d.id.clone()).collect();
    out.detections = site_probe::score_ctx(&ctx);
    let added = out.detections.iter().filter(|d| !before.contains(&d.id)).count();

    // Thin reasons the render disproves: an SPA shell that hydrated, or a bot
    // check the browser got past.
    let hydrated = page.text_len >= HYDRATED_TEXT_LEN;
    let render_challenged = site_probe::looks_like_challenge_title(&page.title);
    out.thin.retain(|t| match t {
        ThinReason::SpaShell => !hydrated,
        ThinReason::Challenge(_) => !(hydrated && !render_challenged),
        _ => true,
    });
    if render_challenged && !out.thin.iter().any(|t| matches!(t, ThinReason::Challenge(_))) {
        out.thin.push(ThinReason::Challenge("bot".into()));
    }

    if let Ok(base) = reqwest::Url::parse(&report.final_url) {
        let host = base.host_str().unwrap_or_default().to_ascii_lowercase();
        let mut urls: Vec<String> = report.stats.third_party_hosts.iter().map(|h| format!("https://{h}/")).collect();
        urls.extend(page.resources.iter().cloned());
        out.stats.third_party_hosts = site_probe::third_party_hosts(&base, &host, &urls);
    }
    out.notes.push(if added > 0 {
        format!("Rendered the page in a hidden browser window: {added} more technolog{} found.", if added == 1 { "y" } else { "ies" })
    } else {
        "Rendered the page in a hidden browser window: nothing new found.".to_string()
    });
    out.ctx = Some(std::sync::Arc::new(ctx));
    out
}

/// What to do with one navigation of the hidden window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavDecision {
    Allow,
    Block,
    /// A redirect to a sign-in service: refused, and reported as a login wall.
    LoginRedirect,
}

/// Navigations allowed per render (the static probe allows 5 redirects too).
pub(crate) const MAX_RENDER_NAVS: usize = 6;

const LOGIN_HOSTS: &[&str] = &[
    "accounts.google.com",
    "login.microsoftonline.com",
    "login.live.com",
    "appleid.apple.com",
];
const LOGIN_HOST_SUFFIXES: &[&str] = &[".okta.com", ".auth0.com", ".onelogin.com"];

/// Top-level navigation policy: https only, public and not sensitive (the
/// probe's own guard), a bounded number of hops, no sign-in redirects. Pure.
pub(crate) fn render_nav_allowed(url: &str, navs_so_far: usize) -> NavDecision {
    if url == "about:blank" {
        return NavDecision::Allow;
    }
    if navs_so_far >= MAX_RENDER_NAVS {
        return NavDecision::Block;
    }
    let Ok(parsed) = reqwest::Url::parse(url) else { return NavDecision::Block };
    if parsed.scheme() != "https" {
        return NavDecision::Block;
    }
    if site_probe::guard_url(url).is_err() {
        return NavDecision::Block;
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if LOGIN_HOSTS.contains(&host.as_str()) || LOGIN_HOST_SUFFIXES.iter().any(|s| host.ends_with(s)) {
        return NavDecision::LoginRedirect;
    }
    NavDecision::Allow
}

/// Sub-resource policy: block private/loopback IP literals and local names;
/// everything public is allowed (a shop page may load a payment script). Pure.
pub(crate) fn render_request_allowed(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("data:") || lower.starts_with("blob:") || lower == "about:blank" {
        return true;
    }
    let Ok(parsed) = reqwest::Url::parse(url) else { return false };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host_raw) = parsed.host_str() else { return false };
    let host = host_raw.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    if host == "localhost"
        || [".localhost", ".local", ".internal", ".lan", ".home.arpa", ".intranet", ".corp"]
            .iter()
            .any(|s| host.ends_with(s))
    {
        return false;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return site_probe::ip_is_public(ip);
    }
    // A bare single-label name ("router", "nas") resolves on the local network.
    host.contains('.')
}

/// Popups from the rendered page are never opened.
pub(crate) fn new_window_policy() -> bool {
    false
}

/// The hidden window's label. It must stay out of every capability file.
pub(crate) const RENDER_LABEL: &str = "stack-render";

// ---------------------------------------------------------------------------
// Windows glue: build the window, harden the WebView2, wait, collect, destroy.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Deny permissions, mute, block private sub-resources, lay out as a desktop
/// browser. Runs on the UI thread inside `with_webview`.
#[cfg(target_os = "windows")]
unsafe fn harden(webview: &tauri::webview::PlatformWebview) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_2, ICoreWebView2_8, COREWEBVIEW2_PERMISSION_STATE_DENY,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    };
    use webview2_com::{
        take_pwstr, CallDevToolsProtocolMethodCompletedHandler, PermissionRequestedEventHandler,
        WebResourceRequestedEventHandler,
    };
    use windows_core::{Interface, PCWSTR, PWSTR};

    let core = webview.controller().CoreWebView2().map_err(|e| e.to_string())?;
    let mut token: i64 = 0;

    core.add_PermissionRequested(
        &PermissionRequestedEventHandler::create(Box::new(|_s, args| {
            if let Some(args) = args {
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
            }
            Ok(())
        })),
        &mut token,
    )
    .map_err(|e| format!("permission handler: {e}"))?;

    if let Ok(c8) = core.cast::<ICoreWebView2_8>() {
        let _ = c8.SetIsMuted(true);
    }

    let filter = wide("*");
    core.AddWebResourceRequestedFilter(PCWSTR(filter.as_ptr()), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
        .map_err(|e| format!("resource filter: {e}"))?;
    let env = core
        .cast::<ICoreWebView2_2>()
        .and_then(|c| c.Environment())
        .map_err(|e| format!("environment: {e}"))?;
    let reason = wide("Blocked by NEXUS");
    let no_headers = wide("");
    core.add_WebResourceRequested(
        &WebResourceRequestedEventHandler::create(Box::new(move |_s, args| {
            let Some(args) = args else { return Ok(()) };
            let req = args.Request()?;
            let mut uri = PWSTR::null();
            req.Uri(&mut uri)?;
            let u = take_pwstr(uri);
            if !render_request_allowed(&u) {
                let resp = env.CreateWebResourceResponse(
                    None,
                    403,
                    PCWSTR(reason.as_ptr()),
                    PCWSTR(no_headers.as_ptr()),
                )?;
                args.SetResponse(&resp)?;
            }
            Ok(())
        })),
        &mut token,
    )
    .map_err(|e| format!("resource handler: {e}"))?;

    let method = wide("Emulation.setDeviceMetricsOverride");
    let params = wide(r#"{"width":1366,"height":900,"deviceScaleFactor":1,"mobile":false}"#);
    let _ = core.CallDevToolsProtocolMethod(
        PCWSTR(method.as_ptr()),
        PCWSTR(params.as_ptr()),
        &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_r, _json| Ok(()))),
    );
    Ok(())
}

/// Run the collector on the UI thread; the answer arrives on `tx`.
#[cfg(target_os = "windows")]
unsafe fn execute_collector(
    webview: &tauri::webview::PlatformWebview,
    script: &str,
    tx: tokio::sync::oneshot::Sender<Result<String, String>>,
) {
    use webview2_com::ExecuteScriptCompletedHandler;
    use windows_core::PCWSTR;
    let core = match webview.controller().CoreWebView2() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(Err(e.to_string()));
            return;
        }
    };
    let js = wide(script);
    let tx = std::sync::Mutex::new(Some(tx));
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |res, json| {
        if let Some(tx) = tx.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(res.map(|_| json).map_err(|e| e.to_string()));
        }
        Ok(())
    }));
    if let Err(e) = core.ExecuteScript(PCWSTR(js.as_ptr()), &handler) {
        tracing::warn!("site_render: ExecuteScript failed: {e}");
    }
}

/// Settle time after the load event, so client-side apps can hydrate.
#[cfg(target_os = "windows")]
const SETTLE_MS: u64 = 1_500;
/// Give up waiting for the load event after this long and collect anyway.
#[cfg(target_os = "windows")]
const LOAD_WAIT_MS: u64 = 8_000;
/// Hard cap for the whole render, window build to destroy.
pub(crate) const RENDER_TOTAL_MS: u64 = 12_000;

/// Outcome of one render attempt.
#[derive(Debug)]
pub(crate) enum RenderResult {
    Page(RenderedPage),
    /// The page tried to send the hidden window to a sign-in service.
    LoginRedirect,
    Cancelled,
    Failed(String),
}

#[cfg(target_os = "windows")]
pub(crate) async fn render<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> RenderResult {
    let deadline = std::time::Duration::from_millis(RENDER_TOTAL_MS);
    // Every request of the page leaves through the guard proxy (public addresses only, port 443, resolved and checked
    // here, not by WebView2), which closes the DNS-rebinding gap. If the guard cannot start, nothing is rendered.
    let proxy = match crate::render_proxy::start(crate::render_proxy::Policy::production()).await {
        Ok(p) => p,
        Err(e) => return RenderResult::Failed(format!("network guard could not start: {e}")),
    };
    let proxy_url = proxy.url();
    let profile = std::env::temp_dir().join(format!("nexus-stack-render-{}", std::process::id()));
    let result = match tokio::time::timeout(deadline, render_inner(app, url, cancel, &proxy_url, &profile)).await {
        Ok(r) => r,
        Err(_) => {
            destroy(app);
            RenderResult::Failed("render timed out".into())
        }
    };
    tracing::info!("[REVERSE] render guard: {} tunnels, {} refused", proxy.tunnelled(), proxy.refused());
    proxy.stop();
    // the throwaway profile (its own WebView2 environment, because the proxy is a browser argument) is removed again
    let _ = std::fs::remove_dir_all(&profile);
    result
}

#[cfg(target_os = "windows")]
fn destroy<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window(RENDER_LABEL) {
        let _ = w.destroy();
    }
}

#[cfg(target_os = "windows")]
async fn render_inner<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    proxy_url: &str,
    profile: &std::path::Path,
) -> RenderResult {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::webview::{NewWindowResponse, PageLoadEvent};
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    if render_nav_allowed(url, 0) != NavDecision::Allow {
        return RenderResult::Failed("address not allowed for rendering".into());
    }
    let parsed: tauri::Url = match url.parse() {
        Ok(u) => u,
        Err(e) => return RenderResult::Failed(format!("bad url: {e}")),
    };
    destroy(app);
    let proxy: tauri::Url = match proxy_url.parse() {
        Ok(u) => u,
        Err(e) => return RenderResult::Failed(format!("guard address: {e}")),
    };

    let navs = Arc::new(AtomicUsize::new(0));
    let login = Arc::new(AtomicBool::new(false));
    let loaded = Arc::new(AtomicBool::new(false));
    let (navs_cb, login_cb, loaded_cb) = (navs.clone(), login.clone(), loaded.clone());

    // 8x8 and mostly off the top-left corner: the OS still counts it as visible
    // (a hidden WebView2 is throttled), but at most a 2x2 corner is on screen.
    let built = WebviewWindowBuilder::new(app, RENDER_LABEL, WebviewUrl::External(parsed))
        .title("NEXUS stack render")
        .inner_size(8.0, 8.0)
        .position(-6.0, -6.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .incognito(true)
        // A proxy is a WebView2 browser argument, which belongs to the environment, so this window gets its own
        // throwaway profile folder (the main windows keep theirs untouched).
        .data_directory(profile.to_path_buf())
        .proxy_url(proxy)
        .on_navigation(move |u| {
            let n = navs_cb.fetch_add(1, Ordering::SeqCst);
            match render_nav_allowed(u.as_str(), n) {
                NavDecision::Allow => true,
                NavDecision::LoginRedirect => {
                    login_cb.store(true, Ordering::SeqCst);
                    false
                }
                NavDecision::Block => false,
            }
        })
        .on_new_window(|_u, _f| {
            if new_window_policy() {
                NewWindowResponse::Allow
            } else {
                NewWindowResponse::Deny
            }
        })
        .on_download(|_w, _e| false)
        .on_page_load(move |_w, payload| {
            if payload.event() == PageLoadEvent::Finished {
                loaded_cb.store(true, Ordering::SeqCst);
            }
        })
        .build();
    let win = match built {
        Ok(w) => w,
        Err(e) => return RenderResult::Failed(format!("window: {e}")),
    };
    let _ = win.set_ignore_cursor_events(true);
    if let Ok(hwnd) = win.hwnd() {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WINDOW_DISPLAY_AFFINITY};
        // WDA_EXCLUDEFROMCAPTURE: never in the tour's screenshots.
        unsafe {
            let _ = SetWindowDisplayAffinity(HWND(hwnd.0 as _), WINDOW_DISPLAY_AFFINITY(17));
        }
    }
    if let Err(e) = win.with_webview(|wv| unsafe {
        if let Err(e) = harden(&wv) {
            tracing::warn!("site_render: hardening incomplete: {e}");
        }
    }) {
        destroy(app);
        return RenderResult::Failed(format!("webview: {e}"));
    }

    let start = std::time::Instant::now();
    let mut loaded_at: Option<std::time::Instant> = None;
    loop {
        if cancel.load(Ordering::Relaxed) {
            destroy(app);
            return RenderResult::Cancelled;
        }
        if login.load(Ordering::SeqCst) {
            destroy(app);
            return RenderResult::LoginRedirect;
        }
        if loaded_at.is_none() && loaded.load(Ordering::SeqCst) {
            loaded_at = Some(std::time::Instant::now());
        }
        let settled = loaded_at.is_some_and(|t| t.elapsed().as_millis() as u64 >= SETTLE_MS);
        if settled || start.elapsed().as_millis() as u64 >= LOAD_WAIT_MS {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let (globals, dom) = site_probe::render_probes();
    let script = build_collector(&globals, &dom);
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Err(e) = win.with_webview(move |wv| unsafe { execute_collector(&wv, &script, tx) }) {
        destroy(app);
        return RenderResult::Failed(format!("collector: {e}"));
    }
    let answer = tokio::time::timeout(std::time::Duration::from_secs(3), rx).await;
    destroy(app);
    match answer {
        Ok(Ok(Ok(raw))) => match parse_rendered(&raw) {
            Ok(page) => RenderResult::Page(page),
            Err(e) => RenderResult::Failed(e),
        },
        Ok(Ok(Err(e))) => RenderResult::Failed(format!("script error: {e}")),
        Ok(Err(_)) => RenderResult::Failed("collector dropped".into()),
        Err(_) => RenderResult::Failed("collector timed out".into()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_uses_no_banned_api() {
        // The collector runs inside a third-party page: it must only read.
        let banned = [
            "fetch(", "XMLHttpRequest", "sendBeacon", "WebSocket", "EventSource", "import(",
            "eval(", "Function(", "postMessage", ".click(", "innerHTML", "outerHTML",
            "createElement", "appendChild", "insertAdjacent", "location.", "document.cookie",
            "localStorage", "sessionStorage", "indexedDB", "setTimeout", "setInterval",
            "open(", "submit(", "dispatchEvent", "document.write", "navigator.",
        ];
        let script = build_collector(&["next.version".into()], &[("#__next".into(), None)]);
        for b in banned {
            assert!(!script.contains(b), "collector must not use {b}");
        }
        assert!(script.contains("getOwnPropertyDescriptor"), "globals must be read as data properties");
        assert!(script.contains("262144"), "output cap present");
    }

    #[test]
    fn build_collector_embeds_only_valid_probes_as_json() {
        let script = build_collector(
            &["next.version".into(), "x\"});alert(1)//".into(), "@marker-not-listed".into()],
            &[("#__next".into(), None), ("a{b}".into(), None), ("[ng-version]".into(), Some("ng-version".into()))],
        );
        assert!(!script.contains("__NEXUS_PROBES__"));
        assert!(script.contains(r#""g":["next.version","@marker-not-listed"]"#), "{}", &script[..400]);
        assert!(!script.contains("alert(1)"), "invalid path must be dropped");
        assert!(!script.contains("a{b}"), "selector with braces must be dropped");
        assert!(script.contains(r#"["[ng-version]","ng-version"]"#));
    }

    fn raw_of(v: serde_json::Value) -> String {
        serde_json::to_string(&v.to_string()).unwrap()
    }

    #[test]
    fn parse_rendered_double_decodes_and_strips_queries() {
        let raw = raw_of(serde_json::json!({
            "v": 1,
            "globals": [["next.version", "14.2.3"]],
            "dom": [["#__next", null, null]],
            "markers": [["@react-root", null], ["not-a-marker", null]],
            "resources": ["https://cdn.example.com/a.js?token=secret#x", "ftp://x/y", "javascript:alert(1)"],
            "scripts": ["https://example.com/_next/static/chunks/main.js?v=1"],
            "inline": ["x".repeat(5000)],
            "generator": "Next.js",
            "text_len": 1200,
            "elements": 300,
            "title": "Home",
            "final_url": "https://example.com/?utm=1"
        }));
        let p = parse_rendered(&raw).unwrap();
        assert_eq!(p.globals, vec![("next.version".to_string(), Some("14.2.3".to_string()))]);
        assert_eq!(p.markers.len(), 1, "only @markers survive");
        assert_eq!(p.resources, vec!["https://cdn.example.com/a.js".to_string()]);
        assert_eq!(p.scripts, vec!["https://example.com/_next/static/chunks/main.js".to_string()]);
        assert_eq!(p.inline[0].chars().count(), MAX_INLINE_CHARS);
        assert_eq!(p.final_url, "https://example.com/");
    }

    #[test]
    fn parse_rendered_refuses_garbage_and_oversize() {
        assert!(parse_rendered("null").is_err());
        assert!(parse_rendered("42").is_err());
        assert!(parse_rendered("\"not json\"").is_err());
        assert!(parse_rendered(&raw_of(serde_json::json!({"v": 2}))).is_err(), "unknown version");
        let huge = "\"".to_string() + &"a".repeat(MAX_RAW_BYTES + 10) + "\"";
        assert!(parse_rendered(&huge).is_err());
        // too many entries are cut back to the caps
        let many: Vec<serde_json::Value> = (0..1000).map(|i| serde_json::json!([format!("g{i}"), null])).collect();
        let p = parse_rendered(&raw_of(serde_json::json!({"v": 1, "globals": many}))).unwrap();
        assert_eq!(p.globals.len(), MAX_GLOBALS);
    }

    fn spa_report() -> SiteReport {
        let url = reqwest::Url::parse("https://app.example.com/").unwrap();
        let html = r#"<html><head><title>App</title><script type="module" src="/assets/index-abc123.js"></script></head><body><div id="root"></div></body></html>"#;
        site_probe::analyse(&url, 200, &[], html, &[], &[])
    }

    #[test]
    fn merge_finds_a_client_rendered_react_app_and_clears_the_shell_reason() {
        let before = spa_report();
        assert!(before.detections.iter().all(|d| d.id != "react"), "static scan cannot see React");
        assert!(before.thin.contains(&ThinReason::SpaShell), "{:?}", before.thin);
        let page = RenderedPage {
            v: 1,
            markers: vec![("@react-root".into(), None)],
            text_len: 2400,
            title: "App".into(),
            ..Default::default()
        };
        let after = merge_rendered_signals(&before, &page);
        let react = after.detections.iter().find(|d| d.id == "react").expect("react found by render");
        assert!(react.confidence >= 0.80, "{}", react.confidence);
        assert!(react.evidence.iter().any(|e| e.contains("(rendered)")));
        assert!(!after.thin.contains(&ThinReason::SpaShell), "hydrated page is not a shell");
        assert!(after.notes.iter().any(|n| n.contains("hidden browser window")));
    }

    #[test]
    fn merge_raises_next_from_inferred_to_detected_with_a_version() {
        let url = reqwest::Url::parse("https://shop.example.com/").unwrap();
        // Only a weak hint statically (support-only signals never add confidence).
        let report = site_probe::analyse(&url, 200, &[], "<html><body><p>shop</p></body></html>", &[], &[]);
        assert!(report.detections.iter().all(|d| d.id != "nextjs"));
        let page = RenderedPage {
            v: 1,
            globals: vec![("next.version".into(), Some("14.2.3".into())), ("__next_f".into(), None)],
            text_len: 900,
            ..Default::default()
        };
        let merged = merge_rendered_signals(&report, &page);
        let next = merged.detections.iter().find(|d| d.id == "nextjs").expect("nextjs");
        assert!(next.confidence >= 0.80);
        assert_eq!(next.version.as_deref(), Some("14.2.3"));
        // implied React arrives with it
        assert!(merged.detections.iter().any(|d| d.id == "react"));
    }

    #[test]
    fn merge_never_lowers_a_confidence() {
        let url = reqwest::Url::parse("https://example.com/").unwrap();
        let headers = vec![("x-powered-by".to_string(), "Next.js".to_string())];
        let report = site_probe::analyse(&url, 200, &headers, "<p>hi</p>", &[], &[]);
        let merged = merge_rendered_signals(&report, &RenderedPage { v: 1, ..Default::default() });
        for d in &report.detections {
            let m = merged.detections.iter().find(|x| x.id == d.id).expect("kept");
            assert!(m.confidence >= d.confidence - 1e-6, "{} fell", d.id);
        }
    }

    #[test]
    fn merge_keeps_a_challenge_the_browser_also_hit() {
        let mut report = spa_report();
        report.thin = vec![ThinReason::Challenge("Cloudflare".into())];
        let page = RenderedPage { v: 1, title: "Just a moment...".into(), text_len: 40, ..Default::default() };
        let merged = merge_rendered_signals(&report, &page);
        assert!(merged.thin.iter().any(|t| matches!(t, ThinReason::Challenge(_))));
        let passed = RenderedPage { v: 1, title: "Welcome".into(), text_len: 3000, ..Default::default() };
        let merged2 = merge_rendered_signals(&report, &passed);
        assert!(!merged2.thin.iter().any(|t| matches!(t, ThinReason::Challenge(_))));
    }

    #[test]
    fn nav_policy_table() {
        assert_eq!(render_nav_allowed("https://example.com/", 0), NavDecision::Allow);
        assert_eq!(render_nav_allowed("about:blank", 0), NavDecision::Allow);
        for bad in [
            "http://example.com/",
            "https://127.0.0.1/",
            "https://10.0.0.5/admin",
            "https://[::1]/",
            "https://router.local/",
            "https://localhost:3000/",
            "file:///C:/Windows/win.ini",
            "javascript:alert(1)",
            "not a url",
        ] {
            assert_eq!(render_nav_allowed(bad, 0), NavDecision::Block, "{bad}");
        }
        assert_eq!(render_nav_allowed("https://example.com/", MAX_RENDER_NAVS), NavDecision::Block);
        assert_eq!(
            render_nav_allowed("https://accounts.google.com/signin", 1),
            NavDecision::LoginRedirect
        );
        assert_eq!(render_nav_allowed("https://acme.okta.com/login", 1), NavDecision::LoginRedirect);
    }

    #[test]
    fn nav_policy_refuses_sensitive_pages() {
        // Same rule as the static probe: banking/password pages are never opened.
        let sensitive = "https://www.paypal.com/signin";
        if crate::screen_context::is_sensitive("", "", Some(sensitive)) {
            assert_eq!(render_nav_allowed(sensitive, 0), NavDecision::Block);
        }
    }

    #[test]
    fn request_policy_table() {
        for ok in [
            "https://cdn.example.com/app.js",
            "http://example.org/img.png",
            "data:image/png;base64,AAAA",
            "blob:https://example.com/123",
            "https://8.8.8.8/x",
        ] {
            assert!(render_request_allowed(ok), "{ok}");
        }
        for bad in [
            "http://127.0.0.1:8080/",
            "http://192.168.1.1/",
            "https://169.254.169.254/latest/meta-data/",
            "http://[::ffff:127.0.0.1]/",
            "http://[fd00::1]/",
            "http://localhost/",
            "http://printer.local/",
            "http://nas/",
            "file:///C:/x",
            "ws://example.com/",
        ] {
            assert!(!render_request_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn popups_are_never_allowed() {
        assert!(!new_window_policy());
    }

    #[test]
    fn render_window_has_no_ipc_capability() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities");
        for entry in std::fs::read_dir(dir).expect("capabilities dir") {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            assert!(!text.contains(RENDER_LABEL), "{} grants IPC to the render window", path.display());
        }
    }

    #[test]
    fn render_probe_lists_come_from_the_table() {
        let (globals, dom) = site_probe::render_probes();
        assert!(globals.contains(&"next.version".to_string()));
        assert!(globals.contains(&"jQuery.fn.jquery".to_string()));
        assert!(!globals.iter().any(|g| g.starts_with('@')), "markers are built in");
        assert!(dom.contains(&("[ng-version]".to_string(), Some("ng-version".to_string()))));
        assert!(globals.iter().all(|g| site_probe::valid_js_path(g)));
        assert!(dom.iter().all(|(s, _)| site_probe::valid_dom_selector(s)));
    }
}
