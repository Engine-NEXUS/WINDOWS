# Change 191: official icons and tile contrast, quieter finish announcements, no barge-in on NEXUS's own voice, a stuck-speaking orb watchdog

Date: 2026-11-10/11. Branches: `icons/antigravity-official` (PR 52), `fix/agent-finish-repeat` (PR 54),
`fix/barge-echo-orb` (PR 53), `fix/orb-stuck-speaking`. Combined build: `integration/three-fixes`
(merge `90fe404`). Research: `docs/research/session-2026-11-10/`. Plan: `docs/features/191-production-readiness-implementation-plan.md`.

## 1. Icons (PR 52)

- `antigravity.svg` replaced with the official mark from `antigravity.google/favicon.svg` (the old file was cropped and
  unmasked); `gemini.svg` gets the official gradient on the star; `kilo.svg` the official yellow mark. Source URL,
  domain, date and hash in `SOURCES.json`.
- New `scripts/gen-icon-tiles.mjs` picks a tile per icon so the mark is visible (brand colour for white marks, light
  tile, dark tile) and writes `frontend/src/settings-sidebar/hub/activityIconTiles.json`; `tileColor()` uses it. CI and
  `ci-local` run it with `--check`. A test checks the contrast of every icon on its tile.
- CI fix: hash of the LF content the repository stores, not of the Windows CRLF working copy.

## 2. Finish announcements (PR 54)

- `finish_worth_telling`: a finish is told only for a turn of at least 45 s and not again within 3 min per agent
  (`agent_attention.rs`); suppressed finishes log `[AGENT] ... stop not told`.
- `turn_start` / `turn_started_ms` (`agent_hooks.rs`): a turn begins at its first Start after the previous Stop
  (Antigravity reports a Start per model call).

## 3. Voice (PR 53)

- `barge_gate.rs`: echo gain learned from every chunk of the first 0.8 s, loudest reference of the last 400 ms,
  threshold 3x, absolute floor 0.03, minimum learned gain 0.05.
- `frontend/src/stage/bargeOrb.ts` + `orbRuntime.ts`: `barge:resume` also leaves the orb's thinking state.
- `lib.rs` + `tts.rs`: a stale `tts-ended` is ignored when a newer `tts-started` arrived in its grace period
  (`tts_end_applies`).

## 4. Orb watchdog (branch `fix/orb-stuck-speaking`)

- `orbIdleWatchdog.ts`: `speaking` with no audio, caption, loading, pending question or ghost session for 4 s ->
  `reset_and_hide`. Previously only a 60 s failsafe covered a stuck speaking orb.

## Verify

- Rust lib 3,178 passed on the combined tree (before the watchdog); 12 barge-gate tests; 213 agent tests.
- Frontend 1,129 passed, `tsc --noEmit` clean, icon gate (95 logos), `gen-icon-tiles --check`, IPC / turn-end /
  hygiene gates, dead-code ratchet (139 known, none new).
- Live on the owner's laptop: Agent Island end-to-end script, 12 of 12 checks; four spoken test lines with no orb flip;
  Antigravity start and stop seen and a 5.7 s task correctly not announced.

## Not verified

- Owner speaking over a reply; the real-room false-positive rate of the new gate.
- A 45 s+ real task announced exactly once.
- The new Antigravity icon, toasts and Hub tiles inside the running app.
- Whether the 4 s speaking watchdog changes what the owner saw (the symptom was not reproduced).
- CI on PRs 53 and 54 and on the combination.
- Codex and OpenCode as real runs.
