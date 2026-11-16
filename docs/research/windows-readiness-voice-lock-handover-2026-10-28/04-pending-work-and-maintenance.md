# 04 — Remaining work, handoff and maintenance

[Handover index](README.md) · [Research/plan](01-research-decisions-and-plan.md) · [Implementation](02-implementation-and-file-inventory.md) · [Evidence](03-verification-failures-and-release-history.md)

Prepared 2026-10-28. This is a backlog and evidence handoff, not permission to operate NEXUS, alter installed profiles or collect audio.

## 1. Restrictions that remain in force

- The owner controls all application starts, stops, restarts and builds manually.
- Do not automatically change native settings, including Voice Lock or diagnostics.
- The one-clip recording consent was completed; it is not ongoing consent for more recording, export or replay.
- Do not assume the historical PID, current lock state, profile validity or sealed clip still exists.
- Preserve unrelated/concurrent work and the primary Git index. Do not rewrite failed evidence or run destructive profile recovery against installed user data.
- Keep default-off rollout until acceptance is supported. A source policy does not imply the current installed setting is Off.
- Keyboard/direct UI recovery must remain available; recovery must not turn an unverified voice into owner authorization.

## 2. Work that remains before each phase can close

| Phase | Remaining task | Required evidence / stop condition |
|---|---|---|
| P1 | Ten physical starts, including failed/cancelled/no-speech paths, on one assessed release | Requested/started/terminal counts reconcile; mismatches remain failures; dispatch and actual action tracked separately |
| P2 | Exact-candidate settings/fault check and real profile recovery | Effective/native/saved agreement; no false Saved; no silent unlock; recovery usable; separate legacy prototype not assumed tested |
| P3 | Private consented corpus, exact candidate artifacts and extraction adapter | Verified artifacts/notices/export parity, disjoint lineage, paired trials, resource measurements, owner/impostor/replay results; no acceptance tuning |
| P4 | Fresh repaired owner command and held-out fidelity evaluation | Correct intended action observed; capture/transcription/intent/target/result correlated; silence/ambient/replay/echo hypotheses assessed without weakening guards |
| P5 | Qualified identity/action/recovery trials |≥ 95% owner acceptance;≥ 300 qualified negatives with zero observed locked non-owner executable actions; separate unfamiliar live, owner replay, playback and synthetic evidence; recovery/handoff checks |
| P6 | Complete exact-source release assessment |Full shared checks, mutation protections, fresh owner-built executable provenance, device proof, second review, raw≥ 90 and all axis minima |

The source tools do not close the actual corpus/model comparison. Declared numeric targets do not close P4 action failures, profile recovery, release provenance or P6.

## 3. Current-source maintenance warning

A read-only check while preparing this handover found that current `stt_groq.rs` exposes `decoder_bias_for_turn`, while the saved `neutral_local_mutation_check.py` still extracts a function beginning with `pub(crate) fn decoder_bias_for`. That is a potential harness/API drift issue to review before the next mutation run. It was not executed or repaired during this documentation-only request.

This observation does not say the native local adapter disappeared: current source inspection still found `dispatch_native_local`, its executor call and adapter tests. It does mean a historical7/7 mutation result must not be labelled a current pass merely because the old record exists.

When updating the harness, review semantics and actual tested functions rather than simply weakening anchors until it exits successfully. Require a healthy baseline, intended assertion failures and separation of source wiring checks from runtime function tests. Preserve the old result with its original source identity.

Current source also evolved around optional decoder hint policies after the frozen neutral-decoder assessment. Re-evaluate those choices against the relevant current source and independent evidence; do not silently treat the old API or default as the whole present system.

## 4. Exact-source evidence packet

For the next acceptance assessment, retain:

1. Git revision or explicitly retained source snapshot, plus complete source-input fingerprint and dirty-tree status.
2. Frontend bundle and executable hashes, build identity and build completion record.
3. Every shared-check output with command, working directory, date, source identity and passed/failed/ignored counts.
4. Mutation outcomes with healthy-baseline proof and exact source/file identities.
5. Trial metadata, condition/grouping design, native lock readback and capture/run/session correlation.
6. Independent observed action success/failure and stage-health observations.
7. All exclusions, failed tests, unmatched starts, ambiguous mappings and unresolved faults.
8. Reviewer reconciliation of code, logs, UI behavior, documentation, scorecard and public claims.

A post-test source change requires re-establishing the affected evidence. Do not point at a historical executable merely because it has the same product name. Check manifest linkage before replacing an instance; under the standing restriction, the owner performs runtime/build steps.

## 5. Manual interaction sequence after the owner supplies a candidate

