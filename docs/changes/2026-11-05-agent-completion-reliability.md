# Agent completion notifications and keep-awake reliability

Current follow-up: the [November 6 cross-check](2026-11-06-agent-watch-cross-check.md) reproduces three additional edge cases. The historical IPC failures below have since been resolved in the current voice code. This original implementation record is retained as written; it is not a production-readiness certificate.

Date: 2026-11-05. Status: implemented in source; offline verification completed separately from device qualification.

## Request and operating boundary

The owner reported repeated agent-completion notifications and asked for implementation and two verification passes. This pass changes the task tracker, hook metadata, CLI-watch bridge, completion delivery and Windows power ownership. It does not build, launch, stop or reconfigure the owner's NEXUS. Existing unrelated edits remain in place. No production-readiness score is promoted.

## Evidence and root causes

Read-only examination of the existing local log found six TTS requests on November 5 containing “While you were away, sir: Antigravity has finished.” Three recent Stop observations before those messages had `fully_idle=Some(false)`. Those logs lacked execution IDs: six requests are observed; six repetitions of one identified logical task cannot be established from that history.

The code did establish several independent defects:

1. `agent_attention` interpreted a newly received Stop as completion, without the stricter outer-task idle check used by the session observer. An invocation ending while Antigravity remained busy could create a new completion message.
2. Background attention, explicit watches, plan reporting and return-from-away summaries published completion independently. Receipt timestamps were used where task identity was needed.
3. The desktop watcher used six seconds without top-level file changes as completion evidence. A long-running agent can legitimately write nothing for much longer.
4. Stop/cancellation ordering allowed a synchronous normal process-exit callback to win before cancellation was recorded.
5. Windows execution-state assertions are thread-local. Acquire, watchdog and release could call the API on different threads.

Microsoft documents the thread scope and limitations of [SetThreadExecutionState](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate). It does not override deliberate sleep or lid closure. OpenCode's [plugin documentation](https://docs.dev.opencode.ai/docs/plugins/) provides session events and child-session information; provider contracts and adapter versions still need real-session qualification. Native notifications emitted by an agent itself are separate from NEXUS notifications.

## Implemented behavior

### Shared task authority

`agent_lifecycle.rs` contains a pure bounded state machine. It distinguishes running, waiting, settling, completed, failed, cancelled, expired and unknown states. Session/execution/event IDs are preferred; a watch launched by NEXUS passes one stable `NEXUS_WATCH_ID` to its children so hook and sidecar evidence address the same task.

A stable session ID takes precedence over optional workspace metadata, so a Start without a folder and a Stop with a folder still address the same task. Folder scoping remains the fallback for legacy events without session IDs.

An explicit busy Stop vetoes completion. Antigravity requires `fullyIdle=true`; unknown output-pattern evidence cannot override a previous explicit busy observation. Accepted completion waits for 15 seconds of quiet. New activity cancels settling. Duplicate Stops do not restart that interval. Failure is sticky within a task. Cancellation suppresses an undelivered completion, including a completion that already settled.

Independent sessions remain independent. Finishing one does not release another's automatic keep-awake reason. Waiting for input releases the automatic task reason; resumption reacquires it. A task exceeding six hours expires with completion unconfirmed, never with success. Separate explicit plan/desktop-watch power reasons retain their own bounds.

The shared automatic power lease renews before its own lifetime expires, acquiring the replacement before releasing the old one. This covers overlapping tasks whose combined running period exceeds a single lease's lifetime; individual tasks remain bounded by their six-hour limit.

### Durable completion receipts

`agent_lifecycle_runtime.rs` coordinates hook intake, owned-watch observations, notification history and speech handoff. Background attention, explicit completion reports and return-from-away completion delivery now share it. Old completion-only away-log entries without task identity are discarded; legacy input-request summaries remain.

