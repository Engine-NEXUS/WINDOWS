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
    Some(Found { name, category, address: (!display.is_empty()).then_some(display), lat, lon, ..Found::default() })
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
        photo,
    })
}

/// Wikipedia summary -> (image url, credit) only when the page is plainly the same place (its coordinates are within
/// 1.5 km of ours). Without that check a name like "Paradise" would show the wrong page. Pure.
pub fn parse_wiki_image(body: &str, lat: f64, lon: f64) -> Option<String> {
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
    (url.starts_with("https://upload.wikimedia.org/") || url.starts_with("https://thumb.wikimedia.org/")).then(|| url.to_string())
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
            "places.displayName,places.formattedAddress,places.location,places.rating,places.currentOpeningHours.openNow,places.regularOpeningHours.weekdayDescriptions,places.nationalPhoneNumber,places.primaryTypeDisplayName,places.editorialSummary,places.photos",
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
    let url = format!("https://nominatim.openstreetmap.org/search?q={}&format=jsonv2&limit=1&accept-language=en", enc(query));
    let r = c.get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    parse_nominatim(&r.text().await.ok()?)
}

struct Route {
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

async fn route(c: &reqwest::Client, key: &str, from: (f64, f64), to: (f64, f64)) -> Route {
    if !key.is_empty() {
        let (drive, bike) = tokio::join!(google_route(c, key, from, to, "DRIVE"), google_route(c, key, from, to, "TWO_WHEELER"));
        if let Some((km, car)) = drive {
            let (bike_min, bike_approx) = match bike {
                Some((_, m)) => (m, false),
                None => (bike_from_car(car), true),
            };
            return Route { km, car_min: car, bike_min, bike_approx, travel_approx: false };
        }
    }
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

async fn photo_for(c: &reqwest::Client, key: &str, f: &Found, query: &str) -> Option<Photo> {
    if let (false, Some((name, credit))) = (key.is_empty(), f.photo.as_ref()) {
        let url = format!("https://places.googleapis.com/v1/{name}/media?maxWidthPx=630");
        match fetch_image(c, &url, Some(key)).await {
            Some(uri) => return Some(Photo { data_uri: uri, kind: "photo", credit: credit.clone().map(|c| format!("Photo: {c} / Google")) }),
            None => tracing::info!("place card: the Google photo could not be fetched; trying Wikipedia"),
        }
    }
    let title = query.split(',').next().unwrap_or(query).trim().replace(' ', "_");
    if !title.is_empty() {
        let url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", enc(&title));
        if let Ok(r) = c.get(url).send().await {
            if r.status().is_success() {
                if let Some(img) = r.text().await.ok().as_deref().and_then(|b| parse_wiki_image(b, f.lat, f.lon)) {
                    if let Some(uri) = fetch_image(c, &img, None).await {
                        return Some(Photo { data_uri: uri, kind: "photo", credit: Some("Photo: Wikipedia".into()) });
                    }
                }
            }
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
    let (route, photo) = tokio::join!(
        async {
            match here {
                Some(h) => Some(route(&c, &key, (h.lat, h.lon), (found.lat, found.lon)).await),
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

// ── window ──────────────────────────────────────────────────────────────────

static PENDING: Mutex<Option<PlaceCard>> = Mutex::new(None);

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
        "maps" => format!("https://www.google.com/maps/dir/?api=1&destination={:.6},{:.6}", card.lat, card.lon),
        "uber" => format!(
            "https://m.uber.com/ul/?action=setPickup&pickup=my_location&dropoff[latitude]={:.6}&dropoff[longitude]={:.6}&dropoff[nickname]={}",
            card.lat,
            card.lon,
            enc(&card.name)
        ),
        _ => return Err("unknown link".into()),
    };
    open::that(url).map_err(|e| e.to_string())
}

/// Show the card in its own panel beside the map.
pub fn show_card<R: Runtime>(app: &AppHandle<R>, card: PlaceCard) -> Result<(), String> {
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
}
