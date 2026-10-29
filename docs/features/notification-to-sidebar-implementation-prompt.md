# Reusable prompt: NEXUS notification-to-sidebar

Date: **2026-10-28**. This file is a future work brief, not a command executed by the documentation task. Copy the prompt only when authorizing the specified implementation scope.

## Full implementation prompt

```text
Work in D:\ULTRON on the proposed NEXUS notification-to-sidebar interaction.

Read the current owner instructions and these documents first:
1. docs/research/notification-to-sidebar/README.md
2. docs/features/notification-to-sidebar-implementation-plan.md
3. docs/features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md
4. docs/testing/notification-brain-testing-and-research.md

Goal: a compact NEXUS alert near the top of the screen opens the exact
GitHub PR/build/repository, Gmail thread or deployment context in the
existing sidebar after an explicit click/keyboard activation. Windows
first. Correct routing precedes animation. Reuse existing policy/window
infrastructure and preserve the active/pinned sidebar view.

Scope of this invocation: N0 and N1 ONLY.
- Inspect the actual event producers, notification brain, Sentinel store,
  unified sidebar/pending-data path, geometry and dependency versions.
- Write a short source-backed work card and list the exact files to edit.
- Define a versioned canonical event and typed target/resolver boundary.
- Preserve full provider/account/owner/repository/PR/run/thread/deployment
  identity. Human-readable project names are display fields, not routing IDs.
- Use mock producers and pure resolver/state tests. Do not access live
  Gmail/GitHub/deployment services or existing installed credentials/data.
- Review old entity-key migration, quiet first-observation baseline,
  duplicate grouping and failure/recovery semantics before changing keys.
- No compact card, animation, new window, native notification adapter or
  provider action is required in N1. Leave those later phases explicitly open.

Owner constraints:
- Do not start, stop, restart or rebuild NEXUS or change runtime settings.
  The owner handles builds/runtime manually. Do not deploy or publish.
- Preserve all unrelated dirty work and the primary Git index. Do not reset,
  clean, discard, commit or promote changes without the owner's instruction.
- Do not modify the active Voice Lock acceptance programme or declare this
  new interaction production ready. A plan is not queue advancement.
- No microphone/recording/profile access, real mail/PR export, secrets in
  logs or automatic merge/reply/rerun/archive/mark-read behavior.

Coding arrangement requested by the owner:
Use OpenCode model opencode/glm-5.3-flash-free for scoped code edits when
available. You plan, inspect and test its output. Give it a narrow file
allowlist, relevant source copies and a short applicable-instructions brief
in an isolated editor directory if the main instruction file overwhelms it.
Review its diff before copying anything into the project. A successful
process exit without output files is not implementation success.
If the editor stalls, produces no files or fails the scoped task, report
that concretely and use the owner's authorized supervisor fallback to make
the narrow repairs. Do not silently substitute a paid model or fabricate
GLM/vendor review credit. Do not stop unrelated tasks or the NEXUS process.

Mandatory N1 invariants:
1. Two accounts or owners with the same repository name do not collide.
2. A click resolves an opaque event ID through validated local state;
   notification text, provider HTML/logs and arbitrary activation URLs are
   not commands or trusted routing instructions.
3. Duplicate observations do not create repeated event/channel delivery.
   A recovery updates the correct entity and keeps original event context.
4. Expired/deleted/unavailable/disconnected targets return an explicit
   safe error, not another account, a default repo or the newest unrelated PR.
5. Event/entity/delivery/read states remain distinct. Viewing NEXUS content
   does not mutate provider state or grant owner/admin authorization.
6. Persistence/schema migration is bounded and preserves quiet baseline;
   it must not replay all historical failures as fresh alerts.

Testing/review workflow:
- Add meaningful tests that fail on the old/missing behavior before the fix.
- Implement the smallest compatible change using existing project patterns.
- Test typed schema/unknown inputs, same-name repos/accounts, duplicate
  failure/recovery, missing/stale/disconnected targets and migration.
- Mutation-check the important identity/validation/dedup safeguards.
- Run focused tests and relevant no-application-build checks. Native
  compile/device/release gates that conflict with the owner restriction
  remain pending; do not bypass that restriction to claim full coverage.
- Re-review code, tests, source identity, documentation and retained failures.
- Update the work card/change record and relevant docs with exactly what
  passed, failed, was excluded and was not run. Avoid unrelated rewrites.

Technical boundaries to retain for later phases:
- System notifications can activate an app; they are not a shared morphing
  surface with the sidebar. Tauri's documented Actions API is mobile-only;
  desktop activation requires an actual verified adapter/integration.
- Separate WebViews need a correlated ready/timeout/cancellation handoff;
  do not promise a shared-element CSS morph across different renderers.
- Use work-area origin, scale factor and safe regions, including negative
  monitor coordinates. Do not build placement from zero-based screen size.
- Arrival must not steal focus or replace pinned content. Custom overlays
  need explicit quiet/meeting/fullscreen/lock/privacy and hit-region policy.
- Avoid permanent extra WebViews, per-frame IPC and model calls for every
  repeated passive event. Measure before choosing animation/resource budgets.

Final report: files edited, actual GLM/fallback contribution, tests and
mutations with results, source identity, unresolved failures, and the next
N2 work card. Do not mark N2-N6 implemented or device-tested in this invocation.
```

## Short prompt for later continuation

```text
Continue the NEXUS notification-to-sidebar plan from the first incomplete
authorized phase. Read its research and implementation plan under docs.
Use the existing GLM editing/supervisor-review arrangement and preserve
owner-only build/runtime control. Plan the phase, add failing regressions,
make narrow edits, test/mutation-check, review and update evidence. Keep
wrong-target/focus/privacy/provider-action protections intact. Report actual
results and every pending release/device gate; do not advance phases that
depend on a failed or unavailable acceptance check.
```

Use the full prompt for the first implementation assignment. The short prompt does not expand previously authorized scope or permit runtime/build/provider operations.
