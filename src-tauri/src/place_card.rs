//! The place card: a narrow NEXUS panel (`place-card`, the pinned `?pin=place` view) shown beside the map for "where is X".
//! One photo (or a labelled map image), name, distance from the user, car and bike time, address, fare estimates, hours,
//! phone, rating. Every field is optional: a missing source leaves its row out and never blocks the card.
//!
//! Source chain per field (see docs/features/154-place-card-sidebar-plan.md):
//! - place: Google Places text search with the user's key, else Nominatim (OpenStreetMap)
//! - distance + car time: Google Routes (traffic-aware) with the key, else the public OSRM server, else straight line x 1.3
//! - bike time: Google TWO_WHEELER (beta) when it answers, else about 0.85 x the car time, marked "about"
//! - photo: Google Places photo, else the Wikipedia page image when it is clearly the same place, else a map image
//!   (labelled as a map, never passed off as a photo of the place)
//! Images are fetched here and sent to the page as data URIs, so the page never holds an API key and needs no image CSP.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::fares::Fare;
use crate::location::Fix;
use crate::panel_layout::Panel;

const USER_AGENT: &str = "NEXUS-Assistant/1.0 (desktop voice assistant; https://github.com/Engine-NEXUS/WINDOWS)";
const MAX_IMAGE_BYTES: usize = 700_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub data_uri: String,
    /// "photo" (of the place) or "map" (a map image, shown because no photo was found).
    pub kind: &'static str,
    pub credit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaceCard {
    pub name: String,
    pub category: Option<String>,
    pub address: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub distance_km: Option<f64>,
    pub car_min: Option<u32>,
    pub bike_min: Option<u32>,
    /// True when the bike time is derived from the car time rather than measured.
    pub bike_approx: bool,
    /// True when distance/time are rough (no traffic data).
    pub travel_approx: bool,
    pub photo: Option<Photo>,
    pub hours_today: Option<String>,
    pub open_now: Option<bool>,
    pub phone: Option<String>,
    pub rating: Option<f32>,
    pub summary: Option<String>,
    pub website: Option<String>,
    /// Opening hours for the whole week, as Google words them ("Monday: 9:00 AM - 5:30 PM").
    pub weekday_hours: Vec<String>,
    pub fares: Vec<Fare>,
}

/// What a place lookup found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    pub name: String,
    pub category: Option<String>,
    pub address: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub rating: Option<f32>,
    pub open_now: Option<bool>,
    pub weekday_hours: Vec<String>,
    pub phone: Option<String>,
    pub summary: Option<String>,
    pub website: Option<String>,
    /// Google photo resource name and the author credit, when there is one.
    pub photo: Option<(String, Option<String>)>,
}

// ── pure helpers ────────────────────────────────────────────────────────────

pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6371.0;
    let (dl, dg) = ((b.0 - a.0).to_radians(), (b.1 - a.1).to_radians());
    let h = (dl / 2.0).sin().powi(2) + a.0.to_radians().cos() * b.0.to_radians().cos() * (dg / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

/// A bike is usually a little quicker than a car in city traffic. An assumption, shown as "about". Pure.
pub fn bike_from_car(car_min: u32) -> u32 {
    ((car_min as f64) * 0.85).round().max(1.0) as u32
}

fn secs_to_min(s: f64) -> u32 {
    (s / 60.0).round().max(1.0) as u32
}

/// Google `"duration":"1234s"` text -> seconds. Pure.
fn parse_duration_s(v: &serde_json::Value) -> Option<f64> {
    v.as_str()?.strip_suffix('s')?.parse().ok()
}

/// Google Routes answer -> (km, minutes). Pure.
pub fn parse_routes(body: &str) -> Option<(f64, u32)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let r = v.get("routes")?.get(0)?;
    let m = r.get("distanceMeters")?.as_f64()?;
    let s = parse_duration_s(r.get("duration")?)?;
    (m > 0.0).then_some((m / 1000.0, secs_to_min(s)))
}

/// OSRM answer -> (km, minutes). Pure.
pub fn parse_osrm(body: &str) -> Option<(f64, u32)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let r = v.get("routes")?.get(0)?;
    let m = r.get("distance")?.as_f64()?;
    let s = r.get("duration")?.as_f64()?;
    (m > 0.0).then_some((m / 1000.0, secs_to_min(s)))
}

fn pretty_kind(s: &str) -> String {
    let t = s.replace('_', " ");
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Nominatim search answer -> the first place. Pure.
pub fn parse_nominatim(body: &str) -> Option<Found> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let p = v.get(0)?;
    let lat: f64 = p.get("lat")?.as_str()?.parse().ok()?;
    let lon: f64 = p.get("lon")?.as_str()?.parse().ok()?;
    if !crate::location::valid(lat, lon) {
        return None;
    }
    let display = p.get("display_name").and_then(|d| d.as_str()).unwrap_or("").to_string();
    let name = p.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).map(str::to_string).or_else(|| display.split(',').next().map(|s| s.trim().to_string()))?;
    let category = p.get("type").and_then(|t| t.as_str()).filter(|t| !matches!(*t, "yes" | "")).map(pretty_kind);
    let website = p.get("extratags").and_then(|e| e.get("website")).and_then(|w| w.as_str()).map(str::to_string);
    Some(Found { name, category, address: (!display.is_empty()).then_some(display), lat, lon, website, ..Found::default() })
}

