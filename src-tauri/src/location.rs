//! Where the user is, so "where is X" / "directions to X" can open as a route from here.
//!
//! No permission dialog of ours, ever: the Windows location switch (Settings > Privacy > Location) is the single
//! permission, and once it is on a desktop app is never asked again. With it off (or no sensor / Wi-Fi fix) the
//! approximate position from the network address is used instead, so the route still starts somewhere sensible, and the
//! user is told once - never repeatedly - how to turn exact location on. The last fix is remembered for ten minutes.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    /// True for the network-address estimate (city level).
    pub approximate: bool,
}

impl Fix {
    /// "lat,lon" - what Google Maps accepts as a start point. Pure.
    pub fn as_point(&self) -> String {
        format!("{:.6},{:.6}", self.lat, self.lon)
    }
}

static LAST: Mutex<Option<(Instant, Fix)>> = Mutex::new(None);
static TOLD: Mutex<bool> = Mutex::new(false);

/// True when a coordinate pair is a real position. Pure.
pub fn valid(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) && !(lat == 0.0 && lon == 0.0)
}

/// Parse the network-address answer (`{"latitude":..,"longitude":..}`, as ipwho.is / ipapi return it). Pure.
pub fn parse_network_fix(body: &str) -> Option<Fix> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let (lat, lon) = (v.get("latitude")?.as_f64()?, v.get("longitude")?.as_f64()?);
    valid(lat, lon).then_some(Fix { lat, lon, approximate: true })
}

#[cfg(target_os = "windows")]
fn windows_fix() -> Option<Fix> {
    use windows::Devices::Geolocation::{Geolocator, PositionAccuracy};
    let g = Geolocator::new().ok()?;
    g.SetDesiredAccuracy(PositionAccuracy::Default).ok()?;
    let p = g.GetGeopositionAsync().ok()?.get().ok()?;
    let c = p.Coordinate().ok()?.Point().ok()?.Position().ok()?;
    valid(c.Latitude, c.Longitude).then_some(Fix { lat: c.Latitude, lon: c.Longitude, approximate: false })
}

async fn network_fix() -> Option<Fix> {
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(2)).timeout(Duration::from_secs(4)).build().ok()?;
    let body = client.get("https://ipwho.is/").send().await.ok()?.text().await.ok()?;
    parse_network_fix(&body)
}

/// The user's position: Windows location first, the network-address estimate otherwise, `None` if neither works.
pub async fn current() -> Option<Fix> {
    if let Some((at, f)) = *LAST.lock().unwrap() {
        if at.elapsed() < Duration::from_secs(600) {
            return Some(f);
        }
    }
    #[cfg(target_os = "windows")]
    let exact = tokio::task::spawn_blocking(windows_fix).await.ok().flatten();
    #[cfg(not(target_os = "windows"))]
    let exact: Option<Fix> = None;
    let fix = match exact {
        Some(f) => Some(f),
        None => network_fix().await,
    };
    if let Some(f) = fix {
        *LAST.lock().unwrap() = Some((Instant::now(), f));
    }
    fix
}

/// True exactly once per run, the first time only an approximate position was available.
pub fn should_tell_about_exact_location(fix: &Fix) -> bool {
    if !fix.approximate {
        return false;
    }
    let mut t = TOLD.lock().unwrap();
    !std::mem::replace(&mut *t, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_text_and_validity() {
        assert_eq!(Fix { lat: 17.3616, lon: 78.4747, approximate: false }.as_point(), "17.361600,78.474700");
        assert!(valid(17.36, 78.47));
        for (a, b) in [(0.0, 0.0), (91.0, 0.0), (0.0, 181.0), (f64::NAN, 1.0)] {
            assert!(!valid(a, b), "{a},{b}");
        }
    }

    #[test]
    fn network_answer_parses_and_junk_is_refused() {
        let f = parse_network_fix(r#"{"success":true,"latitude":17.385,"longitude":78.4867}"#).unwrap();
        assert!(f.approximate && (f.lat - 17.385).abs() < 1e-9);
        assert!(parse_network_fix("{}").is_none() && parse_network_fix("not json").is_none());
        assert!(parse_network_fix(r#"{"latitude":0,"longitude":0}"#).is_none());
    }

    #[test]
    fn the_exact_location_hint_is_given_once_and_only_for_estimates() {
        let exact = Fix { lat: 1.0, lon: 1.0, approximate: false };
        let rough = Fix { lat: 1.0, lon: 1.0, approximate: true };
        assert!(!should_tell_about_exact_location(&exact));
        let first = should_tell_about_exact_location(&rough);
        assert!(!should_tell_about_exact_location(&rough));
        let _ = first;
    }
}
