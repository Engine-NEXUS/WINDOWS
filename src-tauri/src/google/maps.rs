use super::types::{CommuteDelay, GoogleError, PlaceItem, RouteSummary, TravelMode};
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

    /// Construct a Google Maps search URL.
    pub fn build_place_search_url(query: &str) -> String {
        format!(
            "https://www.google.com/maps/search/?api=1&query={}",
            urlencoding_simple(query)
        )
    }
}

fn urlencoding_simple(s: &str) -> String {
    s.replace(' ', "+")
        .replace('&', "%26")
        .replace(',', "%2C")
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
}
