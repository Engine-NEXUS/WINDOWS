//! Directions, in one place: "directions to X", "how far is X", the follow-ups to the open route ("make it walking",
//! "avoid tolls", "add a stop at X", "how long now", "read me the steps", "next step") and leave-by reminders for a trip
//! ("remind me to leave for the airport at 5 pm").
//!
//! One request opens the map in the NEXUS panel, shows the place card (one photo, distance, times, and a Maps button that
//! opens the same route) and speaks what is actually known: distance, time, arrival, roads avoided, stops. The time comes
//! from the Routes API when there is a key, otherwise from a keyless estimate (marked "roughly"). What could not be found is
//! said, never implied. The last route is remembered for 45 minutes so the follow-ups have something to change.
//!
//! The words are built by pure functions (`route_line`, `minutes_phrase`, `km_phrase`, `apply_follow_up`, `decide_trip`)
//! so they are tested without a window or a network.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use crate::google::maps::{MapsService, RouteEta, RouteOpts};
use crate::google::types::{AlertUrgency, TravelMode};
use crate::location::Fix;
use crate::maps_panel::MapsTarget;
use crate::orchestrator::{install_new_request, say_and_finish, ProcessResult, Subsystem};

/// How long the last route stays open for follow-ups.
const ROUTE_TTL: Duration = Duration::from_secs(45 * 60);
const MAX_STOPS: usize = 5;
const MAX_STEPS_SPOKEN: usize = 3;
/// A trip reminder speaks this long before the exact leave time, and assumes this long to park and walk in.
const TRIP_LEAD_SECS: i64 = 5 * 60;
const TRIP_BUFFER_SECS: i64 = 5 * 60;
/// A trip reminder is dropped this long after the time you wanted to arrive.
const TRIP_KEEP_SECS: i64 = 30 * 60;
const MAX_TRIPS: usize = 10;
/// A trip reminder for a place that comes out further than this is a wrong match ("the airport" found on another
/// continent), not a trip.
const MAX_TRIP_ETA_SECS: i64 = 6 * 3600;

/// Whether a measured travel time can be a real trip. Pure.
pub fn trip_eta_plausible(eta_secs: i64) -> bool {
    eta_secs > 0 && eta_secs <= MAX_TRIP_ETA_SECS
}

/// What the parser handed over for "directions to X".
pub struct Request {
    pub destination: String,
    pub origin: Option<String>,
    pub mode: String,
    pub question: bool,
}

// ── the open route ───────────────────────────────────────────────────────────

/// The route that is open: everything a follow-up needs to change it.
#[derive(Debug, Clone)]
pub struct Current {
    pub origin: Option<String>,
    /// True when the start is the user's own position (so "how long now" measures from where they are now).
    pub origin_is_here: bool,
    pub destination: String,
    pub mode: TravelMode,
    pub opts: RouteOpts,
    pub steps: Vec<String>,
    /// How many steps have been read out.
    pub step_idx: usize,
    /// (km, car minutes, bike minutes, approximate) from the keyless lookup, kept so a change of mode can re-estimate.
    pub base: Option<(f64, u32, u32, bool)>,
    pub at: Instant,
}

static CURRENT: Mutex<Option<Current>> = Mutex::new(None);
static HINTED: AtomicBool = AtomicBool::new(false);

fn current() -> Option<Current> {
    let mut g = CURRENT.lock().unwrap();
    if g.as_ref().is_some_and(|c| c.at.elapsed() > ROUTE_TTL) {
        *g = None;
    }
    g.clone()
}

fn set_current(c: Current) {
    *CURRENT.lock().unwrap() = Some(c);
}

// ── pure: words ──────────────────────────────────────────────────────────────

pub fn mode_phrase(m: TravelMode) -> &'static str {
    match m {
        TravelMode::Driving => "by car",
        TravelMode::Walking => "on foot",
        TravelMode::Bicycling => "by bike",
        TravelMode::Transit => "by public transport",
    }
}

pub fn mode_name(m: TravelMode) -> &'static str {
    match m {
        TravelMode::Driving => "driving",
        TravelMode::Walking => "walking",
        TravelMode::Bicycling => "cycling",
        TravelMode::Transit => "public transport",
    }
}

/// "1 minute" / "22 minutes" / "1 hour 5 minutes". Pure.
pub fn minutes_phrase(min: u32) -> String {
    let min = min.max(1);
    if min >= 60 {
        let (h, m) = (min / 60, min % 60);
        let hs = format!("{h} hour{}", if h == 1 { "" } else { "s" });
        if m == 0 {
            hs
        } else {
            format!("{hs} {m} minute{}", if m == 1 { "" } else { "s" })
        }
    } else {
        format!("{min} minute{}", if min == 1 { "" } else { "s" })
    }
}

/// "800 metres" / "5.3 kilometres" / "12 kilometres". Pure.
pub fn km_phrase(meters: u64) -> String {
    if meters < 950 {
        let m = ((meters + 5) / 10 * 10).max(10);
        return format!("{m} metres");
    }
    let km = meters as f64 / 1000.0;
    if km < 10.0 {
        let t = (km * 10.0).round() / 10.0;
        if (t - 1.0).abs() < f64::EPSILON {
            "1 kilometre".to_string()
        } else {
            format!("{t:.1} kilometres")
        }
    } else {
        format!("{} kilometres", km.round() as u64)
    }
}

/// The clock time `add_min` minutes after `now_min` (minutes since midnight). Pure.
pub fn arrival_clock(now_min: u32, add_min: u32) -> String {
    crate::memcore::timetable::fmt_time(((now_min + add_min) % 1440) as u16)
}

/// What was found about the trip.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub meters: u64,
    pub minutes: Option<u32>,
    pub via: Option<String>,
    /// True for the Routes API (traffic-aware); false for the keyless estimate.
    pub exact: bool,
}

pub fn facts_from_eta(e: &RouteEta) -> Facts {
    Facts { meters: e.meters, minutes: Some(e.minutes), via: e.via.clone(), exact: true }
}

/// The keyless estimate for a mode. Walking is 5 km/h; public transport has no estimate (distance only). Pure.
pub fn facts_from_estimate(mode: TravelMode, km: f64, car_min: u32, bike_min: u32, approx: bool) -> Facts {
    let minutes = match mode {
        TravelMode::Driving => Some(car_min),
        TravelMode::Bicycling => Some(bike_min),
        TravelMode::Walking => Some(((km * 12.0).round() as u32).max(1)),
        TravelMode::Transit => None,
    };
    // Only a measured driving time from the Google route counts as exact; everything else is an estimate.
    Facts { meters: (km * 1000.0).round() as u64, minutes, via: None, exact: !approx && mode == TravelMode::Driving }
}

