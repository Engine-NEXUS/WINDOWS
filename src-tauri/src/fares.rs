//! Ride-fare ESTIMATES for the place card. These are never live prices: no Uber, Ola or Rapido data is used (their
//! price APIs are closed to us, see docs/features/154-place-card-sidebar-plan.md). The auto fare follows the Telangana
//! tariff as reported by the press (the official order was not found); cab and bike-taxi are wide, editable
//! assumptions. Pure: distance in, ranges out. An optional `fare_tariff.json` in the app data folder overrides any
//! number, so a wrong assumption can be fixed without a new build.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Fare {
    pub kind: &'static str,
    pub label: &'static str,
    pub low: u32,
    pub high: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Tariff {
    /// Auto: minimum fare, the distance it covers, then per km (Telangana, 2026).
    pub auto_min_fare: f64,
    pub auto_min_km: f64,
    pub auto_per_km: f64,
    /// Auto: multiplier from 11 pm to 5 am.
    pub auto_night_multiplier: f64,
    /// Cab: (low, high) base and per km. An assumption: app cabs have no published Hyderabad rate card.
    pub cab_base: (f64, f64),
    pub cab_per_km: (f64, f64),
    /// Bike taxi: (low, high) base and per km. An assumption (no official tariff).
    pub bike_base: (f64, f64),
    pub bike_per_km: (f64, f64),
}

impl Default for Tariff {
    fn default() -> Self {
        Self {
            auto_min_fare: 30.0,
            auto_min_km: 1.6,
            auto_per_km: 14.0,
            auto_night_multiplier: 1.5,
            cab_base: (60.0, 80.0),
            cab_per_km: (11.0, 18.0),
            bike_base: (25.0, 30.0),
            bike_per_km: (5.0, 8.0),
        }
    }
}

/// The tariff in use: the user's `fare_tariff.json` when it parses, the defaults otherwise.
pub fn load_tariff(dir: &Path) -> Tariff {
    std::fs::read_to_string(dir.join("fare_tariff.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 11 pm to 5 am. Pure.
pub fn is_night(hour: u32) -> bool {
    hour >= 23 || hour < 5
}

fn down5(v: f64) -> u32 {
    ((v / 5.0).floor() * 5.0).max(0.0) as u32
}
fn up5(v: f64) -> u32 {
    ((v / 5.0).ceil() * 5.0).max(0.0) as u32
}

/// Fare ranges for a trip of `km` road kilometres. Empty for a missing or absurd distance. Pure.
pub fn estimate(t: &Tariff, km: f64, night: bool) -> Vec<Fare> {
    if !km.is_finite() || km <= 0.0 || km > 300.0 {
        return Vec::new();
    }
    let mut auto = t.auto_min_fare + (km - t.auto_min_km).max(0.0) * t.auto_per_km;
    if night {
        auto *= t.auto_night_multiplier;
    }
    vec![
        Fare { kind: "auto", label: "Auto", low: down5(auto), high: up5(auto * 1.15) },
        Fare { kind: "bike", label: "Bike taxi", low: down5(t.bike_base.0 + km * t.bike_per_km.0), high: up5(t.bike_base.1 + km * t.bike_per_km.1) },
        Fare { kind: "cab", label: "Cab", low: down5(t.cab_base.0 + km * t.cab_per_km.0), high: up5(t.cab_base.1 + km * t.cab_per_km.1) },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_telangana_slab() {
        let t = Tariff::default();
        let f = estimate(&t, 1.0, false);
        let auto = f.iter().find(|x| x.kind == "auto").unwrap();
        assert_eq!(auto.low, 30, "inside the first 1.6 km it is the minimum fare");
        // 10 km: 30 + 8.4 * 14 = 147.6 -> 145 .. 170
        let f = estimate(&t, 10.0, false);
        let auto = f.iter().find(|x| x.kind == "auto").unwrap();
        assert_eq!((auto.low, auto.high), (145, 170));
    }

    #[test]
    fn night_raises_only_the_auto_and_ranges_are_ordered() {
        let t = Tariff::default();
        let day = estimate(&t, 8.0, false);
        let night = estimate(&t, 8.0, true);
        let a = |v: &Vec<Fare>| v.iter().find(|x| x.kind == "auto").unwrap().low;
        assert!(a(&night) > a(&day));
        let cab = |v: &Vec<Fare>| v.iter().find(|x| x.kind == "cab").unwrap().clone();
        assert_eq!(cab(&day), cab(&night));
        for f in day {
            assert!(f.low <= f.high && f.low % 5 == 0 && f.high % 5 == 0, "{f:?}");
        }
    }

    #[test]
    fn night_hours_and_bad_distances() {
        assert!(is_night(23) && is_night(0) && is_night(4) && !is_night(5) && !is_night(22));
        for km in [0.0, -3.0, f64::NAN, 5000.0] {
            assert!(estimate(&Tariff::default(), km, false).is_empty(), "{km}");
        }
    }

    #[test]
    fn a_user_tariff_file_overrides_only_what_it_names() {
        let dir = std::env::temp_dir().join(format!("nexus_fares_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fare_tariff.json"), r#"{"autoPerKm": 15.0}"#).unwrap();
        let t = load_tariff(&dir);
        assert_eq!(t.auto_per_km, 15.0);
        assert_eq!(t.auto_min_fare, 30.0);
        std::fs::write(dir.join("fare_tariff.json"), "not json").unwrap();
        assert_eq!(load_tariff(&dir), Tariff::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
