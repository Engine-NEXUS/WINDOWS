#![allow(dead_code)]

use super::types::{CommuteDelay, GoogleError, PlaceDetail, PlaceItem, RouteSummary, TravelMode};
use chrono::{DateTime, Utc};
use serde_json::Value;

/// Google Maps domain service.
pub struct MapsService;

impl MapsService {
    /// Parse Directions API JSON response into `RouteSummary`.
    pub fn parse_directions_json(
        json: &Value,
        origin: &str,
        destination: &str,
    ) -> Result<RouteSummary, GoogleError> {
        let route = json
            .pointer("/routes/0")
            .ok_or_else(|| GoogleError::NotFound("No route found for specified points".into()))?;

        let leg = route
            .pointer("/legs/0")
            .ok_or_else(|| GoogleError::SerializationError("Malformed route leg structure".into()))?;

        let distance_text = leg
            .pointer("/distance/text")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown distance")
            .to_string();

        let duration_text = leg
            .pointer("/duration/text")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown duration")
            .to_string();

        let duration_seconds = leg
            .pointer("/duration/value")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let duration_in_traffic_sec = leg
            .pointer("/duration_in_traffic/value")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        let traffic_delay_minutes = if let Some(traffic_sec) = duration_in_traffic_sec {
            if traffic_sec > duration_seconds {
                (traffic_sec - duration_seconds) / 60
            } else {
                0
            }
        } else {
            0
        };

        let mut summary_steps = Vec::new();
        if let Some(steps) = leg.get("steps").and_then(|s| s.as_array()) {
            for step in steps.iter().take(5) {
                if let Some(instruction) = step.get("html_instructions").and_then(|v| v.as_str()) {
                    let stripped = instruction
                        .replace("<b>", "")
                        .replace("</b>", "")
                        .replace("<div style=\"font-size:0.9em\">", " (")
                        .replace("</div>", ")");
                    summary_steps.push(stripped);
                }
            }
        }

        Ok(RouteSummary {
            origin: origin.to_string(),
            destination: destination.to_string(),
            distance_text,
            duration_text,
            duration_seconds,
            traffic_delay_minutes,
            summary_steps,
        })
    }

    /// Detect commute delays and compute suggested departure time based on an arrival deadline.
    pub fn detect_commute_delay(
        normal_duration_min: u32,
        current_duration_min: u32,
        destination: &str,
        arrival_deadline_epoch: i64,
    ) -> CommuteDelay {
        let delay_minutes = if current_duration_min > normal_duration_min {
            current_duration_min - normal_duration_min
        } else {
            0
        };

        let is_congested = delay_minutes >= 10 || (current_duration_min as f32 > normal_duration_min as f32 * 1.25);

        // Required departure = arrival_deadline - current_duration_min (with 5 min safety buffer)
        let departure_epoch = arrival_deadline_epoch - ((current_duration_min as i64 + 5) * 60);
        let rec_dt = DateTime::<Utc>::from_timestamp(departure_epoch, 0).unwrap_or_else(Utc::now);

        CommuteDelay {
            destination: destination.to_string(),
            normal_duration_min,
            current_duration_min,
            delay_minutes,
            is_congested,
            recommended_departure_iso: rec_dt.to_rfc3339(),
        }
    }

    /// Parse Google Places API JSON response into `Vec<PlaceItem>`.
    pub fn parse_places_json(json: &Value) -> Result<Vec<PlaceItem>, GoogleError> {
        let results = json
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| GoogleError::SerializationError("Missing results array in places response".into()))?;

        let mut places = Vec::new();

