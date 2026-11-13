# Research notes: directions gaps, route options, place photos, test isolation and the reminder animation

Date: 2026-11-13 to 2026-11-14. This is the evidence behind `docs/changes/193-...`, `194-...` and `195-...`. It separates what was **measured on this machine**, what comes from **documentation or long-standing behaviour** and what is an **assumption that has not been verified**.

## 1. What the directions feature was missing (audit of the starting state)

Read from `maps_panel.rs`, `run_directions` in `orchestrator.rs`, `parse_directions_command` in `intent_parser.rs` and `google/maps.rs`.

| # | Gap | Evidence in the code |
| --- | --- | --- |
| 1 | Phrases fell through: "take me to X", "take me home", "navigate to X", "how far is X", "how long to get to X", "when should I leave for X", "walking directions to X". | Probe of `parse_deterministic` (section 2). |
| 2 | Awkward splits: a destination with "from" in its name was cut wrongly; more than 10 words rejected. | `rsplit_once(" from ")`, `words > 10`. |
| 3 | Only "home" and "work" were saved places. | `saved_place_kind`. |
| 4 | NEXUS said "Finding the best route" before anything was known. | `directions_line`. |
| 5 | Without a Maps key there was never a time or distance. | ETA only when `read_api_key("google_maps")` was non-empty and a start existed. |
| 6 | Even with a key only the time was spoken: no distance, arrival or traffic note. | `RouteEta::spoken`. |
| 7 | A failed lookup was silent; a blank panel could appear with no explanation. | `tracing::debug!` only. |
| 8 | No follow-ups: every change was a new command. | No route state existed. |
| 9 | Not connected to the leave-by reminder. | `memcore/leavewatch.rs` handles calendar events only. |
| 10 | Escape inside the embedded map is caught by a 40 ms polling thread because a cross-origin frame owns the keyboard. | `maps_panel::watch_escape` (left unchanged). |
| 11 | Three paths for one feature (embed panel, web panel, old window) and the same dispatch at two places in the orchestrator. | Lines of `orchestrator.rs`. |
| 12 | Unproven live: the keyless embed with `saddr` and `daddr` had only URL-text tests. | `maps_panel::tests`. |
| 13 | The start point was a coarse Windows or IP location. | `location::current`. |

Items 1 to 9 and 11 were addressed; 10 and 13 were left; 12 was measured (section 5).

## 2. Phrase probe (measured)

`parse_deterministic` on the starting code, before any change:

| Phrase | Result before | Result after |
| --- | --- | --- |
| "take me home", "take me to work", "take me to Charminar" | none | directions (home, work, Charminar) |
| "take me to settings" | none | none (correctly not a trip; Settings opens through its own path) |
| "how far is Charminar", "how long to get to T-Hub", "what is the distance to Charminar" | none | directions, `question: true` |
| "when should I leave for the airport" | none | trip reminder (asks for a time) |
| "directions to the airport walking" | destination "the airport walking", mode driving | destination "the airport", mode walking |
| "bike directions to T-Hub" | none | directions, bicycling |
| "navigate to Charminar", "go to Charminar" | open app "charminar" | unchanged (browser navigation owns "navigate to" and "go to") |
| "directions home", "route home", "how do I get home" | none | directions to home |
| "walk me to the station", "drive to Gachibowli" | none | directions, walking, driving |

Two false claims found while writing the guard tests, both fixed: "go through the email on screen" became a route stop, and "via email" became a stop named "email".

## 3. Route options (documentation and behaviour)

* **Routes API v2** `computeRoutes`: `routeModifiers.avoidTolls / avoidHighways / avoidFerries` apply to driving (and two-wheeler); `intermediates` takes waypoints but is not allowed for transit; `routingPreference: TRAFFIC_AWARE` is driving only; turn-by-turn text is `routes.legs.steps.navigationInstruction.instructions` and needs the field mask `routes.legs.steps.navigationInstruction`. The API key travels in the `X-Goog-Api-Key` header, never in a URL or log.
* **Maps URLs (`api=1`)**: `avoid=tolls|highways|ferries` and `waypoints=a|b` (pipe-separated, percent-encoded as `%7C`).
* **Keyless embed** (`/maps?...&output=embed`, "Share, Embed a map"): the long-standing `dirflg` letters (`d` drive, `w` walk, `b` bike, `r` transit; `h`, `t`, `f` avoid highways, tolls, ferries) and `daddr=Stop+to:Destination` for stops. **Measured:** adding a stop changed the embedded frame's address (`pb=!1m8` to `pb=!1m10`) and removing it changed it back. **Not verified:** that the avoid letters change the drawn route.

