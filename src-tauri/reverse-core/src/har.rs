//! HAR (HTTP Archive) -> inferred OpenAPI 3.0 for the user's OWN captured session. Static analysis only:
//! nothing is ever sent, replayed or fuzzed.
//!
//! Redaction first, by construction: request/response VALUES are classified (id-shaped? numeric? secret-like?)
//! and then dropped - only names, types and path shapes survive. No example values are emitted at all
//! (example/default leakage is the documented failure mode of mitmproxy2swagger-style tools).
//!
//! Known limits (reported, not hidden): word-like path parameters need >= 3 distinct values plus a
//! structural or response-shape signal to be recognised; websockets are listed but not modelled; GraphQL is
//! reported as one endpoint plus an operation catalogue; pagination is detected by parameter name only.

use crate::redact;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const MAX_BYTES: usize = 256 << 20;
const MAX_ENTRIES: usize = 50_000;
const MAX_BODY: usize = 2 << 20;
const MAX_DEPTH: usize = 7;
const MAX_PROPS: usize = 200;

// ─── public result types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default)]
pub struct EndpointSummary {
    pub method: String,
    pub path: String,
    pub host: String,
    pub requests: usize,
    pub statuses: Vec<u16>,
    pub query: Vec<String>,
    pub request_fields: Vec<String>,
    pub response_fields: Vec<String>,
    pub auth: Vec<String>,
    pub pagination: Vec<String>,
    pub graphql: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GraphqlOp {
    pub kind: String,
    pub name: String,
    pub fields: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct HarResult {
    pub openapi_yaml: String,
    pub endpoints: Vec<EndpointSummary>,
    pub graphql_ops: Vec<GraphqlOp>,
    pub websockets: Vec<String>,
    pub entries: usize,
    pub skipped_static: usize,
    pub skipped_other: usize,
    /// How many credential-bearing headers / params / body fields / values were seen and dropped.
    pub redactions: usize,
    pub warnings: Vec<String>,
}

// ─── small helpers ─────────────────────────────────────────────────────────

fn re(cell: &'static OnceLock<Regex>, p: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(p).unwrap())
}

fn is_uuid(s: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$").is_match(s)
}

fn is_version(s: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^v\d+(\.\d+)*$").is_match(s)
}

fn is_date(s: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^\d{4}-\d{2}-\d{2}(T[\d:.]+Z?)?$").is_match(s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum IdKind {
    Numeric,
    Uuid,
    Opaque,
}

/// Path segments that are values (ids), not names. Versions like `v1` stay literal.
fn id_kind(seg: &str) -> Option<IdKind> {
    if seg.is_empty() || is_version(seg) {
        return None;
    }
    if seg.bytes().all(|b| b.is_ascii_digit()) {
        return Some(IdKind::Numeric);
    }
    if is_uuid(seg) {
        return Some(IdKind::Uuid);
    }
    if is_date(seg) {
        return Some(IdKind::Opaque);
    }
    let digits = seg.bytes().filter(u8::is_ascii_digit).count();
    let alnum = seg.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
    if seg.len() >= 8 && seg.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(IdKind::Opaque);
    }
    if alnum && seg.len() >= 6 && digits >= 1 && seg.bytes().any(|b| b.is_ascii_alphabetic()) {
        return Some(IdKind::Opaque);
    }
    if redact::looks_like_secret_value(seg) {
        return Some(IdKind::Opaque);
    }
    None
}

fn is_apikey_name(n: &str) -> bool {
    let n = n.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "api_key" | "apikey" | "api-key" | "x-api-key" | "x-apikey" | "access_token" | "access-token" | "x-access-token" | "token" | "auth_token"
            | "x-auth-token" | "x-token" | "key" | "x-amz-security-token" | "private-token"
    )
}

fn singular(w: &str) -> String {
    let w = w.to_ascii_lowercase();
    let w = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if let Some(s) = w.strip_suffix("ies") {
        return format!("{s}y");
    }
    if w.ends_with("ss") || w.ends_with("us") {
        return w.to_string();
    }
    w.strip_suffix('s').unwrap_or(w).to_string()
}

fn camel(w: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in w.chars() {
        if c == '-' || c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn param_type_of(v: &str) -> &'static str {
    if v.parse::<i64>().is_ok() {
        "integer"
    } else if v.parse::<f64>().is_ok() {
        "number"
    } else if matches!(v, "true" | "false") {
        "boolean"
    } else {
        "string"
    }
}

fn mime_base(m: &str) -> String {
    m.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

fn is_static(path: &str, mime: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let ext = p.rsplit('.').next().unwrap_or("");
    let ext_static = p.contains('.')
        && matches!(
            ext,
            "js" | "mjs" | "css" | "map" | "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "woff" | "woff2" | "ttf" | "otf" | "webp" | "html" | "htm" | "mp4" | "webm" | "mp3" | "wasm"
        );
    ext_static
        || mime.starts_with("image/")
        || mime.starts_with("font/")
        || mime.starts_with("video/")
        || mime.starts_with("audio/")
        || mime == "text/css"
        || mime == "text/html"
        || mime.contains("javascript")
        || mime == "application/wasm"
}

// ─── schema inference ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct Schema {
    types: BTreeSet<&'static str>,
    format: Option<&'static str>,
    obj_count: usize,
    props: BTreeMap<String, (Schema, usize)>,
    items: Option<Box<Schema>>,
}

fn string_format(s: &str) -> Option<&'static str> {
    if is_uuid(s) {
        Some("uuid")
    } else if is_date(s) {
        Some(if s.contains('T') { "date-time" } else { "date" })
    } else {
        None
    }
}

impl Schema {
    fn merge_value(&mut self, v: &Value, depth: usize, redactions: &mut usize) {
        match v {
            Value::Null => {
                self.types.insert("null");
            }
            Value::Bool(_) => {
                self.types.insert("boolean");
            }
            Value::Number(n) => {
                self.types.insert(if n.is_i64() || n.is_u64() { "integer" } else { "number" });
            }
            Value::String(s) => {
                self.types.insert("string");
                if redact::looks_like_secret_value(s) {
                    *redactions += 1;
                }
                if let Some(f) = string_format(s) {
                    if self.format.is_none() || self.format == Some(f) {
                        self.format = Some(f);
                    }
                } else if self.types.contains("string") && self.format.is_some() {
                    self.format = None;
                }
            }
            Value::Array(a) => {
                self.types.insert("array");
                if depth < MAX_DEPTH {
                    let items = self.items.get_or_insert_with(Default::default);
                    for x in a.iter().take(25) {
                        items.merge_value(x, depth + 1, redactions);
                    }
                }
            }
            Value::Object(o) => {
                self.types.insert("object");
                self.obj_count += 1;
                if depth >= MAX_DEPTH {
                    return;
                }
                for (k, x) in o.iter() {
                    if self.props.len() >= MAX_PROPS && !self.props.contains_key(k) {
                        continue;
                    }
                    if redact::is_secret_name(k) {
                        *redactions += 1;
                    }
                    let e = self.props.entry(k.clone()).or_default();
                    e.1 += 1;
                    e.0.merge_value(x, depth + 1, redactions);
                }
            }
        }
    }

