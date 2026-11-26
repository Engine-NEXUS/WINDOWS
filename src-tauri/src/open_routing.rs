//! Keyless routing and place lookup from open services, so "directions to X" and "how far is X" give a real distance,
//! a real travel time for the chosen mode and turn-by-turn steps without a Google key.
//!
//! - Places: Nominatim (OpenStreetMap), one result, cached, never more than one request a second.
//! - Routes: OSRM. `routing.openstreetmap.de` (FOSSGIS) serves car, bike and foot profiles; the plain OSRM demo
//!   (`router.project-osrm.org`) serves the car profile only, whatever the URL says, so it is used for driving only.
//!   Neither avoids tolls, highways or ferries, and neither knows live traffic: the answer says so.
//!
//! Usage policies (kept in code, not just in a document): a User-Agent that names the application, at most one request a
//! second per service, results cached, a pause after HTTP 429/503, the base addresses can be changed in settings
//! (`openGeocoderUrl`, `openRoutingUrl`) without a new release, and the data is shown with its attribution. Sources and
//! licences: docs/research/directions-and-reminders/02-open-source-routing-and-location-research.md.
//!
//! Privacy: nothing about the user's own address goes here. `plan` refuses to send a place name to the geocoder unless the
//! caller says lookups are allowed, and the caller says no for home, work and "my X". Coordinates are never logged.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::google::maps::RouteOpts;
use crate::google::types::TravelMode;
use crate::place_card::Found;

pub const DEFAULT_GEOCODER: &str = "https://nominatim.openstreetmap.org";
pub const DEFAULT_ROUTER: &str = "https://routing.openstreetmap.de";
pub const ATTRIBUTION: &str = "Route data © OpenStreetMap contributors; routing by FOSSGIS (routing.openstreetmap.de).";

/// The largest answer read from either service.
const MAX_BODY: usize = 1_500_000;
const MAX_STEPS: usize = 150;
const MAX_WAYPOINTS: usize = 7;
const CACHE_CAP: usize = 64;
const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);
const MISS_TTL: Duration = Duration::from_secs(10 * 60);
const BLOCK_DEFAULT: Duration = Duration::from_secs(60);
const BLOCK_MAX: Duration = Duration::from_secs(600);

static MIN_GAP_MS: AtomicU64 = AtomicU64::new(1100);

/// The two services this module talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Geocoder = 0,
    Router = 1,
}

impl Provider {
    fn name(self) -> &'static str {
        match self {
            Provider::Geocoder => "place lookup",
            Provider::Router => "route service",
        }
    }
}

/// Where the two services live (settings can move them; tests point them at a local mock).
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoints {
    pub geocoder: String,
    pub router: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints { geocoder: DEFAULT_GEOCODER.into(), router: DEFAULT_ROUTER.into() }
    }
}

static CONFIGURED: Mutex<Option<Endpoints>> = Mutex::new(None);

/// The addresses in use now.
pub fn endpoints() -> Endpoints {
    CONFIGURED.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Read `openGeocoderUrl` / `openRoutingUrl` from settings.json (empty = the public default). Only `https://` addresses, or
/// `http://` for this computer or a private network, are accepted; anything else is ignored.
pub fn configure<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let j = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());
    let get = |k: &str| j.as_ref().and_then(|j| j.get(k)).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).unwrap_or_default();
    let ep = endpoints_from(&get("openGeocoderUrl"), &get("openRoutingUrl"));
    *CONFIGURED.lock().unwrap_or_else(|e| e.into_inner()) = Some(ep);
}

/// The settings values -> endpoints, falling back to the defaults for an empty or unsafe value. Pure.
pub fn endpoints_from(geocoder: &str, router: &str) -> Endpoints {
    let pick = |v: &str, d: &str| if v.trim().is_empty() { d.to_string() } else { crate::place_card::server_url_ok(v).unwrap_or_else(|| d.to_string()) };
    Endpoints { geocoder: pick(geocoder, DEFAULT_GEOCODER), router: pick(router, DEFAULT_ROUTER) }
}

// ── errors ───────────────────────────────────────────────────────────────────

/// Why an open lookup gave nothing. Carries no place names or coordinates, so it is safe to log.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenError {
    /// The service could not be reached.
    Offline,
    /// It did not answer in time.
    Timeout,
    /// HTTP 429 / 503 / 403, or still paused after one.
    RateLimited,
    /// A place name would have to be sent, and the caller said it must not be (home, work, "my X").
    Refused,
    /// HTTP 5xx.
    Server(u16),
    /// The answer was not what the service documents.
    Bad,
    /// The place was not found.
    NotFound,
    /// The points cannot be joined by a road, path or cycleway for this mode.
    NoRoute,
    /// This mode cannot be done by the open services (public transport).
    Unsupported,
    /// The answer was larger than the cap.
    TooLarge,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Server(c) => write!(f, "server error {c}"),
            other => write!(f, "{}", format!("{other:?}").to_lowercase()),
        }
    }
}

impl OpenError {
    /// One sentence for the user, saying what failed. `place` is the spoken place for a not-found answer. Pure.
    pub fn speak(&self, mode: TravelMode, place: &str) -> String {
        let how = crate::directions::mode_name(mode);
        match self {
            OpenError::Offline => "I couldn't reach the open map services, sir. Check the internet connection.".into(),
            OpenError::Timeout => "The open map services took too long to answer, sir.".into(),
            OpenError::RateLimited => "The open map services are busy, sir; I'll leave them alone for a minute.".into(),
            OpenError::Refused => String::new(),
            OpenError::Server(_) => "The open map services had an error, sir.".into(),
            OpenError::Bad | OpenError::TooLarge => "The open map services gave an answer I couldn't use, sir.".into(),
            OpenError::NotFound => format!("I couldn't find {place} on the open map, sir. Say its full name and the city."),
            OpenError::NoRoute => format!("The open map has no {how} route between those two places, sir."),
            OpenError::Unsupported => "Public transport times need a Maps key, sir. The map shows them.".into(),
        }
    }
}

