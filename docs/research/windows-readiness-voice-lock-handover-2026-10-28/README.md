# NEXUS Windows readiness and Voice Lock: detailed work handover

Prepared: **2026-10-28, Asia/Calcutta**. Main work covered: **2026-10-24–26**, with later project context identified separately. This document set consolidates the research, analysis, plans, implementation edits, tests, failed experiments and remaining acceptance gates from this conversation.

**This is a documentation consolidation, not a new release assessment.** No application build, runtime operation, recording, installed-profile inspection or functional test rerun was performed to prepare it. Historical test counts retain their original source/build attribution.

## Reading order

| Document | Contents |
|---|---|
| [01 — Research, decisions and implementation plan](01-research-decisions-and-plan.md) | Initial audit, statistical interpretation, Architect/MCP limitations, candidate-model research, decision rationale and P1–P6 acceptance contracts |
| [02 — Implementation and file inventory](02-implementation-and-file-inventory.md) | Detailed before/after behavior, native/frontend/tooling edits, integration boundaries, file ownership caveats and current file/hash inventory |
| [03 — Verification, failures and release history](03-verification-failures-and-release-history.md) | Source/build identities, test progression, live failures, one-clip diagnosis, mutation coverage, GLM delegation outcomes and known evidence limitations |
| [04 — Remaining work and maintenance](04-pending-work-and-maintenance.md) | Open gates, exact-release handoff, operational restrictions, privacy boundaries, review checklist and documentation maintenance rules |

## Outcome at the end of this work

The active feature remains **Speaker verification / owner check**, provisionally **41% / 2.05 stars out of 5**. Its production-readiness acceptance has not been achieved. No installer or later feature was promoted by this work.

Implemented source improvements include:

1. Shared-check repairs and isolated exact-source assessment, preserving concurrent edits.
2. Command Hub enrollment, sealed profile storage and fail-closed Voice Lock gates.
3. Whole-session enrollment microphone ownership, three wake-phrase prompts and cancellation recovery.
4. Unique native capture sessions, content-free lifecycle audit and reconciliation tooling.
5. Effective native lock state separated from draft UI settings, verified Save and cross-window synchronization.
6. Accessible Save acknowledgement and async Windows settings-window creation.
7. Neutral decoding after a reproduced owner-prompt duplication failure.
8. Native local-command execution through the existing mapper/executor after authorization.
9. Offline paired-model score analysis and enrollment-isolated embedding scoring.
10. Honest legacy CSV reporting and richer declared-trial qualification/action-fidelity reporting.
11. Regression and mutation checks, dated evidence records, work-card updates and explicit phase closeout gates.

These improvements do not establish unfamiliar-speaker discrimination, replay resistance, a general owner acceptance rate, successful repaired live actions or current release readiness.

## Scope and attribution

This handover covers [change 155](../../changes/155-windows-readiness-baseline-and-voice-lock.md), [156](../../changes/156-voice-enrollment-wake-isolation.md), [163 capture accounting](../../changes/163-voice-capture-accounting.md), [164 native-state UI](../../changes/164-voice-lock-native-state-ui.md), [165 live repairs](../../changes/165-voice-lock-live-verification-fixes.md), [167 neutral decoding/native execution](../../changes/167-neutral-decoding-and-native-local-execution.md), [168](../../changes/168-offline-speaker-verifier-benchmark.md), [169](../../changes/169-enrollment-isolated-embedding-scores.md) and [170](../../changes/170-voice-trial-accounting-and-phase-closeout.md).

Numbered records are not globally unique. For example, `163-focus-mode-lowers-other-apps.md` and `163-voice-capture-accounting.md` are separate changes. Likewise, deploy-watcher change167 is separate from the neutral-decoding change167. Use full filenames when identifying a record.

Focus Mode, owner-less GitHub commands, notification/deployment watchers, later PR-list/Hand Mode fixes and the 2026-10-27 final source pass were concurrent or later project work. They are relevant context, but this handover does not claim their implementation or later suite results as this chat's work. See [the separate owner live checklist](../../testing/live-checklist-2026-10-27.md).