    fn to_json(&self) -> Value {
        let mut m = Map::new();
        let mut t: Vec<&str> = self.types.iter().copied().filter(|t| *t != "null").collect();
        let nullable = self.types.contains("null");
        if t.contains(&"integer") && t.contains(&"number") {
            t.retain(|x| *x != "integer");
        }
        match t.len() {
            0 => {
                if nullable {
                    m.insert("nullable".into(), json!(true));
                }
            }
            1 => {
                m.insert("type".into(), json!(t[0]));
                if nullable {
                    m.insert("nullable".into(), json!(true));
                }
            }
            _ => {
                m.insert("oneOf".into(), Value::Array(t.iter().map(|x| json!({"type": x})).collect()));
            }
        }
        if let Some(f) = self.format {
            if t == ["string"] {
                m.insert("format".into(), json!(f));
            }
        }
        if t.contains(&"object") && !self.props.is_empty() {
            let mut p = Map::new();
            let mut req = Vec::new();
            for (k, (s, c)) in &self.props {
                p.insert(k.clone(), s.to_json());
                if *c == self.obj_count && self.obj_count > 0 {
                    req.push(json!(k));
                }
            }
            m.insert("properties".into(), Value::Object(p));
            if !req.is_empty() {
                m.insert("required".into(), Value::Array(req));
            }
        }
        if t.contains(&"array") {
            m.insert("items".into(), self.items.as_ref().map(|i| i.to_json()).unwrap_or_else(|| json!({})));
        }
        Value::Object(m)
    }

    /// Dotted field paths (`a.b`, `items[].id`) - for the evaluation summary.
    fn field_paths(&self, prefix: &str, out: &mut BTreeSet<String>) {
        for (k, (s, _)) in &self.props {
            let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
            out.insert(p.clone());
            s.field_paths(&p, out);
        }
        if let Some(i) = &self.items {
            i.field_paths(&format!("{prefix}[]"), out);
        }
    }
}

// ─── per-request model (values already dropped) ────────────────────────────