        for res in results {
            let name = res.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let address = res
                .get("vicinity")
                .or_else(|| res.get("formatted_address"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let rating = res.get("rating").and_then(|v| v.as_f64()).map(|f| f as f32);
            let open_now = res.pointer("/opening_hours/open_now").and_then(|v| v.as_bool());

            places.push(PlaceItem {
                name,
                address,
                rating,
                open_now,
            });
        }

        Ok(places)
    }

    /// Construct a Google Maps navigation deep link URL.
    pub fn build_maps_nav_url(origin: &str, destination: &str, mode: TravelMode) -> String {
        let mode_str = match mode {
            TravelMode::Driving => "driving",
            TravelMode::Walking => "walking",
            TravelMode::Bicycling => "bicycling",
            TravelMode::Transit => "transit",
        };
        format!(
            "https://www.google.com/maps/dir/?api=1&origin={}&destination={}&travelmode={}",
            urlencoding_simple(origin),
            urlencoding_simple(destination),
            mode_str
        )
    }

    /// Official Google Maps "directions" URL. Unlike `build_maps_nav_url` this
    /// omits an empty origin (Maps then uses "your location") and fully
    /// percent-encodes every parameter.
    pub fn build_directions_url(origin: Option<&str>, destination: &str, mode: TravelMode) -> String {
        Self::build_directions_url_with(origin, destination, mode, &RouteOpts::default())
    }

    /// The same URL with the route options: `avoid=tolls|highways|ferries` and `waypoints=a|b` (stops on the way).
    pub fn build_directions_url_with(origin: Option<&str>, destination: &str, mode: TravelMode, opts: &RouteOpts) -> String {
        let mode_str = match mode {
            TravelMode::Driving => "driving",
            TravelMode::Walking => "walking",
            TravelMode::Bicycling => "bicycling",
            TravelMode::Transit => "transit",
        };
        let mut url = format!("https://www.google.com/maps/dir/?api=1&destination={}", percent_encode(destination));
        if let Some(o) = origin.map(str::trim).filter(|o| !o.is_empty()) {
            url.push_str(&format!("&origin={}", percent_encode(o)));
        }
        url.push_str(&format!("&travelmode={mode_str}"));
        let avoid = opts.avoid_words();
        if !avoid.is_empty() && mode == TravelMode::Driving {
            url.push_str(&format!("&avoid={}", avoid.join("%7C")));
        }
        let stops: Vec<String> = opts.stops.iter().map(|s| percent_encode(s)).filter(|s| !s.is_empty()).collect();
        if !stops.is_empty() && mode != TravelMode::Transit {
            url.push_str(&format!("&waypoints={}", stops.join("%7C")));
        }
        url
    }

    /// JSON body for Routes API v2 `computeRoutes` (address-based endpoints).
    /// Avoid flags apply to driving only; stops are not allowed for transit.
    pub fn routes_request_body_with(origin: &str, destination: &str, mode: TravelMode, opts: &RouteOpts) -> Value {
        let travel = match mode {
            TravelMode::Driving => "DRIVE",
            TravelMode::Walking => "WALK",
            TravelMode::Bicycling => "BICYCLE",
            TravelMode::Transit => "TRANSIT",
        };
        let mut body = serde_json::json!({
            "origin": { "address": origin },
            "destination": { "address": destination },
            "travelMode": travel,
        });
        // Traffic-aware routing is only valid for driving.
        if mode == TravelMode::Driving {
            body["routingPreference"] = serde_json::json!("TRAFFIC_AWARE");
            if opts.any_avoid() {
                body["routeModifiers"] = serde_json::json!({
                    "avoidTolls": opts.avoid_tolls,
                    "avoidHighways": opts.avoid_highways,
                    "avoidFerries": opts.avoid_ferries,
                });
            }
        }
        if mode != TravelMode::Transit && !opts.stops.is_empty() {
            body["intermediates"] = Value::Array(opts.stops.iter().take(10).map(|s| serde_json::json!({ "address": s })).collect());
        }
        body
    }

    /// Parse a Routes API v2 response (field mask: duration, distanceMeters, description).
    pub fn parse_routes_json(json: &Value) -> Result<RouteEta, GoogleError> {
        let route = json
            .pointer("/routes/0")
            .ok_or_else(|| GoogleError::NotFound("No route found for specified points".into()))?;
        let secs = route
            .get("duration")
            .and_then(|d| d.as_str())
            .and_then(|d| d.strip_suffix('s'))
            .and_then(|d| d.trim().parse::<f64>().ok())
            .ok_or_else(|| GoogleError::SerializationError("route has no duration".into()))?;
        let meters = route.get("distanceMeters").and_then(|m| m.as_u64()).unwrap_or(0);
        let via = route
            .get("description")
            .and_then(|d| d.as_str())
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(String::from);
        let steps: Vec<String> = route
            .get("legs")
            .and_then(|l| l.as_array())
            .map(|legs| {
                legs.iter()
                    .flat_map(|leg| leg.get("steps").and_then(|s| s.as_array()).into_iter().flatten())
                    .filter_map(|st| st.pointer("/navigationInstruction/instructions").and_then(|i| i.as_str()))
                    .map(|i| i.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|i| !i.is_empty())
                    .take(60)
                    .collect()
            })
            .unwrap_or_default();
        Ok(RouteEta { minutes: ((secs / 60.0).round() as u32).max(1), meters, via, steps })
    }

    pub const MAX_ADDRESS_LEN: usize = 250;

    /// Clean, validate and clip an address string to prevent API errors.
    pub fn clean_and_clip_address(addr: &str, field_name: &str) -> Result<String, GoogleError> {
        let trimmed = addr.trim();
        if trimmed.is_empty() {
            return Err(GoogleError::InvalidInput(format!(
                "Please provide a valid {field_name} address for directions."
            )));
        }
        let clipped: String = trimmed.chars().take(Self::MAX_ADDRESS_LEN).collect();
        Ok(clipped)
    }

    /// Map HTTP response error status code and body to a typed GoogleError.
    pub fn map_response_error(code: u16, body: &str) -> GoogleError {
        match code {
            400 => GoogleError::InvalidInput(if body.is_empty() {
                "Google Maps could not locate that address. Please check the spelling.".into()
            } else {
                format!("Bad address or invalid request: {body}")
            }),
            403 => GoogleError::ApiError {
                code: 403,
                message: if body.is_empty() {
                    "Google Maps API key is restricted or unauthorized.".into()
                } else {
                    format!("Google Maps API key restricted: {body}")
                },
            },
            429 => GoogleError::RateLimited,
            _ => GoogleError::ApiError {
                code,
                message: if body.is_empty() {
                    "Google Maps service error.".into()
                } else {
                    body.to_string()
                },
            },
        }
    }

    /// Call Routes API v2. The key travels in a header (never in a URL or log).
    pub async fn compute_route(
        client: &reqwest::Client,
        api_key: &str,
        origin: &str,
        destination: &str,
        mode: TravelMode,
    ) -> Result<RouteEta, GoogleError> {
        Self::compute_route_with(client, api_key, origin, destination, mode, &RouteOpts::default(), false).await
    }

    /// `compute_route` with the route options; `want_steps` also asks for the turn-by-turn text.
    pub async fn compute_route_with(
        client: &reqwest::Client,
        api_key: &str,
        origin: &str,
        destination: &str,
        mode: TravelMode,
        opts: &RouteOpts,
        want_steps: bool,
    ) -> Result<RouteEta, GoogleError> {
        let clean_origin = Self::clean_and_clip_address(origin, "origin")?;
        let clean_dest = Self::clean_and_clip_address(destination, "destination")?;
        let mask = if want_steps {
            "routes.duration,routes.distanceMeters,routes.description,routes.legs.steps.navigationInstruction"
        } else {
            "routes.duration,routes.distanceMeters,routes.description"
        };
        let resp = client
            .post("https://routes.googleapis.com/directions/v2:computeRoutes")
            .header("X-Goog-Api-Key", api_key)
            .header("X-Goog-FieldMask", mask)
            .json(&Self::routes_request_body_with(&clean_origin, &clean_dest, mode, opts))
            .send()
            .await
            .map_err(|e| GoogleError::NetworkError(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_response_error(status.as_u16(), &body));
        }
        let json: Value = resp.json().await.map_err(|e| GoogleError::SerializationError(e.to_string()))?;
        Self::parse_routes_json(&json)
    }

    /// Places API (New) `places:searchText` body. `text_query` already contains the
    /// area ("pharmacy near 12 High St"), so no coordinates are needed.
    pub fn places_request_body(text_query: &str) -> Value {
        serde_json::json!({ "textQuery": text_query, "maxResultCount": 3, "languageCode": "en" })
    }

    /// Parse a Places API (New) response (field mask: displayName, formattedAddress, rating, openNow).
    pub fn parse_places_new_json(json: &Value) -> Vec<PlaceItem> {
        json.get("places")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let name = p.pointer("/displayName/text").and_then(|v| v.as_str())?.trim().to_string();
                        if name.is_empty() {
                            return None;
                        }
                        Some(PlaceItem {
                            name,
                            address: p.get("formattedAddress").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                            rating: p.get("rating").and_then(|v| v.as_f64()).map(|f| f as f32),
                            open_now: p.pointer("/currentOpeningHours/openNow").and_then(|v| v.as_bool()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One place with today's opening hours, for "when is the Charminar" / "is the museum open".
    /// `weekday_hours` holds Google's seven lines in Monday-first order ("Monday: 9:00 AM - 5:30 PM").
    pub fn parse_place_detail_json(json: &Value) -> Option<PlaceDetail> {
        let p = json.get("places")?.as_array()?.first()?;
        let name = p.pointer("/displayName/text").and_then(|v| v.as_str())?.trim().to_string();
        if name.is_empty() {
            return None;
        }
        let hours = p
            .pointer("/regularOpeningHours/weekdayDescriptions")
            .or_else(|| p.pointer("/currentOpeningHours/weekdayDescriptions"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        Some(PlaceDetail {
            item: PlaceItem {
                name,
                address: p.get("formattedAddress").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                rating: p.get("rating").and_then(|v| v.as_f64()).map(|f| f as f32),
                open_now: p.pointer("/currentOpeningHours/openNow").and_then(|v| v.as_bool()),
            },
            weekday_hours: hours,
            summary: p.pointer("/editorialSummary/text").and_then(|v| v.as_str()).map(str::to_string),
        })
    }

    /// Top Places API (New) match with hours. Key in a header only.
    pub async fn place_detail(
        client: &reqwest::Client,
        api_key: &str,
        text_query: &str,
    ) -> Result<Option<PlaceDetail>, GoogleError> {
        let resp = client
            .post("https://places.googleapis.com/v1/places:searchText")
            .header("X-Goog-Api-Key", api_key)
            .header(
                "X-Goog-FieldMask",
                "places.displayName,places.formattedAddress,places.rating,places.currentOpeningHours.openNow,places.regularOpeningHours.weekdayDescriptions,places.editorialSummary",
            )
            .json(&serde_json::json!({ "textQuery": text_query, "maxResultCount": 1, "languageCode": "en" }))
            .send()
            .await
            .map_err(|e| GoogleError::NetworkError(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_response_error(status.as_u16(), &body));
        }
        let json: Value = resp.json().await.map_err(|e| GoogleError::SerializationError(e.to_string()))?;
        Ok(Self::parse_place_detail_json(&json))
    }

    /// Call Places API (New) text search. Key in a header only.
    pub async fn search_places(
        client: &reqwest::Client,
        api_key: &str,
        text_query: &str,
    ) -> Result<Vec<PlaceItem>, GoogleError> {
        let resp = client
            .post("https://places.googleapis.com/v1/places:searchText")
            .header("X-Goog-Api-Key", api_key)
            .header(
                "X-Goog-FieldMask",
                "places.displayName,places.formattedAddress,places.rating,places.currentOpeningHours.openNow",
            )
            .json(&Self::places_request_body(text_query))
            .send()
            .await
            .map_err(|e| GoogleError::NetworkError(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_response_error(status.as_u16(), &body));
        }
        let json: Value = resp.json().await.map_err(|e| GoogleError::SerializationError(e.to_string()))?;
        Ok(Self::parse_places_new_json(&json))
    }

    /// Construct a Google Maps search URL.
    pub fn build_place_search_url(query: &str) -> String {
        format!(
            "https://www.google.com/maps/search/?api=1&query={}",
            urlencoding_simple(query)
        )
    }
}

/// ETA summary from the Routes API.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteEta {
    pub minutes: u32,
    pub meters: u64,
    pub via: Option<String>,
    /// Turn-by-turn text, only when it was asked for.
    pub steps: Vec<String>,
}

/// What the user asked of a route beyond A to B: roads to avoid and stops on the way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RouteOpts {
    pub avoid_tolls: bool,
    pub avoid_highways: bool,
    pub avoid_ferries: bool,
    pub stops: Vec<String>,
}

impl RouteOpts {
    pub fn any_avoid(&self) -> bool {
        self.avoid_tolls || self.avoid_highways || self.avoid_ferries
    }

    /// Words Google's URL uses for the avoided road types, in a fixed order.
    pub fn avoid_words(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.avoid_tolls {
            v.push("tolls");
        }
        if self.avoid_highways {
            v.push("highways");
        }
        if self.avoid_ferries {
            v.push("ferries");
        }
        v
    }

    /// "avoiding tolls and highways, with a stop at X" for the spoken line; empty when there is nothing to say.
    pub fn spoken(&self) -> String {
        let avoid = self.avoid_words();
        let mut parts = vec![];
        if !avoid.is_empty() {
            parts.push(format!("avoiding {}", avoid.join(" and ")));
        }
        match self.stops.len() {
            0 => {}
            1 => parts.push(format!("with a stop at {}", self.stops[0])),
            n => parts.push(format!("with {n} stops")),
        }
        parts.join(", ")
    }
}

/// Percent-encode everything except RFC 3986 unreserved characters.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.trim().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.trim().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_url_carries_avoid_and_stops_only_where_google_allows() {
        let opts = RouteOpts { avoid_tolls: true, avoid_highways: true, avoid_ferries: false, stops: vec!["Cafe Bahar".into(), "ATM".into()] };
        let u = MapsService::build_directions_url_with(Some("Home"), "T-Hub", TravelMode::Driving, &opts);
        assert!(u.contains("&avoid=tolls%7Chighways"), "{u}");
        assert!(u.contains("&waypoints=Cafe%20Bahar%7CATM"), "{u}");
        // walking has no toll roads; transit takes no stops
        let w = MapsService::build_directions_url_with(None, "T-Hub", TravelMode::Walking, &opts);
        assert!(!w.contains("avoid="), "{w}");
        let t = MapsService::build_directions_url_with(None, "T-Hub", TravelMode::Transit, &opts);
        assert!(!t.contains("waypoints="), "{t}");
        // no options = the old URL
        assert_eq!(
            MapsService::build_directions_url_with(Some("A"), "B", TravelMode::Walking, &RouteOpts::default()),
            MapsService::build_directions_url(Some("A"), "B", TravelMode::Walking)
        );
    }

    #[test]
    fn routes_body_has_modifiers_and_intermediates_per_mode() {
        let opts = RouteOpts { avoid_tolls: true, stops: vec!["ATM".into()], ..RouteOpts::default() };
        let d = MapsService::routes_request_body_with("A", "B", TravelMode::Driving, &opts);
        assert_eq!(d["routeModifiers"]["avoidTolls"], true);
        assert_eq!(d["routeModifiers"]["avoidHighways"], false);
        assert_eq!(d["intermediates"][0]["address"], "ATM");
        let w = MapsService::routes_request_body_with("A", "B", TravelMode::Walking, &opts);
        assert!(w.get("routeModifiers").is_none() && w["intermediates"][0]["address"] == "ATM");
        let t = MapsService::routes_request_body_with("A", "B", TravelMode::Transit, &opts);
        assert!(t.get("routeModifiers").is_none() && t.get("intermediates").is_none());
        let plain = MapsService::routes_request_body_with("A", "B", TravelMode::Driving, &RouteOpts::default());
        assert!(plain.get("routeModifiers").is_none() && plain.get("intermediates").is_none() && plain["routingPreference"] == "TRAFFIC_AWARE");
    }

    #[test]
    fn routes_answer_gives_steps_when_present_and_none_when_not() {
        let with = serde_json::json!({"routes":[{"duration":"1320s","distanceMeters":5300,"description":"Road No. 1",
            "legs":[{"steps":[{"navigationInstruction":{"instructions":"Head  north on Main St"}},{"navigationInstruction":{"instructions":"Turn left onto Park Rd"}},{"distanceMeters":10}]}]}]});
        let e = MapsService::parse_routes_json(&with).unwrap();
        assert_eq!((e.minutes, e.meters), (22, 5300));
        assert_eq!(e.steps, ["Head north on Main St", "Turn left onto Park Rd"]);
        let without = serde_json::json!({"routes":[{"duration":"60s","distanceMeters":100}]});
        assert!(MapsService::parse_routes_json(&without).unwrap().steps.is_empty());
    }

    #[test]
    fn route_opts_speak_what_they_change() {
        assert_eq!(RouteOpts::default().spoken(), "");
        let o = RouteOpts { avoid_tolls: true, avoid_ferries: true, stops: vec!["ATM".into()], ..RouteOpts::default() };
        assert_eq!(o.spoken(), "avoiding tolls and ferries, with a stop at ATM");
        assert_eq!(RouteOpts { stops: vec!["a".into(), "b".into()], ..RouteOpts::default() }.spoken(), "with 2 stops");
    }

    #[test]
    fn test_parse_directions_json() {
        let raw = serde_json::json!({
            "routes": [{
                "legs": [{
                    "distance": { "text": "14.2 mi", "value": 22852 },
                    "duration": { "text": "25 mins", "value": 1500 },
                    "duration_in_traffic": { "text": "40 mins", "value": 2400 },
                    "steps": [
                        { "html_instructions": "Head <b>north</b> on 1st Ave" },
                        { "html_instructions": "Turn <b>right</b> onto I-90 E" }
                    ]
                }]
            }]
        });

        let summary = MapsService::parse_directions_json(&raw, "Home", "Office").unwrap();
        assert_eq!(summary.origin, "Home");
        assert_eq!(summary.destination, "Office");
        assert_eq!(summary.distance_text, "14.2 mi");
        assert_eq!(summary.duration_text, "25 mins");
        assert_eq!(summary.duration_seconds, 1500);
        assert_eq!(summary.traffic_delay_minutes, 15);
        assert_eq!(summary.summary_steps.len(), 2);
        assert_eq!(summary.summary_steps[0], "Head north on 1st Ave");
    }

    #[test]
    fn test_commute_delay_detection() {
        // Target arrival: 1727690400 (e.g. 10:00 AM)
        let arrival_epoch = 1727690400;
        let delay = MapsService::detect_commute_delay(20, 35, "Office HQ", arrival_epoch);

        assert_eq!(delay.destination, "Office HQ");
        assert_eq!(delay.normal_duration_min, 20);
        assert_eq!(delay.current_duration_min, 35);
        assert_eq!(delay.delay_minutes, 15);
        assert!(delay.is_congested);

        // Check recommended departure time (arrival - (35 + 5) min = arrival - 2400 sec)
        let expected_departure_epoch = arrival_epoch - (40 * 60);
        let expected_iso = DateTime::<Utc>::from_timestamp(expected_departure_epoch, 0)
            .unwrap()
            .to_rfc3339();
        assert_eq!(delay.recommended_departure_iso, expected_iso);
    }

    #[test]
    fn test_parse_places_json() {
        let raw = serde_json::json!({
            "results": [
                {
                    "name": "Artisan Coffee",
                    "vicinity": "123 Main St, Springfield",
                    "rating": 4.8,
                    "opening_hours": { "open_now": true }
                },
                {
                    "name": "Quiet Library Cafe",
                    "vicinity": "456 Oak Ave",
                    "rating": 4.2,
                    "opening_hours": { "open_now": false }
                }
            ]
        });

        let places = MapsService::parse_places_json(&raw).unwrap();
        assert_eq!(places.len(), 2);
        assert_eq!(places[0].name, "Artisan Coffee");
        assert_eq!(places[0].rating, Some(4.8));
        assert_eq!(places[0].open_now, Some(true));
        assert_eq!(places[1].open_now, Some(false));
    }

    #[test]
    fn test_build_maps_urls() {
        let nav = MapsService::build_maps_nav_url("Central Park", "Times Square", TravelMode::Walking);
        assert_eq!(
            nav,
            "https://www.google.com/maps/dir/?api=1&origin=Central+Park&destination=Times+Square&travelmode=walking"
        );

        let search = MapsService::build_place_search_url("best pizza nearby");
        assert_eq!(
            search,
            "https://www.google.com/maps/search/?api=1&query=best+pizza+nearby"
        );
    }

    #[test]
    fn directions_url_encodes_and_omits_empty_origin() {
        let u = MapsService::build_directions_url(None, "Central Park, NY & Co #1?", TravelMode::Walking);
        assert_eq!(u, "https://www.google.com/maps/dir/?api=1&destination=Central%20Park%2C%20NY%20%26%20Co%20%231%3F&travelmode=walking");
        assert!(!MapsService::build_directions_url(Some("  "), "X", TravelMode::Driving).contains("origin="));
        let u = MapsService::build_directions_url(Some("London Bridge"), "Heathrow", TravelMode::Transit);
        assert!(u.contains("&origin=London%20Bridge") && u.ends_with("travelmode=transit"), "{u}");
    }

    #[test]
    fn places_new_response_parses_and_skips_nameless() {
        let j = serde_json::json!({"places":[
            {"displayName":{"text":"Boots Pharmacy"},"formattedAddress":"12 High St, London","rating":4.3,"currentOpeningHours":{"openNow":true}},
            {"displayName":{"text":" "},"formattedAddress":"x"},
            {"formattedAddress":"no name"},
            {"displayName":{"text":"Lloyds"}}
        ]});
        let p = MapsService::parse_places_new_json(&j);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].name.as_str(), p[0].open_now, p[0].rating), ("Boots Pharmacy", Some(true), Some(4.3)));
        assert_eq!((p[1].name.as_str(), p[1].open_now, p[1].address.as_str()), ("Lloyds", None, ""));
        assert!(MapsService::parse_places_new_json(&serde_json::json!({})).is_empty());
        assert_eq!(MapsService::places_request_body("pharmacy near X")["textQuery"], "pharmacy near X");
    }

