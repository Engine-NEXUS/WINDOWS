# Windows native desktop-control reliability work card

Date: 2026-11-06. Status: **source implemented; offline verification; device qualification pending**.

This card implements the approved native-first programme. It does not advance the production-readiness queue or change any existing score. Speaker verification retains its outstanding gates. No NEXUS application build, launch, shutdown, settings change or real input was performed for this work.

## Observed gaps and fixes

| Failure layer | Baseline gap | Source change | Evidence still needed |
|---|---|---|---|
| Speech | Real speech can be rejected or transcribed incorrectly | Existing Voice Lock and speech gates preserved; separate voice qualification split | Owner voice pass; no new speech accuracy claim |
| Interpretation | Existing clicks have no role/container context | Backward-compatible TargetQuery and bounded choice grammar | Real phrase routing through the shipping pipeline |
| Target resolution | Exact-name misses and duplicate names | Mnemonic normalization, exact contextual matching, reviewed aliases, validated learned identity | Edge, Explorer, Notepad and Settings UIA providers |
| Dispatch | Focus changes, stale callbacks and slow lookup | Process creation/window/control binding, single-use choice, cancellation checks before dispatch, one send | Moving/covered controls, takeover, permission rejection |
| Verification | Any title/window change could count as success | Action-specific state probes; unsupported observations stay Unconfirmed | Exact-release intended-effect checks |

## Phase status

- **A — implemented/offline tested:** two regressions reproduced the old router's false success after unrelated foreground change or disappearance. Semantic-test expectations were updated explicitly; old title-only expectations were not retained as proof.
- **B — implemented/offline tested:** contextual resolver; 2–5 ephemeral numbered choices; 30-second expiry; profile/window/process/observation binding; sidebar-only IPC; authorized voice selection; stale/replayed choices rejected. More than five matches asks for refinement. Learned records remain readable and build invalidation remains enforced.
- **C — implemented/offline tested:** semantic keyboard and native control probes, exact field insertion verification, one dispatch, sequential stop on unconfirmed, bounded blocking-worker waits. Native adapters and extracted orchestration functions are compiled in an isolated harness. This is not a full Tauri application compile.
- **D — qualification infrastructure implemented; device runs not performed:** fixed matrix, local fixture, strict result accounting, CI workflow and runbook. Core **0/300**, held-out **0/20**, owner voice **0/30**.

## Acceptance

1. Core supported matrix: at least 297 verified completions out of all 300 declared runs. Missing apps/capabilities remain failures or blockers in that denominator.
2. Zero observed wrong-target actions, unauthorized actions or dispatches after acknowledged cancellation. Record the number of trials and uncertainty; zero observations is not proof of zero risk.
3. Claimed desktop success must have its specific effect evidence. Expected refusal/cancellation in held-out safety scenarios is assessed separately from successful executable actions.
4. Record all 20 held-out cases and the separate 30-case owner voice pass. Keep speech, interpretation, resolution, dispatch and effect failures separate.
5. Full exact-source native library/integration checks, dead-code guard, fresh release provenance, owner device approval and existing weighted score/axis floors remain mandatory.

## Evidence and next owner action

- [Engineering record](../../changes/2026-11-06-desktop-control-reliability.md)
- [Runbook](runbook.md), [fixed cases](matrix.json), [empty qualification result](qualification-status.json)
- [Offline assessment and per-check logs](../runs/desktop-control-offline-20261106/assessment.json)
- [Mutation results](../runs/desktop-control-mutations-20261106.json)

The owner first runs full application checks and makes a fresh provenance-verified build. Then follow the disposable-fixture runbook. Failed live checks remain failures until repaired and repeated against the repaired build; earlier releases do not qualify later source.

## Final offline result

Both unchanged-source passes passed: **144 Rust helper tests, 851 frontend tests, 145 Worker tests, 9 trial-accounting tests and 8 wiring contracts per pass**, plus frontend types and IPC/turn-end/hygiene/color/opacity/contrast/whitespace guards. **12/12 safety mutations** were killed by assertions. The source-manifest SHA-256 is `bf825a2782496565399dab2ce6d7acc66197a1e710c5bdfd9c58e4e6ce45cd57`. Read-only Rust syntax and Python byte-compilation checks also passed. Documentation links and the 8/8/8/6 matrix counts were checked.

These are offline results. Complete application native/integration checks, dead-code, release provenance and all real-device trials remain not run. No production-readiness score was raised.