Receipts use a separate encrypted `agent_tasks.sealed` file and an application-identity-scoped keychain entry, `agent_task_receipts_v1`. They do not depend on memory being enabled and are not cleared merely because conversational memory is wiped. No output text, raw session IDs or working paths are persisted in this registry: identifiers/output fingerprints are hashed; transient paths are retained only for notification navigation while the process lives.

The completion claim is saved before notification/speech handoff. A failed save defers handoff and permits retry. Missing keys, corrupt state and unavailable credential storage preserve existing bytes and disable this coordinator with an error instead of silently resetting deduplication. Recovery from initialization failure currently needs an owner-controlled restart after the underlying problem is fixed.

This provides at-most-once handoff per retained receipt. It cannot promise exactly-once audible playback: a crash after reserving a receipt can leave delivery unknown. The system avoids automatically replaying that uncertainty. Retention is seven days with bounded task/event capacity; it is not an eternal receipt archive.

### Sidecar and desktop watch changes

Generic “Done”, “Finished”, prompt characters and filesystem silence no longer establish a desktop finish. Real hook evidence or a definitive owned-process exit is required. Desktop matching refuses ambiguous results or a still-busy sibling session. CLI state shown to callers is overlaid with the shared verified phase rather than prematurely exposing a raw adapter result as final.

User cancellation is recorded before contacting the sidecar and before any synchronous exit callback. Poll loops reject late evidence after local cancellation. The authenticated `/observed` route acknowledges the coordinator's result without stopping the coding agent. Its token and Host checks remain enforced. Input-required events retain their own existing notification/reminder behavior; they are not repeated completion messages.

The installed hook scripts are not modified by this source pass. After the owner builds manually, existing integrations need the normal user-approved hook reconnect/refresh and a new agent session to receive the updated templates. Older scripts without a watch alias cannot reliably merge an owned CLI and its independent hook record.

### Windows power ownership

`power_thread.rs` serializes execution-state assertions and releases on one long-lived owner thread. Backend errors propagate; bounded requests time out; a late unacknowledged assertion is cleared on that same owner. Existing lease accounting and the watchdog remain. macOS's existing `caffeinate` backend is unchanged. Linux keep-awake remains unsupported by this backend and must not be advertised as qualified.

## Files and test architecture

| Area | Files |
|---|---|
| Pure state and runtime | `src-tauri/src/agent_lifecycle.rs`, `agent_lifecycle_runtime.rs` |
| Hook identity / intake | `agent_hooks.rs`, `webhook.rs` |
| Shared publishers | `agent_attention.rs`, `watch_runner.rs`, `notify_hub.rs` |
| CLI and desktop watch | `agent_watch.rs`, `resources/agent_watch_sidecar/server.cjs` |
| Power ownership | `keep_awake.rs`, `power_thread.rs` |
| Offline native tests | `scripts/agent-watch-harness/` |
| Contracts and sidecar VM | `scripts/agent-watch-contract.test.mjs`, `resources/agent_watch_sidecar/lifecycle.test.cjs` |
| Repeatable checks | `scripts/verify-agent-watch.py`, `scripts/agent-watch-mutations.py` |
| CI | `.github/workflows/agent-watch-offline.yml` |

The standalone Cargo harness compiles actual lifecycle/power helpers, actual hook-parser sections, existing observer/verdict/tracker/countdown policies and actual runtime glue against fake storage, keyring, notification, audio and OS boundaries. It does not compile the desktop application or exercise real encryption/power behavior. The original sidecar security tests spawn only their own short-lived test server; the new VM tests use fake processes and sockets and run no coding-agent task.

## Verification record

The machine-readable two-pass report is [agent-watch-20261105.json](../testing/runs/agent-watch-20261105.json). It fingerprints the relevant source files and records each check's exit code rather than allowing the last command to hide an earlier failure.

