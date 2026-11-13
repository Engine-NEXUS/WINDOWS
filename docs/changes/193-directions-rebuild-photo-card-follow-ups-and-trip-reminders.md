# Directions rebuild: photo card, honest answers, follow-ups and trip reminders

Date: 2026-11-13. Scope: the "directions to X" feature, from the Maps panel opened by voice to the sidebar card and the spoken answer. Research and the gap list are in `docs/research/directions-and-reminders/01-directions-gaps-routes-options-and-reminder-animation.md`; the forward plan is `docs/features/192-universal-reminder-animation-and-directions-plan.md`.

## What the owner asked for

1. An analysis of what the directions feature was missing.
2. The destination's photo (one is enough) and the Maps link in the sidebar.
3. "Do all of them" for the gaps, then test how well it works.

## Before and after

| Area | Before | After |
| --- | --- | --- |
| Code layout | `run_directions` inside `orchestrator.rs`, three fallbacks, the intent dispatched from two places. | New `src-tauri/src/directions.rs` owns the request, the open route, follow-ups and trip reminders. The orchestrator only dispatches. |
| Sidebar | Only the Maps panel opened. | The place card (one photo, distance, car and bike time, fare estimates, address) opens beside it, built in parallel with the time lookup (6 s cap) so it costs no extra wait. Its Maps button opens the **same route** (start, mode, avoided roads, stops) as the panel. |
| Spoken answer | "Finding the best route to X" before anything was known; a time only with a Routes key and a spoken start. | Distance, time, arrival and roads avoided, from the Routes API with a key or a keyless estimate marked "roughly". What could not be found is said. |
| Phrases | "directions to", "route to", "how do I get to". | Plus "take me to / home", "walk / drive / bike to", "I want to go to", "how far is X", "how long to get to X", "what is the distance to X", "eta to X", more mode words, "my college" style saved places. |
| After opening | A new command for every change. | Follow-ups on the open route: mode, avoid tolls / highways / ferries, add or remove a stop, how long now, read the steps, next step. |
| Leave-by | Only calendar events (`memcore/leavewatch.rs`). | "Remind me to leave for X at 5 pm", saved to disk and checked each minute. |

## Parser

`parse_directions_command` in `src-tauri/src/intent_parser.rs` was rewritten around four sections: saved-home phrases, question heads, ambiguous "go to" heads, and the explicit phrasings. Two new parsers sit before it in `parse_deterministic`: `parse_trip_reminder_command` and `parse_route_follow_up`.

* **Explicit phrasings take any destination** ("directions to", "route to", "how do I get to"): the user has said what they mean.
* **Ambiguous phrasings only claim a plausible place** (`trip_place_ok`): "take me to X", "how far is X", "walk to X", "I want to go to X". The destination is refused when it is a known app, site or thing (`NOT_A_TRIP_TARGETS`: youtube, github, calendar, the moon, sleep, bed, the day, lunch and others), contains a word from `PLACE_INFO_NOT_PLACES` (settings, file, battery, phone and others), looks like a web address, or is longer than eight words.
* **The intent shape changed:** `Directions` gained `question: bool`; new `RouteFollowUp { change, value }` (the field is named `change` because the enum is tagged by `action`) and `TripReminder { destination, arrive_min }`. Both new intents route to `GoogleCenter` in `center.rs`.

Phrases that must **not** become a trip are pinned by `directions_do_not_steal_ordinary_sentences` (28 sentences, including "take me to settings", "how far is the moon", "how long to boil an egg", "I want to go to sleep", "walk the dog"). Phrases that must **not** become a route follow-up are pinned by `route_follow_ups_leave_everyday_words_alone` (23 sentences, including "walking", "avoid", "stop", "go through the email on screen", "via email"). Two of those guards were added because the first draft was wrong: "go through the email on screen" was read as a stop on the route, and "via email" as a stop called "email". The leads "go through " and "via " were removed.

## Saved places

`saved_place_kind` still maps home and work. New: "my college", "my gym" (one or two words after "my") look up a remembered fact named `college`, `college_address`, `my_college` or `college_location`; when none exists NEXUS says "I don't know where your college is yet, sir. Say: remember that my college is, followed by the address."