    #[test]
    fn routes_body_is_traffic_aware_only_for_driving() {
        let d = MapsService::routes_request_body_with("A", "B", TravelMode::Driving, &RouteOpts::default());
        assert_eq!(d["travelMode"], "DRIVE");
        assert_eq!(d["routingPreference"], "TRAFFIC_AWARE");
        let w = MapsService::routes_request_body_with("A", "B", TravelMode::Walking, &RouteOpts::default());
        assert_eq!(w["travelMode"], "WALK");
        assert!(w.get("routingPreference").is_none());
    }

    #[test]
    fn routes_response_parses_duration_distance_and_road() {
        let j = serde_json::json!({"routes":[{"duration":"1680s","distanceMeters":13520,"description":"FDR Drive"}]});
        let eta = MapsService::parse_routes_json(&j).unwrap();
        assert_eq!((eta.minutes, eta.meters, eta.via.as_deref()), (28, 13520, Some("FDR Drive")));
        let j = serde_json::json!({"routes":[{"duration":"3900s"}]});
        assert_eq!(MapsService::parse_routes_json(&j).unwrap().minutes, 65);
        let j = serde_json::json!({"routes":[{"duration":"20s"}]});
        assert_eq!(MapsService::parse_routes_json(&j).unwrap().minutes, 1);
        assert!(MapsService::parse_routes_json(&serde_json::json!({})).is_err());
        assert!(MapsService::parse_routes_json(&serde_json::json!({"routes":[{"distanceMeters":5}]})).is_err());
    }