Focused suite: 118 native policy/parser/helper/glue-shim tests and 15 Node source/sidecar/template tests. Seven safety mutations are killed by assertion failures: explicit busy veto, independent 15-second settle interval, persisted claims, sticky failure, cancellation, authoritative evidence and restart removal of stale holds. The [mutation report](../testing/runs/agent-watch-mutations-20261105.json) identifies each removed rule. Generated OpenCode/Kilo templates are executed with fake clients/fetch to verify root-session identity on Start, permission and Stop, child exclusion, and conservative handling of failed parent lookup.

| Check | Pass 1 | Pass 2 | Scope |
|---|---:|---:|---|
| Native helpers/policies/parser/glue shim | 118 passed | 118 passed | Isolated package; no desktop application build |
| Node contracts/sidecar/templates | 15 passed | 15 passed | Fake agents plus isolated test servers |
| Frontend | 846 passed | 846 passed | Existing full unit suite |
| Worker | 145 passed | 145 passed | Existing full unit suite |
| Frontend and Worker types | Pass | Pass | No application build |
| Turn-end and repository hygiene | Pass | Pass | Source guards |
| Repository event contract | Fail | Fail | Two unrelated voice events listed below |

Initial contract tests failed against the original implementation. During development, harness dependency/extraction errors were corrected; expanded glue coverage exposed label normalization and caught its repair. A first mutation sweep found the settle-duration test was self-referential; an independent 15-second boundary regression was added before the sweep passed. These failures are retained here rather than represented as uninterrupted green results.

Reusing the mutation build cache across temporary mirrors also exposed a harness path bug: its build script had embedded the previous temporary directory at compilation. It now reads the manifest directory at execution, and the sweep was repeated. This was a test-infrastructure failure, not a live application incident.

The pinned toolchain lacked rustfmt locally. Read-only parsing used the already installed stable rustfmt for ten affected modules; it did not rewrite source or install components. Full NEXUS compilation, integration suites, compile-dependent dead-code checks, release provenance and real-device proof were not run under the owner restriction.

The repository IPC checker also reports two unmatched listeners, `barge:duck` and `barge:resume`, in the separately edited voice UI. This pass repaired its own watch event declarations without weakening the checker or adding unrelated baseline exceptions. A full repository green claim must wait for those voice events to be resolved and native checks to pass.

## Qualification limits and owner-operated acceptance

Legacy adapters without stable session/run identifiers fall back to agent/workspace scope. They cannot perfectly distinguish simultaneous sessions or a replayed Start from a new task. Unknown provider semantics fail conservatively. The OpenCode template remains version-specific; newer provider event contracts are not automatically qualified. Completion means observed termination, not proof that the agent's work is correct.

After a manual build, the owner should record exact source/build identity and check:

1. One long Antigravity task with internal invocations and `fullyIdle=false`: no finished notification; keep-awake persists.
2. A true idle completion: one history item and at most one speech handoff after settling. Repeated callback delivery and leaving/returning repeatedly do not repeat it.
3. Two tasks/sessions: first finish announces once while the second retains keep-awake; final finish releases the remaining automatic reason.
4. A real new task with identical final wording receives a distinct completion receipt.
5. Agent asks for permission/input: automatic task hold releases; input notice remains distinct; resuming restores the hold.
6. Cancel during running and settling: no late completion; unrelated tasks remain awake.
7. Failure and timeout report failure/unconfirmed, never successful completion. No unattended power action is introduced.
8. Restart after an announced finish: no replay. Restart with an old active record: no stale automatic awake hold. Confirm notification navigation when transient context is unavailable.
9. Compare Windows power requests before/during/after real work and cancellation. Verify acquire/reassert/clear use one owner thread; respect explicit sleep/lid behavior.
10. Reconnect each installed hook and run its real supported provider version. Confirm environment aliasing for owned CLI children, and distinguish the provider's own notifications from NEXUS's.

Real native credential persistence, encrypted-file recovery, UI delivery, actual speech and device power behavior remain unverified. Those gates prevent calling this feature production-ready solely from passing offline tests.
