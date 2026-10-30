# 153 - Implementation plan: wake, name word, place info, repo + Architect, email, friends map

Date: 2026-10-30. Evidence and reasoning: [research 01](../research/voice-and-maps/01-wake-delay-name-filter-maps-email-architect-plan-2026-10-30.md).
Latency ideas: [voice-latency research](../research/voice-latency/01-how-100ms-voice-responses-work.md).
Every phase ends with: unit tests, `node scripts/ci-local.mjs`, a live script run by the owner on a fresh release, one ledger line.

## Phase W - Wake reliability (built in source, awaiting live proof)
- Done: owner-voice fast path (`wakeOwnerFastPath`, default on), cloud check timeout 10 s to 3 s, `[WAKE]` log line.
- Still to do: (a) start the Rust capture in parallel with the cloud check and discard it on reject; (b) never gate the mic on the stage being ready; (c) let a newer candidate replace a stuck check.
- Live script: say "Nexus" 20 times at normal distance and 10 at 1 m. Pass = 95 % reach listening in under 300 ms (read the `[WAKE]` lines), no 3 s stalls.
- Then run `nexus wake score` on your recordings; the false alarms per hour must not rise.

## Phase N - "Nexus" said while awake is ignored
- Files: `wake_names.rs` (new pure `is_name_only`), the capture receiver in `wakeword_oww.rs` (drop before `stt:transcript`), `directed.rs` for hot-mic sessions.
- Rule: name-only transcript while a turn is open means drop it, keep the mic open, do not advance the "didn't catch that" ladder. "Nexus open chrome" and "what is nexus" are unchanged.
- Tests: exact, near and split spellings, with punctuation, with a greeting; inside a Ghost session; during speech (barge-in still works).

## Phase P - Place info ("where / when is the Charminar")
- New `ParsedIntent::PlaceInfo { query }`, parsed after Directions and Places so neither is stolen; guard against non-places.
- Execution: one Places lookup (name, address, hours today, rating), open Maps on the place, speak one line, sidebar card with the details. "When" is answered from the opening hours.
- Tests: parser matrix (positives and the existing idiom negatives), Places JSON fixture, missing-key message, no key means open Maps only.
- Live script: three place questions, one unknown place, one with the key removed.

## Phase R - "Analyse this repo" shows overview and Architect together
- Layout first (pure, tested like `layoutSlots`): sidebar docked right at its width, Architect as its own window docked to its left; neither covers the other on 1366x768 and 1920x1080 at 100 % and 150 %.
- `AnalyseRepo` starts the Architect and the Worker analysis in parallel; speak one line; either window closes alone.
- Risk: the Architect currently lives inside the single unified sidebar window, so this needs a second window label and its own geometry.

## Phase E - "Analyse this email" with directions
- New `AnalyseThisEmail`: open email by Gmail thread id (URL) or vision fallback, one model call returning `{title, dates, deadline, venue, address, links, actions}`, validated in code.
- Sidebar card with those fields; if a venue or address exists, open Maps on it (Phase P path) and offer directions from home. No venue means no map.
- Tests: three real hackathon emails (venue, online only, two venues) plus the sensitive-window refusal.

## Phase F - Friends and group on the map (needs a decision first)
- Nothing exists: no location source and no friend positions in the code.
- Decide the source (phone link, shared-location links, or manual check-ins) and the privacy rules (consent per person, expiry, local only). Then design the view (Google Maps cannot be embedded, so it would be an in-app map or markers sent to the Maps window).
- Not scheduled until the decision is made.

## Voice speed (from the latency research)
1. Speak from the first model clause; 2. cached acknowledgements for commands; 3. a short filler within about 200 ms for slow answers; 4. start the model early on a tentative end of speech; 5. add a `first_audio_ms` value to the existing `[LAT]` line.

## Tracking
Add scorecard entries (live 0 until proven): Wake reliability, Name-word filter, Place info, Repo + Architect, Email analysis. Maps (card 45) and Architect (card 61) stay as they are. Proof for each = one line in `docs/testing/live-proof-ledger.md` after a live run.
