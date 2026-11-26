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

/// Why Windows gave no exact position. Said to the user in words, never guessed: each case has its own sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Windows refused this app (Settings > Privacy > Location, "Let desktop apps access your location" is off).
    Denied,
    /// The Windows location switch is off.
    Off,
    /// This computer reports no location sensor or service.
    NoSensor,
    /// Location is on but no fix arrived (no Wi-Fi scan, indoors, still starting).
    NoFix,
}

/// The Windows location states that matter here, mirrored so the decision can be tested without Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinStatus {
    Disabled,
    NotAvailable,
    Other,
}

const E_ACCESSDENIED: i32 = 0x8007_0005_u32 as i32;

/// The failed position request's error code and the locator's status -> what to tell the user. Pure.
pub fn classify(error_code: Option<i32>, status: WinStatus) -> Problem {
    if error_code == Some(E_ACCESSDENIED) {
        return Problem::Denied;
    }
    match status {
        WinStatus::Disabled => Problem::Off,
        WinStatus::NotAvailable => Problem::NoSensor,
        WinStatus::Other => Problem::NoFix,
    }
}

/// The sentence for "I only have an approximate position". Pure.
pub fn approximate_sentence(p: Option<Problem>) -> &'static str {
    match p {
        Some(Problem::Denied) => "Windows is blocking location for desktop apps, sir, so I'm using your network address, which is only accurate to the city. Allow it in Settings, Privacy, Location.",
        Some(Problem::Off) => "Location is turned off in Windows, sir, so I'm using your network address, which is only accurate to the city. Turn it on in Settings, Privacy, Location.",
        Some(Problem::NoSensor) => "This computer has no location service, sir, so I'm using your network address, which is only accurate to the city.",
        Some(Problem::NoFix) | None => "Windows couldn't get a location fix, sir, so I'm using your network address, which is only accurate to the city.",
    }
}

/// The sentence for "I have no position at all". Pure.
pub fn no_position_sentence(p: Option<Problem>) -> &'static str {
    match p {
        Some(Problem::Denied) => "Windows is blocking location for desktop apps and I couldn't use your network address either, sir.",
        Some(Problem::Off) => "Location is turned off in Windows and I couldn't use your network address either, sir.",
        _ => "I couldn't tell where you are, sir.",
    }
}

#[cfg(target_os = "windows")]
fn windows_fix() -> Result<Fix, Problem> {
    use windows::Devices::Geolocation::{Geolocator, PositionAccuracy, PositionStatus};
    let g = Geolocator::new().map_err(|_| Problem::NoSensor)?;
    let _ = g.SetDesiredAccuracy(PositionAccuracy::Default);
    let fail = |code: Option<i32>| {
        let status = match g.LocationStatus() {
            Ok(s) if s == PositionStatus::Disabled => WinStatus::Disabled,
            Ok(s) if s == PositionStatus::NotAvailable => WinStatus::NotAvailable,
            _ => WinStatus::Other,
        };
        classify(code, status)
    };
    let op = g.GetGeopositionAsync().map_err(|e| fail(Some(e.code().0)))?;
    let p = op.get().map_err(|e| fail(Some(e.code().0)))?;
    let c = p.Coordinate().and_then(|c| c.Point()).and_then(|pt| pt.Position()).map_err(|_| Problem::NoFix)?;
    if valid(c.Latitude, c.Longitude) {
        Ok(Fix { lat: c.Latitude, lon: c.Longitude, approximate: false })
    } else {
        Err(Problem::NoFix)
    }
}

static LAST_PROBLEM: Mutex<Option<Problem>> = Mutex::new(None);

/// Why the last exact position failed, if it did.
pub fn problem() -> Option<Problem> {
    *LAST_PROBLEM.lock().unwrap_or_else(|e| e.into_inner())
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
    let exact = match tokio::task::spawn_blocking(windows_fix).await {
        Ok(Ok(f)) => {
            *LAST_PROBLEM.lock().unwrap_or_else(|e| e.into_inner()) = None;
            Some(f)
        }
        Ok(Err(p)) => {
            *LAST_PROBLEM.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
            None
        }
        Err(_) => None,
    };
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
    fn a_location_failure_is_named_not_guessed() {
        assert_eq!(classify(Some(E_ACCESSDENIED), WinStatus::Other), Problem::Denied);
        assert_eq!(classify(Some(E_ACCESSDENIED), WinStatus::Disabled), Problem::Denied, "a refusal wins over the status");
        assert_eq!(classify(Some(-1), WinStatus::Disabled), Problem::Off);
        assert_eq!(classify(None, WinStatus::NotAvailable), Problem::NoSensor);
        assert_eq!(classify(Some(-1), WinStatus::Other), Problem::NoFix);
        for p in [Problem::Denied, Problem::Off, Problem::NoSensor, Problem::NoFix] {
            assert!(approximate_sentence(Some(p)).contains("network address") && approximate_sentence(Some(p)).contains("city"));
            assert!(!no_position_sentence(Some(p)).is_empty());
        }
        assert!(approximate_sentence(Some(Problem::Denied)).contains("blocking"));
        assert!(approximate_sentence(Some(Problem::Off)).contains("turned off"));
        assert!(no_position_sentence(Some(Problem::Off)).contains("turned off"));
        assert_eq!(approximate_sentence(None), approximate_sentence(Some(Problem::NoFix)));
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