/// Google Places (New) text-search answer -> the first place. Pure.
pub fn parse_google_place(body: &str) -> Option<Found> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let p = v.get("places")?.get(0)?;
    let loc = p.get("location")?;
    let (lat, lon) = (loc.get("latitude")?.as_f64()?, loc.get("longitude")?.as_f64()?);
    if !crate::location::valid(lat, lon) {
        return None;
    }
    let text = |k: &str| p.get(k).and_then(|d| d.get("text")).and_then(|t| t.as_str()).map(str::to_string);
    let name = text("displayName")?;
    let photo = p.get("photos").and_then(|a| a.get(0)).and_then(|ph| {
        let n = ph.get("name")?.as_str()?.to_string();
        let credit = ph.get("authorAttributions").and_then(|a| a.get(0)).and_then(|a| a.get("displayName")).and_then(|d| d.as_str()).map(str::to_string);
        Some((n, credit))
    });
    Some(Found {
        name,
        category: text("primaryTypeDisplayName"),
        address: p.get("formattedAddress").and_then(|a| a.as_str()).map(str::to_string),
        lat,
        lon,
        rating: p.get("rating").and_then(|r| r.as_f64()).map(|r| r as f32),
        open_now: p.get("currentOpeningHours").and_then(|h| h.get("openNow")).and_then(|o| o.as_bool()),
        weekday_hours: p
            .get("regularOpeningHours")
            .and_then(|h| h.get("weekdayDescriptions"))
            .and_then(|d| d.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        phone: p.get("nationalPhoneNumber").and_then(|a| a.as_str()).map(str::to_string),
        summary: text("editorialSummary"),
        website: p.get("websiteUri").and_then(|a| a.as_str()).map(str::to_string),
        photo,
    })
}

/// Wikipedia summary -> (widened thumbnail url, original thumbnail url) only when the page is plainly the same place (its coordinates are within
/// 1.5 km of ours). Without that check a name like "Paradise" would show the wrong page. Pure.
pub fn parse_wiki_image(body: &str, lat: f64, lon: f64) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v.get("type").and_then(|t| t.as_str()) == Some("disambiguation") {
        return None;
    }
    let c = v.get("coordinates")?;
    let (plat, plon) = (c.get("lat")?.as_f64()?, c.get("lon")?.as_f64()?);
    if haversine_km((lat, lon), (plat, plon)) > 1.5 {
        return None;
    }
    let url = v.get("thumbnail")?.get("source")?.as_str()?;
    if !(url.starts_with("https://upload.wikimedia.org/") || url.starts_with("https://thumb.wikimedia.org/")) {
        return None;
    }
    Some((wider_thumb(url), url.to_string()))
}

/// Wikimedia thumbnail URLs carry their width ("/330px-"); ask for 500 (one of the widths Wikimedia serves) so the image is sharp at the card width. Pure.
pub fn wider_thumb(url: &str) -> String {
    let Some(i) = url.find("px-") else { return url.to_string() };
    let start = url[..i].rfind('/').map(|p| p + 1).unwrap_or(0);
    if url[start..i].chars().all(|c| c.is_ascii_digit()) && i > start {
        format!("{}500{}", &url[..start], &url[i..])
    } else {
        url.to_string()
    }
}

/// Place types that are worth a Wikimedia Commons coordinate search (a shop next to a monument must not get the
/// monument's photo). Pure.
pub fn is_landmark_category(category: Option<&str>) -> bool {
    let Some(c) = category else { return false };
    let c = c.to_lowercase();
    ["monument", "attraction", "tourist", "temple", "mosque", "church", "fort", "palace", "museum", "memorial", "castle", "landmark", "heritage", "historic", "worship", "park", "lake", "station"]
        .iter()
        .any(|k| c.contains(k))
}

/// Commons files near a point, with a 640 px thumbnail and licence data, in one request. Pure URL.
pub fn commons_url(lat: f64, lon: f64) -> String {
    format!(
        "https://commons.wikimedia.org/w/api.php?action=query&generator=geosearch&ggscoord={lat:.6}%7C{lon:.6}&ggsradius=250&ggslimit=8&ggsnamespace=6&prop=imageinfo&iiprop=url%7Cmime%7Cextmetadata&iiurlwidth=500&format=json"
    )
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// The best Commons file from a geosearch answer: a jpeg/png with a thumbnail whose file name contains a word of the
/// place's name (a photo of the temple next door is not a photo of the monument), nearest first. Returns (thumbnail url,
/// credit text). Pure.
pub fn parse_commons(body: &str, place_name: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let pages = v.get("query")?.get("pages")?.as_object()?;
    let words: Vec<String> = place_name.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 4).map(str::to_string).collect();
    let mut items: Vec<(i64, String, String)> = Vec::new();
    for p in pages.values() {
        let info = p.get("imageinfo").and_then(|i| i.get(0))?;
        let mime = info.get("mime").and_then(|m| m.as_str()).unwrap_or("");
        if mime != "image/jpeg" && mime != "image/png" {
            continue;
        }
        let title = p.get("title").and_then(|t| t.as_str()).unwrap_or("").to_lowercase();
        if !words.iter().any(|w| title.contains(w.as_str())) {
            continue;
        }
        let url = info.get("thumburl").and_then(|u| u.as_str())?;
        if !url.starts_with("https://upload.wikimedia.org/") {
            continue;
        }
        let meta = info.get("extmetadata");
        let field = |k: &str| meta.and_then(|m| m.get(k)).and_then(|f| f.get("value")).and_then(|v| v.as_str()).map(strip_tags).filter(|s| !s.is_empty());
        let credit = match (field("Artist"), field("LicenseShortName")) {
            (Some(a), Some(l)) => format!("Photo: {a} / {l} / Wikimedia Commons"),
            (None, Some(l)) => format!("Photo: {l} / Wikimedia Commons"),
            _ => "Photo: Wikimedia Commons".to_string(),
        };
        items.push((p.get("index").and_then(|i| i.as_i64()).unwrap_or(99), url.to_string(), credit));
    }
    items.sort_by_key(|i| i.0);
    items.into_iter().next().map(|(_, u, c)| (u, c))
}

