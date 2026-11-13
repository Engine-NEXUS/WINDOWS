# 193 - Features page in the Command Hub (phase 1: on/off)

**Date:** 2026-11-13. Plan: `C:\Users\Chitkul Lakshya\.claude\plans\reactive-crafting-yeti.md` (approved).

## What it is
One page, Command Hub -> **Features**, with a switch for every feature that has a settings key (56 rows in 7 groups: Voice, Screen and vision,
Desktop control, Memory and proactive, Notifications, Agents, System). Each row says what it does, whether it applies at once or after a
restart, what it exposes (mic or camera, cloud, desktop), whether it protects you, and which other switch it needs.

## How it behaves
- Data-driven: `hub/featureRegistry.ts` is the only list. A new switch is one row.
- A write sends only that key through `settings_patch`, then reads the settings back; the switch moves only when NEXUS confirms the new
  value (`hub/featureWrite.ts`). A refused or ignored write shows a plain error and the real state.
- Turning off a protection (Voice Lock, wake check, directed gate, name hiding, shutdown plan) asks first.
- **Safe mode** turns off what listens, watches, acts on the desktop or sends data out. It never touches Voice Lock, local-only speech, the
  wake check or Start with Windows. **Restore defaults** skips the same protections, so it cannot quietly lower one.
- Search and filters: All, On, Off, Needs care.
- Memory and Alerts pages now write with `settings_patch` too (they used to write the whole settings object back, which could undo a change
  made elsewhere).

## Guard against drift
`scripts/check-feature-registry.mjs` (CI and `ci-local`) fails when a boolean in `NexusSettings` is neither a row nor in
`hub/featureInternal.json` with a reason, or when a row names a setting that does not exist. Three settings have no reader in the Rust code
(`wakeWordEnabled`, `meetingModeAuto`, `suppressTtsInMeetings`); they are listed as internal and need a decision (wire or remove).

## Verified
- Vitest 1,156 pass (new: 15 registry + 7 write tests); TypeScript clean; production build ok; the drift check passes.
- Mutation checks, each killed by a test: ignoring the read-back (3 tests), safe mode ignoring protections (2), restore-defaults lowering
  protections (1).

## Not verified
- The page has not been opened in the running app (needs a build). No live flip of any feature yet.
- The readiness badge is not shown: no row claims one until the generator (phase 4) exists.
- No Test buttons yet (phase 2), no switches for always-on features (phase 3), no change log (phase 4).
- Switching a feature off is as real as that feature's own reader: keys that are read once at startup are marked "after restart".

## Live check (owner, to agree before)
Open Features, switch five features off and on (wake sound, Hand Mode, mail watch, agent island, screen tour), confirm each really stops and
restarts within its stated time, run Safe mode and Restore defaults once, and confirm no other setting changed.

## Phases 2 to 4 (2026-11-13)

- **Test buttons and full check.** New IPC `run_system_audit` (the startup audit as OK/WARN/FAIL rows) and `notify_test` (one fixed `notify_selftest` scenario through the real pipeline; unknown names refused). `featureTests.ts` maps a feature to a real command: wake sound, Agent Island preview, mic check, WhatsApp bridge check, vision key check, and test alerts for the notification rows. A row without a test says so; a failing command shows FAIL, never a fake green.
- **Missing switches (default on, restart to apply):** `webhookListener`, `keepAwakePoller`, `sttPrewarm`, each read once at startup. Not given a switch (gated deep in the orchestrator or risky to make inert): proactive ticker, dictation, music, tiling, NLU sidecar, 9Router, compound commands, live browser/WhatsApp, architect. Dead settings `wakeWordEnabled`, `meetingModeAuto`, `suppressTtsInMeetings` stay listed as internal pending a wire-or-remove decision.
- **Readiness.** `scripts/gen-feature-readiness.mjs` reads `docs/testing/scorecard-data.json` through `featureReadiness.map.json` and writes `featureReadiness.json` (live-proof percent per row, 26 rows mapped); `--check` runs in CI and `ci-local`. Rows not mapped show "Live proof not tracked".
- **Change log.** "What I switched" keeps the last 50 changes in the browser's local storage (per viewer, display only).
- **Verified:** frontend 1,161 tests, tsc, production build, registry drift check, readiness drift check, IPC gate, Rust round-trip test for the three new settings. Not run: the page in the live app, the Test buttons, a restart with each new switch off. Live checklist: flip wake sound, Hand Mode, mail watch and Agent Island off and on; press each Test button; run the full check; send a test alert and listen.