## 4. The place photo chain (existing code, now reused)

From `place_card.rs`: Google Places photo with the owner's key; else the Wikipedia page image when it is clearly the same place (position check); else a Wikimedia Commons photo near the coordinates for landmark categories; else the place's own website `og:image` (robots.txt respected); else a labelled map image (Esri), never passed off as a photo. Images are fetched in Rust and sent to the page as data URIs, so the page holds no API key. **Measured:** Charminar returned a real Wikipedia photo (decoded 500 by 706 with its credit); "T-HUB Corporate Office" returned the labelled map image.

**Privacy decision.** The chain sends the destination text to Nominatim and Wikipedia. For "home", "work" and "my X" that would send the owner's own address to third parties, which the route itself does not do, so those destinations build no card.

## 5. Keyless travel estimates (existing code, measured)

The place card's route chain is: the owner's routing server, then Google Routes, then the public OSRM server, then a straight line times 1.3 at 25 km/h. Bike time is about 0.85 of the car time and marked approximate. The directions answer reuses this through `place_card::quick_route`. Measured on the test machine (no Maps key): Charminar 22 km, 29 minutes; T-Hub 28 km, 36 minutes. Walking is computed at 5 km/h (28 km gave 5 hours 38 minutes). These are estimates and are spoken as "roughly".

## 6. Leave-by timing (existing constants reused)

`memcore/leavewatch.rs` uses a 5 minute buffer ("parking, walking in") and speaks 5 minutes before the leave time; a trip longer than 3 hours is "a wrong geocode, not a commute". Trip reminders reuse the buffer and the lead and raise the sanity limit to 6 hours (a user may really drive 5 hours). **Measured failure that set the limit:** "the airport" geocoded to an airport 124 hours away.

## 7. Test isolation on Windows (documentation and measurement)

* `SetThreadExecutionState` with `ES_DISPLAY_REQUIRED` asks Windows to keep the display on while set; it postpones the display timeout and the screensaver. NEXUS makes this request while speaking or working.
* `keybd_event` and `SendInput` generate input that resets the idle timer (`GetLastInputInfo`) and can dismiss a screensaver. The earlier harness used them for `Ctrl+Space`.
* A process started with a different `lpDesktop` creates its windows on that desktop (`CreateDesktop`), which the interactive user never sees. **Measured:** 13 NEXUS and WebView processes, 0 visible windows on the default desktop; the user's idle counter kept growing; master volume unchanged. (A different desktop does not isolate the process-wide screen-awake request, which is why quiet mode also no-ops it.)
* WebView2 can be given a debug port with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`; the stage page then lets a script call the application's own `invoke` commands.

## 8. The reminder animation (design research)

* **Digits that move:** an odometer (a strip of digits translated by whole `em` steps, one column per digit) is the cleanest way to show "from this time to that time" because each digit travels through real values and settles on an exact alignment. A transition on `transform: translateY(calc(var(--i) * -1em))` animates when the custom property changes. Measured settle alignment: each strip offset equals its digit index.
* **Disappearing "in the air":** a blur-and-fade in place (opacity to 0, `filter: blur()` growing, a slight scale) reads as dissolving, whereas a translate toward the corner reads as "sliding away". The owner asked for the first.
* **Orb consistency.** From the project notes: pure white particles on an obsidian capsule (`#000`, 10 px corners, 1 px `rgba(255,255,255,0.16)` edge, deep shadow); captions reveal letter by letter; the entrance easing is `0.72s cubic-bezier(0.22, 1, 0.36, 1)`; the orb's particle text was removed on 2026-10-06.
* **Universality** means the animation consumes one payload (`reminder:set`) and knows nothing about the producer. Producers already in the codebase that would send it: trip reminders (`directions.rs`), calendar leave-by (`memcore/leavewatch.rs`), timetable slot reminders (`memcore/scheduler.rs`), the voice reminder and timer commands, study slots.
* **User control** is a switch because a persistent chip is visual noise for some users and essential for others; the user's choice must dominate whatever a producer asks.

## 9. Assumptions and unknowns

| Item | Status |
| --- | --- |
| Avoid letters in the keyless embed change the route | Unverified |
| Turn-by-turn steps from the Routes API | Parser tested on a sample; never run against the API (no key on the test machine) |
| A trip reminder fires and is spoken at its time | Decision function tested; not seen live |
| Owner's voice, wake word, speech recognition | Not exercised (the microphone reported zero signal during the run) |
| Whether the chip should fade in place instead of dropping in | Open question for the owner |
| Reminder Roll in the real stage window (not the browser preview) | Not built yet |
