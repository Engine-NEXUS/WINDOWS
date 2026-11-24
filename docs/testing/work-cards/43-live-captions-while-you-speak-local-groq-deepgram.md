# Work card 43 — Live captions while you speak (local / Groq / Deepgram)

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Voice front end. Queue position 43 of 69. Raw score **72.4%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 97 | 90 | met |
| T | 90 | 70 | met |
| L | 40 | 70 | **gap 30** |
| S | 85 | 80 | met |
| I | 68 | 60 | met |

Evidence today: 4 + 14 stream tests (real localhost WebSocket) + pre-roll 3 + stability 1 + Groq request 3 (stand-in server) + typing 9 + fake mic 2; 10 mutations; whole-app script (scripts/test-live-captions-e2e.mjs); second round: warm-server preference 1, model status 6 + card model 3, live source script (cold/warm) and Hub card check.

## Known gap (from the scorecard)

2026-11-24: real local engine measured with synthetic speech through the whole app (first caption about 0.65 s, median word lag about 0.8 s, 0 confirmed words changed, words in the pill while speech still playing). Fixed: text only grows, all lines kept, pre-roll + seed fed, model off the event loop, unconfirmed tail dimmed. NOT run: a real microphone or person, Groq or Deepgram with a key; Groq (the default source with a key) is 1.5 to 2.5 s behind. Second round: a warm local server is preferred over Groq, the 250 MB model status + background download + Hub card exist (proven in the real app). Open: auto-start of the download, keeping the server warm by choice, the end-of-speech flush. docs/research/live-captions-4-star-2026-11-24.md

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: Edge TTS connection, Kokoro ONNX (lazy), rodio playback, volume lease watchdog
- Under Hand Mode Focus Mode: cache refresh, voice sync and swap downloads deferred

## How to close each gap

- **L** (40 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.

## Shared gates (must pass on the assessed revision)

- `cargo check --lib` clean and `cargo test --lib -- --test-threads=1`; frontend `tsc --noEmit`, `vitest run`, `npm run build`; Worker tests if touched.
- `node scripts/check-turn-ends.mjs`, `check-repo-hygiene.mjs`, `check-ipc-contract.mjs`, `dead-code-audit.mjs`, `check-window-opacity.mjs`.
- Fresh release with provenance (`node scripts/build-provenance.mjs verify`) before any live claim.

## Live script (owner)

1. Write the 5–10 exact spoken or clicked steps that prove the feature, including one failure case (offline, wrong window, denied permission) and the expected honest message.
2. Run them on the fresh release; save the console log to `docs/testing/runs/<date>-<slug>.log`.
3. Record pass/fail per step, sample sizes where it is a rate, and the build identity in `docs/testing/live-proof-ledger.md`.

## Done when

- [ ] Every axis gate above is met with evidence, not estimates.
- [ ] Safety rules each have a mutation that fails a test.
- [ ] A live-proof-ledger line exists for the assessed build.
- [ ] `scorecard-data.json` updated, cards regenerated, queue order rechecked.