    #[test]
    fn routes_response_zero_distance_and_minimum_eta() {
        let j = serde_json::json!({
            "routes": [{
                "duration": "0s",
                "distanceMeters": 0
            }]
        });
        let eta = MapsService::parse_routes_json(&j).unwrap();
        assert_eq!(eta.minutes, 1, "duration of 0 seconds clamps to minimum 1 minute ETA");
        assert_eq!(eta.meters, 0);
    }

    #[test]
    fn routes_response_missing_duration_fails_with_serialization_error() {
        let j = serde_json::json!({
            "routes": [{
                "distanceMeters": 500
            }]
        });
        let res = MapsService::parse_routes_json(&j);
        assert!(matches!(res, Err(GoogleError::SerializationError(ref msg)) if msg.contains("duration")));
    }

    #[test]
    fn directions_leg_summary_zero_distance_and_unknown_duration() {
        let raw = serde_json::json!({
            "routes": [{
                "legs": [{
                    "distance": { "text": "0 ft", "value": 0 },
                    "steps": []
                }]
            }]
        });
        let summary = MapsService::parse_directions_json(&raw, "Here", "Here").unwrap();
        assert_eq!(summary.distance_text, "0 ft");
        assert_eq!(summary.duration_text, "Unknown duration");
        assert_eq!(summary.duration_seconds, 0);
        assert_eq!(summary.traffic_delay_minutes, 0);
    }

