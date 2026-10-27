# Repaired Voice Lock settings — device verification

Run: 2026-10-25 18:26–18:33 UTC (2026-10-25/26 IST). Assessor: BOT, real Windows laptop. Speaker feature remains **41% (2.05/5), in progress**.

## Exact candidate

- Revision: `3e5b4a803b2a22bc28986189377942f127234fa7`.
- Source/input: `342c6f390461e0c69d0c6aa6b66536d84d9cd2c16423f0f8e35604d206dc9124`.
- Release: `0b10fd3a1416a540`; optimized build took 61m37s.
- Executable SHA256: `4a47ca0248975454aed3cc7a6bf963f0d22380a7b7d863c51f26cd1df0589cc3`.
- Launched isolated readiness executable as PID 12064; source, frontend and executable provenance verified before launch and after testing. See [launch](windows1-20261025-voice-lock-ui-v2-live-launch.json), [final verification](windows1-20261025-voice-lock-ui-v2-post-live-provenance.json).

Full shared checks on this source: 2,533 Rust library + 19 integration, 38 ignored; frontend 687 + TypeScript/build; Worker 145 + TypeScript; Python 60; NLU and repository guards; 33 mutations killed. Dead code: 148 known, none new. [Machine assessment](windows1-20261025-voice-lock-ui-v2-assessment.json).

## Device results

UI input used Windows UI Automation through Computer Use. Native IPC readbacks checked the actual routing and persisted flags independently of the display. Browser-rendered screenshots supplied visual evidence because native windows intentionally exclude screen capture.

| Check | Observed result |
|---|---|
| Initial Audio | Effective On, draft On, native/saved On, enrolled five-clip profile; no accessible Saved before Save. |
| Draft Off without Save | Displayed effective On/draft Off; both native flags stayed On. [Readback](windows1-20261025-voice-lock-ui-v2-live-unsaved-readback.json), [visual](windows1-20261025-voice-lock-ui-v2-hub-draft.png). |
| Save Off / On | Both native flags changed to the requested value; UI matched and Saved appeared after confirmation. [Off](windows1-20261025-voice-lock-ui-v2-live-save-off-readback.json), [On](windows1-20261025-voice-lock-ui-v2-live-save-on-readback.json). |
| Escape / Ctrl+Shift+S | Reopened Audio with effective/draft On and no stale Saved confirmation. |
| Standalone settings open | `open_settings_window` returned in 640 ms, actual NEXUS Settings window appeared, process remained Responding=true. [Timing](windows1-20261025-voice-lock-ui-v2-live-open-legacy.json). No watchdog blackout/panic in the run logs. |
| Cross-window native change | Save Off in standalone settings produced native/saved Off. Focusing Hub refreshed effective **and untouched draft** to Off and cleared old acknowledgement. [Native](windows1-20261025-voice-lock-ui-v2-live-cross-window-off.json). |
| Explicit draft preservation | Hub draft On/effective Off remained so after manual native Refresh. Then normal Save restored On. |
| False acknowledgement simulation | Confirmed interception of one settings patch, no native write; effective/native stayed Off, requested On failed verification, visible mismatch and no Saved. |
| Refused write simulation | Confirmed one intercepted patch; visible refusal, effective Unknown and no Saved. |
| Eight-second delayed write simulation | Confirmed one pending native patch. UI showed Saving/Unknown, disabled Save and switch, no Saved; native stayed Off during delay. After completion native/saved and UI became On. The brief final Saved announcement was not sampled; final confirmed state was. |
| Profile-status error simulation | Confirmed one intercepted profile-status request; UI showed Unknown and error, no Saved. Removing simulation and Refresh recovered On with the original enrolled profile. No profile bytes were changed. |
| Cleanup | All fetch interceptors removed, no pending delayed write, original native/saved On restored, profile still five clips, wake listening resumed, standalone settings closed, Hub dismissed. [Final native readback](windows1-20261025-voice-lock-ui-v2-live-final-readback.json), [visual](windows1-20261025-voice-lock-ui-v2-hub-final.png). |

Controlled-fault UI observations and interception counters are in [JSONL](windows1-20261025-voice-lock-ui-v2-live-ui-observations.jsonl) and matching `fault-*-interception.json`, delay and profile evidence. The first two JSONL entries used an ineffective invoke wrapper: Tauri defines invoke as non-writable. That setup did not simulate a failure; its actual successful native Save is **excluded from fault proof**. The helper was repaired to intercept fetch, assert installation and record actual interception counts; all fault claims above use those confirmed repetitions. The intermediate label `delayed-write-settled` was sampled while still pending; only `delayed-write-final-readback` establishes completion.

## Limits and retained warnings

The native standalone settings window defaults to the same Command Hub component; it was initially called legacy in the protocol. The old `SettingsApp` prototype at `?view=legacy` was **not** separately exercised on device. Its automated regressions remain distinct from this shipping-window proof. The startup-task guard was attempted before standalone Save, but the original invoke wrapper was ineffective. The shipped Hub Save path does not call `set_autostart`, so these operations did not exercise the legacy prototype's startup-task side effect.

This passes the repaired **shipping settings-flow** device checks, not the complete speaker-verification work card. Real missing/corrupt/deleted-profile recovery, enrollment attribution repeat, locked owner trials, unfamiliar live speakers, TV/replay and ten-start capture reconciliation remain pending. Zero owner or negative trials are inferred from these UI tests.

Retained startup warnings: missing optional Tier 3 command models, Groq menu-model mismatch and registry lookup failure. A microphone-silence warning appeared after the controlled pause/resume period; it does not establish a driver failure or owner rejection. Check the next actual voice attempt against its capture logs. No threshold/default-off/authorization changes were made during verification.

Second pass compared native flags, UI observations, the actual settings entrypoint, opener source, run logs and frozen-source provenance. Score remains 41%; queue does not advance.

## Subsequent owner voice failure

The owner then reported rejected/nothing. This is **not** a successful end-to-end voice check: two speech captures were owner-verified, but one app transcript duplicated the verb and another was repeat-loop filtered; a later stage watchdog blackout was logged. Initial `RUST_LOG=warn` hid structured capture accounting, so the reported one attempt cannot be cleanly matched to these multiple captures. The same executable was restarted as PID4564 with explicit INFO/audit logging; native On/profile5 survived, microphone telemetry is nonzero. [Failure and restart evidence](windows1-20261026-owner-001-result.json). Settings-flow passes above remain scoped; feature readiness and command fidelity do not pass.
