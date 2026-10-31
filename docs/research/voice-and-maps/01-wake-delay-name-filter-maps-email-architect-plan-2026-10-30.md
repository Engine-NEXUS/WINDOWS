# Wake delay, "Nexus" as a plain word, Maps, email, Architect: research and plan (2026-10-30)

Plan and research only. Nothing in the application was changed. Evidence is the source, the 28 Oct 2026 runtime log
(`%APPDATA%\com.nexus.assistant\nexus_unified.log`, 19:19-20:20 UTC, about one hour) and the scorecard. Nothing was run live.

## 1. Saying "Nexus" wakes the orb but the mic does not listen, or listens late

**What the log shows.** In that hour there were 6 wake candidates (the model scored 0.76-0.78 on all of them):

| Outcome | Count | What happened |
|---|---:|---|
| Verified, wake fired | 2 | the speech check read "Nexus." |
| Rejected by the speech check | 2 | it heard "Mixes." / "Mixer." and "Next." and suppressed the wake |
| Timed out | 1 | the speech check waited the full 10 s, then suppressed the wake |
| Dropped | 2 | "candidate dropped (already in flight)" while that 10 s wait was running |

So roughly half of the wake attempts in that hour produced no listening at all, and one blocked every further wake for 10 s.
Every wake that did succeed first paid a cloud round trip (median 0.47 s, from "cross-checking" to the result).

**Cause (source).** `verifyWake` defaults to on and the mode is `always` (`wakeword_oww.rs`, `verify_candidate`, see also
AGENTS.md change 91): the candidate audio is sent to Groq and the wake fires only if the transcript looks like "Nexus". There is no
timeout shorter than 10 s (`VERIFY_TIMEOUT_SECS`), only one check runs at a time (`VERIFY_IN_FLIGHT`), and after a pass there is
another 150 ms drain plus up to 2 s waiting for the stage (`wake_orb_src` path) before the orb events are sent.
The orb can still light up from a different path (the instant-fire or hotkey path), which matches "it wakes up but does not listen".

**Plan (in order, each measurable with `[LAT]` and the verify log lines):**
1. Switch the default to `wakeVerifyMode = tiered` with `wakeVerifyHigh` around 0.75 (the code and `wake_calibrate.py --verify` already exist). Candidates the model is confident about fire at once; only the uncertain band pays the round trip.
2. Cut `VERIFY_TIMEOUT_SECS` from 10 to about 2, and let a second candidate replace a stuck check instead of being dropped.
3. Start the Rust capture (`start_stt_capture_seeded`) in parallel with the check and throw the audio away if the check rejects, so a verified wake loses no words. The seed code already handles audio that arrives after the detection point.
4. Do not wait for the stage before arming the mic; emit the orb event when ready, never gate the mic on it.
5. Add one `[WAKE]` summary line per candidate (prob, verify result, ms to capture start) so the next log gives a real rate. Target: fired-within-300 ms for at least 95 % of owner wakes, no 10 s stalls.
6. Re-run the owner wake set with `nexus wake score` before and after (false alarms per hour must not rise).

## 2. "Nexus" said while NEXUS is already awake must be ignored

**Today.** `strip_wake_prefix` only removes the name when something follows it. A bare "Nexus" is kept as a transcript
(`hey nexus` parses as a greeting; plain `nexus` falls through to the cloud model, which is how you get "Nexus is...").

**Plan.** One pure function `is_name_only(transcript)` built on `wake_names` (exact, near and split forms, with an optional "hey/ok",
and trailing punctuation). Use it at one choke point, in the capture receiver before `stt:transcript` is emitted, whenever a turn is
already open (listening, speaking, Ghost session, confirmation window):
- name-only transcript: drop it, keep the mic open, do not reply, do not count it as an empty miss (so the "didn't catch that" ladder does not advance);
- name plus a command ("nexus open chrome"): unchanged, the name is stripped as now;
- a question about the product ("what is nexus") is not name-only and still goes through.
Tests: bare/near spellings, with and without punctuation, inside a Ghost session, during TTS (barge-in must still work), plus a regression that "nexus open chrome" and "hey nexus" at idle still behave as today.

## 3. Maps: what exists and what does not

- The map window is the **real Google Maps** in its own window (`dyn_windows::open_external_window`, https only, no app IPC), not a copy. Google blocks embedding, so it cannot be drawn inside the sidebar. It re-points one window and uses the same geometry as the Architect.
- Spoken "directions to X" / "how do I get to X" / "route from A to B" opens it and speaks a traffic-aware ETA only when you give an origin and have a Routes key. "Nearest pharmacy" style searches work only with a proximity word (near, nearest, nearby).
- **"When is the Charminar" / "where is the Charminar": not handled.** `parse_places_command` requires a proximity marker, and "when is" is a time question. It would reach the cloud model as text and no map opens. This is a gap, not a bug in the window.
- **Friends / group mode: does not exist in the code.** There is no live-location source at all (no device location, no friend positions), and the Free Device Links work is pairing and files only. A "friends on the map" view therefore needs a new feature, not a fix.
- Existing evidence: work card 45 (Maps) scores 72.7 %, live axis 55 of 70 needed. Nothing has been run on the real window with the current build.