    #[test]
    fn directions_leg_summary_traffic_delay_calculation() {
        // Traffic duration longer than normal duration -> positive delay
        let raw_delay = serde_json::json!({
            "routes": [{
                "legs": [{
                    "duration": { "text": "20 mins", "value": 1200 },
                    "duration_in_traffic": { "text": "35 mins", "value": 2100 }
                }]
            }]
        });
        let s1 = MapsService::parse_directions_json(&raw_delay, "A", "B").unwrap();
        assert_eq!(s1.traffic_delay_minutes, 15);

        // Traffic duration shorter than or equal to normal duration -> 0 delay
        let raw_no_delay = serde_json::json!({
            "routes": [{
                "legs": [{
                    "duration": { "text": "20 mins", "value": 1200 },
                    "duration_in_traffic": { "text": "18 mins", "value": 1080 }
                }]
            }]
        });
        let s2 = MapsService::parse_directions_json(&raw_no_delay, "A", "B").unwrap();
        assert_eq!(s2.traffic_delay_minutes, 0);

        // No duration_in_traffic specified -> 0 delay
        let raw_missing = serde_json::json!({
            "routes": [{
                "legs": [{
                    "duration": { "text": "20 mins", "value": 1200 }
                }]
            }]
        });
        let s3 = MapsService::parse_directions_json(&raw_missing, "A", "B").unwrap();
        assert_eq!(s3.traffic_delay_minutes, 0);
    }