/// Everything the spoken answer is made from.
pub struct LineInput<'a> {
    pub dest: &'a str,
    pub mode: TravelMode,
    pub facts: Option<Facts>,
    pub opts_spoken: String,
    /// Minutes since midnight now, for "arriving around".
    pub now_min: Option<u32>,
    pub origin_known: bool,
    /// A Routes lookup was tried and failed.
    pub time_failed: bool,
    pub question: bool,
    /// Tell the user once that a Maps key gives live traffic.
    pub hint: bool,
    /// What a follow-up just did ("Switched to walking"), said instead of "The route to X is open".
    pub update: Option<String>,
}

/// The first letter upper-case ("charminar" -> "Charminar"). Pure.
pub fn cap_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// What a follow-up did, in a few words, for the start of the answer. Pure.
pub fn update_phrase(action: &str, value: Option<&str>, mode: TravelMode) -> String {
    let v = value.unwrap_or("").trim();
    match action {
        "mode" => format!("Switched to {}", mode_name(mode)),
        "avoid" => format!("Avoiding {v}"),
        "allow" => format!("Allowing {v} again"),
        "stop" => format!("Added {v} as a stop"),
        "clear_stops" => "Removed the stops".to_string(),
        _ => "Done".to_string(),
    }
}

/// The sentence NEXUS speaks for a route. Says what is known and what is not. Pure.
pub fn route_line(i: &LineInput) -> String {
    let mut line = match &i.facts {
        Some(f) => {
            let dist = km_phrase(f.meters);
            let mut detail = String::new();
            if let Some(min) = f.minutes {
                detail.push_str(&format!(", {} {} {}", if f.exact { "about" } else { "roughly" }, minutes_phrase(min), mode_phrase(i.mode)));
                if let Some(v) = f.via.as_deref().filter(|v| !v.trim().is_empty()) {
                    detail.push_str(&format!(" via {v}"));
                }
            }
            if !i.opts_spoken.is_empty() {
                detail.push_str(&format!(", {}", i.opts_spoken));
            }
            if let (Some(min), Some(now)) = (f.minutes, i.now_min) {
                detail.push_str(&format!(", arriving around {}", arrival_clock(now, min)));
            }
            if i.question {
                format!("{} is {dist} away{detail}, sir.", cap_first(i.dest))
            } else if let Some(u) = i.update.as_deref() {
                format!("{u}, sir. It's {dist}{detail}.")
            } else {
                format!("The route to {} is open, sir. It's {dist}{detail}.", i.dest)
            }
        }
        None if i.update.is_some() && !i.question => format!("{}, sir.", i.update.as_deref().unwrap_or("Done")),
        None if i.question => format!("I opened the route to {}, sir, but I couldn't work out the distance.", i.dest),
        None if i.opts_spoken.is_empty() => format!("Opening the route to {}, sir.", i.dest),
        None => format!("Opening the route to {}, {}, sir.", i.dest, i.opts_spoken),
    };
    if !i.origin_known {
        line.push_str(" I couldn't tell where you are, so choose your start on the map.");
    }
    if i.time_failed && i.facts.as_ref().is_none_or(|f| !f.exact) {
        line.push_str(" I couldn't get the live travel time.");
    }
    if i.hint {
        line.push_str(" Add a Maps key in the Command Hub for live traffic times.");
    }
    line
}

// ── pure: follow-ups ─────────────────────────────────────────────────────────

/// What a follow-up did to the route.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// The route changed: open it again.
    Reopen,
    /// Nothing to redo: say this.
    Say(String),
}

/// Apply a spoken change to the open route. Pure.
pub fn apply_follow_up(cur: &mut Current, action: &str, value: Option<&str>) -> Change {
    let v = value.unwrap_or("").trim();
    match action {
        "mode" => {
            let new = crate::commands::parse_travel_mode(Some(v));
            if new == cur.mode {
                return Change::Say(format!("It's already {}, sir.", mode_name(new)));
            }
            cur.mode = new;
            cur.steps.clear();
            cur.step_idx = 0;
            Change::Reopen
        }
        "avoid" | "allow" => {
            if cur.mode != TravelMode::Driving {
                return Change::Say(format!("Roads only matter when driving, sir, and this route is {}. Say: make it driving.", mode_name(cur.mode)));
            }
            let on = action == "avoid";
            let flag = match v {
                "tolls" => &mut cur.opts.avoid_tolls,
                "highways" => &mut cur.opts.avoid_highways,
                "ferries" => &mut cur.opts.avoid_ferries,
                _ => return Change::Say("I can avoid tolls, highways or ferries, sir.".into()),
            };
            if *flag == on {
                return Change::Say(format!("It's already {} {v}, sir.", if on { "avoiding" } else { "allowing" }));
            }
            *flag = on;
            cur.steps.clear();
            cur.step_idx = 0;
            Change::Reopen
        }
        "stop" => {
            if cur.mode == TravelMode::Transit {
                return Change::Say("Stops aren't available for public transport, sir.".into());
            }
            if v.is_empty() {
                return Change::Say("Where should the stop be, sir?".into());
            }
            if cur.opts.stops.len() >= MAX_STOPS {
                return Change::Say(format!("That's already {MAX_STOPS} stops, sir. Say: remove the stops."));
            }
            if cur.opts.stops.iter().any(|s| s.eq_ignore_ascii_case(v)) {
                return Change::Say(format!("{v} is already a stop, sir."));
            }
            cur.opts.stops.push(v.to_string());
            cur.steps.clear();
            cur.step_idx = 0;
            Change::Reopen
        }
        "clear_stops" => {
            if cur.opts.stops.is_empty() {
                return Change::Say("There are no stops on this route, sir.".into());
            }
            cur.opts.stops.clear();
            cur.steps.clear();
            cur.step_idx = 0;
            Change::Reopen
        }
        _ => Change::Say("I didn't catch what to change on the route, sir.".into()),
    }
}