// ── pure helpers ─────────────────────────────────────────────────────────────

fn enc(s: &str) -> String {
    crate::place_card::enc(s)
}

/// `"17.36,78.47"` -> a point, when the whole text is two valid numbers. Pure.
pub fn parse_point(text: &str) -> Option<(f64, f64)> {
    let (a, b) = text.trim().split_once(',')?;
    let (lat, lon) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
    crate::location::valid(lat, lon).then_some((lat, lon))
}

/// Whether a spoken place is the user's own (home, work, "my gym"). Such a name is never sent to a geocoder. Pure.
pub fn spoken_is_private(spoken: &str) -> bool {
    let s = spoken.trim().to_lowercase();
    s.starts_with("my ") || crate::orchestrator::saved_place_kind(&s).is_some()
}

/// The path (after the base address) for a mode, or `None` when the service cannot do it honestly. Pure.
///
/// - `routing.openstreetmap.de` has one server per profile: `routed-car`, `routed-bike`, `routed-foot`.
/// - The OSRM demo `router.project-osrm.org` answers every profile with the car profile, so only driving is allowed.
/// - Any other address is taken to be your own OSRM server with the standard profile names.
pub fn profile_path(base: &str, mode: TravelMode) -> Option<String> {
    let host = tauri::Url::parse(base).ok()?.host_str()?.to_lowercase();
    if host == "routing.openstreetmap.de" {
        let p = match mode {
            TravelMode::Driving => "routed-car",
            TravelMode::Bicycling => "routed-bike",
            TravelMode::Walking => "routed-foot",
            TravelMode::Transit => return None,
        };
        return Some(format!("{p}/route/v1/driving"));
    }
    if host == "router.project-osrm.org" {
        return (mode == TravelMode::Driving).then(|| "route/v1/driving".to_string());
    }
    let p = match mode {
        TravelMode::Driving => "driving",
        TravelMode::Bicycling => "cycling",
        TravelMode::Walking => "walking",
        TravelMode::Transit => return None,
    };
    Some(format!("route/v1/{p}"))
}

/// The geocoder request address. Every byte of the query is percent-encoded. Pure.
pub fn geocode_url(base: &str, query: &str) -> String {
    format!("{}/search?q={}&format=jsonv2&limit=1&accept-language=en&extratags=1", base.trim_end_matches('/'), enc(query.trim()))
}

/// The route request address for points `(lat, lon)` in order. Pure.
pub fn route_url(base: &str, mode: TravelMode, points: &[(f64, f64)]) -> Option<String> {
    let path = profile_path(base, mode)?;
    let coords = points.iter().map(|(la, lo)| format!("{lo:.6},{la:.6}")).collect::<Vec<_>>().join(";");
    Some(format!("{}/{path}/{coords}?overview=false&steps=true&alternatives=false&generate_hints=false", base.trim_end_matches('/')))
}

/// A route from the open service.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenRoute {
    pub meters: u64,
    pub minutes: u32,
    /// The main road(s), for "via ...". Driving only.
    pub via: Option<String>,
    pub steps: Vec<String>,
    /// Avoid options that this service cannot honour ("tolls").
    pub ignored: Vec<&'static str>,
}

fn compass(bearing: f64) -> &'static str {
    const W: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    W[(((bearing % 360.0 + 360.0) % 360.0 + 22.5) / 45.0) as usize % 8]
}

fn ordinal(n: u64) -> String {
    match n {
        1 => "first".into(),
        2 => "second".into(),
        3 => "third".into(),
        4 => "fourth".into(),
        5 => "fifth".into(),
        6 => "sixth".into(),
        n => format!("{n}th"),
    }
}

/// One OSRM step as a spoken instruction. Pure.
pub fn step_text(kind: &str, modifier: Option<&str>, name: &str, reference: &str, bearing_after: Option<f64>, exit: Option<u64>, distance_m: u64, last_of_leg: bool, stop_no: Option<usize>) -> String {
    let road = if !name.trim().is_empty() {
        name.trim().to_string()
    } else if !reference.trim().is_empty() {
        reference.trim().to_string()
    } else {
        String::new()
    };
    let onto = if road.is_empty() { String::new() } else { format!(" onto {road}") };
    let on = if road.is_empty() { String::new() } else { format!(" on {road}") };
    let m = modifier.unwrap_or("").trim();
    let turn = match m {
        "left" => "Turn left".to_string(),
        "right" => "Turn right".to_string(),
        "slight left" => "Bear left".to_string(),
        "slight right" => "Bear right".to_string(),
        "sharp left" => "Turn sharp left".to_string(),
        "sharp right" => "Turn sharp right".to_string(),
        "uturn" => "Make a U-turn".to_string(),
        _ => "Continue straight".to_string(),
    };
    let action = match kind {
        "depart" => match bearing_after {
            Some(b) => format!("Head {}{on}", compass(b)),
            None => format!("Start{on}"),
        },
        "arrive" => {
            let side = match m {
                "left" => ", on your left",
                "right" => ", on your right",
                _ => "",
            };
            return match (last_of_leg, stop_no) {
                (true, Some(n)) => format!("Reach stop {n}{side}"),
                _ => format!("Arrive at your destination{side}"),
            };
        }
        "turn" => format!("{turn}{onto}"),
        "end of road" => format!("At the end of the road {}{onto}", turn.to_lowercase()),
        "new name" => format!("Continue{onto}"),
        "continue" => if m == "straight" || m.is_empty() { format!("Continue straight{on}") } else { format!("{turn}{onto}") },
        "merge" => format!("Merge{}{onto}", if m.is_empty() { String::new() } else { format!(" {m}") }),
        "on ramp" => format!("Take the ramp{}{onto}", if m.is_empty() { String::new() } else { format!(" on the {m}") }),
        "off ramp" => format!("Take the exit{}{onto}", if m.is_empty() { String::new() } else { format!(" on the {m}") }),
        "fork" => format!("Keep {}{onto}", if m.contains("left") { "left" } else if m.contains("right") { "right" } else { "straight" }),
        "roundabout" | "rotary" => match exit {
            Some(n) => format!("At the roundabout take the {} exit{onto}", ordinal(n)),
            None => format!("Enter the roundabout{onto}"),
        },
        "roundabout turn" => format!("At the roundabout {}{onto}", turn.to_lowercase()),
        "exit roundabout" | "exit rotary" => format!("Leave the roundabout{onto}"),
        _ => format!("Continue{on}"),
    };
    if distance_m >= 30 {
        format!("{action}, then {} to the next turn", crate::directions::km_phrase(distance_m))
    } else {
        action
    }
}