    #[test]
    fn travel_mode_routes_mapping_and_traffic_preference() {
        let driving = MapsService::routes_request_body_with("Home", "Work", TravelMode::Driving, &RouteOpts::default());
        assert_eq!(driving["travelMode"], "DRIVE");
        assert_eq!(driving["routingPreference"], "TRAFFIC_AWARE");

        let walking = MapsService::routes_request_body_with("Home", "Park", TravelMode::Walking, &RouteOpts::default());
        assert_eq!(walking["travelMode"], "WALK");
        assert!(walking.get("routingPreference").is_none());

        let cycling = MapsService::routes_request_body_with("Home", "Store", TravelMode::Bicycling, &RouteOpts::default());
        assert_eq!(cycling["travelMode"], "BICYCLE");
        assert!(cycling.get("routingPreference").is_none());

        let transit = MapsService::routes_request_body_with("Home", "Airport", TravelMode::Transit, &RouteOpts::default());
        assert_eq!(transit["travelMode"], "TRANSIT");
        assert!(transit.get("routingPreference").is_none());
    }

    #[test]
    fn travel_mode_directions_url_mapping() {
        for (mode, expected) in [
            (TravelMode::Driving, "driving"),
            (TravelMode::Walking, "walking"),
            (TravelMode::Bicycling, "bicycling"),
            (TravelMode::Transit, "transit"),
        ] {
            let u1 = MapsService::build_directions_url(Some("A"), "B", mode);
            assert!(u1.ends_with(&format!("travelmode={expected}")), "{u1}");

            let u2 = MapsService::build_maps_nav_url("A", "B", mode);
            assert!(u2.ends_with(&format!("travelmode={expected}")), "{u2}");
        }
    }

