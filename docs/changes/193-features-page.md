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