/// OSRM route answer -> the route, or what was wrong. Pure.
pub fn parse_route(body: &str, mode: TravelMode) -> Result<OpenRoute, OpenError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| OpenError::Bad)?;
    match v.get("code").and_then(|c| c.as_str()) {
        Some("Ok") => {}
        Some("NoRoute") | Some("NoSegment") => return Err(OpenError::NoRoute),
        _ => return Err(OpenError::Bad),
    }
    let r = v.get("routes").and_then(|r| r.get(0)).ok_or(OpenError::Bad)?;
    let meters = r.get("distance").and_then(|d| d.as_f64()).filter(|d| d.is_finite() && *d > 0.0).ok_or(OpenError::Bad)?;
    let secs = r.get("duration").and_then(|d| d.as_f64()).filter(|d| d.is_finite() && *d >= 0.0).ok_or(OpenError::Bad)?;
    let legs = r.get("legs").and_then(|l| l.as_array()).ok_or(OpenError::Bad)?;
    let mut steps = vec![];
    for (li, leg) in legs.iter().enumerate() {
        let last_leg = li + 1 == legs.len();
        let Some(ls) = leg.get("steps").and_then(|s| s.as_array()) else { continue };
        for s in ls {
            let man = s.get("maneuver");
            let kind = man.and_then(|m| m.get("type")).and_then(|t| t.as_str()).unwrap_or("");
            let dist = s.get("distance").and_then(|d| d.as_f64()).unwrap_or(0.0).max(0.0) as u64;
            if dist == 0 && kind != "arrive" && kind != "depart" {
                continue;
            }
            let text = step_text(
                kind,
                man.and_then(|m| m.get("modifier")).and_then(|t| t.as_str()),
                s.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                s.get("ref").and_then(|n| n.as_str()).unwrap_or(""),
                man.and_then(|m| m.get("bearing_after")).and_then(|b| b.as_f64()),
                man.and_then(|m| m.get("exit")).and_then(|e| e.as_u64()),
                dist,
                !last_leg,
                Some(li + 1),
            );
            steps.push(text);
            if steps.len() >= MAX_STEPS {
                break;
            }
        }
    }
    let via = (mode == TravelMode::Driving)
        .then(|| {
            legs.iter()
                .filter_map(|l| l.get("summary").and_then(|s| s.as_str()))
                .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
                .find(|s| !s.is_empty())
        })
        .flatten();
    Ok(OpenRoute { meters: meters.round() as u64, minutes: ((secs / 60.0).round() as u32).max(1), via, steps, ignored: vec![] })
}

// ── rate limit, pause after a refusal, geocode cache ─────────────────────────

static LAST_AT: Mutex<[Option<Instant>; 2]> = Mutex::new([None, None]);
static BLOCKED_UNTIL: Mutex<[Option<Instant>; 2]> = Mutex::new([None, None]);
static CACHE: Mutex<VecDeque<(String, Instant, Option<Found>)>> = Mutex::new(VecDeque::new());
/// One place lookup at a time: two callers asking for the same place wait, and the second one is a cache hit.
static GEO_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// How long the service is still paused after a refusal, if it is. Pure given the clock.
pub fn paused_for(p: Provider) -> Option<Duration> {
    lock(&BLOCKED_UNTIL)[p as usize].and_then(|t| t.checked_duration_since(Instant::now())).filter(|d| !d.is_zero())
}

fn pause(p: Provider, d: Duration) {
    lock(&BLOCKED_UNTIL)[p as usize] = Some(Instant::now() + d.min(BLOCK_MAX));
    tracing::info!("open routing: {} refused or busy; pausing it for {} s", p.name(), d.min(BLOCK_MAX).as_secs());
}