    #[test]
    fn urlencoding_encodes_ampersand_hash_percent_unicode_and_spaces() {
        let input = "Café & Bakery #42 100% Good";
        let encoded = urlencoding_simple(input);
        assert!(encoded.contains("%26"), "must encode &: {encoded}");
        assert!(encoded.contains("%23"), "must encode #: {encoded}");
        assert!(encoded.contains("%25"), "must encode %: {encoded}");
        assert!(encoded.contains("%C3%A9"), "must encode unicode: {encoded}");
        assert!(encoded.contains('+'), "must encode spaces: {encoded}");
        assert!(!encoded.contains('#'), "must not contain raw #: {encoded}");
        assert!(!encoded.contains('&'), "must not contain raw &: {encoded}");
        assert!(!encoded.contains(' '), "must not contain raw space: {encoded}");

        let deep_link = MapsService::build_directions_url(Some("Café #1"), "100% Bakery & Tea", TravelMode::Driving);
        assert!(!deep_link.contains('#'));
        assert!(deep_link.contains("%23"));
        assert!(deep_link.contains("%25"));
        assert!(deep_link.contains("%26"));
        assert!(deep_link.contains("%C3%A9"));
    }

    #[test]
    fn place_search_url_encodes_special_and_unicode_characters() {
        let url = MapsService::build_place_search_url("café & tacos near Apt #4 (100% fresh)");
        assert!(url.starts_with("https://www.google.com/maps/search/?api=1&query="));
        let query_string = url.split_once('?').map(|(_, qs)| qs).unwrap_or("");
        let query_val = query_string
            .split('&')
            .find_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                if k == "query" {
                    Some(v)
                } else {
                    None
                }
            })
            .expect("query parameter must be present");
        assert!(query_val.contains("%26"), "must encode &: {query_val}");
        assert!(query_val.contains("%23"), "must encode #: {query_val}");
        assert!(query_val.contains("%25"), "must encode %: {query_val}");
        assert!(query_val.contains("%C3%A9"), "must encode unicode: {query_val}");
        assert!(!query_val.contains('#'), "must not contain raw #: {query_val}");
        assert!(!query_val.contains('&'), "must not contain unescaped &: {query_val}");
        assert!(!query_val.contains(' '), "must not contain raw space: {query_val}");
    }

    #[test]
    fn maps_empty_or_blank_endpoints_rejected_with_speakable_message() {
        let err1 = MapsService::clean_and_clip_address("", "origin").unwrap_err();
        assert!(matches!(err1, GoogleError::InvalidInput(ref msg) if msg.contains("origin")));

        let err2 = MapsService::clean_and_clip_address("   \t  ", "destination").unwrap_err();
        assert!(matches!(err2, GoogleError::InvalidInput(ref msg) if msg.contains("destination")));
    }

    #[test]
    fn maps_very_long_address_is_clipped() {
        let long_addr = "123 Very Long Street Name Suite 400 Extra Details ".repeat(10);
        assert!(long_addr.chars().count() > MapsService::MAX_ADDRESS_LEN);

        let clipped = MapsService::clean_and_clip_address(&long_addr, "destination").unwrap();
        assert_eq!(clipped.chars().count(), MapsService::MAX_ADDRESS_LEN);
        assert!(long_addr.starts_with(&clipped));
    }

    #[test]
    fn maps_response_error_mapping_400_403_429_500() {
        let err_400 = MapsService::map_response_error(400, "INVALID_ARGUMENT: address not found");
        assert!(matches!(err_400, GoogleError::InvalidInput(ref msg) if msg.contains("address")));

        let err_403 = MapsService::map_response_error(403, "API_KEY_RESTRICTED");
        assert!(matches!(err_403, GoogleError::ApiError { code: 403, ref message } if message.contains("restricted")));

        let err_429 = MapsService::map_response_error(429, "RESOURCE_EXHAUSTED");
        assert_eq!(err_429, GoogleError::RateLimited);

        let err_500 = MapsService::map_response_error(500, "INTERNAL_ERROR");
        assert!(matches!(err_500, GoogleError::ApiError { code: 500, .. }));
    }
}