#[derive(Debug, Clone, Default)]
struct Req {
    origin: String,
    method: String,
    segs: Vec<(String, Option<IdKind>)>,
    status: u16,
    query: Vec<(String, &'static str)>,
    req_mime: String,
    req_schema: Option<Value>,
    req_form_fields: Vec<String>,
    resp_mime: String,
    resp_schema: Option<Value>,
    resp_keys: BTreeSet<String>,
    auth: Vec<String>,
    graphql: Option<Vec<(String, String, Vec<String>)>>,
}

fn split_url(url: &str) -> Option<(String, String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let rest = rest.split('#').next().unwrap_or("");
    let (authority, tail) = match rest.find(|c| c == '/' || c == '?') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = authority.rsplit('@').next().unwrap_or(authority); // userinfo is dropped, never kept
    let (path, query) = match tail.split_once('?') {
        Some((p, q)) => (p, q),
        None => (tail, ""),
    };
    Some((format!("{}://{}", scheme.to_ascii_lowercase(), host.to_ascii_lowercase()), if path.is_empty() { "/".into() } else { path.to_string() }, query.to_string()))
}

fn headers(h: &Value) -> Vec<(String, String)> {
    h.as_array()
        .map(|a| a.iter().filter_map(|x| Some((x["name"].as_str()?.to_string(), x["value"].as_str().unwrap_or("").to_string()))).collect())
        .unwrap_or_default()
}

fn body_text(content: &Value) -> Option<String> {
    let t = content["text"].as_str()?;
    if t.len() > MAX_BODY {
        return None;
    }
    if content["encoding"].as_str() == Some("base64") {
        use base64::Engine;
        let b = base64::engine::general_purpose::STANDARD.decode(t).ok()?;
        return String::from_utf8(b).ok();
    }
    Some(t.to_string())
}

/// Very small GraphQL scanner: operation kind, name and the top-level fields of the selection set.
fn parse_graphql(q: &str) -> (String, String, Vec<String>) {
    static R: OnceLock<Regex> = OnceLock::new();
    let head = re(&R, r"(?s)^\s*(query|mutation|subscription)?\s*([A-Za-z_]\w*)?");
    let kind;
    let name;
    if let Some(c) = head.captures(q) {
        kind = c.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "query".into());
        name = c.get(2).map(|m| m.as_str().to_string()).filter(|n| !matches!(n.as_str(), "query" | "mutation" | "subscription")).unwrap_or_else(|| "(anonymous)".into());
    } else {
        kind = "query".into();
        name = "(anonymous)".into();
    }
    let mut depth = 0i32;
    let mut paren = 0i32;
    let mut fields = Vec::new();
    let b: Vec<char> = q.chars().collect();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '(' => paren += 1,
            ')' => paren -= 1,
            '{' if paren == 0 => depth += 1,
            '}' if paren == 0 => depth -= 1,
            c if (c.is_ascii_alphabetic() || c == '_') && depth == 1 && paren == 0 => {
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                    i += 1;
                }
                let ident: String = b[s..i].iter().collect();
                let mut j = i;
                while j < b.len() && b[j].is_whitespace() {
                    j += 1;
                }
                // `alias: field` - the alias is not a field name; the field that follows is picked up next
                if !(j < b.len() && b[j] == ':') && !fields.contains(&ident) {
                    fields.push(ident);
                }
                continue;
            }
            '"' => {
                // skip string literals
                i += 1;
                while i < b.len() && b[i] != '"' {
                    if b[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    (kind, name, fields)
}

fn extract_req(entry: &Value, redactions: &mut usize, warnings: &mut Vec<String>) -> Result<Option<Req>, &'static str> {
    let rq = &entry["request"];
    let url = rq["url"].as_str().ok_or("no url")?;
    let method = rq["method"].as_str().unwrap_or("GET").to_ascii_uppercase();
    let (origin, path, query) = split_url(url).ok_or("bad url")?;
    let resp = &entry["response"];
    let status = resp["status"].as_u64().unwrap_or(0) as u16;
    let rmime = mime_base(resp["content"]["mimeType"].as_str().unwrap_or(""));
    if origin.starts_with("ws") || entry.get("_webSocketMessages").is_some() {
        return Ok(None);
    }
    if is_static(&path, &rmime) {
        return Err("static");
    }
    if method == "OPTIONS" || method == "HEAD" || status == 0 {
        return Err("other");
    }
    let mut r = Req { origin, method, status, resp_mime: rmime.clone(), ..Default::default() };

    // path -> (literal, id kind); secret-looking segments are templated, never kept
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        let k = id_kind(seg);
        if k.is_some() && redact::looks_like_secret_value(seg) {
            *redactions += 1;
        }
        r.segs.push((if k.is_some() { String::new() } else { seg.to_string() }, k));
    }
    // query: names + value type only
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k.is_empty() {
            continue;
        }
        if redact::is_secret_name(k) || redact::looks_like_secret_value(v) {
            *redactions += 1;
        }
        if is_apikey_name(k) {
            r.auth.push(format!("apiKey:query:{}", k.to_ascii_lowercase()));
        }
        r.query.push((k.to_string(), param_type_of(v)));
    }
    // headers: names only; auth scheme (never the credential)
    for (n, v) in headers(&rq["headers"]) {
        let ln = n.to_ascii_lowercase();
        if ln == "authorization" {
            *redactions += 1;
            let scheme = v.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
            r.auth.push(match scheme.as_str() {
                "bearer" => "http:bearer".to_string(),
                "basic" => "http:basic".to_string(),
                "" => "http:unknown".to_string(),
                s => format!("http:{}", s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()),
            });
        } else if ln == "cookie" {
            *redactions += 1;
            let first = v.split(';').next().and_then(|c| c.split('=').next()).unwrap_or("").trim().to_string();
            if !first.is_empty() && first.len() <= 64 && first.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.') {
                r.auth.push(format!("apiKey:cookie:{first}"));
            }
        } else if is_apikey_name(&ln) {
            *redactions += 1;
            r.auth.push(format!("apiKey:header:{ln}"));
        } else if redact::is_secret_name(&ln) {
            *redactions += 1;
        }
    }
    for (n, _) in headers(&resp["headers"]) {
        if redact::is_secret_name(&n) {
            *redactions += 1; // e.g. Set-Cookie: seen, value dropped
        }
    }
    r.auth.sort();
    r.auth.dedup();

    // request body
    let pd = &rq["postData"];
    let rq_mime = mime_base(pd["mimeType"].as_str().unwrap_or(""));
    r.req_mime = rq_mime.clone();
    if let Some(text) = pd["text"].as_str().filter(|t| t.len() <= MAX_BODY) {
        if rq_mime.contains("json") || text.trim_start().starts_with('{') || text.trim_start().starts_with('[') {
            if let Ok(v) = serde_json::from_str::<Value>(text) {
                // GraphQL?
                let gq: Vec<&Value> = match &v {
                    Value::Array(a) => a.iter().collect(),
                    o => vec![o],
                };
                let ops: Vec<(String, String, Vec<String>)> = gq
                    .iter()
                    .filter_map(|x| x["query"].as_str().map(|q| {
                        let (k, n, f) = parse_graphql(q);
                        (k, x["operationName"].as_str().map(str::to_string).unwrap_or(n), f)
                    }))
                    .collect();
                if !ops.is_empty() && r.method == "POST" {
                    r.graphql = Some(ops);
                } else {
                    let mut s = Schema::default();
                    s.merge_value(&v, 0, redactions);
                    r.req_schema = Some(s.to_json());
                    let mut fp = BTreeSet::new();
                    s.field_paths("", &mut fp);
                    r.req_form_fields = fp.into_iter().collect();
                }
            }
        } else if rq_mime.contains("x-www-form-urlencoded") {
            r.req_form_fields = text
                .split('&')
                .filter_map(|p| p.split('=').next())
                .filter(|k| !k.is_empty())
                .map(|k| {
                    if redact::is_secret_name(k) {
                        *redactions += 1;
                    }
                    k.to_string()
                })
                .collect();
        } else if rq_mime.starts_with("multipart/") {
            if let Some(params) = pd["params"].as_array() {
                r.req_form_fields = params.iter().filter_map(|p| p["name"].as_str().map(str::to_string)).collect();
            }
        }
    }
    // response body
    if let Some(text) = body_text(&resp["content"]) {
        if rmime.contains("json") || rmime.is_empty() {
            match serde_json::from_str::<Value>(&text) {
                Ok(v) => {
                    let mut s = Schema::default();
                    s.merge_value(&v, 0, redactions);
                    r.resp_keys = match &v {
                        Value::Object(o) => o.keys().cloned().collect(),
                        Value::Array(a) => a.first().and_then(|x| x.as_object()).map(|o| o.keys().map(|k| format!("[].{k}")).collect()).unwrap_or_default(),
                        _ => BTreeSet::new(),
                    };
                    r.resp_schema = Some(s.to_json());
                    if r.resp_mime.is_empty() {
                        r.resp_mime = "application/json".into();
                    }
                }
                Err(_) => {
                    if rmime.contains("json") {
                        warnings.push("a response advertised JSON but did not parse as JSON".into());
                    }
                }
            }
        }
    }
    Ok(Some(r))
}

// ─── path trie, parameter collapsing, endpoint assembly ────────────────────

#[derive(Debug, Default)]
struct Hit {
    method: String,
    status: u16,
    resp_keys: BTreeSet<String>,
    req: usize, // index into the request list
}

#[derive(Debug, Default)]
struct Node {
    children: BTreeMap<String, Node>,
    hits: Vec<Hit>,
    /// id kinds seen in the position this node represents (only meaningful for the `{}` child)
    kinds: BTreeSet<IdKind>,
    /// distinct literal words that were folded into this `{}` node
    words: BTreeSet<String>,
}

const PARAM: &str = "{}";

fn merge_nodes(into: &mut Node, from: Node) {
    into.hits.extend(from.hits);
    into.kinds.extend(from.kinds);
    into.words.extend(from.words);
    for (k, c) in from.children {
        merge_nodes(into.children.entry(k).or_default(), c);
    }
}

fn resp_similarity(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}

fn node_resp_keys(n: &Node, method: &str) -> Option<BTreeSet<String>> {
    n.hits.iter().filter(|h| h.method == method && (200..300).contains(&h.status) && !h.resp_keys.is_empty()).map(|h| h.resp_keys.clone()).next()
}

/// Union of 2xx response key sets of every hit at or below `n` for `method`.
fn deep_resp(n: &Node, method: &str, out: &mut BTreeSet<String>) {
    for h in &n.hits {
        if h.method == method && (200..300).contains(&h.status) {
            out.extend(h.resp_keys.iter().cloned());
        }
    }
    for c in n.children.values() {
        deep_resp(c, method, out);
    }
}