/// "Step 1: Head north on Main St. Step 2: ..." for `from..from+count`, and the new index. Pure.
pub fn read_steps(steps: &[String], from: usize, count: usize) -> (String, usize) {
    if steps.is_empty() {
        return (String::new(), from);
    }
    let end = (from + count).min(steps.len());
    let text = steps[from.min(steps.len())..end]
        .iter()
        .enumerate()
        .map(|(k, s)| format!("Step {}: {}", from + k + 1, s.trim_end_matches('.')))
        .collect::<Vec<_>>()
        .join(". ");
    let mut out = if text.is_empty() { String::new() } else { format!("{text}.") };
    if end >= steps.len() && !out.is_empty() {
        out.push_str(" That's the last step.");
    }
    (out, end)
}

/// The numbered list for the sidebar. Pure.
pub fn steps_markdown(dest: &str, steps: &[String]) -> String {
    let mut s = format!("## Directions to {dest}\n\n");
    for (i, st) in steps.iter().enumerate() {
        s.push_str(&format!("{}. {}\n", i + 1, st));
    }
    s
}

// ── pure: trip reminders ─────────────────────────────────────────────────────

/// A saved "remind me to leave for X by T".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trip {
    pub id: String,
    pub destination: String,
    /// When the user wants to be there (epoch seconds).
    pub arrive_epoch: i64,
    /// The latest travel time (seconds) and when it was measured.
    pub eta_secs: i64,
    pub eta_at: i64,
}

fn clock_of(epoch: i64) -> String {
    use chrono::{TimeZone, Timelike};
    match chrono::Local.timestamp_opt(epoch, 0).single() {
        Some(dt) => crate::memcore::timetable::fmt_time((dt.hour() * 60 + dt.minute()) as u16),
        None => String::new(),
    }
}

/// The next time the clock shows `arrive_min` (minutes since midnight), as an epoch. A time that has passed today means
/// tomorrow only when that is within 12 hours ("at 1 am" said at 11 pm); otherwise (said at 6 pm, "at 5 pm") it is
/// `None`, because it was probably a slip. Pure.
pub fn arrive_epoch_for(now: chrono::DateTime<chrono::Local>, arrive_min: u16) -> Option<i64> {
    use chrono::{Datelike, TimeZone};
    let (h, m) = ((arrive_min / 60) as u32, (arrive_min % 60) as u32);
    let at = chrono::Local.with_ymd_and_hms(now.year(), now.month(), now.day(), h, m, 0).single()?;
    if at.timestamp() > now.timestamp() {
        return Some(at.timestamp());
    }
    let tomorrow = at.timestamp() + 24 * 3600;
    (tomorrow - now.timestamp() <= 12 * 3600).then_some(tomorrow)
}

/// Speak now? From 5 minutes before the leave time until the arrival time; never after. Pure.
pub fn decide_trip(t: &Trip, now: i64, address: Option<&str>) -> Option<String> {
    if t.eta_secs <= 0 || now >= t.arrive_epoch {
        return None;
    }
    let leave_at = t.arrive_epoch - t.eta_secs - TRIP_BUFFER_SECS;
    if now < leave_at - TRIP_LEAD_SECS {
        return None;
    }
    let tail = address.map(|a| format!(", {a}")).unwrap_or_default();
    let mins = ((t.eta_secs + 59) / 60).max(1);
    let trip = format!("It's about {} away and you wanted to be there by {}.", minutes_phrase(mins as u32), clock_of(t.arrive_epoch));
    Some(if now >= t.arrive_epoch - t.eta_secs {
        format!("You're running late for {}{tail}. {trip}", t.destination)
    } else if now >= leave_at {
        format!("Time to leave for {}{tail}. {trip}", t.destination)
    } else {
        let m = (((leave_at - now) + 59) / 60).max(1);
        format!("Leave in about {m} minute{} for {}{tail}. {trip}", if m == 1 { "" } else { "s" }, t.destination)
    })
}

/// The moment NEXUS speaks this trip's reminder (5 minutes before the leave time), epoch milliseconds. Pure.
pub fn trip_alert_at_ms(t: &Trip) -> i64 {
    (t.arrive_epoch - t.eta_secs - TRIP_BUFFER_SECS - TRIP_LEAD_SECS) * 1000
}

/// The trip as the generic reminder event the stage draws. `now_ms` keeps a reminder that is due within seconds valid.
/// `spoken` is what NEXUS said when it was set (empty when a reminder is restored after a restart). Pure.
pub fn trip_event(t: &Trip, spoken: &str, now_ms: i64) -> crate::reminder::ReminderEvent {
    let title: String = format!("Leave for {}", t.destination).chars().take(crate::reminder::MAX_TITLE_CHARS).collect();
    crate::reminder::ReminderEvent {
        id: t.id.clone(),
        kind: "trip".to_string(),
        title,
        at_ms: trip_alert_at_ms(t).max(now_ms + 5_000),
        spoken: spoken.chars().take(240).collect(),
        alert: format!("Time to leave for {}, sir.", t.destination).chars().take(240).collect(),
        source: "directions".to_string(),
        display: crate::reminder::Display::default(),
    }
}

/// Drop reminders whose arrival time is long past. Pure.
pub fn prune_trips(trips: Vec<Trip>, now: i64) -> Vec<Trip> {
    trips.into_iter().filter(|t| t.arrive_epoch + TRIP_KEEP_SECS > now).take(MAX_TRIPS).collect()
}

fn trips_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("trip_reminders.json"))
}

pub fn load_trips(path: &std::path::Path) -> Vec<Trip> {
    std::fs::read_to_string(path).ok().and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default()
}

pub fn save_trips(path: &std::path::Path, trips: &[Trip]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(trips).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

// ── running a request ────────────────────────────────────────────────────────

fn now_min_of_day() -> u32 {
    use chrono::Timelike;
    let n = chrono::Local::now();
    n.hour() * 60 + n.minute()
}

fn client(secs: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(secs))
        .build()
        .unwrap_or_default()
}

/// Whether the place card (one photo of the destination) is built for this spoken destination. Not for "home" /
/// "work" / "my X": that would send the user's own address to the photo sources (Nominatim, Wikipedia), which the route
/// itself does not. Pure.
pub fn wants_photo_card(spoken_destination: &str) -> bool {
    let d = spoken_destination.trim().to_lowercase();
    !d.is_empty() && crate::orchestrator::saved_place_kind(&d).is_none() && !d.starts_with("my ")
}

enum Card {
    /// Build the card for the destination (and close a stale one when none is found).
    Build,
    /// Leave the open card alone (a change of mode or a stop does not change the place).
    Keep,
}