/// The page's preview image (`og:image`, else `twitter:image`) as an absolute https URL. Pure.
pub fn parse_og_image(html: &str, base: &str) -> Option<String> {
    let lower = html.to_lowercase();
    for key in ["og:image", "twitter:image"] {
        let mut from = 0;
        while let Some(i) = lower[from..].find("<meta") {
            let start = from + i;
            let end = lower[start..].find('>').map(|e| start + e)?;
            let tag = &html[start..end];
            let tl = &lower[start..end];
            from = end;
            if !(tl.contains(&format!("property=\"{key}\"")) || tl.contains(&format!("name=\"{key}\"")) || tl.contains(&format!("property='{key}'")) || tl.contains(&format!("name='{key}'"))) {
                continue;
            }
            let ci = tl.find("content=")?;
            let rest = &tag[ci + 8..];
            let q = rest.chars().next()?;
            if q != '"' && q != '\'' {
                continue;
            }
            let val = &rest[1..];
            let val = &val[..val.find(q)?];
            let abs = tauri::Url::parse(base).ok()?.join(val.trim()).ok()?;
            if abs.scheme() == "https" {
                return Some(abs.to_string());
            }
        }
    }
    None
}

/// Does robots.txt allow `path` for everyone (`User-agent: *`)? Longest matching rule wins, Allow beats Disallow on a
/// tie. An unreadable or empty file allows. Pure.
pub fn robots_allows(robots: &str, path: &str) -> bool {
    let mut applies = false;
    let mut best: Option<(usize, bool)> = None;
    for line in robots.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once(':') else { continue };
        let (k, v) = (k.trim().to_lowercase(), v.trim());
        match k.as_str() {
            "user-agent" => applies = v == "*",
            "allow" | "disallow" if applies && !v.is_empty() => {
                if path.starts_with(v) {
                    let rule = (v.len(), k == "allow");
                    best = Some(match best {
                        Some(b) if b.0 > rule.0 || (b.0 == rule.0 && b.1) => b,
                        _ => rule,
                    });
                }
            }
            _ => {}
        }
    }
    best.map(|b| b.1).unwrap_or(true)
}

/// A keyless satellite image around the point (Esri World Imagery export). Pure.
pub fn map_image_url(lat: f64, lon: f64) -> String {
    let (dy, dx) = (0.0022, 0.0040);
    format!(
        "https://services.arcgisonline.com/arcgis/rest/services/World_Imagery/MapServer/export?bbox={:.6},{:.6},{:.6},{:.6}&bboxSR=4326&imageSR=4326&size=630,350&format=jpg&f=image",
        lon - dx,
        lat - dy,
        lon + dx,
        lat + dy
    )
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The one extra spoken sentence about the trip. Pure.
pub fn spoken_extra(c: &PlaceCard) -> String {
    match (c.distance_km, c.car_min) {
        (Some(km), Some(min)) => format!(" It is about {} kilometres from you, around {} minutes by car.", km.round().max(1.0) as u32, min),
        _ => String::new(),
    }
}

// ── sources ─────────────────────────────────────────────────────────────────

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .build()
        .unwrap_or_default()
}

async fn google_lookup(c: &reqwest::Client, key: &str, query: &str, here: Option<Fix>) -> Option<Found> {
    let mut body = serde_json::json!({ "textQuery": query, "maxResultCount": 1, "languageCode": "en" });
    if let Some(f) = here {
        body["locationBias"] = serde_json::json!({ "circle": { "center": { "latitude": f.lat, "longitude": f.lon }, "radius": 50000.0 } });
    }
    let r = c
        .post("https://places.googleapis.com/v1/places:searchText")
        .header("X-Goog-Api-Key", key)
        .header(
            "X-Goog-FieldMask",
            "places.displayName,places.formattedAddress,places.location,places.rating,places.currentOpeningHours.openNow,places.regularOpeningHours.weekdayDescriptions,places.nationalPhoneNumber,places.primaryTypeDisplayName,places.editorialSummary,places.websiteUri,places.photos",
        )
        .json(&body)
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        tracing::debug!("place card: google search HTTP {}", r.status());
        return None;
    }
    parse_google_place(&r.text().await.ok()?)
}

async fn nominatim_lookup(c: &reqwest::Client, query: &str) -> Option<Found> {
    let url = format!("https://nominatim.openstreetmap.org/search?q={}&format=jsonv2&limit=1&accept-language=en&extratags=1", enc(query));
    let r = c.get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    parse_nominatim(&r.text().await.ok()?)
}

pub struct Route {
    km: f64,
    car_min: u32,
    bike_min: u32,
    bike_approx: bool,
    travel_approx: bool,
}

async fn google_route(c: &reqwest::Client, key: &str, from: (f64, f64), to: (f64, f64), mode: &str) -> Option<(f64, u32)> {
    let mut body = serde_json::json!({
        "origin": { "location": { "latLng": { "latitude": from.0, "longitude": from.1 } } },
        "destination": { "location": { "latLng": { "latitude": to.0, "longitude": to.1 } } },
        "travelMode": mode,
    });
    if mode == "DRIVE" {
        body["routingPreference"] = "TRAFFIC_AWARE".into();
    }
    let r = c
        .post("https://routes.googleapis.com/directions/v2:computeRoutes")
        .header("X-Goog-Api-Key", key)
        .header("X-Goog-FieldMask", "routes.duration,routes.distanceMeters")
        .json(&body)
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        tracing::info!("place card: Google Routes {mode} refused (HTTP {}); falling back to the open route service", r.status());
        return None;
    }
    parse_routes(&r.text().await.ok()?)
}