fn deep_methods(n: &Node, out: &mut BTreeSet<String>) {
    for h in &n.hits {
        out.insert(h.method.clone());
    }
    for c in n.children.values() {
        deep_methods(c, out);
    }
}

/// Fold sibling literal words into `{}` when the evidence says they are values of ONE parameter. Needs
/// >= 3 distinct words and one of three independent signals:
///  A. they all continue with a shared literal child (`/orgs/acme/repos`, `/orgs/globex/repos`, ...);
///  B. they are leaves whose 2xx responses have near-identical JSON shape for the same method;
///  C. each continues only with word leaves (no ids) and, per method, everything below them returns
///     near-identical shapes (`/user/starred/{org}/{repo}`).
/// Runs TOP-DOWN so that a folded parent exposes its (now pooled) children to the next level.
fn collapse(node: &mut Node) {
    let words: Vec<String> = node.children.keys().filter(|k| k.as_str() != PARAM).cloned().collect();
    if words.len() >= 3 {
        let mut fold: BTreeSet<String> = BTreeSet::new();
        // A
        let mut cont: BTreeMap<String, usize> = BTreeMap::new();
        for w in &words {
            for k in node.children[w].children.keys().filter(|k| k.as_str() != PARAM) {
                *cont.entry(k.clone()).or_default() += 1;
            }
        }
        if let Some((shared, _)) = cont.iter().filter(|(_, c)| **c >= 3).max_by_key(|(_, c)| **c) {
            for w in &words {
                if node.children[w].children.contains_key(shared) {
                    fold.insert(w.clone());
                }
            }
        }
        // B
        let leaves: Vec<&String> = words.iter().filter(|w| node.children[*w].children.is_empty()).collect();
        for method in ["GET", "PUT", "PATCH", "DELETE", "POST"] {
            let shaped: Vec<(&String, BTreeSet<String>)> = leaves.iter().filter_map(|w| node_resp_keys(&node.children[*w], method).map(|k| (*w, k))).collect();
            if shaped.len() >= 3 {
                let mut best: Vec<&String> = Vec::new();
                for (_, k0) in &shaped {
                    let g: Vec<&String> = shaped.iter().filter(|(_, k)| resp_similarity(k0, k) >= 0.8).map(|(w, _)| *w).collect();
                    if g.len() > best.len() {
                        best = g;
                    }
                }
                if best.len() >= 3 {
                    fold.extend(best.into_iter().cloned());
                }
            }
        }
        // C
        let c_words: Vec<&String> = words
            .iter()
            .filter(|w| {
                let n = &node.children[*w];
                !n.children.is_empty() && n.hits.is_empty() && n.children.iter().all(|(k, c)| k.as_str() != PARAM && c.children.is_empty())
            })
            .collect();
        if c_words.len() >= 3 {
            let mut methods: BTreeSet<String> = BTreeSet::new();
            for w in &c_words {
                deep_methods(&node.children[*w], &mut methods);
            }
            let mut good: Vec<&String> = Vec::new();
            for w0 in &c_words {
                let g: Vec<&String> = c_words
                    .iter()
                    .filter(|w| {
                        methods.iter().all(|m| {
                            let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
                            deep_resp(&node.children[*w0], m, &mut a);
                            deep_resp(&node.children[**w], m, &mut b);
                            // a method seen under only one of the two words is no evidence either way
                            a.is_empty() || b.is_empty() || resp_similarity(&a, &b) >= 0.8
                        })
                    })
                    .copied()
                    .collect();
                if g.len() > good.len() {
                    good = g;
                }
            }
            if good.len() >= 3 {
                fold.extend(good.into_iter().cloned());
            }
        }
        if fold.len() >= 3 {
            let mut target = node.children.remove(PARAM).unwrap_or_default();
            for w in fold {
                if let Some(c) = node.children.remove(&w) {
                    target.words.insert(w);
                    merge_nodes(&mut target, c);
                }
            }
            node.children.insert(PARAM.to_string(), target);
        }
    }
    for c in node.children.values_mut() {
        collapse(c);
    }
}

struct Endpoint {
    host: String,
    method: String,
    path: String,
    hit_reqs: Vec<usize>,
}

fn walk(node: &Node, origin: &str, segs: &mut Vec<String>, names: &mut Vec<String>, out: &mut Vec<Endpoint>) {
    let mut by_method: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for h in &node.hits {
        by_method.entry(h.method.as_str()).or_default().push(h.req);
    }
    for (m, reqs) in by_method {
        out.push(Endpoint { host: origin.to_string(), method: m.to_string(), path: format!("/{}", segs.join("/")), hit_reqs: reqs });
    }
    for (k, c) in &node.children {
        if k == PARAM {
            let prev = segs.last().cloned().unwrap_or_default();
            let numeric_like = c.words.is_empty();
            let base = if prev.is_empty() || prev.starts_with('{') { "id".to_string() } else { camel(&singular(&prev)) };
            let mut name = if numeric_like && base != "id" { format!("{base}Id") } else { base };
            let mut n = 2;
            while names.contains(&name) {
                name = format!("{name}{n}");
                n += 1;
            }
            names.push(name.clone());
            segs.push(format!("{{{name}}}"));
            walk(c, origin, segs, names, out);
            segs.pop();
            names.pop();
        } else {
            segs.push(k.clone());
            walk(c, origin, segs, names, out);
            segs.pop();
        }
    }
}

fn kinds_of(node_kinds: &BTreeSet<IdKind>) -> (&'static str, Option<&'static str>) {
    if node_kinds.len() == 1 {
        match node_kinds.iter().next().unwrap() {
            IdKind::Numeric => ("integer", None),
            IdKind::Uuid => ("string", Some("uuid")),
            IdKind::Opaque => ("string", None),
        }
    } else {
        ("string", None)
    }
}

// ─── entry point ───────────────────────────────────────────────────────────

