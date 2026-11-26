# Open-source routing, place lookup and device location for Directions (2026-11-25)

Question: how can "directions to X", "how far is X" and "read the steps" give a **real** distance, a **real** travel time for the chosen mode and **turn-by-turn steps** without a Google key, and tell the user honestly what it does not know? Companion to `01-directions-gaps-routes-options-and-reminder-animation.md`. Built in `src-tauri/src/open_routing.rs`; record `docs/changes/208-maps-open-source-routing-steps-and-location.md`.

What was checked live is marked **(probed)**: one tiny request per service on 2026-11-25, a short Hyderabad trip, identified by the application name only. Everything else was read from the sources linked.

## 1. What the feature had before

* Place lookup: Nominatim, one hard-coded address, no cache, no pause, no way to move it.
* Distance and time without a key: the OSRM demo server, **car profile only**, one request, bike time guessed as 0.85 x the car time, walking guessed as 12 minutes a km, a straight line x 1.3 as the last resort. Spoken as "roughly".
* No turn-by-turn steps without a Google key (the answer was "they need a Maps key").
* Device location: Windows Geolocator, then the network address (city level) after any failure, with one fixed hint, so a refusal by Windows, a switch that is off and "no sensor" were all the same sentence.

## 2. Routing engines compared

| Project | Licence (checked via the GitHub API) | Fit |
|---|---|---|
| OSRM `Project-OSRM/osrm-backend` | BSD-2-Clause | Fast, steps with maneuvers, a public demo server and a FOSSGIS server with car, bike and foot. **Chosen.** Paper: Luxen and Vetter, "Real-time routing with OpenStreetMap data", ACM SIGSPATIAL GIS 2011. |
| Valhalla | MIT (licence file; the GitHub API reports NOASSERTION) | Multi-modal incl. transit with GTFS, but needs a server you run; no public free instance with a usage policy suitable for a desktop app. Kept as the "own server" option. |
| GraphHopper | Apache-2.0 | Public API is keyed and rate-limited; the open server needs your own data. |
| OpenRouteService | GPL-3.0 | **Not used**: NEXUS ships no GPL code or data (see AGENTS.md on Wappalyzer). A call to a public HTTP API would not link GPL code, but the free API is keyed and the data would have to be shown under its terms; not worth it when OSRM works keyless. |

### Public OSRM servers (policy read from the sources)