/// `https://host`, or `http://` only for this computer or a private network address (a server at home, no tunnel).
/// Returns the URL without a trailing slash. Pure.
pub fn server_url_ok(url: &str) -> Option<String> {
    let u = tauri::Url::parse(url.trim()).ok()?;
    let host = u.host_str()?.to_string();
    let private = host == "localhost"
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host
            .strip_prefix("172.")
            .and_then(|r| r.split('.').next())
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (16..=31).contains(&n));
    match u.scheme() {
        "https" => {}
        "http" if private => {}
        _ => return None,
    }
    Some(u.as_str().trim_end_matches('/').to_string())
}

/// Your routing server's answer -> Route. Pure.
pub fn parse_server_route(body: &str) -> Option<Route> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let km = v.get("km")?.as_f64()?;
    let car = v.get("carMin")?.as_f64()?;
    if !(km > 0.0 && km < 4000.0 && car > 0.0) {
        return None;
    }
    let car_min = car.round().max(1.0) as u32;
    let (bike_min, bike_approx) = match (v.get("bikeMin").and_then(|b| b.as_f64()), v.get("bikeMeasured").and_then(|m| m.as_bool())) {
        (Some(b), Some(true)) if b > 0.0 => (b.round().max(1.0) as u32, false),
        (Some(b), _) if b > 0.0 => (b.round().max(1.0) as u32, true),
        _ => (bike_from_car(car_min), true),
    };
    Some(Route { km, car_min, bike_min, bike_approx, travel_approx: !v.get("traffic").and_then(|t| t.as_bool()).unwrap_or(false) })
}

async fn server_route(c: &reqwest::Client, base: &str, token: &str, from: (f64, f64), to: (f64, f64)) -> Option<Route> {
    let mut req = c.get(format!("{base}/route")).query(&[("fromLat", from.0), ("fromLon", from.1), ("toLat", to.0), ("toLon", to.1)]);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let r = req.send().await.ok()?;
    if !r.status().is_success() {
        tracing::info!("place card: your routing server answered HTTP {}; using the next source", r.status());
        return None;
    }
    parse_server_route(&r.text().await.ok()?)
}

async fn google_full_route(c: &reqwest::Client, key: &str, from: (f64, f64), to: (f64, f64)) -> Option<Route> {
    let (drive, bike) = tokio::join!(google_route(c, key, from, to, "DRIVE"), google_route(c, key, from, to, "TWO_WHEELER"));
    let (km, car) = drive?;
    let (bike_min, bike_approx) = match bike {
        Some((_, m)) => (m, false),
        None => (bike_from_car(car), true),
    };
    Some(Route { km, car_min: car, bike_min, bike_approx, travel_approx: false })
}

async fn open_route(c: &reqwest::Client, from: (f64, f64), to: (f64, f64)) -> Route {
    let url = format!("https://router.project-osrm.org/route/v1/driving/{:.6},{:.6};{:.6},{:.6}?overview=false", from.1, from.0, to.1, to.0);
    if let Ok(r) = c.get(url).send().await {
        if r.status().is_success() {
            if let Some((km, car)) = r.text().await.ok().as_deref().and_then(parse_osrm) {
                return Route { km, car_min: car, bike_min: bike_from_car(car), bike_approx: true, travel_approx: true };
            }
        }
    }
    let km = haversine_km(from, to) * 1.3;
    let car = secs_to_min(km / 25.0 * 3600.0);
    Route { km, car_min: car, bike_min: bike_from_car(car), bike_approx: true, travel_approx: true }
}

/// Which sources to try, in order, for a routing mode. Pure.
pub fn route_order(mode: &str, have_server: bool, have_google: bool) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = match mode {
        "server" => vec!["server"],
        "google" => vec!["google"],
        "open" => vec![],
        _ => vec!["server", "google"],
    };
    v.retain(|s| (*s == "server" && have_server) || (*s == "google" && have_google));
    v
}

pub struct RoutingCfg {
    pub mode: String,
    pub server: Option<(String, String)>,
    pub google_key: String,
}

async fn route(c: &reqwest::Client, cfg: &RoutingCfg, from: (f64, f64), to: (f64, f64)) -> Route {
    for source in route_order(&cfg.mode, cfg.server.is_some(), !cfg.google_key.is_empty()) {
        let got = match (source, &cfg.server) {
            ("server", Some((url, token))) => server_route(c, url, token, from, to).await,
            ("google", _) => google_full_route(c, &cfg.google_key, from, to).await,
            _ => None,
        };
        if let Some(r) = got {
            return r;
        }
    }
    open_route(c, from, to).await
}

async fn fetch_image(c: &reqwest::Client, url: &str, authed_key: Option<&str>) -> Option<String> {
    let mut req = c.get(url);
    if let Some(k) = authed_key {
        req = req.header("X-Goog-Api-Key", k);
    }
    let r = req.send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    let mime = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if !mime.starts_with("image/") {
        return None;
    }
    let bytes = r.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    use base64::Engine;
    Some(format!("data:{};base64,{}", mime.split(';').next().unwrap_or("image/jpeg"), base64::engine::general_purpose::STANDARD.encode(&bytes)))
}