**Privacy rule.** For "home", "work" and any "my X", no photo lookup is made. The place card sends the destination to Nominatim and Wikipedia; the user's own address must not go there. `wants_photo_card` is tested for this.

## The answer NEXUS speaks

Built by the pure function `route_line` from `LineInput` (tested without a window or a network).

| Situation | Example (real output from the live run) |
| --- | --- |
| Question with a keyless estimate | "Charminar is 22 kilometres away, roughly 29 minutes by car, arriving around 11:47 PM, sir. Add a Maps key in the Command Hub for live traffic times." (the key hint is said once per session) |
| Route opened | "The route to t-hub in hyderabad is open, sir. It's 28 kilometres, roughly 36 minutes by car, arriving around 11:55 PM." |
| Follow-up | "Switched to walking, sir. It's 28 kilometres, roughly 5 hours 38 minutes on foot, arriving around 4:57 AM." |
| Follow-up with a stop | "Added cafe bahar as a stop, sir. It's 28 kilometres, roughly 36 minutes by car, avoiding tolls, with a stop at cafe bahar, arriving around 11:55 PM." |
| No distance known | "Opening the route to X, sir." / "I opened the route to X, sir, but I couldn't work out the distance." |
| No start point | "... I couldn't tell where you are, so choose your start on the map." |
| Routes lookup failed | "... I couldn't get the live travel time." |

"Roughly" means the keyless estimate; "about" means the Routes API. Public transport has no keyless time estimate, so only the distance is given. Walking uses 5 km/h. A place name starts the sentence with a capital.

## Route options in every layer

| Layer | Change |
| --- | --- |
| `google/maps.rs` | New `RouteOpts { avoid_tolls, avoid_highways, avoid_ferries, stops }`. `build_directions_url_with` adds `&avoid=tolls%7Chighways` (driving only) and `&waypoints=a%7Cb` (not for transit). `routes_request_body_with` adds `routeModifiers` (driving only) and `intermediates` (not for transit). `compute_route_with(..., want_steps)` asks for `routes.legs.steps.navigationInstruction`, parsed into `RouteEta.steps`. The old `RouteEta::spoken` and the no-options body function were deleted because nothing used them any more (the dead-code ratchet requires that). |
| `maps_panel.rs` | `MapsTarget::Directions` carries `opts`. The keyless embed URL uses Google's long-standing `dirflg` letters (`d`, `w`, `b`, `r`; `h` / `t` / `f` to avoid highways / tolls / ferries) and `daddr=Stop+to:Destination` for stops. |
| `place_card.rs` | `show_card_for_route` stores the route link used by the card's Maps button; `maps_link` accepts only `https://www.google.com/maps/` links and falls back to the plain destination link. New `quick_route` (place lookup plus route, no photo) used for the keyless estimate and for trip reminders. |

## Follow-ups

The last route is remembered for 45 minutes (`ROUTE_TTL`). `apply_follow_up` is pure and tested.

| Say | Result |
| --- | --- |
| "make it walking", "switch to cycling", "what about the bus", "by car instead" | The mode changes and the route reopens. |
| "avoid tolls", "no ferries", "avoid the highways" | Only for driving; otherwise "Roads only matter when driving, sir..." |
| "allow tolls" | Removes the avoidance. |
| "add a stop at X", "stop at X on the way", "add the ATM as a stop" | Adds a stop (at most 5; none for public transport; a repeat is refused). |
| "remove the stops" | Clears them. |
| "how long now", "what's the eta" | Measures again from where the user is now. |
| "read me the steps", "next step", "repeat the step" | Needs the Routes API; otherwise "I don't have the turn-by-turn steps, sir. They need a Maps key; the map shows them." The numbered list also opens in the sidebar. |

When no route is open every follow-up answers "No route is open, sir. Say: directions to, and a place."

## Trip reminders (the first producer for the universal reminder)

"Remind me to leave for the airport at 5 pm", "when should I leave for X to be there by 6 pm", "remind me when to leave for X" (without a time NEXUS asks for one).