* `router.project-osrm.org` (OSRM demo wiki, "Demo server"): "restricted to reasonable, non-commercial use-cases. Do not exceed 1 request per second. We provide no guarantees wrt. uptime, latency, or data updates." **(probed) Every profile name in the URL gets the car profile**: a `walking` request returned the same 5159.7 m / 458.8 s as `driving`. So the code uses it for **driving only** and refuses to answer walking or cycling from it.
* `routing.openstreetmap.de` (FOSSGIS): separate servers per profile, `routed-car`, `routed-bike`, `routed-foot`. **(probed)** the same trip gave 5159.7 m / 459 s by car, 4963 m / 1261 s by bike and 4379 m / 3507 s on foot, with 16, 29 and 38 steps. Policy from its about page: display the required attribution and a "fix the map" link, a valid user agent, one request per second at most, no scraping and no heavy use; request logs are kept (so the user's route request is stored on their server: noted in the privacy line below).
* **(probed) `exclude=toll`, `exclude=motorway`, `exclude=ferry` all answer `InvalidValue: Exclude flag combination is not supported`** on `routed-car`. So avoiding tolls, highways or ferries cannot be done on the public servers. The app says so instead of silently ignoring it: "The open route can't avoid tolls, sir, so this one ignores that; the map shows the choice."
* A multi-waypoint request returns one leg per stop **(probed)**; the code reads "arrive" at the end of a non-final leg as "Reach stop N".
* Public transport: neither public server does it. The app says "Public transport times need a Maps key".

## 3. Place lookup

* **Nominatim** (OpenStreetMap Foundation), usage policy read on 2026-11-25: absolute maximum **1 request per second**; "Provide a valid HTTP Referer or User-Agent identifying the application (stock User-Agents as set by http libraries will not do)"; attribution; results under ODbL; "Apps must make sure that they can switch the service at our request at any time (in particular, switching should be possible without requiring a software update)"; "periodic requests from apps are considered bulk geocoding ... strongly discouraged"; "Results must be cached on your side. Clients sending repeatedly the same query may be classified as faulty and blocked"; no auto-complete; "Please do not submit personal data".
  * What the code does about each: the application name goes in the User-Agent (`NEXUS-Assistant/1.0`, the public project address, no person); one lookup at a time with a 1.1 s gap; results cached 24 h (not found: 10 min) so a repeated question or a trip reminder re-measuring every 10 minutes never repeats a lookup; the service address is a setting (`openGeocoderUrl`) read on every use, no new release needed; a 429, 503 or 403 pauses the service for the `Retry-After` time (default 60 s, 10 min for a 403, never longer than 10 min); and **a home, work or "my X" address is never sent** (section 6).
* Alternatives (not used by default): Photon (komoot, Apache-2.0, built for search-as-you-type), Pelias (MIT). Both are servers you run or keyed services; they are what `openGeocoderUrl` can point to.

## 4. Device location

* The app already calls the WinRT `Geolocator` (Microsoft Learn, "Geolocator Class"). When the user has switched location off for desktop apps, `GetGeopositionAsync` throws `UnauthorizedAccessException` (HRESULT `0x80070005`, E_ACCESSDENIED); the page's own sample says "Location access was denied or revoked" and points to Settings > Privacy > Location.
* `Geolocator.LocationStatus` says `Disabled` (switch off) or `NotAvailable` (no sensor/service). The code now keeps the reason (`location::Problem`: `Denied`, `Off`, `NoSensor`, `NoFix`) and says it in words, instead of one fixed hint.
* When Windows gives no fix the network-address estimate (an existing call to `ipwho.is`, city level) is still used, and the answer now says "That's measured from your approximate position" every time, not only once, because a city-centre start gives a wrong distance.

## 5. Findings that changed the design

1. The OSRM demo is car-only whatever the URL says. A "walking" time from it would be a car time; the previous estimate (12 min per km) was at least labelled "roughly". The new code uses FOSSGIS for foot and bike and refuses the demo for them.
2. The public servers cannot avoid tolls: an honest sentence was needed, not a silent ignore.
3. A trip reminder to "home" re-measured the time every 10 minutes by sending the resolved home address to Nominatim (through `place_card::quick_route`). That was an existing privacy leak, found while tracing the callers; trips now remember `private` and only a Maps key can time them.
4. Caching and a global gate matter more than the 1 request per second figure: one spoken question started three concurrent lookups of the same place (card, estimate, route). One lookup at a time plus the cache makes it one request.

## 6. Privacy

* Sent to the open services: the destination name (and stops, and a spoken start) to Nominatim; start and destination coordinates to the route server; the application name. FOSSGIS logs route requests. The user's own coordinates already went to the same services before this work (the old OSRM call) and to `ipwho.is` for the network fix.
* Never sent: home, work, "my X" (`open_routing::spoken_is_private`, checked inside `plan` before any request, again for stops added later, and kept per trip). For those trips the route can still open in the Maps window and, with a Maps key, be timed by the Routes API as before.
* Not logged: place names and coordinates; errors carry no data.

## 7. What only a live run can prove

The probes above show the services answer as the parser expects. They do not prove: the spoken answers sound right on the real speaker, the Windows location refusal wording on a machine where it is refused, the card and the open route together under the 1 request per second gap, behaviour when the services are slow for a long time, or a trip reminder firing on an open-map time.

## Sources

* OSRM demo server usage: https://github.com/Project-OSRM/osrm-backend/wiki/Demo-server
* FOSSGIS routing server about/usage page: https://routing.openstreetmap.de/about.html
* Nominatim usage policy: https://operations.osmfoundation.org/policies/nominatim/
* OSRM paper: Luxen, D. and Vetter, C., "Real-time routing with OpenStreetMap data", Proc. 19th ACM SIGSPATIAL GIS, 2011.
* Windows Geolocator: https://learn.microsoft.com/en-us/uwp/api/windows.devices.geolocation.geolocator
* Licences: GitHub repository metadata for Project-OSRM/osrm-backend (BSD-2-Clause), osm-search/Nominatim (GPL-3.0, a server we do not run or ship), graphhopper/graphhopper (Apache-2.0), GIScience/openrouteservice (GPL-3.0), valhalla/valhalla (MIT), pelias/pelias (MIT), komoot/photon (Apache-2.0).