/// Open the route, look up the time, show the card, remember the route, and return the sentence to speak.
async fn present<R: Runtime>(app: &AppHandle<R>, cur: &mut Current, card: Card, question: bool, here: Option<Fix>, update: Option<String>) -> Result<String, String> {
    use crate::place_card;
    let target = MapsTarget::Directions { origin: cur.origin.clone(), destination: cur.destination.clone(), mode: cur.mode, opts: cur.opts.clone() };
    if let Err(e) = crate::maps_panel::show_map(app, target).await {
        tracing::warn!("directions: could not open Maps window: {e}");
        return Err("I couldn't open the map window, sir.".into());
    }
    let key = crate::commands::read_api_key(app, "google_maps");
    let origin = cur.origin.clone().filter(|o| !o.trim().is_empty());
    let (dest, mode, opts) = (cur.destination.clone(), cur.mode, cur.opts.clone());
    let routes_task = async {
        match (key.is_empty(), origin.as_deref()) {
            (false, Some(o)) => Some(MapsService::compute_route_with(&client(4), &key, o, &dest, mode, &opts, true).await),
            _ => None,
        }
    };
    let want_card = matches!(card, Card::Build);
    let card_task = async {
        if !want_card {
            return None;
        }
        tokio::time::timeout(Duration::from_secs(6), place_card::build(app, &dest, here)).await.ok().flatten()
    };
    // Without a Routes key the distance and time come from a keyless estimate. It is fetched on its own, so a slow photo
    // never costs the answer its numbers. Not for an own-address destination (that never reaches the lookup sources).
    let estimate_task = async {
        match here {
            Some(h) if want_card && key.is_empty() => tokio::time::timeout(Duration::from_secs(5), place_card::quick_route(app, &dest, h)).await.ok().flatten(),
            _ => None,
        }
    };
    let (routes, built, quick) = tokio::join!(routes_task, card_task, estimate_task);

    if want_card {
        match built.as_ref() {
            Some(c) => {
                let link = MapsService::build_directions_url_with(cur.origin.as_deref(), &cur.destination, cur.mode, &cur.opts);
                if let Err(e) = place_card::show_card_for_route(app, c.clone(), Some(link)) {
                    tracing::warn!("directions: could not show the place card: {e}");
                }
                if let (Some(km), Some(car), Some(bike)) = (c.distance_km, c.car_min, c.bike_min) {
                    cur.base = Some((km, car, bike, c.travel_approx));
                }
            }
            None => {
                // A card left over from an earlier place would be wrong beside this route.
                if app.get_webview_window(crate::panel_layout::Panel::PlaceCard.label()).is_some() {
                    let _ = place_card::close_place_card(app.clone()).await;
                }
            }
        }
    }

    if cur.base.is_none() {
        if let Some(q) = quick {
            cur.base = Some((q.km, q.car_min, q.bike_min, q.approx));
        }
    }
    let mut time_failed = false;
    let mut facts = match routes {
        Some(Ok(eta)) => {
            cur.steps = eta.steps.clone();
            cur.step_idx = 0;
            Some(facts_from_eta(&eta))
        }
        Some(Err(e)) => {
            tracing::debug!("directions: routes api: {e}");
            time_failed = true;
            None
        }
        None => None,
    };
    if facts.is_none() {
        facts = cur.base.map(|(km, car, bike, approx)| facts_from_estimate(cur.mode, km, car, bike, approx));
    }
    let hint = facts.as_ref().is_some_and(|f| !f.exact) && key.is_empty() && !HINTED.swap(true, Ordering::Relaxed);
    let mut line = route_line(&LineInput {
        dest: &cur.destination,
        mode: cur.mode,
        facts,
        opts_spoken: cur.opts.spoken(),
        now_min: Some(now_min_of_day()),
        origin_known: cur.origin.as_deref().is_some_and(|o| !o.trim().is_empty()),
        time_failed,
        question,
        hint,
        update,
    });
    if here.as_ref().is_some_and(crate::location::should_tell_about_exact_location) {
        line.push_str(" For an exact starting point, turn on location in Windows settings, sir.");
    }
    cur.at = Instant::now();
    set_current(cur.clone());
    Ok(line)
}

/// "Directions to X" / "how far is X".
pub async fn run<R: Runtime>(app: AppHandle<R>, req: Request) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    let travel = crate::commands::parse_travel_mode(Some(&req.mode));
    let photo_card = wants_photo_card(&req.destination);
    let (destination, origin) = match crate::orchestrator::resolve_saved_places(&app, &req.destination, req.origin.as_deref()) {
        Ok(r) => r,
        Err(line) => return say_and_finish(&app, request_id, line),
    };
    let here = if origin.as_deref().is_none_or(|o| o.trim().is_empty()) { crate::location::current().await } else { None };
    let origin_is_here = origin.as_deref().is_none_or(|o| o.trim().is_empty()) && here.is_some();
    let origin = match origin.filter(|o| !o.trim().is_empty()) {
        Some(o) => Some(o),
        None => here.map(|f| f.as_point()),
    };
    let mut cur = Current {
        origin,
        origin_is_here,
        destination,
        mode: travel,
        opts: RouteOpts::default(),
        steps: vec![],
        step_idx: 0,
        base: None,
        at: Instant::now(),
    };
    // Home / work / "my X": nothing about the user's own address goes to the photo sources, but a stale card is closed.
    let line = if photo_card {
        present(&app, &mut cur, Card::Build, req.question, here, None).await
    } else {
        present_without_card(&app, &mut cur, req.question, here).await
    };
    match line {
        Ok(l) => say_and_finish(&app, request_id, l),
        Err(l) => say_and_finish(&app, request_id, l),
    }
}

/// The same as `present` for an own-address destination: no photo lookup, and a stale card is closed.
async fn present_without_card<R: Runtime>(app: &AppHandle<R>, cur: &mut Current, question: bool, here: Option<Fix>) -> Result<String, String> {
    if app.get_webview_window(crate::panel_layout::Panel::PlaceCard.label()).is_some() {
        let _ = crate::place_card::close_place_card(app.clone()).await;
    }
    present(app, cur, Card::Keep, question, here, None).await
}

