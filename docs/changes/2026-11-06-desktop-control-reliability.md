# Native desktop-control reliability implementation — 2026-11-06

## Result and evidence boundary

The approved Windows programme is implemented in source, with an isolated offline test harness, safety mutations, sidebar clarification, action-specific result verification and qualification assets. **It is not device-qualified or production-ready.** No NEXUS application build, launch, stop, restart, settings change, actual keyboard/mouse dispatch or live account operation was performed. Existing unrelated contributor-setup, agent-watch and other working-tree changes are preserved.

The exact tested file hashes and dirty-checkout HEAD are recorded in [the offline assessment](../testing/runs/desktop-control-offline-20261106/assessment.json). That manifest covers the changed runtime paths, UI, helper/tests and fixed cases; it is not executable provenance or a claim that all repository files were frozen. Full Tauri/native library and integration tests, compile-dependent dead-code checks and release provenance remain owner/CI work.

## Why this change

The old shortcut router could mark an action verified when the foreground window disappeared or any other window/title changed. That is evidence of activity, not evidence that the requested app received the intended action. Accessible clicks also lacked a bounded, actionable duplicate-label flow. Slow observations could outlive cancellation, and generic dispatch acknowledgements could be spoken as completion.

Two isolated regressions reproduced the old false-success behavior before the implementation: an unrelated foreground window and a disappearing foreground. Semantic fake observations then replaced seven older tests whose expectations relied on the removed title heuristic. Those old assertions were incompatible with the approved success contract; they were not silently counted as positive evidence.

## Native decision rules and observations

`desktop_control.rs` introduces TargetQuery, TargetSnapshot, WindowIdentity, Candidate, EffectState, EffectSpec, a fakeable Observer and expiring single-use choices. TargetQuery keeps the label plus optional role/container/learned alias. Window identity includes HWND, PID, process creation time and exact executable. Candidate snapshots carry UIA runtime ID, automation ID and observation generation.

Resolver order is validated learned automation identity, exact accessible name, mnemonic/whitespace/case-normalized exact name, then a reviewed per-executable alias table. There is no unrestricted substring or model-confidence selection. Eligibility requires enabled, visible, non-password and permitted controls with a readable runtime identity. Legacy click commands and learning records remain readable. Learning can additionally retain a hash of the parent container; existing app-build invalidation is preserved.

`desktop_native.rs` supplies Windows observations and dispatch. It re-resolves runtime/automation identity inside the original process/window, compares current name/role/container and rechecks protections immediately before action. It uses selection, toggle, expand/collapse and invoke patterns, then the existing guarded pointer route if necessary. Pointer dispatch requires the current point to hit the intended UIA runtime identity before and after movement. No screenshot/OCR/vision fallback or browser-controller dependency was added.

## Clarification and cancellation

Two to five matches produce numbered choices with label, role and containing panel in the existing sidebar. More matches require a more specific request. Labels are bounded and sanitized; the UI sends only token and number. Detailed identities remain native and ephemeral.

The pending request binds the contributor/personal application identity, original cancellation flag, Voice Lock state, window/process identity, observation generation and 30-second lifetime. Matching tokens are consumed before dispatch. A wrong token cannot consume a newer choice. Selection re-resolves the live control; it does not click saved coordinates. The sidebar may restore the exact original window only after identity/protection checks. Voice restoration is restricted to the actual sidebar HWND, not every NEXUS window.

Escape, explicit cancellation, sidebar closure, expiry, a non-choice authorized command or observed unrelated foreground movement invalidates the choice. Active selections register cancellation before focus restoration. Continuations keep their original cancellation and permissions binding, and can publish a further clarification without losing it. Request-identity comparison prevents a late completion from attaching its authorization to another request's choices.

A final integration review caught that normal wake/barge-in cancellation would destroy pending choices before “choose two” reached the parser. Capture transitions now cancel running work while preserving a waiting choice; explicit cancellation still clears it. The frontend capture handoff awaits its cancellation acknowledgement before opening a new turn, so a late cancel cannot race the new selection. Native reason policy, frontend invocation and wiring have regressions. This does not weaken the normal voice ownership gate.