/// Wait until the service's one-request-a-second gap has passed, and claim the slot.
async fn throttle(p: Provider) {
    let gap = Duration::from_millis(MIN_GAP_MS.load(Ordering::Relaxed));
    let wait = {
        let mut g = lock(&LAST_AT);
        let now = Instant::now();
        let at = match g[p as usize] {
            Some(prev) if prev + gap > now => prev + gap,
            _ => now,
        };
        g[p as usize] = Some(at);
        at.saturating_duration_since(now)
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

fn client(total_secs: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(crate::place_card::USER_AGENT)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(total_secs))
        .build()
        .unwrap_or_default()
}

/// One GET: paused services are not called, the gap is kept, the body is read up to the cap. 2xx and 400 (OSRM's way of
/// saying "no route") return the body; everything else is an error.
async fn get(p: Provider, url: &str, total_secs: u64) -> Result<String, OpenError> {
    if paused_for(p).is_some() {
        return Err(OpenError::RateLimited);
    }
    throttle(p).await;
    let mut resp = client(total_secs).get(url).send().await.map_err(|e| {
        tracing::debug!("open routing: {} request failed (timeout: {})", p.name(), e.is_timeout());
        if e.is_timeout() { OpenError::Timeout } else { OpenError::Offline }
    })?;
    let status = resp.status();
    match status.as_u16() {
        429 | 503 | 403 => {
            let secs = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok());
            pause(p, secs.map(Duration::from_secs).unwrap_or(if status.as_u16() == 403 { BLOCK_MAX } else { BLOCK_DEFAULT }));
            return Err(OpenError::RateLimited);
        }
        s if s >= 500 => return Err(OpenError::Server(s)),
        s if !(200..300).contains(&s) && s != 400 => return Err(OpenError::Bad),
        _ => {}
    }
    if resp.content_length().is_some_and(|n| n as usize > MAX_BODY) {
        return Err(OpenError::TooLarge);
    }
    let mut bytes: Vec<u8> = vec![];
    loop {
        match resp.chunk().await {
            Ok(Some(c)) => {
                if bytes.len() + c.len() > MAX_BODY {
                    return Err(OpenError::TooLarge);
                }
                bytes.extend_from_slice(&c);
            }
            Ok(None) => break,
            Err(e) => return Err(if e.is_timeout() { OpenError::Timeout } else { OpenError::Offline }),
        }
    }
    String::from_utf8(bytes).map_err(|_| OpenError::Bad)
}

fn cache_key(q: &str) -> String {
    q.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn cache_get(key: &str) -> Option<Option<Found>> {
    let mut c = lock(&CACHE);
    while c.front().is_some_and(|(_, at, f)| at.elapsed() > if f.is_some() { CACHE_TTL } else { MISS_TTL }) {
        c.pop_front();
    }
    c.iter().find(|(k, at, f)| k == key && at.elapsed() <= if f.is_some() { CACHE_TTL } else { MISS_TTL }).map(|(_, _, f)| f.clone())
}

fn cache_put(key: String, found: Option<Found>) {
    let mut c = lock(&CACHE);
    c.retain(|(k, _, _)| *k != key);
    c.push_back((key, Instant::now(), found));
    while c.len() > CACHE_CAP {
        c.pop_front();
    }
}

/// Find one place by name. Cached for a day (a place that was not found, ten minutes), one lookup at a time, at most one
/// request a second. Never call this with the user's own address; `plan` and the callers in `place_card` keep it away.
pub async fn geocode(ep: &Endpoints, query: &str) -> Result<Found, OpenError> {
    let q = query.trim();
    if q.is_empty() {
        return Err(OpenError::NotFound);
    }
    let key = cache_key(q);
    let _gate = GEO_GATE.lock().await;
    if let Some(hit) = cache_get(&key) {
        return hit.ok_or(OpenError::NotFound);
    }
    let body = get(Provider::Geocoder, &geocode_url(&ep.geocoder, q), 6).await?;
    let found = crate::place_card::parse_nominatim(&body);
    if found.is_none() && !(body.trim_start().starts_with('[') || body.trim_start().starts_with('{')) {
        return Err(OpenError::Bad);
    }
    cache_put(key, found.clone());
    found.ok_or(OpenError::NotFound)
}

/// One route through `points` (`lat, lon`), already resolved.
pub async fn route(ep: &Endpoints, mode: TravelMode, points: &[(f64, f64)]) -> Result<OpenRoute, OpenError> {
    if points.len() < 2 || points.len() > MAX_WAYPOINTS {
        return Err(OpenError::Bad);
    }
    let url = route_url(&ep.router, mode, points).ok_or(OpenError::Unsupported)?;
    let body = get(Provider::Router, &url, 8).await?;
    parse_route(&body, mode)
}

/// What to route.
pub struct PlanRequest<'a> {
    /// A "lat,lon" point or a spoken place.
    pub origin: &'a str,
    pub destination: &'a str,
    pub stops: &'a [String],
    pub mode: TravelMode,
    pub opts: &'a RouteOpts,
    /// False when any of the places is the user's own: nothing is sent to the geocoder, and a name that would have to be is
    /// refused.
    pub lookups_allowed: bool,
}

async fn resolve(ep: &Endpoints, text: &str, allowed: bool) -> Result<(f64, f64), OpenError> {
    if let Some(p) = parse_point(text) {
        return Ok(p);
    }
    if !allowed || spoken_is_private(text) {
        return Err(OpenError::Refused);
    }
    let f = geocode(ep, text).await?;
    Ok((f.lat, f.lon))
}

/// Resolve the places, then route from the origin through the stops to the destination.
pub async fn plan(ep: &Endpoints, req: &PlanRequest<'_>) -> Result<OpenRoute, OpenError> {
    if req.mode == TravelMode::Transit {
        return Err(OpenError::Unsupported);
    }
    // Refuse before any request is made: one private name anywhere means the geocoder is never called.
    let names: Vec<&str> = std::iter::once(req.origin).chain(req.stops.iter().map(String::as_str)).chain(std::iter::once(req.destination)).collect();
    if names.iter().any(|n| parse_point(n).is_none() && (!req.lookups_allowed || spoken_is_private(n))) {
        return Err(OpenError::Refused);
    }
    let mut points = vec![];
    for n in names {
        points.push(resolve(ep, n, req.lookups_allowed).await?);
    }
    let mut r = route(ep, req.mode, &points).await?;
    if req.mode == TravelMode::Driving {
        r.ignored = req.opts.avoid_words();
    }
    Ok(r)
}

/// The sentence about avoid options this service cannot do. Pure.
pub fn ignored_sentence(ignored: &[&str]) -> Option<String> {
    if ignored.is_empty() {
        return None;
    }
    let list = match ignored {
        [one] => (*one).to_string(),
        [a, b] => format!("{a} and {b}"),
        many => format!("{} and {}", many[..many.len() - 1].join(", "), many[many.len() - 1]),
    };
    Some(format!("The open route can't avoid {list}, sir, so this one ignores that; the map shows the choice."))
}