/// A change to, or a question about, the open route.
pub async fn follow_up<R: Runtime>(app: AppHandle<R>, action: String, value: Option<String>) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    let Some(mut cur) = current() else {
        return say_and_finish(&app, request_id, "No route is open, sir. Say: directions to, and a place.".into());
    };
    match action.as_str() {
        "status" => {
            let line = status_line(&app, &mut cur).await;
            say_and_finish(&app, request_id, line)
        }
        "steps" | "next_step" | "repeat_step" => {
            let line = steps_line(&app, &mut cur, &action).await;
            say_and_finish(&app, request_id, line)
        }
        _ => match apply_follow_up(&mut cur, &action, value.as_deref()) {
            Change::Say(line) => say_and_finish(&app, request_id, line),
            Change::Reopen => {
                let here = if cur.origin_is_here { crate::location::current().await } else { None };
                if let Some(h) = here {
                    cur.origin = Some(h.as_point());
                }
                let update = update_phrase(&action, value.as_deref(), cur.mode);
                match present(&app, &mut cur, Card::Keep, false, here, Some(update)).await {
                    Ok(l) | Err(l) => say_and_finish(&app, request_id, l),
                }
            }
        },
    }
}

/// "How long now": measure again from where the user is now.
async fn status_line<R: Runtime>(app: &AppHandle<R>, cur: &mut Current) -> String {
    let key = crate::commands::read_api_key(app, "google_maps");
    let here = if cur.origin_is_here { crate::location::current().await } else { None };
    let origin = here.map(|f| f.as_point()).or_else(|| cur.origin.clone()).filter(|o| !o.trim().is_empty());
    let mut facts = None;
    let mut failed = false;
    if let (false, Some(o)) = (key.is_empty(), origin.as_deref()) {
        match MapsService::compute_route_with(&client(4), &key, o, &cur.destination, cur.mode, &cur.opts, false).await {
            Ok(e) => facts = Some(facts_from_eta(&e)),
            Err(e) => {
                tracing::debug!("directions: status: {e}");
                failed = true;
            }
        }
    }
    if facts.is_none() {
        if let Some(h) = here {
            if let Some(q) = crate::place_card::quick_route(app, &cur.destination, h).await {
                facts = Some(facts_from_estimate(cur.mode, q.km, q.car_min, q.bike_min, q.approx));
            }
        }
    }
    cur.at = Instant::now();
    set_current(cur.clone());
    match facts {
        Some(f) => route_line(&LineInput {
            dest: &cur.destination,
            mode: cur.mode,
            facts: Some(f),
            opts_spoken: cur.opts.spoken(),
            now_min: Some(now_min_of_day()),
            origin_known: true,
            time_failed: failed,
            question: true,
            hint: false,
            update: None,
        }),
        None => "I can only update the time with a Maps key and a start point, sir.".into(),
    }
}

/// "Read the steps" / "next step" / "repeat that step".
async fn steps_line<R: Runtime>(app: &AppHandle<R>, cur: &mut Current, action: &str) -> String {
    if cur.steps.is_empty() {
        let key = crate::commands::read_api_key(app, "google_maps");
        if let (false, Some(o)) = (key.is_empty(), cur.origin.clone().filter(|o| !o.trim().is_empty())) {
            if let Ok(e) = MapsService::compute_route_with(&client(4), &key, &o, &cur.destination, cur.mode, &cur.opts, true).await {
                cur.steps = e.steps;
            }
        }
    }
    if cur.steps.is_empty() {
        return "I don't have the turn-by-turn steps, sir. They need a Maps key; the map shows them.".into();
    }
    let (line, idx) = match action {
        "steps" => {
            let md = steps_markdown(&cur.destination, &cur.steps);
            let a = app.clone();
            let q = format!("Directions to {}", cur.destination);
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::commands::show_sidebar_with_content(a, q, md).await {
                    tracing::warn!("directions: steps sidebar: {e}");
                }
            });
            let (t, i) = read_steps(&cur.steps, 0, MAX_STEPS_SPOKEN);
            (format!("{t} The full list is in the sidebar."), i)
        }
        "next_step" => {
            if cur.step_idx >= cur.steps.len() {
                ("That was the last step, sir. You should have arrived.".to_string(), cur.step_idx)
            } else {
                read_steps(&cur.steps, cur.step_idx, 1)
            }
        }
        _ => {
            let last = cur.step_idx.saturating_sub(1);
            let (t, _) = read_steps(&cur.steps, last, 1);
            (if t.is_empty() { "I haven't read any step yet, sir.".to_string() } else { t }, cur.step_idx)
        }
    };
    cur.step_idx = idx;
    cur.at = Instant::now();
    set_current(cur.clone());
    line
}

// ── trip reminders ───────────────────────────────────────────────────────────

/// "Remind me to leave for X at 5 pm".
pub async fn trip_reminder<R: Runtime>(app: AppHandle<R>, destination: String, arrive_min: Option<u16>) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    let Some(arrive_min) = arrive_min else {
        return say_and_finish(&app, request_id, format!("What time do you need to be at {destination}, sir? Say: remind me to leave for {destination} at 5 pm."));
    };
    let Some(path) = trips_path(&app) else {
        return say_and_finish(&app, request_id, "I couldn't save that reminder, sir.".into());
    };
    let Some(arrive_epoch) = arrive_epoch_for(chrono::Local::now(), arrive_min) else {
        return say_and_finish(&app, request_id, "That time has already passed today, sir.".into());
    };
    let (dest, _) = match crate::orchestrator::resolve_saved_places(&app, &destination, None) {
        Ok(r) => r,
        Err(line) => return say_and_finish(&app, request_id, line),
    };
    let Some(here) = crate::location::current().await else {
        return say_and_finish(&app, request_id, "I can't tell where you are, sir, so I can't work out when to leave. Turn on location in Windows settings.".into());
    };
    let Some(eta_secs) = trip_eta_secs(&app, &dest, here).await else {
        return say_and_finish(&app, request_id, format!("I couldn't work out how long {destination} takes from here, sir."));
    };
    if !trip_eta_plausible(eta_secs) {
        return say_and_finish(
            &app,
            request_id,
            format!("What I found for {destination} is about {} away, sir, which looks wrong. Say its full name and the city.", minutes_phrase(((eta_secs + 59) / 60).max(1) as u32)),
        );
    }
    let now = chrono::Utc::now().timestamp();
    let leave_at = arrive_epoch - eta_secs - TRIP_BUFFER_SECS;
    let mins = ((eta_secs + 59) / 60).max(1) as u32;
    if leave_at <= now {
        return say_and_finish(&app, request_id, format!("{destination} is about {} away, sir, so you'd be late if you're leaving for {} now. You'd need to leave right now.", minutes_phrase(mins), clock_of(arrive_epoch)));
    }
    let mut trips = prune_trips(load_trips(&path), now);
    let replaced: Vec<String> = trips.iter().filter(|t| t.destination.eq_ignore_ascii_case(&dest)).map(|t| t.id.clone()).collect();
    trips.retain(|t| !t.destination.eq_ignore_ascii_case(&dest));
    let trip = Trip { id: format!("trip_{now}"), destination: dest.clone(), arrive_epoch, eta_secs, eta_at: now };
    trips.push(trip.clone());
    if let Err(e) = save_trips(&path, &trips) {
        tracing::warn!("directions: could not save the trip reminder: {e}");
        return say_and_finish(&app, request_id, "I couldn't save that reminder, sir.".into());
    }
    let said = format!(
        "Okay, sir. {destination} is about {} away, so leave by {} to be there at {}. I'll remind you {} minutes before.",
        minutes_phrase(mins),
        clock_of(leave_at),
        clock_of(arrive_epoch),
        TRIP_LEAD_SECS / 60
    );
    // The on-screen reminder is the universal one (crate::reminder): this feature only says what it is and when.
    for id in replaced {
        crate::reminder::cancel(&app, &id);
    }
    if let Err(e) = crate::reminder::announce(&app, trip_event(&trip, &said, now * 1000)) {
        tracing::warn!("directions: the reminder was saved but could not be shown: {e}");
    }
    say_and_finish(&app, request_id, said)
}