## Effect verification and recovery

| Supported route | Evidence used |
|---|---|
| App focus/open follow-up | Exact intended executable/window becomes foreground; five-second budget; one activation request |
| Tab/item selection | The intended UIA item is selected |
| Toggle | The observed target reaches the expected toggle state |
| Menu expansion | Expected expand/collapse state, or a newly observed expected named menu/dialog/surface under the bound app |
| Existing text insertion | The same field's readable contents match the exact insertion/replacement, including Unicode boundaries |
| Pane scroll | The same scrollable ancestor's finite scroll percentage changes in the requested direction |
| Browser tab action | Expected count or selected index in the observable tab group; Ctrl+9 correctly means the last tab |
| Browser back/forward | Observable address/document state changes in the bound window |

Other effects remain Unconfirmed. Dispatch alone cannot produce Verified. Title changes are not used as success. Generic menus with no trustworthy accessible result may remain unconfirmed; this is intentional and will reduce the real qualification score until provider support is proven.

Effect polling is 50 ms with a two-second deadline, five seconds for focus. Cancellation/window identity and deadlines are checked around observations, including after a slow call. Blocking accessibility runs on existing blocking workers. Outer Ghost/choice waits expire after 20 seconds, cancel remaining work and reject late results. A COM call already inside Windows cannot be preempted or rolled back; the runbook records that limit.

The keyboard adapter also rechecks PID/process creation time before sending, so a reused HWND and unchanged executable name cannot stand in for the original process. A dedicated regression changes only that lifetime and requires zero sends.

Each state-changing step dispatches at most once. Timeouts do not automatically resend. Compound tasks retain the five-step cap and continue only after Verified. Unsupported/lost acknowledgements stop with honest receipts. Ordinary shortcut callers still receive a sent-but-unconfirmed result without triggering a legacy retry. Native Ghost launch failures no longer fall through to a second blind launcher. New persistent diagnostic lines contain operation ID, route, step, timing and outcome, not window/control contents or inserted text.

Typing binds its target immediately before insertion and verifies it afterward. It does not add a capture-start editor binding during speech transcription. Existing dictation undo behavior was not redesigned. These limits, short away-and-back focus switches between 250 ms monitor polls, older Settings hosts and unsupported UIA properties remain explicit qualification concerns.

## File map

| Files | Purpose |
|---|---|
| `src-tauri/src/desktop_control.rs` | Pure resolver, effects, observation boundary, insertion comparison, choice state |
| `src-tauri/src/desktop_native.rs` | Real Windows UIA/Win32 observations and guarded single dispatch |
| `src-tauri/src/desktop_choices.rs` | Ephemeral coordinator, sidebar IPC, original-window restoration, expiry/cancellation |
| `src-tauri/src/ghost_center.rs` | Context grammar, choice grammar, bounded sequential orchestration and receipts |
| `src-tauri/src/shortcuts/router.rs`, `exec.rs` | Semantic success and owning-request cancellation through the existing keyboard router |
| `src-tauri/src/ghost_learning.rs` | Optional parent identity hash, backward-readable records |
| `src-tauri/src/dictation.rs`, `orchestrator.rs` | Exact-field result handling, honest unconfirmed response, focus identity and capture/cancel integration |
| `src-tauri/src/lib.rs`, `ghost.rs`, `commands.rs` | Module/IPC registration and existing cancel/close boundaries |
| `frontend/src/sidebar/DesktopChoices.tsx`, model/tests/CSS, `UnifiedSidebar.tsx` | Accessible numbered choice panel and bounded submission |
| `frontend/src/audio/recorder.ts`, `stage/orbRuntime.ts`, `net/orchestrator.ts` and test | Distinguish a capture transition from explicit cancellation |
| `scripts/desktop-control-harness/` | Compile/test actual decision rules, Windows adapter, coordinator and extracted orchestrator functions with inert app/input boundaries |
| `scripts/desktop-control-contract.test.mjs` | Cross-file authorization, cancellation, UI registration and capture wiring assertions |
| `scripts/desktop-control-mutations.py` | Disposable source mirror; assertion failures required; checkout never mutated |
| `scripts/desktop_control_trials.py`, tests | Strict device-result accounting and uncertainty; no execution |
| `scripts/verify-desktop-control.py` | Two unchanged-source passes, file fingerprints and per-check logs |
| `.github/workflows/desktop-control-offline.yml` | Windows/Linux offline helper/type/contract CI; no application build or live input |