#[cfg(test)]
pub(crate) fn set_min_gap_ms(ms: u64) {
    MIN_GAP_MS.store(ms, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    *lock(&LAST_AT) = [None, None];
    *lock(&BLOCKED_UNTIL) = [None, None];
    lock(&CACHE).clear();
    MIN_GAP_MS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) static TEST_GUARD: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memcore::test_http::start_mock_server;
    use std::sync::atomic::{AtomicBool, Ordering as O};
    use std::sync::Arc;

    const NOMINATIM_OK: &str = r#"[{"lat":"17.3616","lon":"78.4747","display_name":"Charminar, Char Kaman, Hyderabad","name":"Charminar","type":"attraction"}]"#;
    const OSRM_OK: &str = r#"{"code":"Ok","routes":[{"distance":5159.7,"duration":458.8,"legs":[{"summary":"Charminar Road, Darul Shifa Road","steps":[
      {"distance":182.3,"name":"Charminar Road","maneuver":{"type":"depart","bearing_after":101,"modifier":"right"}},
      {"distance":244.3,"name":"","ref":"NH765","maneuver":{"type":"turn","modifier":"left"}},
      {"distance":0.0,"name":"","maneuver":{"type":"new name","modifier":"straight"}},
      {"distance":120.0,"name":"Ring Road","maneuver":{"type":"roundabout","exit":2}},
      {"distance":0.0,"name":"Main Road","maneuver":{"type":"arrive","modifier":"left"}}]}]}],"waypoints":[]}"#;

    fn ep(base: &str) -> Endpoints {
        Endpoints { geocoder: base.to_string(), router: base.to_string() }
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    fn guarded() -> std::sync::MutexGuard<'static, ()> {
        let g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        g
    }

    fn count(requests: &Arc<Mutex<Vec<String>>>, needle: &str) -> usize {
        requests.lock().unwrap().iter().filter(|r| r.contains(needle)).count()
    }

    fn server(handler: impl Fn(&str) -> (u16, Vec<(&'static str, String)>, String) + Send + Sync + 'static) -> (String, Arc<Mutex<Vec<String>>>, Arc<AtomicBool>) {
        start_mock_server(handler)
    }

    fn json_ok(body: &'static str) -> (u16, Vec<(&'static str, String)>, String) {
        (200, vec![("Content-Type", "application/json".into())], body.to_string())
    }

    // ── pure ───────────────────────────────────────────────────────────────

    #[test]
    fn places_are_percent_encoded_byte_for_byte() {
        let u = geocode_url("https://nominatim.example/", "Apartment #4, 5% off & café 東京");
        assert!(u.starts_with("https://nominatim.example/search?q="), "{u}");
        let q = u.split("q=").nth(1).unwrap().split('&').next().unwrap();
        assert!(q.contains("%23") && q.contains("%25") && q.contains("%26"), "# % & must be encoded: {q}");
        assert!(q.contains("%C3%A9") && q.contains("%E6%9D%B1"), "non-ASCII must be UTF-8 percent-encoded: {q}");
        assert!(!q.contains(' ') && !q.contains('#') && !q.contains('&'), "{q}");
    }

    #[test]
    fn the_route_address_has_longitude_first_and_the_right_profile() {
        let pts = [(17.3616, 78.4747), (17.385, 78.4867)];
        let car = route_url("https://routing.openstreetmap.de", TravelMode::Driving, &pts).unwrap();
        assert!(car.starts_with("https://routing.openstreetmap.de/routed-car/route/v1/driving/78.474700,17.361600;78.486700,17.385000?"), "{car}");
        assert!(car.contains("steps=true") && car.contains("generate_hints=false"));
        assert!(route_url("https://routing.openstreetmap.de", TravelMode::Walking, &pts).unwrap().contains("/routed-foot/"));
        assert!(route_url("https://routing.openstreetmap.de", TravelMode::Bicycling, &pts).unwrap().contains("/routed-bike/"));
        assert!(route_url("https://routing.openstreetmap.de", TravelMode::Transit, &pts).is_none());
        // the OSRM demo answers every profile with the car profile: only driving may use it
        assert!(route_url("https://router.project-osrm.org", TravelMode::Driving, &pts).is_some());
        assert!(route_url("https://router.project-osrm.org", TravelMode::Walking, &pts).is_none());
        assert!(route_url("https://router.project-osrm.org", TravelMode::Bicycling, &pts).is_none());
        // your own server uses the standard profile names
        assert!(route_url("https://osrm.example", TravelMode::Walking, &pts).unwrap().contains("/route/v1/walking/"));
    }

    #[test]
    fn settings_addresses_must_be_safe() {
        assert_eq!(endpoints_from("", ""), Endpoints::default());
        assert_eq!(endpoints_from("http://evil.example", "ftp://x"), Endpoints::default());
        let e = endpoints_from("https://geo.example/", "http://127.0.0.1:5000");
        assert_eq!(e.geocoder, "https://geo.example");
        assert_eq!(e.router, "http://127.0.0.1:5000");
    }

    #[test]
    fn own_places_are_private() {
        for p in ["home", "my home", "work", "the office", "my gym", "My college"] {
            assert!(spoken_is_private(p), "{p}");
        }
        for p in ["Charminar", "Hyderabad airport", "T-Hub"] {
            assert!(!spoken_is_private(p), "{p}");
        }
        assert_eq!(parse_point("17.36, 78.47"), Some((17.36, 78.47)));
        assert_eq!(parse_point("Charminar, Hyderabad"), None);
        assert_eq!(parse_point("0,0"), None);
    }

    #[test]
    fn steps_and_totals_parse_into_spoken_text() {
        let r = parse_route(OSRM_OK, TravelMode::Driving).unwrap();
        assert_eq!(r.meters, 5160);
        assert_eq!(r.minutes, 8);
        assert_eq!(r.via.as_deref(), Some("Charminar Road"));
        assert_eq!(r.steps.len(), 4, "the zero-length 'new name' step is dropped: {:?}", r.steps);
        assert!(r.steps[0].starts_with("Head east on Charminar Road"), "{:?}", r.steps[0]);
        assert!(r.steps[1].starts_with("Turn left onto NH765"), "a road with only a ref is named by its ref: {:?}", r.steps[1]);
        assert!(r.steps[2].starts_with("At the roundabout take the second exit onto Ring Road"), "{:?}", r.steps[2]);
        assert_eq!(r.steps[3], "Arrive at your destination, on your left");
        // walking has no "via": it is a road name that means nothing on foot
        assert_eq!(parse_route(OSRM_OK, TravelMode::Walking).unwrap().via, None);
    }

    #[test]
    fn a_stop_is_not_the_destination() {
        let body = r#"{"code":"Ok","routes":[{"distance":900,"duration":300,"legs":[
          {"summary":"A","steps":[{"distance":0,"name":"","maneuver":{"type":"arrive","modifier":"right"}}]},
          {"summary":"B","steps":[{"distance":0,"name":"","maneuver":{"type":"arrive"}}]}]}]}"#;
        let r = parse_route(body, TravelMode::Driving).unwrap();
        assert_eq!(r.steps, vec!["Reach stop 1, on your right".to_string(), "Arrive at your destination".to_string()]);
    }

    #[test]
    fn bad_answers_are_refused_not_guessed() {
        assert_eq!(parse_route("not json", TravelMode::Driving), Err(OpenError::Bad));
        assert_eq!(parse_route(r#"{"code":"NoRoute","message":"x"}"#, TravelMode::Walking), Err(OpenError::NoRoute));
        assert_eq!(parse_route(r#"{"code":"InvalidValue"}"#, TravelMode::Driving), Err(OpenError::Bad));
        assert_eq!(parse_route(r#"{"code":"Ok","routes":[]}"#, TravelMode::Driving), Err(OpenError::Bad));
        assert_eq!(parse_route(r#"{"code":"Ok","routes":[{"distance":0,"duration":5,"legs":[]}]}"#, TravelMode::Driving), Err(OpenError::Bad));
        assert_eq!(parse_route(r#"{"code":"Ok","routes":[{"distance":10,"duration":-3,"legs":[]}]}"#, TravelMode::Driving), Err(OpenError::Bad));
    }

    #[test]
    fn errors_say_what_failed_without_naming_a_place() {
        assert!(OpenError::NotFound.speak(TravelMode::Driving, "Foo Bar").contains("Foo Bar"));
        assert!(OpenError::NoRoute.speak(TravelMode::Walking, "").contains("walking"));
        assert!(OpenError::Unsupported.speak(TravelMode::Transit, "").contains("Maps key"));
        for e in [OpenError::Offline, OpenError::Timeout, OpenError::RateLimited, OpenError::Server(500), OpenError::Bad, OpenError::TooLarge] {
            assert!(!e.speak(TravelMode::Driving, "Foo").is_empty() && !e.speak(TravelMode::Driving, "Foo").contains("Foo"));
            assert!(!e.to_string().contains("Foo"));
        }
        assert_eq!(OpenError::Server(502).to_string(), "server error 502");
    }

    #[test]
    fn ignored_avoid_options_are_named_honestly() {
        assert_eq!(ignored_sentence(&[]), None);
        assert!(ignored_sentence(&["tolls"]).unwrap().contains("can't avoid tolls"));
        assert!(ignored_sentence(&["tolls", "highways"]).unwrap().contains("tolls and highways"));
        assert!(ignored_sentence(&["tolls", "highways", "ferries"]).unwrap().contains("tolls, highways and ferries"));
    }

    // ── against a local mock server ─────────────────────────────────────────

    #[test]
    fn a_place_is_found_sent_with_an_app_name_and_cached() {
        let _g = guarded();
        let (base, requests, stop) = server(|_| json_ok(NOMINATIM_OK));
        let f = run(geocode(&ep(&base), "  Charminar  ")).unwrap();
        assert_eq!(f.name, "Charminar");
        // the same place again (different spacing and case) is a cache hit: no second request
        let again = run(geocode(&ep(&base), "charminar")).unwrap();
        assert_eq!(again, f);
        assert_eq!(count(&requests, "GET /search"), 1);
        let req = requests.lock().unwrap()[0].to_lowercase();
        assert!(req.contains("user-agent: nexus-assistant/"), "an application name is sent: {req}");
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn an_unknown_place_is_not_found_and_not_asked_again_at_once() {
        let _g = guarded();
        let (base, requests, stop) = server(|_| json_ok("[]"));
        assert_eq!(run(geocode(&ep(&base), "Nowhereville")), Err(OpenError::NotFound));
        assert_eq!(run(geocode(&ep(&base), "nowhereville")), Err(OpenError::NotFound));
        assert_eq!(count(&requests, "GET /search"), 1, "Nominatim asks that the same query is not repeated");
        assert_eq!(run(geocode(&ep(&base), "   ")), Err(OpenError::NotFound));
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn http_429_pauses_the_service_and_the_pause_is_kept() {
        let _g = guarded();
        let (base, requests, stop) = server(|_| (429, vec![("Retry-After", "30".into())], "slow down".into()));
        assert_eq!(run(geocode(&ep(&base), "Charminar")), Err(OpenError::RateLimited));
        let left = paused_for(Provider::Geocoder).expect("paused after 429");
        assert!(left > Duration::from_secs(20) && left <= Duration::from_secs(30), "Retry-After is honoured: {left:?}");
        assert_eq!(run(geocode(&ep(&base), "Another place")), Err(OpenError::RateLimited));
        assert_eq!(count(&requests, "GET /search"), 1, "no request is sent while paused");
        // the other service is not paused by this one
        assert!(paused_for(Provider::Router).is_none());
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn a_server_error_is_reported_and_does_not_pause_for_good() {
        let _g = guarded();
        let (base, _r, stop) = server(|_| (500, vec![], "oops".into()));
        assert_eq!(run(geocode(&ep(&base), "Charminar")), Err(OpenError::Server(500)));
        assert!(paused_for(Provider::Geocoder).is_none());
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn a_pause_is_capped() {
        let _g = guarded();
        let (base, _r, stop) = server(|_| (429, vec![("Retry-After", "99999".into())], String::new()));
        let _ = run(geocode(&ep(&base), "x y"));
        assert!(paused_for(Provider::Geocoder).unwrap() <= BLOCK_MAX);
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn junk_answers_are_bad_not_found() {
        let _g = guarded();
        let (base, _r, stop) = server(|_| (200, vec![], "<html>proxy login</html>".into()));
        assert_eq!(run(geocode(&ep(&base), "Charminar")), Err(OpenError::Bad));
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn a_huge_answer_is_cut_off() {
        let _g = guarded();
        let big = format!("[{}]", "0,".repeat(MAX_BODY));
        let (base, _r, stop) = server(move |_| (200, vec![], big.clone()));
        assert_eq!(run(geocode(&ep(&base), "Charminar")), Err(OpenError::TooLarge));
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn a_chunked_answer_with_no_length_is_cut_off_while_it_streams() {
        use std::io::{Read, Write};
        let _g = guarded();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf);
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
                let chunk = vec![b'0'; 100_000];
                // 2 MB with no Content-Length: only the running count can stop it
                for _ in 0..20 {
                    if sock.write_all(format!("{:x}\r\n", chunk.len()).as_bytes()).is_err() || sock.write_all(&chunk).is_err() || sock.write_all(b"\r\n").is_err() {
                        return;
                    }
                }
                let _ = sock.write_all(b"0\r\n\r\n");
            }
        });
        let base = format!("http://{addr}");
        assert_eq!(run(geocode(&ep(&base), "Charminar")), Err(OpenError::TooLarge));
    }

    #[test]
    fn a_slow_service_times_out_and_an_absent_one_is_offline() {
        let _g = guarded();
        let (base, _r, stop) = server(|_| {
            std::thread::sleep(Duration::from_millis(1500));
            json_ok("[]")
        });
        let started = Instant::now();
        // the real lookups wait 6 s and routes 8 s; a 1 s total proves the mapping without a long test
        let r = run(get(Provider::Geocoder, &format!("{base}/search?q=a"), 1));
        assert_eq!(r, Err(OpenError::Timeout));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(paused_for(Provider::Geocoder).is_none(), "a slow answer is not a refusal");
        stop.store(true, O::Relaxed);
        // nothing listens here: offline, and fast
        let dead = Endpoints { geocoder: "http://127.0.0.1:9".into(), router: "http://127.0.0.1:9".into() };
        assert_eq!(run(geocode(&dead, "Charminar")), Err(OpenError::Offline));
        assert_eq!(run(route(&dead, TravelMode::Driving, &[(1.0, 1.0), (2.0, 2.0)])), Err(OpenError::Offline));
    }

    #[test]
    fn the_gap_between_requests_is_kept() {
        let _g = guarded();
        set_min_gap_ms(300);
        let (base, _r, stop) = server(|_| json_ok(NOMINATIM_OK));
        let t = Instant::now();
        run(async {
            let _ = geocode(&ep(&base), "one").await;
            let _ = geocode(&ep(&base), "two").await;
            let _ = geocode(&ep(&base), "three").await;
        });
        assert!(t.elapsed() >= Duration::from_millis(560), "three requests need two gaps: {:?}", t.elapsed());
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn a_route_is_parsed_and_the_mode_picks_the_server() {
        let _g = guarded();
        let (base, requests, stop) = server(|_| json_ok(OSRM_OK));
        // a custom address uses the standard profile name
        let r = run(route(&ep(&base), TravelMode::Walking, &[(17.3616, 78.4747), (17.385, 78.4867)])).unwrap();
        assert_eq!(r.minutes, 8);
        assert!(requests.lock().unwrap()[0].contains("GET /route/v1/walking/78.474700,17.361600;78.486700,17.385000"), "{:?}", requests.lock().unwrap()[0]);
        // fewer than two points, or more than the cap, are never sent
        assert_eq!(run(route(&ep(&base), TravelMode::Driving, &[(1.0, 1.0)])), Err(OpenError::Bad));
        assert_eq!(run(route(&ep(&base), TravelMode::Driving, &[(1.0, 1.0); 8])), Err(OpenError::Bad));
        assert_eq!(count(&requests, "GET /route"), 1);
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn osrm_says_no_route_with_http_400() {
        let _g = guarded();
        let (base, _r, stop) = server(|_| (400, vec![], r#"{"code":"NoRoute","message":"Impossible route between points"}"#.into()));
        assert_eq!(run(route(&ep(&base), TravelMode::Walking, &[(17.0, 78.0), (-33.0, 151.0)])), Err(OpenError::NoRoute));
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn route_failures_map_to_speakable_errors() {
        let _g = guarded();
        let (base, _r, stop) = server(|_| (502, vec![], String::new()));
        assert_eq!(run(route(&ep(&base), TravelMode::Driving, &[(1.0, 1.0), (2.0, 2.0)])), Err(OpenError::Server(502)));
        stop.store(true, O::Relaxed);
        let (base, _r, stop) = server(|_| (200, vec![], "{\"code\":".into()));
        assert_eq!(run(route(&ep(&base), TravelMode::Driving, &[(1.0, 1.0), (2.0, 2.0)])), Err(OpenError::Bad));
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn plan_geocodes_a_name_and_routes_from_a_point() {
        let _g = guarded();
        let (base, requests, stop) = server(|req| if req.starts_with("GET /search") { json_ok(NOMINATIM_OK) } else { json_ok(OSRM_OK) });
        let opts = RouteOpts::default();
        let r = run(plan(&ep(&base), &PlanRequest { origin: "17.385000,78.486700", destination: "Charminar", stops: &[], mode: TravelMode::Driving, opts: &opts, lookups_allowed: true })).unwrap();
        assert_eq!(r.minutes, 8);
        assert_eq!(count(&requests, "GET /search"), 1, "only the destination is looked up; a point needs no lookup");
        assert!(r.ignored.is_empty());
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn stops_become_waypoints_in_order() {
        let _g = guarded();
        let (base, requests, stop) = server(|req| if req.starts_with("GET /search") { json_ok(NOMINATIM_OK) } else { json_ok(OSRM_OK) });
        let opts = RouteOpts::default();
        let stops = vec!["17.370000,78.480000".to_string()];
        let _ = run(plan(&ep(&base), &PlanRequest { origin: "17.385000,78.486700", destination: "17.361600,78.474700", stops: &stops, mode: TravelMode::Driving, opts: &opts, lookups_allowed: true })).unwrap();
        let req = requests.lock().unwrap().iter().find(|r| r.contains("GET /route")).cloned().unwrap();
        assert!(req.contains("78.486700,17.385000;78.480000,17.370000;78.474700,17.361600"), "{req}");
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn avoid_options_are_reported_as_ignored_for_driving_only() {
        let _g = guarded();
        let (base, _r, stop) = server(|_| json_ok(OSRM_OK));
        let opts = RouteOpts { avoid_tolls: true, avoid_highways: true, ..RouteOpts::default() };
        let mk = |mode| PlanRequest { origin: "17.385,78.4867", destination: "17.3616,78.4747", stops: &[], mode, opts: &opts, lookups_allowed: true };
        let drive = run(plan(&ep(&base), &mk(TravelMode::Driving))).unwrap();
        assert_eq!(drive.ignored, vec!["tolls", "highways"]);
        let walk = run(plan(&ep(&base), &mk(TravelMode::Walking))).unwrap();
        assert!(walk.ignored.is_empty(), "avoid options mean nothing on foot");
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn public_transport_is_not_pretended() {
        let _g = guarded();
        let (base, requests, stop) = server(|_| json_ok(OSRM_OK));
        let opts = RouteOpts::default();
        let r = run(plan(&ep(&base), &PlanRequest { origin: "17.385,78.4867", destination: "17.3616,78.4747", stops: &[], mode: TravelMode::Transit, opts: &opts, lookups_allowed: true }));
        assert_eq!(r, Err(OpenError::Unsupported));
        assert!(requests.lock().unwrap().is_empty());
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn the_users_own_address_never_reaches_the_geocoder() {
        let _g = guarded();
        let (base, requests, stop) = server(|req| if req.starts_with("GET /search") { json_ok(NOMINATIM_OK) } else { json_ok(OSRM_OK) });
        let opts = RouteOpts::default();
        let plan_for = |origin: &str, dest: &str, stops: Vec<String>, allowed: bool| {
            let o = opts.clone();
            let (origin, dest) = (origin.to_string(), dest.to_string());
            let e = ep(&base);
            run(async move { plan(&e, &PlanRequest { origin: &origin, destination: &dest, stops: &stops, mode: TravelMode::Driving, opts: &o, lookups_allowed: allowed }).await })
        };
        // a spoken own place is refused even when lookups are otherwise allowed
        assert_eq!(plan_for("17.385,78.4867", "home", vec![], true), Err(OpenError::Refused));
        assert_eq!(plan_for("17.385,78.4867", "my gym", vec![], true), Err(OpenError::Refused));
        assert_eq!(plan_for("17.385,78.4867", "Charminar", vec!["my office".into()], true), Err(OpenError::Refused));
        assert_eq!(plan_for("work", "Charminar", vec![], true), Err(OpenError::Refused));
        // a public name BEFORE a private one is not looked up either: the refusal comes before any request
        assert_eq!(plan_for("Charminar", "home", vec![], true), Err(OpenError::Refused));
        assert_eq!(plan_for("T-Hub", "Charminar", vec!["Cafe".into(), "my office".into()], true), Err(OpenError::Refused));
        // the caller can switch lookups off: a name is then refused, but points still work
        assert_eq!(plan_for("17.385,78.4867", "Charminar", vec![], false), Err(OpenError::Refused));
        assert!(plan_for("17.385,78.4867", "17.3616,78.4747", vec![], false).is_ok());
        assert_eq!(count(&requests, "GET /search"), 0, "no private name was ever sent to the geocoder");
        assert!(!requests.lock().unwrap().iter().any(|r| r.to_lowercase().contains("home") || r.to_lowercase().contains("gym") || r.to_lowercase().contains("office")));
        stop.store(true, O::Relaxed);
    }

    #[test]
    fn a_place_that_cannot_be_found_stops_the_plan_before_routing() {
        let _g = guarded();
        let (base, requests, stop) = server(|req| if req.starts_with("GET /search") { json_ok("[]") } else { json_ok(OSRM_OK) });
        let opts = RouteOpts::default();
        let r = run(plan(&ep(&base), &PlanRequest { origin: "17.385,78.4867", destination: "Atlantis", stops: &[], mode: TravelMode::Driving, opts: &opts, lookups_allowed: true }));
        assert_eq!(r, Err(OpenError::NotFound));
        assert_eq!(count(&requests, "GET /route"), 0);
        stop.store(true, O::Relaxed);
    }
}