**Plan.**
1. New intent `PlaceInfo { query }` for "where is / what is / when is / is X open / show me X" when X looks like a place and no other parser claims it (a small place-word guard like `NOT_PLACES`, plus a Places lookup so an unknown noun is not guessed).
2. Behaviour: one Places call (name, address, today's hours, rating), open Maps on that place, speak one line ("Charminar is in Hyderabad, open until 5:30 pm"), put the details in the sidebar. "When" is answered from the opening hours, not from a date.
3. Live test first: open the window, drive it with the three phrases (directions with and without origin, nearest, place info), and check size, focus, position beside the sidebar, repeat requests reusing the window, and a missing key.
4. Friends / group: only after a decision on the location source (phone link, shared-location links, or manual check-ins). Needs privacy rules first (consent per person, expiry, local only). Not planned for implementation yet.

## 4. "Analyse this email" with a hackathon: details in the sidebar plus directions

**Today.** The email watchers read Gmail (history, deadline changes) and "analyse the email ... internship" scans internship mail;
"watch this email" registers a watch. No intent takes the open email, extracts a venue and opens directions.

**Plan.** Intent `AnalyseThisEmail`: get the email (browser URL thread id first, vision fallback; both already exist), one model call that returns a fixed shape
`{title, dates, deadline, venue, address, links, actions}`, validated in code like the screen tour; sidebar card with those fields; if `address` or `venue` exists, open Maps on it (same `PlaceInfo` path) and offer "Directions from home?" (home comes from remembered facts). Never guess a location: no venue means no map.
Tests: three real hackathon emails (with venue, online-only, two venues), plus the sensitive-window refusal.

## 5. Architect Mapper: how well it works

- Strong on paper: work card 61 scores 79.9 % (build 100, tests 85, live 70 met; safety 78 of 80). 29 Rust tests, 23 pure tests for the repo analysis. Phase 1 diagram comes from parallel GitHub calls and a local heuristic (about 1-1.5 s), then the model relabels layers.
- Weak spot for what you asked: it shares the **one unified sidebar window** with the response sidebar (`open_architect_window` prepares the "sidebar" window in the architect view). Two views cannot be on screen at once, and the Maps window uses the same centred geometry.
- "Analyse this repo" today (`AnalyseRepo`) sends the Worker analysis to the response sidebar only. It does not open the Architect.

**Plan for "analyse this repo" showing both.**
1. Decide the layout: response sidebar docked right at its normal width; Architect as a second window docked immediately to its left (give it its own window label and a `sidebar_geometry("architect", dock)` that shares the monitor with the sidebar, not the centred 960 px box).
2. On `AnalyseRepo`, run both in parallel: the Architect starts at once (heuristic diagram first), the analysis fills the sidebar when it arrives; speak one line only.
3. Keep either one closable alone; Escape closes the one you are in.
4. Test on 1366x768 and 1920x1080 at 100 % and 150 %: both fit, no overlap, with the existing slot tests as the template.

## 6. Which tool and sub-center, and how to show it (the routing table to add)

Rule used by the Command Center today: the parser picks an intent, `center.rs` maps it to a sub-center, the orchestrator runs it, and the display is chosen per intent. The new rows:

| You say | Intent | Sub-center / tool | Display |
|---|---|---|---|
| directions to X | `Directions` (exists) | MapsCenter / Maps window, Routes API | map window + one spoken ETA line |
| where / when is the Charminar | `PlaceInfo` (new) | MapsCenter / Places API | map window + sidebar card + one line |
| nearest pharmacy | `PlacesSearch` (exists) | MapsCenter / Places API | map window + one line |
| analyse this email | `AnalyseThisEmail` (new) | GoogleCenter / Gmail + model | sidebar card; map window if a venue exists |
| analyse this repo | `AnalyseRepo` (exists) | GitHubCenter + Architect | sidebar (right) + Architect (left) together |
| friends / group on the map | none yet | needs a location source | not planned |

## 7. How to track progress

Use the scorecard so it stays visible: add queue entries (live 0 until proven) for **Wake reliability**, **Name-word filter**, **Place info**, **Email analysis with directions** and **Repo analysis with Architect**, and keep Maps (45) and Architect (61) as they are. For each, the proof is one line in `live-proof-ledger.md` after a run on a fresh release. Suggested numbers to record: wake success rate and median ms-to-listening from the new `[WAKE]` line; false alarms per hour; and pass/fail of the live scripts above.

## Suggested build order

1. Wake delay (section 1), because it affects every command. 2. Name-word filter (2), small and independent. 3. Place info (3). 4. Repo plus Architect layout (5). 5. Email analysis (4). 6. Friends and group, only after a location-source decision.