* The travel time is measured from where the user is now (Routes API with a key, otherwise `quick_route`). The leave time is arrival minus travel minus a 5 minute buffer; NEXUS speaks 5 minutes before that.
* Stored in `trip_reminders.json` in the application data folder (atomic write, survives a restart, at most 10, dropped 30 minutes after the arrival time). A damaged file reads as an empty list.
* `directions::start` runs once a minute (one small file read when there are none), re-measures when the leave time is within 40 minutes (at most every 10 minutes), and speaks through `proactive_policy::submit` (High). It speaks "Leave in about N minutes", "Time to leave" or "You're running late", never after the arrival time, and only once.
* **A time that has passed** means tomorrow only when that is within 12 hours ("at 1 am" said at 11 pm). Said at 6 pm, "at 5 pm" is refused as probably a slip.
* **A wrong match is refused.** A live run resolved "the airport" to an airport on another continent (124 hours). A trip over 6 hours is now answered "What I found for the airport is about N hours away, sir, which looks wrong. Say its full name and the city."

The on-screen animation for this reminder is a separate, universal feature (`docs/changes/195-reminder-roll-universal-reminder-animation.md`). Nothing in `directions.rs` knows about it.

## Tests

* `directions::tests`: 16 (units, route line, follow-ups, stops, steps, photo-card privacy, trip decisions, pruning, arrival time, persistence, the 6 hour sanity limit).
* `google::maps::tests`: URL options, request body per mode, steps parsing, spoken options.
* `maps_panel::tests`: avoid flags and stops in the embed URL, per mode.
* `place_card::tests`: the Maps button follows a route link only when it is a Google Maps link.
* `intent_parser::tests`: `directions_new_ways_to_ask`, `directions_questions_are_flagged_as_questions`, `directions_do_not_steal_ordinary_sentences`, `route_follow_ups_are_understood`, `route_follow_ups_leave_everyday_words_alone`, `trip_reminders_are_understood`, plus the existing `test_directions_phrases`.
* Whole suite: 3,225 Rust library tests, 20 integration tests; dead-code ratchet 139 known and none new; IPC contract, hygiene and work-card drift gates pass.

## Live proof

2026-11-13, build `3fc76bbe872cb4cb`, on a hidden Windows desktop in quiet mode (`docs/changes/194-test-quiet-mode-and-hidden-desktop-runner.md`), through the application's own `orchestrator_process` command. 14 turns, answers read from the log:

* Every answer carried distance, time and arrival; follow-ups changed the route and the embedded map's address changed with them (the stop appeared in the embed URL and was removed again).
* A landmark (Charminar) got a real photo: Wikipedia, decoded 500 by 706, credit shown, with a "Directions" button, car and bike times and fare estimates. T-Hub, an ordinary business, got the labelled "Map" image (no photo exists for it in the sources).
* "take me to settings" opened Settings and "how far is the moon" went to the knowledge path.
* A reminder to a wrong airport match was refused; a reminder to Charminar was saved and then deleted before the run ended so it could not fire on the owner's real NEXUS.

Bugs found by the live run and fixed in the same pass: the photo lookup (6 s cap) delayed or removed the distance and time (the estimate is now fetched independently); a wrong airport match gave a 124 hour reminder; "at 1 am" said at 11 pm was refused as passed; follow-up answers repeated "The route to X is open" every time; place names started a sentence in lower case; "Time to leave for leave for the airport".

## Not verified

* Turn-by-turn steps and live traffic: no Maps key is configured on the test machine, so the steps path is covered by a unit test on a sample response only.
* Whether Google's embedded map honours the `dirflg` avoid letters (the stops were seen reflected in the embed address; the avoid letters were not visibly checked).
* The card's Maps button opening a browser, and a trip reminder actually firing at its time.
* Speech recognition (the microphone was silent during the run), and the owner's own voice.
* A photo for an ordinary business.
* The location code logs "The system was unable to find the specified registry key or value" on this machine; it is existing behaviour and was not touched.

## Rollback

Revert the commit. No migration: the only new stored file is `trip_reminders.json` (delete it to clear reminders), and no setting was added.