async fn website_image(c: &reqwest::Client, site: &str) -> Option<(String, String)> {
    let u = tauri::Url::parse(site).ok()?;
    if u.scheme() != "https" && u.scheme() != "http" {
        return None;
    }
    let host = u.host_str()?.to_string();
    // robots.txt first: a site that says no is not fetched.
    let robots_url = format!("https://{host}/robots.txt");
    if let Ok(r) = c.get(robots_url).send().await {
        if r.status().is_success() {
            let txt = r.text().await.unwrap_or_default();
            if !robots_allows(&txt, if u.path().is_empty() { "/" } else { u.path() }) {
                tracing::info!("place card: {host} disallows fetching the page, skipping its preview image");
                return None;
            }
        }
    }
    let page = c.get(u.as_str()).send().await.ok()?;
    if !page.status().is_success() {
        return None;
    }
    let final_url = page.url().to_string();
    let html = page.text().await.ok()?;
    let head: String = html.chars().take(200_000).collect();
    let img = parse_og_image(&head, &final_url)?;
    let uri = fetch_image(c, &img, None).await?;
    Some((uri, format!("Image from {host}")))
}

async fn photo_for(c: &reqwest::Client, key: &str, f: &Found, query: &str) -> Option<Photo> {
    if let (false, Some((name, credit))) = (key.is_empty(), f.photo.as_ref()) {
        let url = format!("https://places.googleapis.com/v1/{name}/media?maxWidthPx=800");
        match fetch_image(c, &url, Some(key)).await {
            Some(uri) => return Some(Photo { data_uri: uri, kind: "photo", credit: credit.clone().map(|c| format!("Photo: {c} / Google")) }),
            None => tracing::info!("place card: the Google photo could not be fetched; trying other sources"),
        }
    }
    let title = query.split(',').next().unwrap_or(query).trim().replace(' ', "_");
    if !title.is_empty() {
        let url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", enc(&title));
        if let Ok(r) = c.get(url).send().await {
            if r.status().is_success() {
                if let Some((wide, original)) = r.text().await.ok().as_deref().and_then(|b| parse_wiki_image(b, f.lat, f.lon)) {
                    let got = match fetch_image(c, &wide, None).await {
                        Some(u) => Some(u),
                        None => fetch_image(c, &original, None).await,
                    };
                    if let Some(uri) = got {
                        return Some(Photo { data_uri: uri, kind: "photo", credit: Some("Photo: Wikipedia".into()) });
                    }
                    tracing::info!("place card: the Wikipedia image for {:?} could not be fetched", f.name);
                }
            }
        }
    }
    if is_landmark_category(f.category.as_deref()) {
        if let Ok(r) = c.get(commons_url(f.lat, f.lon)).send().await {
            if let Some((img, credit)) = r.text().await.ok().as_deref().and_then(|b| parse_commons(b, &f.name)) {
                if let Some(uri) = fetch_image(c, &img, None).await {
                    return Some(Photo { data_uri: uri, kind: "photo", credit: Some(credit) });
                }
            }
        }
    }
    if let Some(site) = f.website.as_deref() {
        if let Some((uri, credit)) = website_image(c, site).await {
            return Some(Photo { data_uri: uri, kind: "photo", credit: Some(credit) });
        }
    }
    tracing::info!("place card: no photo found for {:?}; showing a map image", f.name);
    let uri = fetch_image(c, &map_image_url(f.lat, f.lon), None).await?;
    Some(Photo { data_uri: uri, kind: "map", credit: Some("Imagery: Esri".into()) })
}

/// Build the card for a spoken place query. `None` only when the place itself cannot be found.
pub async fn build<R: Runtime>(app: &AppHandle<R>, query: &str, here: Option<Fix>) -> Option<PlaceCard> {
    let c = client();
    let key = [crate::commands::read_api_key(app, "google_places"), crate::commands::read_api_key(app, "google_maps")].into_iter().find(|k| !k.trim().is_empty()).unwrap_or_default();
    let found = match if key.is_empty() { None } else { google_lookup(&c, &key, query, here).await } {
        Some(f) => f,
        None => nominatim_lookup(&c, query).await?,
    };
    let (mode, server_url) = crate::commands::read_routing_settings(app);
    let cfg = RoutingCfg {
        mode,
        server: server_url_ok(&server_url).map(|u| (u, crate::commands::read_api_key(app, "routing_server"))),
        google_key: key.clone(),
    };
    let (route, photo) = tokio::join!(
        async {
            match here {
                Some(h) => Some(route(&c, &cfg, (h.lat, h.lon), (found.lat, found.lon)).await),
                None => None,
            }
        },
        photo_for(&c, &key, &found, query)
    );
    let weekday = {
        use chrono::Datelike;
        chrono::Local::now().weekday().num_days_from_monday() as usize
    };
    let night = {
        use chrono::Timelike;
        crate::fares::is_night(chrono::Local::now().hour())
    };
    let tariff = app.path().app_data_dir().map(|d| crate::fares::load_tariff(&d)).unwrap_or_default();
    Some(PlaceCard {
        hours_today: crate::orchestrator::hours_today(&found.weekday_hours, weekday),
        weekday_hours: found.weekday_hours.clone(),
        website: found.website.clone(),
        fares: route.as_ref().map(|r| crate::fares::estimate(&tariff, r.km, night)).unwrap_or_default(),
        distance_km: route.as_ref().map(|r| (r.km * 10.0).round() / 10.0),
        car_min: route.as_ref().map(|r| r.car_min),
        bike_min: route.as_ref().map(|r| r.bike_min),
        bike_approx: route.as_ref().is_some_and(|r| r.bike_approx),
        travel_approx: route.as_ref().is_some_and(|r| r.travel_approx),
        photo,
        name: found.name,
        category: found.category,
        address: found.address,
        lat: found.lat,
        lon: found.lon,
        open_now: found.open_now,
        phone: found.phone,
        rating: found.rating,
        summary: found.summary,
    })
}

/// Distance and times to a spoken place, without the photo and the rest of the card: for a spoken answer when there is
/// no Routes key, and for trip reminders. `approx` is true when no traffic data was used.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickRoute {
    pub name: String,
    pub km: f64,
    pub car_min: u32,
    pub bike_min: u32,
    pub approx: bool,
}