This is the ordering of evidence collection, not an instruction for the assistant to start the app or change settings automatically.

1. Identify the exact owner-started candidate and recorded source/build. Confirm effective native protection state through the agreed observation process before classifying a locked trial.
2. Test the repaired single intended harmless action with exactly one requested capture; keep the action result independent from owner identity.
3. Repeat enrollment isolation for all three phrase variants and completion/cancellation recovery on that candidate. Do not count calibration/enrollment as acceptance.
4. Run an ordered owner batch across distance/noise; reconcile all physical starts, not just the successful subset.
5. Run separately labelled unfamiliar live, owner replay, other-speaker and synthetic playback cases. Include owner wake followed by non-owner command and non-owner confirmation answers.
6. Exercise restart, model/capture failure, missing/corrupt/deleted-profile and keyboard recovery safely, preserving the installed owner data. Simulated status faults are not the same as real recovery proof.
7. Observe stage heartbeat/blackout behavior throughout. A UI hang or wrong action remains a failed result even if identity verified.
8. Record failures and successes, then perform the second review. Keep the queue on speaker verification until the whole acceptance contract passes.

## 6. Tool reference and safe boundaries

The commands below document existing analysis interfaces. They are not executed by this handover and do not authorize microphone capture, model downloads, threshold installation or app control.

| Tool | Input | Output / qualification |
|---|---|---|
| `capture_audit_summary.py` |Explicit saved native log plus run ID/expected starts|Lifecycle accounting; no independently observed action success|
| `speaker_benchmark.py` |Strict paired scalar-score JSON|Calibration-only threshold and held-out metrics; declared artifact/resource metadata|
| `speaker_embedding_scores.py` |Explicit private offline vector export|Enrollment-only max-cosine scores and vector-free provenance; no extraction/native parity|
| `speaker_trial_summary.py` |Historical five-column CSV|Descriptive statistics; never qualifies numeric readiness|
| `voice_trial_evidence.py` |Reviewed declared trial JSON|Qualified/excluded counts, safety failures, identity/fidelity separation; readiness undetermined|
| Mutation scripts |Temporary copies and synthetic fixtures|Necessary safeguards in tested software; no real voice-security acceptance|

Do not put audio, vectors, keys, contact/participant names or installed-profile copies in public documentation. Metadata syntax cannot prove that an anonymized label, sample lineage or claimed observation is authentic.

## 7. Documentation maintenance rules

- Keep this handover dated. Update it with an explicit addendum rather than changing an old pass into a new-source claim.
- Cite full filenames for duplicate change numbers.
- Link tests to their exact source/build; do not sum suites across revisions.
- Distinguish “not run,” “failed,” “excluded,” “unavailable,” “passed simulation” and “passed device test.”
- Keep calibration separate from acceptance and negative classes separate from each other.
- Do not promote a delivered transcript, dispatch receipt, native setting or synthetic test to observed action/security proof.
- Treat current scorecard/generated work cards as the expanding queue source. The older65-feature prose is a historical snapshot; the current inspected queue has68 in-scope entries.
- Preserve later project work separately: the 2026-10-27 live checklist and later suite notes do not overwrite this chat's earlier source identities.
- Never mark a feature ready solely because every tool output says pass. The minimum axes, raw score, complete shared checks, fresh provenance, real evidence and no contradictory failure all remain necessary.

## 8. Completion status of this documentation request

The requested research/analysis/plan/edit/test history is consolidated in this five-file set, cross-linked from the documentation indexes. Validation of these Markdown files is recorded after creation: local links, inventory paths, whitespace, evidence-table arithmetic and current metadata consistency. No functional suite, application build, release launch, profile access or voice trial is implied by that documentation validation.

The engineering programme remains **in progress at 41% /2.05 stars**, with no queue advancement. Its remaining gates are explicit rather than hidden behind the size of the implemented source or the number of tests.

### Documentation QA performed on 2026-10-28

- Five Markdown documents cover 52 source/integration file entries and the associated research, implementation and evidence records.
- 137 local references within the handover resolved at validation time; the four index/changelog entry points were checked separately.
- Current scorecard metadata was checked: 72 total entries, 68 in scope; the recorded speaker axes calculate to 41% /2.05 stars.
- Mutation accounting was checked: 15+11+7+6+1 native/UI, 18 offline benchmark/embedding and 21 trial-accounting safeguards total 79.
- The tool files covered by the latest two per-file hash assessments matched their recorded digests at the documentation read. Native/frontend current files were not thereby certified.
- Markdown link targets, source inventory paths and trailing whitespace were checked. No application build, functional suite, native state readback, profile inspection or recording was run for this request.
