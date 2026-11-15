# Agent watch cross-check — 2026-11-06

Status: five original root causes have source fixes; three additional edge cases remain open. The feature is not production-qualified. This is a verification/reporting pass, with no application source repair, build, launch, shutdown, settings change or real power operation.

## What still passes

All feature-source and test-file fingerprints in the November 5 two-pass report still match the checkout. Fresh checks passed 118 isolated native helper/policy/parser/glue tests and 15 Node sidecar/template/contract tests. The seven earlier mutation assertions remain recorded evidence, not a new mutation run today. The earlier frontend 846 and Worker 145 results were not rerun in this cross-check.

The repository IPC check now passes: the previously missing `barge:duck` and `barge:resume` emitters exist in the current voice code. This is a change since the November 5 report, not evidence that the old failed check passed then. Turn-end, repository hygiene and the affected tracked files' whitespace check also pass today.

| Original problem | Current result | Evidence limit |
|---|---|---|
| Antigravity invocation Stop mistaken for a task finish | Fixed in source; explicit fully-idle check and busy veto pass | Real provider/device trial pending |
| Multiple completion publishers repeat one task | Shared durable receipts; duplicate callback/restart/away tests pass | Audible delivery and real keychain still unverified |
| Six seconds of file silence interpreted as completion | Removed; generic output words also rejected | Real desktop integration pending |
| Cancellation loses to a late normal-exit callback | Cancellation ordering and late-callback tests pass | Real CLI cancellation trial pending |
| Windows power calls run on different threads | Dedicated owner thread; fake-backend tests pass | Actual Windows power-state proof pending |

These are five addressed original causes, not a measured percentage of production readiness.

## Open issues found by the deeper review

### 1. Older busy sibling can be excluded from a newer watch

`Registry::busy_since` checks both active phase and `observed_ms >= since`. Both `watch_runner::look_at` and `desktop_result` use a watch-start time as that cutoff. A session already active before that time, with no more recent hook, is excluded even when it remains running. Another session's completion can consequently be treated as the only relevant end.

Desired regression: session A starts at time 1, watch begins at 50, session B stops at 100 and settles. Session A must still veto an all-finished decision. The isolated native probe checks exactly this. This is particularly important before relying on a watch to permit a power-plan decision; the audit does not execute or demonstrate an actual shutdown.

Next fix: query all currently active matching sessions independently of when the watch began, while keeping the time filter for selecting terminal outcomes. Add a mixed old/new-session integration test before changing the rule.

### 2. Provisional sidecar completion hides definitive process exit

The sidecar marks `task_complete` as terminal `done`. `setStatus` ignores all later updates once terminal, so a subsequent `session_stopped` callback cannot replace its detail with `process_exit`. The Rust poller also only processes changes to the status string. This matters when the shared registry still has an explicit busy veto: an unknown adapter Stop cannot clear it, but a definitive exit should resolve it.

Desired regression: send `task_complete`, then a real normal `session_stopped` callback. The final record must retain the definitive exit evidence while cancellation remains final. The isolated VM probe checks this sequence. Later failure evidence can also be discarded by this terminal guard; that consequence follows from the code and should receive its own failure-exit regression in the repair.

Next fix: separate provisional adapter output from definitive process outcome, expose an evidence revision or sequence to the poller, and allow stronger evidence without reopening cancellation or repeating a completion receipt.

### 3. Automatic OpenCode/Kilo tracking lacks permission-resume activity

Both generated templates post Start for a user message, Wait for a permission request and Stop for idle. They do not handle a busy/resume status, and assistant-message updates are ignored. After a permission request releases the automatic task keep-awake reason, continuing without a new user prompt does not send the activity needed to reacquire it through these templates.

Desired regression: user message → permission request → simulated busy status/assistant activity, with no new user prompt. A resumed-activity event must reach NEXUS for the same session. The VM probes execute both actual templates with fake clients and fetch. These are simulated adapter inputs; provider/version-specific real resume contracts still need qualification. An explicit desktop watch can retain a separate pinned lease and mask this problem, which is why the automatic-only route needs its own test.

Next fix: validate the supported provider resume events, forward only root-session activity, maintain the same task identity and ensure repeated activity cannot create duplicate completion receipts.

## Reproduction and evidence

Run `python scripts/audit-agent-watch-edges.py`. It copies the small helper harness to a temporary directory and executes actual sidecar/template code with fake boundaries. It does not change application source, build NEXUS, launch agents, contact a real provider, access the owner's credentials or exercise OS power APIs. Exit 1 from assertion failures means these desired behaviors remain unimplemented; compilation/harness errors are not counted as confirmed bugs.

The raw probe results and source fingerprints are in [agent-watch-edge-audit-20261106.json](../testing/runs/agent-watch-edge-audit-20261106.json). These deliberately red audit probes are separate from the 118+15 existing passing tests.

Observed result: all four desired-behavior probes failed by assertions, with no compilation or fixture failure: one native busy-sibling probe, one definitive-exit VM probe, and one permission-resume VM probe for each of OpenCode and Kilo. Thus three issue classes are reproduced in isolated code paths. This is not a claim of four newly observed live-device incidents.

## What remains before closure

1. Repair the three edge cases, retain their regressions and mutation-check the new rules. Repeat focused checks twice on the final source.
2. Run the full native library/integration/dead-code gates and assess the combined source after concurrent edits. Produce and verify a fresh release manually under the owner's build policy.
3. Refresh installed hooks through the normal approved flow and run real tasks: busy invocations, permission/resume, concurrent old/new sessions, finish, failure, cancellation, restart and away/return. Check one completion history item/speech handoff and real Windows power release.

Legacy hooks without stable identities, version-specific provider contracts and Linux's unsupported keep-awake backend remain scope limitations. No live score or production-readiness rating is raised by this audit. No claim is made about whether the owner's currently running executable contains these source changes.