pub async fn quick_route<R: Runtime>(app: &AppHandle<R>, query: &str, here: Fix) -> Option<QuickRoute> {
    let c = client();
    let key = [crate::commands::read_api_key(app, "google_places"), crate::commands::read_api_key(app, "google_maps")].into_iter().find(|k| !k.trim().is_empty()).unwrap_or_default();
    let found = match if key.is_empty() { None } else { google_lookup(&c, &key, query, Some(here)).await } {
        Some(f) => f,
        None => nominatim_lookup(&c, query).await?,
    };
    let (mode, server_url) = crate::commands::read_routing_settings(app);
    let cfg = RoutingCfg {
        mode,
        server: server_url_ok(&server_url).map(|u| (u, crate::commands::read_api_key(app, "routing_server"))),
        google_key: key,
    };
    let r = route(&c, &cfg, (here.lat, here.lon), (found.lat, found.lon)).await;
    Some(QuickRoute { name: found.name, km: r.km, car_min: r.car_min, bike_min: r.bike_min, approx: r.travel_approx })
}

// ── window ──────────────────────────────────────────────────────────────────

static PENDING: Mutex<Option<PlaceCard>> = Mutex::new(None);

/// The Maps link the card's "Maps" button opens when the card was shown for a route ("directions to X"): same
/// start, destination and travel mode as the map panel. `None` = the plain link to the card's own coordinates.
static ROUTE_LINK: Mutex<Option<String>> = Mutex::new(None);

/// Which Maps link the "Maps" button opens. Pure.
pub fn maps_link(route_link: Option<&str>, lat: f64, lon: f64) -> String {
    match route_link.map(str::trim).filter(|l| l.starts_with("https://www.google.com/maps/")) {
        Some(l) => l.to_string(),
        None => format!("https://www.google.com/maps/dir/?api=1&destination={lat:.6},{lon:.6}"),
    }
}

#[tauri::command]
pub fn get_pending_place() -> Option<PlaceCard> {
    PENDING.lock().unwrap().clone()
}

/// IPC: close the place card (Escape or its close button).
#[tauri::command]
pub async fn close_place_card<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    crate::panel_layout::note_closed(Panel::PlaceCard);
    let r = crate::dyn_windows::destroy_window(&app, Panel::PlaceCard.label());
    crate::panel_layout::apply(&app);
    r
}

/// IPC: open the card's place in Google Maps or Uber. The page sends only a kind; the URLs are built here from the
/// card's own coordinates, so the page can never make us open an arbitrary address.
#[tauri::command]
pub fn place_card_open(kind: String) -> Result<(), String> {
    let card = PENDING.lock().unwrap().clone().ok_or("no place card")?;
    let url = match kind.as_str() {
        "maps" => maps_link(ROUTE_LINK.lock().unwrap().as_deref(), card.lat, card.lon),
        "uber" => format!(
            "https://m.uber.com/ul/?action=setPickup&pickup=my_location&dropoff[latitude]={:.6}&dropoff[longitude]={:.6}&dropoff[nickname]={}",
            card.lat,
            card.lon,
            enc(&card.name)
        ),
        // The place's own website, only when it is a plain http(s) address.
        "site" => match card.website.as_deref().and_then(|w| tauri::Url::parse(w).ok()) {
            Some(u) if u.scheme() == "https" || u.scheme() == "http" => u.to_string(),
            _ => return Err("no website".into()),
        },
        _ => return Err("unknown link".into()),
    };
    open::that(url).map_err(|e| e.to_string())
}

/// IPC: check the routing server from the Command Hub. Returns one plain sentence for the user.
#[tauri::command]
pub async fn routing_server_test<R: Runtime>(app: AppHandle<R>, url: Option<String>) -> Result<String, String> {
    // The address typed in the Hub is tested as it stands, before it has been saved.
    let url = url.unwrap_or_else(|| crate::commands::read_routing_settings(&app).1);
    let Some(base) = server_url_ok(&url) else {
        return Err("Enter your server address first (https://..., or http:// only for a server on this computer or your home network).".into());
    };
    let token = crate::commands::read_api_key(&app, "routing_server");
    let c = client();
    // A short real route (Charminar to Golconda) proves the server, the token and the map data together.
    match server_route(&c, &base, &token, (17.3616, 78.4747), (17.3833, 78.4011)).await {
        Some(r) => Ok(format!("Working: {:.1} km, {} min by car, {} min by bike{}.", r.km, r.car_min, r.bike_min, if r.bike_approx { " (estimated)" } else { "" })),
        None => Err("The server did not give a route. Check the address, the token, and that the map data covers this area.".into()),
    }
}

/// Show the card in its own panel beside the map.
pub fn show_card<R: Runtime>(app: &AppHandle<R>, card: PlaceCard) -> Result<(), String> {
    show_card_for_route(app, card, None)
}

