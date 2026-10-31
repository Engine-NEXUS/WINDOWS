# 180 - Wake fast path, name-only filter, place info, analyse-this-email, Architect in its own window

Date: 2026-10-31. Plan: [153](../features/153-wake-name-maps-email-repo-implementation-plan.md). Evidence: [research 01](../research/voice-and-maps/01-wake-delay-name-filter-maps-email-architect-plan-2026-10-30.md).
Source and tests only. None of it was run in the live app.

## What changed
| Area | Change |
|---|---|
| Wake | **Correction (2026-10-31 live log):** the owner-voice fast path (skip the cloud speech check when Voice Lock reads >= 0.90 and the model >= 0.70) woke NEXUS on the owner's ordinary speech (prob 0.84-0.88, voice 0.95-0.97), because a voice match does not tell "Nexus" from other words. It is now OFF by default (`wakeOwnerFastPath`) and the cloud check is back on every wake. Kept: cloud check timeout 10 s to 3 s, the `[WAKE]` log line. **Real bug found in the same log and fixed:** every neural wake logged `ERROR stt-capture: missing lifecycle at endpoint; dropping untracked buffer`: the seeded capture start armed the buffer without the capture lifecycle, so the command spoken after the wake word was thrown away (this is "it wakes but does not listen"). Seeded and unseeded starts now share `start_stt_capture_inner`; a wiring-pin test guards it. |
| Name word | `wake_names::is_name_only`: a transcript that is only the name (optionally with a greeting, repeated) is dropped in the capture receiver, as a silent miss, so a lone "Nexus" is never sent to the cloud model. |
| Place info | New `PlaceInfo` intent: "where / when is X", "when does X close", "is X open", "X on the map". One Places lookup with opening hours, Maps opens on the place, one spoken line, details in the sidebar. Controls, people, events and ideas are excluded (`PLACE_INFO_*` lists, tested). |
| Email | New `AnalyseThisEmail` intent and `email_brief.rs`: sensitive windows refused, screenshot read by Gemini, answer validated in code (https links only, no online venues on the map, lengths clamped). Sidebar card, map only when a venue or address exists. |
| Architect window | `architect-map` is its own window (`sidebar.html?pin=architect`), placed left of the 520 px response sidebar by `sidebar_geometry("architect-beside")`. "Analyse this repo" now opens it at once while the Worker analysis fills the sidebar. A pinned window never follows `sidebar:set_view`, never moves the sidebar, and Escape closes only itself (`close_architect_map`). |

## Tests
Rust: owner fast path (7 cases), name-only (10 yes / 10 no), place-info phrases (7 positive, 17 guards), hours and spoken line, email brief (7), geometry for six screens, repo-name splitting. Frontend: pinned view (2). Dev browser: `?pin=architect` mounts only the Architect view.

## Not verified / risks
- Wake: the owner fast path trusts the local voice match; a false-alarm count is still needed (`nexus wake score`). Listening-while-checking and replacing a stuck check are not built.
- A dropped name shows up as a silent miss, so two in a row make the mic nag once.
- Place info: "when is X" is claimed unless X contains an event word; an unlisted event noun would open a map.
- Email: not tried on a real email; needs a Gemini key.
- Architect window: needs a live check of position at 125/150 % scaling, focus, and the Maps window (still centred) overlapping it. `DEV_KEEP_WINDOWS_ALIVE` in `dyn_windows.rs` is still true (keeps the sidebar resident); flip before a release.