## Verification and failed attempts retained

Final counts are in the linked assessment/logs. The final two passes run the same file manifest and include the offline Rust harness, frontend types/full tests, Worker tests, trial reporter, wiring contracts, IPC, turn-end, repository hygiene, color, opacity, contrast and whitespace checks. Mutation results are [recorded separately](../testing/runs/desktop-control-mutations-20261106.json). Mutation kills require assertion failures; compiler failures do not count.

During development:

- The old behavior failed the two intended regressions.
- An initial mutation mirror omitted the keyboard seed and dictation corpus files, causing compilation errors. The mirror now copies these inputs; those errors were not credited as mutation kills.
- A mutation exposed a non-advancing fake clock. The helper loop hung; its exact identified helper process was stopped, its clock fixed, and the checks rerun. No NEXUS process was targeted.
- Concurrent helper/mutation output initially hit a Windows executable file lock. Mutation outputs are now isolated under their own Cargo target directory.
- A Python edit hit Windows' default text-decoding error before editing the orchestrator. Explicit UTF-8 fixed the helper. Extracted orchestrator type checking also caught and repaired an incorrect nested Result in the timeout return.
- The new frontend capture test initially asserted that cancellation was the last IPC call, overlooking the existing subsequent stop-TTS call. It now checks the cancellation arguments independently; its failed log is retained.
- The capture-wiring contract initially assumed LF while the source had CRLF. The failed log is retained alongside final logs; the contract reader normalizes line endings. Source changes during review required rerunning the unchanged-source passes instead of inheriting earlier counts.

The harness uses real production Rust modules and real Windows UIA dependencies. Application handles, desktop input and side effects are inert boundaries; Tauri command attributes are excluded from the coordinator extraction. It proves the checked logic and adapter type compatibility, not Tauri macro expansion, the complete application binary, a running UI, or live provider behavior.

## Qualification and readiness

[Work card](../testing/desktop-control/work-card.md), [owner runbook](../testing/desktop-control/runbook.md), [fixed matrix](../testing/desktop-control/matrix.json), and [current empty device assessment](../testing/desktop-control/qualification-status.json).

The matrix is Edge 8 + Explorer 8 + Notepad 8 + Settings 6, repeated ten times. Core 0/300, held-out 0/20, voice 0/30. Expected safe refusals are assessed separately in held-out cases; they do not count as executable core completions. Missing capabilities remain in the denominator. The reporter checks source/build consistency, duplicate slots, finite timings, explicit evidence references and safety counts, and reports a Wilson interval. It never declares production ready.

No live-proof ledger success was added and no scorecard values changed. Required next gates are the complete exact-source native library/integration checks and dead-code ratchet, fresh release provenance, all real-device runs including display scaling/dual monitors, owner voice/visual review, and existing weighted readiness thresholds. Full feature parity across arbitrary apps or platforms is not claimed.

## Final offline result

Both unchanged-source passes passed: **144 Rust helper tests, 851 frontend tests, 145 Worker tests, 9 trial-accounting tests and 8 wiring contracts per pass**, plus frontend types and IPC/turn-end/hygiene/color/opacity/contrast/whitespace guards. **12/12 safety mutations** were killed by assertions. The source-manifest SHA-256 is `bf825a2782496565399dab2ce6d7acc66197a1e710c5bdfd9c58e4e6ce45cd57`. Read-only Rust syntax and Python byte-compilation checks also passed. Documentation links and the 8/8/8/6 matrix counts were checked.

These are offline results. Complete application native/integration checks, dead-code, release provenance and all real-device trials remain not run. No production-readiness score was raised.