pub fn analyze_har(bytes: &[u8]) -> Result<HarResult, String> {
    if bytes.len() > MAX_BYTES {
        return Err(format!("HAR file is larger than {} MB; refusing to load it", MAX_BYTES >> 20));
    }
    let doc: Value = serde_json::from_slice(bytes).map_err(|e| format!("not valid JSON: {e}"))?;
    let entries = doc["log"]["entries"].as_array().ok_or("not a HAR file (no log.entries)")?;
    let mut res = HarResult { entries: entries.len().min(MAX_ENTRIES), ..Default::default() };
    if entries.len() > MAX_ENTRIES {
        res.warnings.push(format!("only the first {MAX_ENTRIES} of {} entries were analysed", entries.len()));
    }
    let mut reqs: Vec<Req> = Vec::new();
    let mut ws: BTreeSet<String> = BTreeSet::new();
    for e in entries.iter().take(MAX_ENTRIES) {
        match extract_req(e, &mut res.redactions, &mut res.warnings) {
            Ok(Some(r)) => reqs.push(r),
            Ok(None) => {
                if let Some((o, p, _)) = e["request"]["url"].as_str().and_then(split_url) {
                    let shape: Vec<String> = p.split('/').filter(|s| !s.is_empty()).map(|s| if id_kind(s).is_some() { "{id}".into() } else { s.to_string() }).collect();
                    ws.insert(format!("{o}/{}", shape.join("/")));
                }
            }
            Err("static") => res.skipped_static += 1,
            Err(_) => res.skipped_other += 1,
        }
    }
    res.websockets = ws.into_iter().collect();
    if !res.websockets.is_empty() {
        res.warnings.push("WebSocket traffic was found; it is listed but not modelled in the OpenAPI document.".into());
    }
    if reqs.is_empty() {
        res.warnings.push("No API-like requests were found in this capture.".into());
    }

    // tries per origin
    let mut tries: BTreeMap<String, Node> = BTreeMap::new();
    for (i, r) in reqs.iter().enumerate() {
        let mut n = tries.entry(r.origin.clone()).or_default();
        for (lit, kind) in &r.segs {
            let key = if kind.is_some() { PARAM.to_string() } else { lit.clone() };
            n = n.children.entry(key).or_default();
            if let Some(k) = kind {
                n.kinds.insert(*k);
            }
        }
        n.hits.push(Hit { method: r.method.clone(), status: r.status, resp_keys: r.resp_keys.clone(), req: i });
    }
    // root-level hits (path "/") live on the origin node itself
    let mut eps: Vec<Endpoint> = Vec::new();
    for (origin, node) in tries.iter_mut() {
        collapse(node);
        walk(node, origin, &mut Vec::new(), &mut Vec::new(), &mut eps);
    }
    for e in eps.iter_mut() {
        if e.path == "/" || e.path.is_empty() {
            e.path = "/".into();
        }
    }

    // node kinds for path-parameter typing: recompute from requests by template match is overkill -
    // derive from the endpoint's own requests instead.
    let mut paths: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    let mut schemes: BTreeMap<String, Value> = BTreeMap::new();
    let mut gql: BTreeMap<(String, String), GraphqlOp> = BTreeMap::new();
    let mut origins: BTreeSet<String> = BTreeSet::new();
    for ep in &eps {
        origins.insert(ep.host.clone());
        let rs: Vec<&Req> = ep.hit_reqs.iter().map(|i| &reqs[*i]).collect();
        let is_gql = rs.iter().any(|r| r.graphql.is_some());
        let mut summary = EndpointSummary { method: ep.method.clone(), path: ep.path.clone(), host: ep.host.clone(), requests: rs.len(), graphql: is_gql, ..Default::default() };
        let mut op = Map::new();
        // path parameters
        let mut parameters: Vec<Value> = Vec::new();
        let names: Vec<String> = ep.path.split('/').filter(|s| s.starts_with('{')).map(|s| s.trim_matches(|c| c == '{' || c == '}').to_string()).collect();
        if !names.is_empty() {
            // type from the concrete requests: all-numeric segments at that position -> integer
            let tmpl: Vec<&str> = ep.path.split('/').filter(|s| !s.is_empty()).collect();
            for (pi, name) in names.iter().enumerate() {
                let pos = tmpl.iter().enumerate().filter(|(_, s)| s.starts_with('{')).nth(pi).map(|(i, _)| i);
                let mut kinds: BTreeSet<IdKind> = BTreeSet::new();
                let mut word = false;
                if let Some(p) = pos {
                    for r in &rs {
                        if let Some((lit, k)) = r.segs.get(p) {
                            match k {
                                Some(k) => {
                                    kinds.insert(*k);
                                }
                                None => word = !lit.is_empty() || word,
                            }
                        }
                    }
                }
                let (ty, fmt) = if word { ("string", None) } else { kinds_of(&kinds) };
                let mut sch = json!({"type": ty});
                if let Some(f) = fmt {
                    sch["format"] = json!(f);
                }
                parameters.push(json!({"name": name, "in": "path", "required": true, "schema": sch}));
            }
        }
        // query parameters
        let mut qseen: BTreeMap<String, (usize, BTreeSet<&'static str>)> = BTreeMap::new();
        for r in &rs {
            let mut uniq: BTreeSet<&String> = BTreeSet::new();
            for (k, t) in &r.query {
                let e = qseen.entry(k.clone()).or_default();
                if uniq.insert(k) {
                    e.0 += 1;
                }
                e.1.insert(t);
            }
        }
        for (k, (c, ts)) in &qseen {
            let ty = if ts.len() == 1 { *ts.iter().next().unwrap() } else if ts.iter().all(|t| matches!(*t, "integer" | "number")) { "number" } else { "string" };
            parameters.push(json!({"name": k, "in": "query", "required": *c == rs.len(), "schema": {"type": ty}}));
            summary.query.push(k.clone());
        }
        let pag: Vec<String> = qseen
            .keys()
            .filter(|k| matches!(k.to_ascii_lowercase().as_str(), "page" | "per_page" | "perpage" | "page_size" | "pagesize" | "limit" | "offset" | "cursor" | "after" | "before" | "skip" | "take" | "next_page_token" | "pagetoken" | "size"))
            .cloned()
            .collect();
        if !pag.is_empty() {
            let style = if pag.iter().any(|k| matches!(k.to_ascii_lowercase().as_str(), "cursor" | "after" | "before" | "next_page_token" | "pagetoken")) {
                "cursor"
            } else if pag.iter().any(|k| matches!(k.to_ascii_lowercase().as_str(), "offset" | "skip")) {
                "offset"
            } else {
                "page"
            };
            op.insert("x-pagination".into(), json!({"style": style, "parameters": pag}));
            summary.pagination = pag;
        }
        if !parameters.is_empty() {
            op.insert("parameters".into(), Value::Array(parameters));
        }
        // request body
            let mut rb_mime: BTreeSet<String> = BTreeSet::new();
        let mut req_fields: BTreeSet<String> = BTreeSet::new();
        let mut any_body = false;
        if is_gql {
            // GraphQL: the operation catalogue replaces a body schema
            for r in &rs {
                if let Some(ops) = &r.graphql {
                    for (k, n, f) in ops {
                        let e = gql.entry((k.clone(), n.clone())).or_insert_with(|| GraphqlOp { kind: k.clone(), name: n.clone(), ..Default::default() });
                        e.count += 1;
                        for x in f {
                            if !e.fields.contains(x) {
                                e.fields.push(x.clone());
                            }
                        }
                    }
                }
            }
            op.insert(
                "requestBody".into(),
                json!({"required": true, "content": {"application/json": {"schema": {"type": "object", "required": ["query"], "properties": {"query": {"type": "string"}, "variables": {"type": "object"}, "operationName": {"type": "string"}}}}}}),
            );
        } else {
            let mut json_schemas: Vec<&Value> = Vec::new();
            for r in &rs {
                if !r.req_mime.is_empty() {
                    rb_mime.insert(r.req_mime.clone());
                    any_body = true;
                }
                if let Some(s) = &r.req_schema {
                    json_schemas.push(s);
                }
                req_fields.extend(r.req_form_fields.iter().cloned());
            }
            if any_body {
                let mut content = Map::new();
                for m in &rb_mime {
                    if m.contains("json") {
                        content.insert(m.clone(), json!({"schema": merge_json_schemas(&json_schemas)}));
                    } else if !req_fields.is_empty() && (m.contains("x-www-form-urlencoded") || m.starts_with("multipart/")) {
                        let props: Map<String, Value> = req_fields.iter().map(|f| (f.clone(), json!({"type": "string"}))).collect();
                        content.insert(m.clone(), json!({"schema": {"type": "object", "properties": props}}));
                    } else {
                        content.insert(m.clone(), json!({}));
                    }
                }
                op.insert("requestBody".into(), json!({"content": content}));
            }
        }
        summary.request_fields = req_fields.iter().cloned().collect();
        if !is_gql {
            for s in json_field_paths(&rs.iter().filter_map(|r| r.req_schema.as_ref()).collect::<Vec<_>>()) {
                if !summary.request_fields.contains(&s) {
                    summary.request_fields.push(s);
                }
            }
        }
        // responses
        let mut by_status: BTreeMap<u16, Vec<&Req>> = BTreeMap::new();
        for r in &rs {
            by_status.entry(r.status).or_default().push(r);
        }
        let mut responses = Map::new();
        let mut resp_fields: BTreeSet<String> = BTreeSet::new();
        for (st, group) in &by_status {
            let mimes: BTreeSet<&String> = group.iter().map(|r| &r.resp_mime).filter(|m| !m.is_empty()).collect();
            let schemas: Vec<&Value> = group.iter().filter_map(|r| r.resp_schema.as_ref()).collect();
            let mut o = Map::new();
            o.insert("description".into(), json!(format!("Observed response ({} request{})", group.len(), if group.len() == 1 { "" } else { "s" })));
            if !mimes.is_empty() {
                let mut content = Map::new();
                for m in mimes {
                    if m.contains("json") && !schemas.is_empty() {
                        content.insert(m.clone(), json!({"schema": merge_json_schemas(&schemas)}));
                    } else {
                        content.insert(m.clone(), json!({}));
                    }
                }
                o.insert("content".into(), Value::Object(content));
            }
            if (200..300).contains(st) {
                resp_fields.extend(json_field_paths(&schemas));
            }
            responses.insert(st.to_string(), Value::Object(o));
            summary.statuses.push(*st);
        }
        if responses.is_empty() {
            responses.insert("default".into(), json!({"description": "Observed response"}));
        }
        op.insert("responses".into(), Value::Object(responses));
        summary.response_fields = resp_fields.into_iter().collect();
        // security
        let mut sec: BTreeSet<String> = BTreeSet::new();
        for r in &rs {
            sec.extend(r.auth.iter().cloned());
        }
        let any_unauth = rs.iter().any(|r| r.auth.is_empty());
        let mut sec_reqs: Vec<Value> = Vec::new();
        for s in &sec {
            let parts: Vec<&str> = s.split(':').collect();
            let (id, def) = match parts.as_slice() {
                ["http", "bearer"] => ("bearerAuth".to_string(), json!({"type": "http", "scheme": "bearer"})),
                ["http", "basic"] => ("basicAuth".to_string(), json!({"type": "http", "scheme": "basic"})),
                ["http", other] => (format!("{other}Auth"), json!({"type": "http", "scheme": other})),
                ["apiKey", place, name] => (format!("apiKey_{place}_{}", name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()), json!({"type": "apiKey", "in": place, "name": name})),
                _ => continue,
            };
            schemes.insert(id.clone(), def);
            sec_reqs.push(json!({ id: [] }));
        }
        if !sec_reqs.is_empty() {
            if any_unauth {
                sec_reqs.push(json!({}));
            }
            op.insert("security".into(), Value::Array(sec_reqs));
        }
        summary.auth = sec.into_iter().collect();
        op.insert("summary".into(), json!(format!("{} {} (inferred from {} captured request{})", ep.method, ep.path, rs.len(), if rs.len() == 1 { "" } else { "s" })));
        if is_gql {
            op.insert("x-graphql".into(), json!(true));
        }
        paths.entry(ep.path.clone()).or_default().insert(ep.method.to_ascii_lowercase(), Value::Object(op));
        res.endpoints.push(summary);
    }
    res.graphql_ops = gql.into_values().collect();

    let mut doc = Map::new();
    doc.insert("openapi".into(), json!("3.0.3"));
    doc.insert(
        "info".into(),
        json!({
            "title": "Inferred API",
            "version": "0.0.0-inferred",
            "description": "Reconstructed from one captured session of an API the user is authorised to analyse. No example values are included; credentials and identifiers were dropped before inference. Everything here is a lead derived from observed traffic, not a specification."
        }),
    );
    doc.insert("servers".into(), Value::Array(origins.iter().map(|o| json!({"url": o})).collect()));
    let mut p = Map::new();
    for (k, v) in paths {
        p.insert(k, Value::Object(v));
    }
    doc.insert("paths".into(), Value::Object(p));
    if !schemes.is_empty() {
        doc.insert("components".into(), json!({"securitySchemes": schemes}));
    }
    if !res.graphql_ops.is_empty() {
        doc.insert("x-graphql-operations".into(), serde_json::to_value(&res.graphql_ops).unwrap_or(Value::Null));
    }
    if !res.websockets.is_empty() {
        doc.insert("x-websockets".into(), json!(res.websockets));
    }
    res.openapi_yaml = serde_yaml::to_string(&Value::Object(doc)).map_err(|e| format!("could not render OpenAPI: {e}"))?;
    Ok(res)
}

/// Merge already-built JSON schemas of the same position: union of properties, `required` = intersection.
fn merge_json_schemas(list: &[&Value]) -> Value {
    if list.is_empty() {
        return json!({});
    }
    let mut acc = list[0].clone();
    for s in &list[1..] {
        acc = merge2(&acc, s);
    }
    acc
}

fn merge2(a: &Value, b: &Value) -> Value {
    let (Some(ao), Some(bo)) = (a.as_object(), b.as_object()) else { return a.clone() };
    let mut out = ao.clone();
    // type
    if ao.get("type") != bo.get("type") {
        let ta = ao.get("type").and_then(|t| t.as_str());
        let tb = bo.get("type").and_then(|t| t.as_str());
        match (ta, tb) {
            (Some("integer"), Some("number")) | (Some("number"), Some("integer")) => {
                out.insert("type".into(), json!("number"));
            }
            (Some(x), Some(y)) => {
                out.remove("type");
                out.insert("oneOf".into(), json!([{"type": x}, {"type": y}]));
            }
            (None, Some(_)) => {
                if let Some(t) = bo.get("type") {
                    out.insert("type".into(), t.clone());
                }
            }
            _ => {}
        }
    }
    if bo.get("nullable").is_some() {
        out.insert("nullable".into(), json!(true));
    }
    if ao.get("format") != bo.get("format") {
        out.remove("format");
    }
    // properties
    if let (Some(pa), Some(pb)) = (ao.get("properties").and_then(|p| p.as_object()), bo.get("properties").and_then(|p| p.as_object())) {
        let mut props = pa.clone();
        for (k, v) in pb {
            let merged = match props.get(k) {
                Some(x) => merge2(x, v),
                None => v.clone(),
            };
            props.insert(k.clone(), merged);
        }
        out.insert("properties".into(), Value::Object(props));
        let ra: BTreeSet<&str> = ao.get("required").and_then(|r| r.as_array()).map(|r| r.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        let rb: BTreeSet<&str> = bo.get("required").and_then(|r| r.as_array()).map(|r| r.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        let both: Vec<Value> = ra.intersection(&rb).map(|s| json!(s)).collect();
        if both.is_empty() {
            out.remove("required");
        } else {
            out.insert("required".into(), Value::Array(both));
        }
    } else if bo.get("properties").is_some() {
        out.insert("properties".into(), bo["properties"].clone());
        if let Some(r) = bo.get("required") {
            out.insert("required".into(), r.clone());
        }
    }
    if let (Some(ia), Some(ib)) = (ao.get("items"), bo.get("items")) {
        out.insert("items".into(), merge2(ia, ib));
    } else if let Some(i) = bo.get("items") {
        out.insert("items".into(), i.clone());
    }
    Value::Object(out)
}

fn json_field_paths(schemas: &[&Value]) -> BTreeSet<String> {
    fn rec(s: &Value, prefix: &str, out: &mut BTreeSet<String>) {
        if let Some(p) = s.get("properties").and_then(|p| p.as_object()) {
            for (k, v) in p {
                let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                out.insert(path.clone());
                rec(v, &path, out);
            }
        }
        if let Some(i) = s.get("items") {
            rec(i, &format!("{prefix}[]"), out);
        }
    }
    let mut out = BTreeSet::new();
    for s in schemas {
        rec(s, "", &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(method: &str, url: &str, status: u16, req_headers: &[(&str, &str)], body: Option<&str>, resp: Option<&str>) -> Value {
        let hs: Vec<Value> = req_headers.iter().map(|(n, v)| json!({"name": n, "value": v})).collect();
        let mut rq = json!({"method": method, "url": url, "headers": hs});
        if let Some(b) = body {
            rq["postData"] = json!({"mimeType": "application/json", "text": b});
        }
        json!({"request": rq, "response": {"status": status, "headers": [], "content": {"mimeType": if resp.is_some() {"application/json"} else {""}, "text": resp.unwrap_or("")}}})
    }

    fn har(entries: Vec<Value>) -> Vec<u8> {
        serde_json::to_vec(&json!({"log": {"version": "1.2", "entries": entries}})).unwrap()
    }

    #[test]
    fn numeric_and_uuid_segments_become_parameters_after_one_request() {
        let h = har(vec![
            entry("GET", "https://api.test/v1/users/42", 200, &[], None, Some(r#"{"id":42,"name":"a"}"#)),
            entry("GET", "https://api.test/v1/orders/3f2504e0-4f89-11d3-9a0c-0305e82c3301/items", 200, &[], None, Some("[]")),
        ]);
        let r = analyze_har(&h).unwrap();
        let ps: Vec<&str> = r.endpoints.iter().map(|e| e.path.as_str()).collect();
        assert!(ps.contains(&"/v1/users/{userId}"), "{ps:?}");
        assert!(ps.contains(&"/v1/orders/{orderId}/items"), "{ps:?}");
        assert!(r.openapi_yaml.contains("openapi: 3.0.3"));
        assert!(r.openapi_yaml.contains("format: uuid"));
    }

    #[test]
    fn word_params_need_three_values_and_a_structural_signal() {
        let mk = |org: &str| entry("GET", &format!("https://api.test/orgs/{org}/repos"), 200, &[], None, Some("[]"));
        let r = analyze_har(&har(vec![mk("acme"), mk("globex"), mk("initech")])).unwrap();
        assert!(r.endpoints.iter().any(|e| e.path == "/orgs/{org}/repos"), "{:?}", r.endpoints.iter().map(|e| &e.path).collect::<Vec<_>>());
        // only two values: stays literal (we do not guess)
        let r = analyze_har(&har(vec![mk("acme"), mk("globex")])).unwrap();
        assert!(r.endpoints.iter().any(|e| e.path == "/orgs/acme/repos"));
    }

    #[test]
    fn distinct_resources_are_not_collapsed_into_a_parameter() {
        let r = analyze_har(&har(vec![
            entry("GET", "https://api.test/api/users", 200, &[], None, Some(r#"[{"id":1,"name":"x"}]"#)),
            entry("GET", "https://api.test/api/orders", 200, &[], None, Some(r#"[{"total":3}]"#)),
            entry("GET", "https://api.test/api/items", 200, &[], None, Some(r#"[{"sku":"a"}]"#)),
            entry("GET", "https://api.test/api/carts", 200, &[], None, Some(r#"[{"lines":[]}]"#)),
        ]))
        .unwrap();
        let ps: Vec<&str> = r.endpoints.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(ps.len(), 4, "{ps:?}");
        assert!(ps.iter().all(|p| !p.contains('{')), "{ps:?}");
    }

    #[test]
    fn same_shaped_leaf_responses_collapse_slugs() {
        let mk = |slug: &str| entry("GET", &format!("https://api.test/products/{slug}"), 200, &[], None, Some(&format!(r#"{{"slug":"{slug}","price":3,"name":"n"}}"#)));
        let r = analyze_har(&har(vec![mk("blue-widget"), mk("red-gadget"), mk("green-gizmo")])).unwrap();
        assert!(r.endpoints.iter().any(|e| e.path == "/products/{product}"), "{:?}", r.endpoints.iter().map(|e| &e.path).collect::<Vec<_>>());
    }

    #[test]
    fn secrets_never_reach_the_output() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyMSIsInJvbGUiOiJhZG1pbiJ9.c2lnbmF0dXJlc2lnbmF0dXJl";
        // built at runtime so no secret-shaped literal exists in the source (GitHub push protection)
        let key_owned = format!("sk_{}_{}", "live", "51HqXyZAbCdEfGhIjKlMnOpQr");
        let key = key_owned.as_str();
        let pw = "hunter2-correct-horse";
        let cookie = "SESSIONSECRETVALUE123456";
        let h = har(vec![entry(
            "POST",
            &format!("https://user:p4ssw0rd@api.test/login?api_key={key}&q=1"),
            200,
            &[("Authorization", &format!("Bearer {jwt}")), ("Cookie", &format!("sid={cookie}; theme=dark")), ("X-Api-Key", key)],
            Some(&format!(r#"{{"email":"a@b.c","password":"{pw}","remember":true}}"#)),
            Some(&format!(r#"{{"access_token":"{jwt}","refresh_token":"{key}","user":{{"id":1}}}}"#)),
        )]);
        let r = analyze_har(&h).unwrap();
        let all = format!("{}{}", r.openapi_yaml, serde_json::to_string(&r).unwrap());
        for needle in [jwt, key, pw, cookie, "p4ssw0rd", "a@b.c", "hunter2"] {
            assert!(!all.contains(needle), "secret leaked: {needle}");
        }
        assert!(r.redactions >= 5, "redactions={}", r.redactions);
        assert!(r.openapi_yaml.contains("bearerAuth") && r.openapi_yaml.contains("scheme: bearer"));
        assert!(r.openapi_yaml.contains("password"), "field NAMES are kept");
    }

    #[test]
    fn graphql_is_one_endpoint_plus_operation_catalogue() {
        let q = |body: &str| entry("POST", "https://api.test/graphql", 200, &[], Some(body), Some(r#"{"data":{}}"#));
        let r = analyze_har(&har(vec![
            q(r#"{"query":"query GetUser($id: ID!) { user(id: $id) { id name } viewer { login } }","variables":{"id":"1"}}"#),
            q(r#"{"query":"mutation CreatePost($t: String!) { createPost(title: $t) { id } }"}"#),
            q(r#"{"query":"query GetUser($id: ID!) { user(id: $id) { id } }"}"#),
        ]))
        .unwrap();
        assert_eq!(r.endpoints.iter().filter(|e| e.path == "/graphql").count(), 1);
        let names: Vec<(&str, &str)> = r.graphql_ops.iter().map(|o| (o.kind.as_str(), o.name.as_str())).collect();
        assert!(names.contains(&("query", "GetUser")) && names.contains(&("mutation", "CreatePost")), "{names:?}");
        let gu = r.graphql_ops.iter().find(|o| o.name == "GetUser").unwrap();
        assert_eq!(gu.count, 2);
        assert!(gu.fields.contains(&"user".to_string()) && gu.fields.contains(&"viewer".to_string()));
        assert!(!r.openapi_yaml.contains("variables: {\"id\""));
    }

    #[test]
    fn static_assets_websockets_and_junk_are_handled() {
        let mut ws = entry("GET", "wss://api.test/socket/12345678", 101, &[], None, None);
        ws["_webSocketMessages"] = json!([{"type": "send", "data": "hi"}]);
        let r = analyze_har(&har(vec![
            entry("GET", "https://api.test/app.js", 200, &[], None, None),
            entry("GET", "https://api.test/logo.png", 200, &[], None, None),
            ws,
            entry("GET", "https://api.test/ping", 200, &[], None, Some(r#"{"ok":true}"#)),
        ]))
        .unwrap();
        assert_eq!(r.skipped_static, 2);
        assert_eq!(r.websockets.len(), 1);
        assert!(r.websockets[0].contains("{id}"));
        assert_eq!(r.endpoints.len(), 1);
        assert!(!r.warnings.is_empty());
    }

    #[test]
    fn query_params_pagination_and_required_tracking() {
        let r = analyze_har(&har(vec![
            entry("GET", "https://api.test/items?page=1&limit=20&q=a", 200, &[], None, Some("[]")),
            entry("GET", "https://api.test/items?page=2&limit=20", 200, &[], None, Some("[]")),
        ]))
        .unwrap();
        let e = &r.endpoints[0];
        assert_eq!(e.pagination, vec!["limit".to_string(), "page".to_string()]);
        assert!(r.openapi_yaml.contains("x-pagination"));
        let doc: Value = serde_yaml::from_str(&r.openapi_yaml).unwrap();
        let params = doc["paths"]["/items"]["get"]["parameters"].as_array().unwrap();
        let q = params.iter().find(|p| p["name"] == "q").unwrap();
        let page = params.iter().find(|p| p["name"] == "page").unwrap();
        assert_eq!(q["required"], false);
        assert_eq!(page["required"], true);
        assert_eq!(page["schema"]["type"], "integer");
    }

    #[test]
    fn schemas_merge_required_is_intersection_and_nullable_is_tracked() {
        let r = analyze_har(&har(vec![
            entry("GET", "https://api.test/me", 200, &[], None, Some(r#"{"id":1,"name":"a","bio":null}"#)),
            entry("GET", "https://api.test/me", 200, &[], None, Some(r#"{"id":2,"name":"b","bio":"x","extra":true}"#)),
        ]))
        .unwrap();
        let doc: Value = serde_yaml::from_str(&r.openapi_yaml).unwrap();
        let s = &doc["paths"]["/me"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
        assert_eq!(s["properties"]["bio"]["nullable"], true);
        let req: Vec<&str> = s["required"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert!(req.contains(&"id") && req.contains(&"name") && !req.contains(&"extra"), "{req:?}");
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(analyze_har(b"not json").is_err());
        assert!(analyze_har(br#"{"x":1}"#).is_err());
        assert!(analyze_har(br#"{"log":{"entries":[{"request":{"url":123}}, {}, null]}}"#).is_ok());
        let r = analyze_har(&har(vec![])).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("No API-like")));
    }

    #[test]
    fn output_is_valid_yaml_with_expected_top_level_keys() {
        let r = analyze_har(&har(vec![entry("GET", "https://api.test/a", 200, &[("Authorization", "Basic Zm9vOmJhcg==")], None, Some("{}"))])).unwrap();
        let doc: Value = serde_yaml::from_str(&r.openapi_yaml).unwrap();
        for k in ["openapi", "info", "servers", "paths", "components"] {
            assert!(doc.get(k).is_some(), "missing {k}");
        }
        assert_eq!(doc["components"]["securitySchemes"]["basicAuth"]["scheme"], "basic");
    }
}