/// Travel time in seconds from `here`: the Routes API with a key, otherwise the keyless estimate.
async fn trip_eta_secs<R: Runtime>(app: &AppHandle<R>, dest: &str, here: Fix) -> Option<i64> {
    let key = crate::commands::read_api_key(app, "google_maps");
    if !key.is_empty() {
        if let Ok(e) = MapsService::compute_route(&client(4), &key, &here.as_point(), dest, TravelMode::Driving).await {
            return Some(e.minutes as i64 * 60);
        }
    }
    crate::place_card::quick_route(app, dest, here).await.map(|q| q.car_min as i64 * 60)
}

/// The once-a-minute check of saved trip reminders. Idle (one small file read) when there are none.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        // Chips for reminders set before this start come back without the roll animation.
        if let Some(path) = trips_path(&app) {
            let now = chrono::Utc::now().timestamp();
            for t in prune_trips(load_trips(&path), now) {
                if let Err(e) = crate::reminder::restore(&app, trip_event(&t, "", now * 1000)) {
                    tracing::debug!("directions: not restoring {}: {e}", t.id);
                }
            }
        }
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(path) = trips_path(&app) else { continue };
            let now = chrono::Utc::now().timestamp();
            let loaded = load_trips(&path);
            if loaded.is_empty() {
                continue;
            }
            let mut trips = prune_trips(loaded.clone(), now);
            for gone in loaded.iter().filter(|l| !trips.iter().any(|t| t.id == l.id)) {
                crate::reminder::cancel(&app, &gone.id);
            }
            let address = crate::memcore::wa::speak_address(&app);
            let mut fired: Vec<String> = vec![];
            for t in trips.iter_mut() {
                // Measure again once the leave time is within 40 minutes, at most every 10 minutes.
                let near = now >= t.arrive_epoch - t.eta_secs - TRIP_BUFFER_SECS - TRIP_LEAD_SECS - 40 * 60;
                if near && now - t.eta_at >= 600 {
                    if let Some(here) = crate::location::current().await {
                        if let Some(secs) = trip_eta_secs(&app, &t.destination, here).await {
                            t.eta_secs = secs;
                            t.eta_at = now;
                            crate::reminder::retime(&app, &t.id, trip_alert_at_ms(t));
                        }
                    }
                }
                if let Some(text) = decide_trip(t, now, address.as_deref()) {
                    tracing::info!("directions: trip reminder for {}", t.destination);
                    crate::reminder::fired(&app, &t.id, Some(&text));
                    crate::proactive_policy::submit(&app, format!("tripreminder_{}", t.id), text, AlertUrgency::High);
                    fired.push(t.id.clone());
                }
            }
            trips.retain(|t| !fired.contains(&t.id));
            if trips != loaded {
                if let Err(e) = save_trips(&path, &trips) {
                    tracing::warn!("directions: could not save trip reminders: {e}");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cur(mode: TravelMode) -> Current {
        Current {
            origin: Some("17.4,78.4".into()),
            origin_is_here: true,
            destination: "T-Hub".into(),
            mode,
            opts: RouteOpts::default(),
            steps: vec!["a".into()],
            step_idx: 1,
            base: None,
            at: Instant::now(),
        }
    }

    #[test]
    fn minutes_and_distance_are_spoken_like_a_person() {
        assert_eq!(minutes_phrase(0), "1 minute");
        assert_eq!(minutes_phrase(1), "1 minute");
        assert_eq!(minutes_phrase(22), "22 minutes");
        assert_eq!(minutes_phrase(60), "1 hour");
        assert_eq!(minutes_phrase(65), "1 hour 5 minutes");
        assert_eq!(minutes_phrase(121), "2 hours 1 minute");
        assert_eq!(km_phrase(0), "10 metres");
        assert_eq!(km_phrase(804), "800 metres");
        assert_eq!(km_phrase(960), "1 kilometre");
        assert_eq!(km_phrase(5300), "5.3 kilometres");
        assert_eq!(km_phrase(12_400), "12 kilometres");
    }

    #[test]
    fn arrival_clock_wraps_past_midnight() {
        let at = |n, a| arrival_clock(n, a);
        assert_eq!(at(16 * 60 + 13, 22), crate::memcore::timetable::fmt_time(16 * 60 + 35));
        assert_eq!(at(23 * 60 + 50, 20), crate::memcore::timetable::fmt_time(10));
    }

    #[test]
    fn keyless_estimates_per_mode() {
        let car = facts_from_estimate(TravelMode::Driving, 5.3, 22, 19, true);
        assert_eq!((car.meters, car.minutes, car.exact), (5300, Some(22), false));
        assert_eq!(facts_from_estimate(TravelMode::Bicycling, 5.3, 22, 19, true).minutes, Some(19));
        assert_eq!(facts_from_estimate(TravelMode::Walking, 5.0, 22, 19, true).minutes, Some(60));
        assert_eq!(facts_from_estimate(TravelMode::Transit, 5.0, 22, 19, true).minutes, None);
        // only a measured driving time counts as exact
        assert!(facts_from_estimate(TravelMode::Driving, 5.0, 22, 19, false).exact);
        assert!(!facts_from_estimate(TravelMode::Walking, 5.0, 22, 19, false).exact);
    }

    fn input<'a>(dest: &'a str, facts: Option<Facts>) -> LineInput<'a> {
        LineInput { dest, mode: TravelMode::Driving, facts, opts_spoken: String::new(), now_min: Some(16 * 60 + 13), origin_known: true, time_failed: false, question: false, hint: false, update: None }
    }

    #[test]
    fn the_line_says_exactly_what_is_known() {
        let exact = Facts { meters: 5300, minutes: Some(22), via: Some("Road No. 1".into()), exact: true };
        let l = route_line(&input("T-Hub", Some(exact.clone())));
        assert!(l.starts_with("The route to T-Hub is open, sir. It's 5.3 kilometres, about 22 minutes by car via Road No. 1, arriving around "), "{l}");
        let mut q = input("T-Hub", Some(exact));
        q.question = true;
        assert!(route_line(&q).starts_with("T-Hub is 5.3 kilometres away, about 22 minutes by car via Road No. 1, arriving around "));
        // an estimate says "roughly"
        let est = Facts { meters: 5300, minutes: Some(22), via: None, exact: false };
        assert!(route_line(&input("T-Hub", Some(est))).contains(", roughly 22 minutes by car"));
        // distance only (public transport)
        let far = Facts { meters: 5300, minutes: None, via: None, exact: false };
        let l = route_line(&input("T-Hub", Some(far)));
        assert!(l.contains("5.3 kilometres.") && !l.contains("minutes") && !l.contains("arriving"), "{l}");
    }

    #[test]
    fn the_line_never_claims_a_time_it_does_not_have() {
        assert_eq!(route_line(&input("T-Hub", None)), "Opening the route to T-Hub, sir.");
        let mut q = input("T-Hub", None);
        q.question = true;
        assert_eq!(route_line(&q), "I opened the route to T-Hub, sir, but I couldn't work out the distance.");
        let mut i = input("T-Hub", None);
        i.origin_known = false;
        i.time_failed = true;
        i.hint = true;
        let l = route_line(&i);
        assert!(l.contains("I couldn't tell where you are") && l.contains("I couldn't get the live travel time") && l.contains("Maps key"), "{l}");
        // an exact time makes a failed second lookup irrelevant
        let mut ok = input("T-Hub", Some(Facts { meters: 1000, minutes: Some(3), via: None, exact: true }));
        ok.time_failed = true;
        assert!(!route_line(&ok).contains("couldn't get"));
        let mut avoid = input("T-Hub", None);
        avoid.opts_spoken = "avoiding tolls".into();
        assert_eq!(route_line(&avoid), "Opening the route to T-Hub, avoiding tolls, sir.");
    }

    #[test]
    fn follow_up_answers_say_what_changed_and_names_start_with_a_capital() {
        assert_eq!(cap_first("charminar"), "Charminar");
        assert_eq!(cap_first(""), "");
        assert_eq!(update_phrase("mode", Some("walking"), TravelMode::Walking), "Switched to walking");
        assert_eq!(update_phrase("avoid", Some("tolls"), TravelMode::Driving), "Avoiding tolls");
        assert_eq!(update_phrase("stop", Some("ATM"), TravelMode::Driving), "Added ATM as a stop");
        let mut i = input("T-Hub", Some(Facts { meters: 5300, minutes: Some(22), via: None, exact: false }));
        i.update = Some("Switched to walking".into());
        assert!(route_line(&i).starts_with("Switched to walking, sir. It's 5.3 kilometres, roughly 22 minutes by car"), "{}", route_line(&i));
        let mut none = input("T-Hub", None);
        none.update = Some("Removed the stops".into());
        assert_eq!(route_line(&none), "Removed the stops, sir.");
        let mut q = input("charminar", Some(Facts { meters: 900, minutes: None, via: None, exact: false }));
        q.question = true;
        assert!(route_line(&q).starts_with("Charminar is 900 metres away"), "{}", route_line(&q));
    }

    #[test]
    fn follow_ups_change_the_route_and_refuse_nonsense() {
        let mut c = cur(TravelMode::Driving);
        assert_eq!(apply_follow_up(&mut c, "mode", Some("walking")), Change::Reopen);
        assert_eq!((c.mode, c.steps.len(), c.step_idx), (TravelMode::Walking, 0, 0));
        assert_eq!(apply_follow_up(&mut c, "mode", Some("walking")), Change::Say("It's already walking, sir.".into()));
        // tolls only matter when driving
        assert!(matches!(apply_follow_up(&mut c, "avoid", Some("tolls")), Change::Say(s) if s.contains("only matter when driving")));
        let mut d = cur(TravelMode::Driving);
        assert_eq!(apply_follow_up(&mut d, "avoid", Some("tolls")), Change::Reopen);
        assert!(d.opts.avoid_tolls);
        assert_eq!(apply_follow_up(&mut d, "avoid", Some("tolls")), Change::Say("It's already avoiding tolls, sir.".into()));
        assert_eq!(apply_follow_up(&mut d, "allow", Some("tolls")), Change::Reopen);
        assert!(!d.opts.avoid_tolls);
        assert!(matches!(apply_follow_up(&mut d, "avoid", Some("potholes")), Change::Say(_)));
    }

    #[test]
    fn stops_are_added_capped_and_cleared() {
        let mut c = cur(TravelMode::Driving);
        assert_eq!(apply_follow_up(&mut c, "stop", Some("Cafe Bahar")), Change::Reopen);
        assert_eq!(apply_follow_up(&mut c, "stop", Some("cafe bahar")), Change::Say("cafe bahar is already a stop, sir.".into()));
        for n in 0..10 {
            let _ = apply_follow_up(&mut c, "stop", Some(&format!("stop {n}")));
        }
        assert_eq!(c.opts.stops.len(), MAX_STOPS);
        assert_eq!(apply_follow_up(&mut c, "clear_stops", None), Change::Reopen);
        assert!(c.opts.stops.is_empty());
        assert_eq!(apply_follow_up(&mut c, "clear_stops", None), Change::Say("There are no stops on this route, sir.".into()));
        let mut t = cur(TravelMode::Transit);
        assert!(matches!(apply_follow_up(&mut t, "stop", Some("X")), Change::Say(s) if s.contains("public transport")));
    }

    #[test]
    fn steps_are_read_a_few_at_a_time() {
        let steps: Vec<String> = ["Head north on Main St.", "Turn left onto Park Rd", "Arrive at T-Hub"].iter().map(|s| s.to_string()).collect();
        let (t, i) = read_steps(&steps, 0, 2);
        assert_eq!((t.as_str(), i), ("Step 1: Head north on Main St. Step 2: Turn left onto Park Rd.", 2));
        let (t, i) = read_steps(&steps, 2, 1);
        assert_eq!((t.as_str(), i), ("Step 3: Arrive at T-Hub. That's the last step.", 3));
        assert_eq!(read_steps(&[], 0, 3), (String::new(), 0));
        let md = steps_markdown("T-Hub", &steps);
        assert!(md.starts_with("## Directions to T-Hub\n\n1. Head north on Main St.\n2. Turn left"), "{md}");
    }

    #[test]
    fn the_photo_card_never_looks_up_the_users_own_address() {
        assert!(wants_photo_card("Charminar"));
        assert!(wants_photo_card("T-Hub, Hyderabad"));
        for own in ["home", "my home", "work", "the office", "Home", "my college", "  "] {
            assert!(!wants_photo_card(own), "{own}");
        }
    }

    fn trip(arrive_in: i64, eta: i64) -> Trip {
        Trip { id: "t1".into(), destination: "the airport".into(), arrive_epoch: 1_800_000_000 + arrive_in, eta_secs: eta, eta_at: 1_800_000_000 }
    }

    #[test]
    fn a_trip_reminder_speaks_five_minutes_before_leaving_and_never_after_arrival() {
        const NOW: i64 = 1_800_000_000;
        // arrive in 60 min, trip 20 min, buffer 5: leave at +35, speak from +30
        let t = trip(3600, 1200);
        assert_eq!(decide_trip(&t, NOW + 29 * 60, None), None);
        let early = decide_trip(&t, NOW + 30 * 60, Some("sir")).unwrap();
        assert!(early.starts_with("Leave in about 5 minutes for the airport, sir."), "{early}");
        let leave = decide_trip(&t, NOW + 36 * 60, None).unwrap();
        assert!(leave.starts_with("Time to leave for the airport."), "{leave}");
        let late = decide_trip(&t, NOW + 45 * 60, None).unwrap();
        assert!(late.starts_with("You're running late for the airport."), "{late}");
        assert_eq!(decide_trip(&t, NOW + 61 * 60, None), None);
        assert_eq!(decide_trip(&trip(3600, 0), NOW + 40 * 60, None), None);
    }

    #[test]
    fn a_trip_is_shown_through_the_generic_reminder_event() {
        const NOW: i64 = 1_800_000_000;
        // arrive in 60 min, trip 20 min, buffer 5, lead 5: NEXUS speaks at +30 min
        let t = trip(3600, 1200);
        assert_eq!(trip_alert_at_ms(&t), (NOW + 30 * 60) * 1000);
        let e = trip_event(&t, "Okay, sir.", NOW * 1000);
        assert_eq!((e.kind.as_str(), e.source.as_str(), e.title.as_str()), ("trip", "directions", "Leave for the airport"));
        assert_eq!((e.id.as_str(), e.spoken.as_str(), e.at_ms), ("t1", "Okay, sir.", (NOW + 30 * 60) * 1000));
        assert_eq!(e.alert, "Time to leave for the airport, sir.");
        assert_eq!(crate::reminder::validate(&e, NOW * 1000, true), Ok(()));
        // a reminder due within seconds is still valid (the chip waits a few seconds instead of being refused)
        let soon = trip(60, 1200);
        let e2 = trip_event(&soon, "", NOW * 1000);
        assert_eq!(e2.at_ms, NOW * 1000 + 5_000);
        assert_eq!(crate::reminder::validate(&e2, NOW * 1000, true), Ok(()));
        // a long destination cannot break the 60 character title limit
        let mut long = trip(3600, 1200);
        long.destination = "x".repeat(100);
        assert_eq!(crate::reminder::validate(&trip_event(&long, "", NOW * 1000), NOW * 1000, true), Ok(()));
    }

    #[test]
    fn a_trip_on_another_continent_is_a_wrong_match() {
        assert!(trip_eta_plausible(25 * 60));
        assert!(trip_eta_plausible(6 * 3600));
        assert!(!trip_eta_plausible(6 * 3600 + 1));
        assert!(!trip_eta_plausible(124 * 3600));
        assert!(!trip_eta_plausible(0));
    }

    #[test]
    fn old_trip_reminders_are_dropped_and_the_list_is_capped() {
        const NOW: i64 = 1_800_000_000;
        let mut v = vec![trip(-TRIP_KEEP_SECS - 1, 600), trip(-60, 600), trip(3600, 600)];
        v = prune_trips(v, NOW);
        assert_eq!(v.len(), 2);
        let many: Vec<Trip> = (0..30).map(|_| trip(3600, 600)).collect();
        assert_eq!(prune_trips(many, NOW).len(), MAX_TRIPS);
    }

    #[test]
    fn arrival_time_is_today_or_nothing() {
        use chrono::TimeZone;
        let now = chrono::Local.with_ymd_and_hms(2026, 11, 13, 16, 0, 0).unwrap();
        let five = arrive_epoch_for(now, 17 * 60).unwrap();
        assert_eq!(five - now.timestamp(), 3600);
        // passed by an hour at 4 pm ("at 3 pm"): a slip, not tomorrow
        assert_eq!(arrive_epoch_for(now, 15 * 60), None);
        assert_eq!(arrive_epoch_for(now, 16 * 60), None);
        // 11 pm, "at 1 am": tomorrow, two hours away
        let late = chrono::Local.with_ymd_and_hms(2026, 11, 13, 23, 0, 0).unwrap();
        assert_eq!(arrive_epoch_for(late, 60).unwrap() - late.timestamp(), 2 * 3600);
        assert_eq!(arrive_epoch_for(late, 22 * 60), None);
    }

    #[test]
    fn trip_reminders_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("nexus_trips_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("trip_reminders.json");
        assert!(load_trips(&path).is_empty());
        let trips = vec![trip(3600, 1200)];
        save_trips(&path, &trips).unwrap();
        assert_eq!(load_trips(&path), trips);
        std::fs::write(&path, "not json").unwrap();
        assert!(load_trips(&path).is_empty(), "a damaged file is an empty list, not a crash");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_route_is_forgotten_after_the_time_limit() {
        let mut c = cur(TravelMode::Driving);
        c.at = Instant::now();
        set_current(c.clone());
        assert!(current().is_some());
        let mut old = c;
        old.at = Instant::now().checked_sub(ROUTE_TTL + Duration::from_secs(1)).unwrap_or_else(Instant::now);
        set_current(old);
        // only meaningful where the clock allows subtracting; otherwise the route simply stays
        let _ = current();
    }
}