## Queue and rubric

The original agreed Windows programme contained 65 in-scope features. The files inspected for this handover contain 72 total scorecard entries; the current generator excludes four out-of-target entries, leaving 68 in the Windows queue. Later additions include Focus Mode, owner-less GitHub voice commands and the notification brain. The old dated queue prose still describes the 65-feature snapshot; preserve that history and use the [current generated card index](../../testing/work-cards/README.md) for the expanded order.

| Axis | Weight | Speaker score recorded | Minimum for acceptance |
|---|---:|---:|---:|
| B — built/integrated behavior |20%|75|90|
| T — automated test evidence |20%|55|70|
| L — real-device/live proof |30%|10|70|
| S — safety/failure handling |15%|50|80|
| I — independent/comparative evidence |15%|30|60|

Weighted raw score: `0.20×75 + 0.20×55 + 0.30×10 + 0.15×50 + 0.15×30 = 41%`. Stars: `41 / 20 = 2.05/5`. The axes are rubric estimates, not biometric accuracy measurements.

Acceptance requires raw score≥ 90% /4.5 stars **and** every axis minimum, safety regressions, exact-source shared checks, fresh verified release provenance, real-device evidence and no known failure contradicting the claim. Administrator control, the admin brain, macOS/Linux parity and optional partial captions stay outside this Windows queue.

## Evidence distinctions used throughout

| Evidence | What it supports | What it does not support |
|---|---|---|
| Source review | A defect, wiring choice or intended policy in inspected code | Actual device behavior |
| Unit/regression tests | Specified behavior under the tested fixtures | Broad voice accuracy or Windows UI success |
| Mutation failure | A tested safeguard is necessary for a regression to pass | Every possible bypass being impossible |
| Frozen suite/build identity | Particular checks and executable inputs correspond to a recorded source | Other source revisions or later edits |
| Native state readback | A setting/profile status at the stated time | Current state or identity accuracy |
| Owner interaction report | What the owner observed in that interaction | Exact-source attribution when a build was switched |
| One-clip controlled replay | A reproducible prompt effect for that PCM sample | Held-out accuracy or proof of acoustic echo |
| Offline embedding/score report | Computation on declared, validated experimental metadata | Authentic corpus lineage, real model inference or action success |
| Independent observed action | The requested action actually happened in a specific trial | General owner/impostor rates without a qualified corpus |

## Operational boundary

The owner explicitly took control of application builds and runtime on 2026-10-26. Do not automatically start, stop, restart or rebuild NEXUS or alter runtime settings. Historical process IDs and the last observed Voice Lock state are not current-state facts. Earlier authorized launches are retained as history and are not instructions to repeat them.

The last repaired frozen release recorded by this work was `d846dd346c470b60`, source `f0988d95afbfac06176c923c2227e7e79e841013`. A later source-only checkpoint used dirty-tree fingerprint `6def3963dc8380f45294c0c1ab9b373727090def2f00eb842278ae5da78077f4`; that checkpoint was not a new release. Both are detailed in document03.

## Canonical records

- [Ordered P1–P6 implementation plan](../../features/152-voice-lock-evidence-and-reliability-plan.md).
- [Speaker work card](../../testing/speaker-verification-work-card.md) and [phase closeout](../../testing/voice-lock-phase-closeout.md).
- [Scorecard data](../../testing/scorecard-data.json), [queue snapshot](../../testing/windows1-production-queue.md), [generated work cards](../../testing/work-cards/README.md).
- [Live-proof ledger](../../testing/live-proof-ledger.md).
- [Last frozen decoder/execution assessment](../../testing/runs/windows1-20261026-neutral-local-assessment.json).
- [Source-only phase checkpoint](../../testing/runs/windows1-20261026-phase-closeout-assessment.json).

Use these dated records for their own snapshots. This handover explains their relationship; it does not overwrite failed results, renew old consent, or manufacture a later acceptance result.