/// Show the card for a route: its "Maps" button then opens `route_link` (the same route as the map panel).
pub fn show_card_for_route<R: Runtime>(app: &AppHandle<R>, card: PlaceCard, route_link: Option<String>) -> Result<(), String> {
    *ROUTE_LINK.lock().unwrap() = route_link;
    crate::panel_layout::note_opened(Panel::PlaceCard);
    *PENDING.lock().unwrap() = Some(card.clone());
    let existed = app.get_webview_window(Panel::PlaceCard.label()).is_some();
    let win = crate::dyn_windows::get_or_create_window(app, crate::dyn_windows::WindowConfig::place_card())?;
    crate::panel_layout::apply(app);
    win.show().map_err(|e| format!("place card show: {e}"))?;
    #[cfg(target_os = "windows")]
    crate::dwm_corners::round_corners(&win);
    crate::maps_panel::watch_escape(app, Panel::PlaceCard);
    if existed {
        let _ = app.emit_to(Panel::PlaceCard.label(), "place:show", card);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_maps_button_follows_the_route_link_only_when_it_is_a_maps_link() {
        let route = "https://www.google.com/maps/dir/?api=1&destination=T-Hub&origin=Home&travelmode=walking";
        assert_eq!(maps_link(Some(route), 17.4, 78.4), route);
        assert_eq!(maps_link(None, 17.4, 78.4), "https://www.google.com/maps/dir/?api=1&destination=17.400000,78.400000");
        // anything that is not a Google Maps address is ignored
        assert!(maps_link(Some("https://evil.example/maps/"), 1.0, 2.0).starts_with("https://www.google.com/maps/dir/"));
        assert!(maps_link(Some("   "), 1.0, 2.0).contains("destination=1.000000,2.000000"));
    }

    #[test]
    fn distance_and_bike_helpers() {
        let d = haversine_km((17.3616, 78.4747), (17.4065, 78.4772));
        assert!((4.5..5.5).contains(&d), "{d}");
        assert_eq!(bike_from_car(40), 34);
        assert_eq!(bike_from_car(1), 1);
    }

    #[test]
    fn routes_and_osrm_answers_parse() {
        assert_eq!(parse_routes(r#"{"routes":[{"distanceMeters":22600,"duration":"2880s"}]}"#), Some((22.6, 48)));
        assert_eq!(parse_osrm(r#"{"routes":[{"distance":8200.5,"duration":1100.0}]}"#), Some((8.2005, 18)));
        for bad in ["{}", r#"{"routes":[]}"#, "nope", r#"{"routes":[{"distanceMeters":0,"duration":"5s"}]}"#] {
            assert!(parse_routes(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn nominatim_answer_gives_name_category_and_position() {
        let f = parse_nominatim(r#"[{"lat":"17.3616","lon":"78.4747","display_name":"Charminar, Char Kaman, Hyderabad","name":"Charminar","type":"attraction"}]"#).unwrap();
        assert_eq!((f.name.as_str(), f.category.as_deref()), ("Charminar", Some("Attraction")));
        assert!(parse_nominatim("[]").is_none() && parse_nominatim(r#"[{"lat":"0","lon":"0"}]"#).is_none());
    }

    #[test]
    fn google_place_answer_keeps_every_field_it_has() {
        let body = r#"{"places":[{"displayName":{"text":"Paradise Biryani"},"formattedAddress":"Secunderabad","location":{"latitude":17.44,"longitude":78.49},"rating":4.3,"currentOpeningHours":{"openNow":true},"regularOpeningHours":{"weekdayDescriptions":["Monday: 11:00 AM - 11:00 PM"]},"nationalPhoneNumber":"040 1234","primaryTypeDisplayName":{"text":"Restaurant"},"photos":[{"name":"places/X/photos/Y","authorAttributions":[{"displayName":"Asha"}]}]}]}"#;
        let f = parse_google_place(body).unwrap();
        assert_eq!(f.name, "Paradise Biryani");
        assert_eq!(f.category.as_deref(), Some("Restaurant"));
        assert_eq!(f.photo, Some(("places/X/photos/Y".to_string(), Some("Asha".to_string()))));
        assert_eq!(f.weekday_hours.len(), 1);
        assert!(parse_google_place(r#"{"places":[]}"#).is_none());
    }

    #[test]
    fn wikipedia_image_is_used_only_for_the_same_place() {
        let body = |lat: f64| format!(r#"{{"type":"standard","coordinates":{{"lat":{lat},"lon":78.4747}},"thumbnail":{{"source":"https://upload.wikimedia.org/a/b.jpg"}}}}"#);
        assert!(parse_wiki_image(&body(17.3616), 17.3616, 78.4747).is_some());
        let thumb = r#"{"type":"standard","coordinates":{"lat":17.3616,"lon":78.4747},"thumbnail":{"source":"https://thumb.wikimedia.org/wikipedia/commons/thumb/7/71/C.jpg/330px-C.jpg"}}"#;
        assert!(parse_wiki_image(thumb, 17.3616, 78.4747).is_some(), "Wikipedia serves thumbnails from thumb.wikimedia.org");
        assert!(parse_wiki_image(&body(18.5), 17.3616, 78.4747).is_none(), "a different place with the same name");
        assert!(parse_wiki_image(r#"{"type":"disambiguation"}"#, 1.0, 1.0).is_none());
        let no_coords = r#"{"thumbnail":{"source":"https://upload.wikimedia.org/a/b.jpg"}}"#;
        assert!(parse_wiki_image(no_coords, 1.0, 1.0).is_none());
        let off_site = r#"{"coordinates":{"lat":1.0,"lon":1.0},"thumbnail":{"source":"https://evil.example/x.jpg"}}"#;
        assert!(parse_wiki_image(off_site, 1.0, 1.0).is_none());
    }

    #[test]
    fn map_image_url_is_keyless_and_centred() {
        let u = map_image_url(17.0, 78.0);
        assert!(u.starts_with("https://services.arcgisonline.com/") && !u.contains("key"), "{u}");
    }

    #[test]
    fn spoken_extra_needs_both_numbers() {
        let mut c = PlaceCard { distance_km: Some(22.6), car_min: Some(48), ..PlaceCard::default() };
        assert!(spoken_extra(&c).contains("23 kilometres") && spoken_extra(&c).contains("48 minutes"));
        c.car_min = None;
        assert!(spoken_extra(&c).is_empty());
    }

    #[test]
    fn wikimedia_thumbnails_are_widened() {
        assert_eq!(wider_thumb("https://thumb.wikimedia.org/wikipedia/commons/thumb/7/71/C.jpg/330px-C.jpg?utm=1"), "https://thumb.wikimedia.org/wikipedia/commons/thumb/7/71/C.jpg/500px-C.jpg?utm=1");
        assert_eq!(wider_thumb("https://upload.wikimedia.org/a/b.jpg"), "https://upload.wikimedia.org/a/b.jpg");
    }

    #[test]
    fn only_landmarks_get_the_coordinate_photo_search() {
        for c in ["Monument", "Tourist attraction", "Hindu temple", "Fort"] {
            assert!(is_landmark_category(Some(c)), "{c}");
        }
        for c in ["Restaurant", "Shopping mall", "Pharmacy"] {
            assert!(!is_landmark_category(Some(c)), "{c}");
        }
        assert!(!is_landmark_category(None));
    }

    #[test]
    fn commons_answer_gives_nearest_matching_jpeg_with_credit() {
        let body = r#"{"query":{"pages":{
            "1":{"index":2,"title":"File:Charminar night view.jpg","imageinfo":[{"mime":"image/jpeg","thumburl":"https://upload.wikimedia.org/x/b.jpg","extmetadata":{"Artist":{"value":"<a href=\"x\">Asha</a>"},"LicenseShortName":{"value":"CC BY-SA 4.0"}}}]},
            "2":{"index":1,"title":"File:Bhagyalaxmi devi.jpg","imageinfo":[{"mime":"image/jpeg","thumburl":"https://upload.wikimedia.org/x/idol.jpg"}]},
            "3":{"index":3,"title":"File:Charminar plan.svg","imageinfo":[{"mime":"image/svg+xml","thumburl":"https://upload.wikimedia.org/x/a.png"}]},
            "4":{"index":4,"title":"File:Charminar x.jpg","imageinfo":[{"mime":"image/jpeg","thumburl":"https://evil.example/c.jpg"}]}}}}"#;
        let (u, c) = parse_commons(body, "Charminar").unwrap();
        assert_eq!(u, "https://upload.wikimedia.org/x/b.jpg", "the nearer file does not name the place, so it is skipped");
        assert_eq!(c, "Photo: Asha / CC BY-SA 4.0 / Wikimedia Commons");
        assert!(parse_commons(body, "Golconda Fort").is_none(), "no file names this place");
        assert!(parse_commons("{}", "Charminar").is_none());
    }

    #[test]
    fn og_image_is_read_resolved_and_must_be_https() {
        let html = r#"<html><head><meta charset="utf-8"><meta property="og:image" content="/img/hero.jpg"><title>x</title></head>"#;
        assert_eq!(parse_og_image(html, "https://shop.example/menu").as_deref(), Some("https://shop.example/img/hero.jpg"));
        let tw = r#"<meta name='twitter:image' content='https://cdn.example/a.png'>"#;
        assert_eq!(parse_og_image(tw, "https://shop.example/").as_deref(), Some("https://cdn.example/a.png"));
        assert!(parse_og_image(r#"<meta property="og:image" content="http://insecure.example/a.jpg">"#, "https://s.example/").is_none());
        assert!(parse_og_image("<html></html>", "https://s.example/").is_none());
    }

    #[test]
    fn robots_rules_are_honoured() {
        let r = "User-agent: Googlebot\nDisallow: /\n\nUser-agent: *\nDisallow: /private\nAllow: /private/public\n";
        assert!(robots_allows(r, "/"));
        assert!(!robots_allows(r, "/private/x"));
        assert!(robots_allows(r, "/private/public/y"));
        assert!(robots_allows("", "/anything"));
        assert!(!robots_allows("User-agent: *\nDisallow: /", "/menu"));
    }

    #[test]
    fn server_urls_must_be_https_or_private_http() {
        assert_eq!(server_url_ok("https://route.example.com/").as_deref(), Some("https://route.example.com"));
        assert_eq!(server_url_ok("http://192.168.1.20:8080").as_deref(), Some("http://192.168.1.20:8080"));
        assert_eq!(server_url_ok("http://localhost:8080/").as_deref(), Some("http://localhost:8080"));
        assert!(server_url_ok("http://route.example.com").is_none(), "plain http over the internet leaks the token");
        assert!(server_url_ok("http://172.40.0.1").is_none() && server_url_ok("ftp://x.example").is_none() && server_url_ok("nonsense").is_none());
    }

    #[test]
    fn server_answer_parses_and_marks_estimates() {
        let r = parse_server_route(r#"{"km":22.6,"carMin":47.6,"bikeMin":41,"bikeMeasured":true}"#).unwrap();
        assert_eq!((r.car_min, r.bike_min, r.bike_approx, r.travel_approx), (48, 41, false, true));
        let r = parse_server_route(r#"{"km":5,"carMin":20}"#).unwrap();
        assert_eq!((r.bike_min, r.bike_approx), (17, true));
        let r = parse_server_route(r#"{"km":5,"carMin":20,"traffic":true}"#).unwrap();
        assert!(!r.travel_approx);
        for bad in ["{}", "nope", r#"{"km":0,"carMin":5}"#, r#"{"km":9999,"carMin":5}"#] {
            assert!(parse_server_route(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn routing_order_follows_the_mode() {
        assert_eq!(route_order("auto", true, true), vec!["server", "google"]);
        assert_eq!(route_order("auto", false, true), vec!["google"]);
        assert_eq!(route_order("auto", false, false), Vec::<&str>::new());
        assert_eq!(route_order("server", true, true), vec!["server"]);
        assert_eq!(route_order("google", true, true), vec!["google"]);
        assert!(route_order("open", true, true).is_empty());
        assert!(route_order("server", false, true).is_empty(), "a server mode with no server falls to the open service");
    }
}
