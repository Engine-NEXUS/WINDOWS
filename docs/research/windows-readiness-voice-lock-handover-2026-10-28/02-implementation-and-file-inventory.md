# 02 — Implementation edits and file inventory

[Handover index](README.md) · [Research/plan](01-research-decisions-and-plan.md) · [Verification](03-verification-failures-and-release-history.md) · [Remaining work](04-pending-work-and-maintenance.md)

This records the scope of changes and investigated integration points. Many files already contained unrelated work and evolved later. A row does not claim authorship of an entire file, a clean current Git diff, or present-day passing tests. The numbered change records remain the authoritative explanation of the original edits.

## 1. Shared baseline and repository gates — change155

### Before

The primary checkout was heavily dirty and changing concurrently. IPC checks mixed genuine command/event inconsistencies with comment-parsing false positives. Seven NLU training rows overlapped locked evaluation data. Some fixtures/assertions referred to earlier settings/data behavior. Full feature proof could not be attributed safely to a moving source tree.

### Edits

- Repaired CRLF/comment handling in `generate_handler!` inspection.
- Corrected a CodeRabbit speech-command mismatch and dormant/unregistered command marker handling.
- Connected Agent Watch event listeners and wake-sound preview paths, removed obsolete wrappers/emissions and reduced known one-sided IPC baselines.
- Repaired a settings fixture and two stale Python assertions.
- Quarantined the seven leaking training rows and regenerated the split lock only after validation. Locked evaluation rows were not changed to make the checks pass.
- Hardened source fingerprinting to include shipped OWW assets/configuration and ignore generated Python caches.
- Preserved the primary index/branch/concurrent edits and assessed a retained source snapshot in the existing managed readiness checkout.

### Result and limit

The frozen baseline passed its shared checks and produced a provenance-verified release. Its live enrollment interaction then failed. Therefore automated baseline success was retained without promoting the feature. See [change155](../../changes/155-windows-readiness-baseline-and-voice-lock.md) and [baseline assessment](../../testing/runs/windows1-20261025-assessment.md).

## 2. Enrollment/profile safety — changes155–156

### User-visible changes

Command Hub gained enrollment/status/deletion access using existing IPC. Enrollment requires ten clips: five profile samples and five withheld owner calibration clips. Both halves prompt Nexus, Hey Nexus and OK Nexus. A valid profile is required before enabling the lock.

### Storage and failure handling

- Profile data is authenticated/encrypted with a persisted profile-specific Windows key.
- Conversation-memory key rotation cannot strand the profile.
- Valid legacy formats can migrate after successful authentication.
- Missing/corrupt data, failed authentication/model output, invalid vectors and uncertain captured ownership do not silently authorize a locked action.
- Corrupt settings preserve protection; unrelated full settings saves preserve the Voice Lock field. Only an explicit speaker-verification patch changes it.
- Corrupt-profile replacement requires valid re-enrollment; OS deletion failure is reported rather than called success.
- Keyboard and direct settings recovery remain reachable. Ctrl+Space starts capture but does not bypass verification.

### Matching enrollment to the live pipeline

Enrollment previously skipped required normalization/reshape. It now shares live mel preprocessing, resets between clips, selects a peak-energy audio window and checks feature shape/finite values. The real-model synthetic-tone regression checks inference execution and clip isolation; it does not measure speaker recognition.

### Whole-session microphone isolation

Browser recording now awaits native microphone pause before countdown/recording. The session keeps that baton for all ten clips. Browser tracks stop before native resume. Native callbacks, capture starts, queued wake candidates and delayed wake dispatch honor the pause barrier. Resume resets state/grace behavior.

Cancellation during pending browser permission was an additional reproduced gap. Navigation now releases the native baton promptly; a stream granted later is immediately stopped and cannot begin a recording after cancellation. Device/cleanup/resume errors reach the UI.

## 3. P1 capture accounting — change163

### Identity and lifecycle

Each request receives a process-run ID and monotonic attempt ID. Each accepted capture receives a distinct native session. Cancellation generation and diagnostic request identity are not conflated.

The audit attempt travels with the audio packet. Start/callback/abort and final stale checking/proof registration/delivery share appropriate barriers. A later capture cannot inherit an older packet's result or proof. Duplicate requests preserve the active capture but receive their own terminal outcome.

Terminal outcomes cover blocked microphone ownership, duplicates, cancellation, supersession, missing receiver, stale work, empty/phantom audio, owner holds, STT errors/filtering, delivery failure/success and abandonment. A crash remains unfinished evidence instead of receiving an invented success.

### Privacy and provenance

Canonical lifecycle records omit audio, transcripts, profile vectors, speaker scores, secrets, contacts and file paths. Executable/manifest linkage is established before capture. That linkage does not independently prove the entire source tree.

Routing receipt is a separate `[CAPTURE-DISPATCH]` event. The log reader checks stages, duplicate terminals, reused sessions, unexpected dispatch, marker injection inside quoted text and expected start counts. `--expected-starts 10` prevents a seven-capture subset from passing as a ten-start batch.

## 4. P2 effective state and verified Save — changes164–165

The UI now represents three distinct concepts:

| Concept | Meaning |
|---|---|
| Draft toggle | The user's unsaved requested value |
| Effective native state | The last completed native routing/readback value: On, Off or Unknown |
| Verified Save acknowledgement | Native/saved state agrees with the submitted change after Save |

Both settings surfaces refresh on relevant mount/focus/visibility/profile events. Saving patches only dirty fields, preserves edits made during a slow Save, rejects overlapping saves and prevents stale reads from overwriting newer state. Untouched drafts track external native changes while explicit edits remain dirty.

Missing/unreadable profiles refuse enable; profile loss during Save invalidates confirmation. Fake acknowledgement, refused/delayed writes and native/settings disagreement do not become a verified protected state.

The first live candidate exposed Saved text through accessibility before Save because opacity alone hid it visually. The repaired status component removes unverified text from the accessibility tree and invalidates old acknowledgements on refresh. Switches have accessible names/state.

The synchronous native settings opener hung Windows window creation and coincided with a stage blackout. The command and tray dispatch became asynchronous. The Future contract regression proves the function shape; it does not create a real WebView. Replacement device checks subsequently passed for the shipping Hub/standalone surface. The separate legacy prototype and real profile recovery were not thereby proven.

## 5. P4 neutral decoding and native local execution — change167

### Decoder change

The same retained PCM returned one command under neutral decoding and duplicated commands under the owner vocabulary. Captured owner decoding therefore switched to a neutral policy; the old vocabulary remained available for controlled comparison but disabled by the recorded source policy. The frontend Groq normal/verbose path used the neutral default too.

This change did not remove repetition, hallucination, correction or authorization guards. Later current source includes separately named hint-selection APIs; do not assume every current optional hint path is identical to the frozen change167 API.

### Execution gap

Native capture returned directly to Rust, skipping the frontend executor. The ordinary native LocalCommand branch still assumed the frontend would execute it and emitted Done. The repair invokes the existing local-intent mapper and executor after existing authorization/action gates, once for an eligible native session.

Sessionless legacy turns are skipped by the adapter to prevent duplicate execution. Pre-dispatch cancellation prevents execution; cancellation after completion suppresses a late reply. Real refusal/result/error is propagated rather than fabricating success. Unsupported mappings retain their existing behavior.

Tests inject an executor closure and run no OS actions. They check one call, target/result propagation, error/refusal propagation and no call for legacy/cancelled/unsupported input. Two mutation checks inspect call-path wiring; the remaining decoder/dispatch mutations run isolated real-function tests.

## 6. P3 scalar benchmark — change168

`speaker_benchmark.py` accepts a strict versioned JSON score experiment. It requires declared source/model hashes, paired model trial sets/metadata, valid identity/partition labels and finite measurements. Unknown fields and malformed identifiers are rejected.

Sample/group reuse across calibration/acceptance and unfamiliar identity leakage are refused. Owner and live non-owner classes are required in both measured partitions. Thresholds use calibration only; held-out acceptance never changes them.

Reports separate owner, live non-owner, owner replay, other-speaker playback and synthetic playback, including per-condition metrics, uncertainty, group/sample/speaker counts and declared resource measurements. A complete canonical-input fingerprint identifies the data. No model runs, threshold is installed, action success inferred or automatic winner chosen.

## 7. P3 enrollment-isolated embedding scoring — change169

`speaker_embedding_scores.py` accepts explicitly provided offline vectors and computes `max_enrollment_cosine_v1` scores. It requires at least three distinct owner enrollment samples and one consistent owner identity, with paired trial metadata across models and no partition/group/sample leakage.

Vectors are dimension-checked, finite, non-boolean and nonzero. Scale-first normalization prevents finite magnitude overflow/underflow. Only enrollment vectors form the template; calibration/acceptance cannot adapt it. Enrollment rows are omitted from measured trial denominators.

The report adds preprocessing hashes, dimensions, enrollment counts and the complete original-export fingerprint without publishing embeddings. The Python rule is conceptually aligned with maximum-cosine verification, not proof of native f32/audio-inference parity.

A review also found that an ordinary median could overflow when adding two large finite latency values. A failing regression led to a stable nonnegative midpoint calculation. This was a numerical report repair, not a runtime speech change.

## 8. P4/P5 trial accounting — change170

### Legacy CSV

The five-column CSV lacks lock/capture/source qualification. It now always returns `numeric_targets_met=false`, identifies legacy unqualified evidence, labels mixed negative statistics and provides separate speaker/condition views. Ragged rows, padded identifiers/headers/conditions and comma-delimited blank records fail cleanly. Truly empty lines may remain empty lines; they do not manufacture trials.

### Rich declared-metadata format

The separate JSON reporter requires source/build declarations, run/session attribution, speaker kind, recording lineage, partition, wake path, lock/readback and verifier reachability, valid-command status, observed action and nullable fidelity observations.

Only acceptance rows with lock enabled, an attributed capture, available reached verification and a valid executable command enter the numeric screen. Exclusion reasons remain counted. All locked rows still contribute negative/unverified action failures, including otherwise excluded rows. Lock-off controls remain distinct.

True requested-action success requires an observed action and measured correct intent/target. Verified owner action success/failure/unmeasured counts remain separate from biometric acceptance. Reports cannot silently call a failed app launch an owner rejection or call dispatch receipt success.

The numeric screen requires the agreed owner rate/sample count and a live unfamiliar observation; it is insufficient for population claims or release readiness. The report always retains `production_readiness=not_determined` and the independent-reconciliation warning.

## 9. Integration references versus edited files

The following existing boundaries were reused or investigated; their presence does not imply that their entire implementation was newly authored here:

- `command_center::to_local_intent` and `command_executor::execute_command`: reused mapping/execution machinery.
- `frontend/src/audio/recorder.ts`: native capture bypass explained the executor gap.
- `stt_diagnostics.rs`: existing sealed diagnostic storage/retention used under one-clip consent.
- Existing voice IPC command registration and recovery hotkeys.
- Main-checkout notification/GitHub/Focus modules: concurrent work preserved during isolated assessments.

## 10. File inventory and current-copy caveat

The inventory below is generated by reading files on 2026-10-28. Its “historical tool hash” column compares only explicitly recorded file digests from the dated Python assessments. A matching digest does not rerun tests, and missing per-file history does not imply a file was untested. Native/frontend files evolved after the frozen release; no current whole-project pass is inferred.

The exact tested snapshots, builds and failed attempts are catalogued in document03. This is an implementation map, not a replacement for Git history or a new source assessment.

| File | Change/reference scope | Responsibility | Historical tool hash comparison |
|---|---|---|---|
| [src-tauri/src/voice_profile.rs](../../../src-tauri/src/voice_profile.rs) | Modified shared native source | Sealed profile, validation, calibration and owner classification | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/wakeword_oww.rs](../../../src-tauri/src/wakeword_oww.rs) | Modified shared native source | Enrollment pause/capture barriers, unique sessions, packet audit and stale checks | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/capture_audit.rs](../../../src-tauri/src/capture_audit.rs) | Added native module | Content-free attempt lifecycle and executable/manifest linkage | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/commands.rs](../../../src-tauri/src/commands.rs) | Modified shared IPC/settings source | Enrollment/profile commands, explicit lock patch behavior, async settings opener | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs) | Modified shared integration source | Module/command registration and test integration | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/center.rs](../../../src-tauri/src/center.rs) | Modified/shared policy boundary | Ownership/action disposition and correction safety | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/orchestrator.rs](../../../src-tauri/src/orchestrator.rs) | Modified shared routing source | Locked authorization, correlated routing and native local execution adapter | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/stt_groq.rs](../../../src-tauri/src/stt_groq.rs) | Modified shared decoder source | Neutral owner command policy; current decoder API evolved later | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/stt.rs](../../../src-tauri/src/stt.rs) | Modified shared transcription source | Neutral frontend Groq defaults | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/tray.rs](../../../src-tauri/src/tray.rs) | Modified shared UI integration | Await async settings open without synchronous event-handler deadlock | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/command_center.rs](../../../src-tauri/src/command_center.rs) | Existing reused integration boundary | Local intent mapping reused; not a new action capability | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/command_executor.rs](../../../src-tauri/src/command_executor.rs) | Existing reused integration boundary | Actual command executor reused after authorization | No per-file comparison asserted; see exact-source records |
| [src-tauri/src/stt_diagnostics.rs](../../../src-tauri/src/stt_diagnostics.rs) | Existing diagnostic integration boundary | Sealed one-clip storage/retention; no present installed-data access | No per-file comparison asserted; see exact-source records |
| [frontend/src/setup/VoiceEnrollment.tsx](../../../frontend/src/setup/VoiceEnrollment.tsx) | Modified enrollment UI | Ten-clip enrollment/calibration, phrase prompts and error handling | No per-file comparison asserted; see exact-source records |
| [frontend/src/setup/enrollmentSession.ts](../../../frontend/src/setup/enrollmentSession.ts) | Added session helper | Awaited microphone baton and cancellation/late-stream cleanup | No per-file comparison asserted; see exact-source records |
| [frontend/src/setup/enrollmentSession.test.ts](../../../frontend/src/setup/enrollmentSession.test.ts) | Added lifecycle regressions | Ordering, all clips, errors, cancellation and pending permission | No per-file comparison asserted; see exact-source records |
| [frontend/src/settings/voiceLockState.ts](../../../frontend/src/settings/voiceLockState.ts) | Added settings state model | Effective/draft separation, verified Save, epochs and dirty preservation | No per-file comparison asserted; see exact-source records |
| [frontend/src/settings/voiceLockState.test.ts](../../../frontend/src/settings/voiceLockState.test.ts) | Added settings regressions | Slow/refused/overlapping saves, stale refresh, profile and native disagreement | No per-file comparison asserted; see exact-source records |
| [frontend/src/settings/VoiceLockStatus.tsx](../../../frontend/src/settings/VoiceLockStatus.tsx) | Added status UI | Effective native state and accessible verified acknowledgement | No per-file comparison asserted; see exact-source records |
| [frontend/src/settings/VoiceLockStatus.test.tsx](../../../frontend/src/settings/VoiceLockStatus.test.tsx) | Added rendered status regressions | Native/draft wording and inaccessible unconfirmed Saved | No per-file comparison asserted; see exact-source records |
| [frontend/src/settings/SettingsApp.tsx](../../../frontend/src/settings/SettingsApp.tsx) | Modified shared settings UI | Legacy settings integration and async draft/save handling | No per-file comparison asserted; see exact-source records |
| [frontend/src/settings-sidebar/SettingsSidebarApp.tsx](../../../frontend/src/settings-sidebar/SettingsSidebarApp.tsx) | Modified shared Command Hub | Audio enrollment and native lock state integration | No per-file comparison asserted; see exact-source records |
| [frontend/src/settings-sidebar/voiceEnrollmentHub.css](../../../frontend/src/settings-sidebar/voiceEnrollmentHub.css) | Added presentation integration | Enrollment surface styling in the Hub | No per-file comparison asserted; see exact-source records |
| [frontend/src/net/orchestrator.ts](../../../frontend/src/net/orchestrator.ts) | Modified shared orchestration source | Voice confirmation ownership guard and related turn handling | No per-file comparison asserted; see exact-source records |
| [frontend/src/net/orchestrator.test.ts](../../../frontend/src/net/orchestrator.test.ts) | Modified shared regression suite | Locked confirmation and voice-routing regressions | No per-file comparison asserted; see exact-source records |
| [frontend/src/audio/recorder.ts](../../../frontend/src/audio/recorder.ts) | Investigated existing capture boundary | Native turns skip frontend action execution; diagnosed integration assumption | No per-file comparison asserted; see exact-source records |
| [scripts/check-ipc-contract.mjs](../../../scripts/check-ipc-contract.mjs) | Modified repository guard | CRLF/comment parsing and genuine command/event mismatch checks | No per-file comparison asserted; see exact-source records |
| [scripts/baselines/ipc-unused-commands.txt](../../../scripts/baselines/ipc-unused-commands.txt) | Updated baseline | Reduced known unused IPC commands; later counts separately attributed | No per-file comparison asserted; see exact-source records |
| [scripts/baselines/events-unmatched.txt](../../../scripts/baselines/events-unmatched.txt) | Updated baseline | Known one-sided event baseline after cleanup | No per-file comparison asserted; see exact-source records |
| [scripts/build-provenance.mjs](../../../scripts/build-provenance.mjs) | Hardened existing provenance tool | Include shipped model/config inputs; ignore generated Python caches | No per-file comparison asserted; see exact-source records |
| [scripts/build-provenance.test.mjs](../../../scripts/build-provenance.test.mjs) | Updated provenance regression | Source/bundle/executable drift and input coverage | No per-file comparison asserted; see exact-source records |
| [server/nlu/dataset.json](../../../server/nlu/dataset.json) | Baseline data edit | Training leakage quarantine described in change155 | No per-file comparison asserted; see exact-source records |
| [server/nlu/data/split_lock.json](../../../server/nlu/data/split_lock.json) | Baseline lock update | Regenerated only after clean validation; evaluation set unchanged | No per-file comparison asserted; see exact-source records |
| [scripts/capture_audit_summary.py](../../../scripts/capture_audit_summary.py) | Added evidence reader | Strict lifecycle/run/session/dispatch reconciliation | No per-file comparison asserted; see exact-source records |
| [scripts/capture_audit_summary_test.py](../../../scripts/capture_audit_summary_test.py) | Added reader regressions | Incomplete batches, malformed/injected markers and inconsistent lifecycle | No per-file comparison asserted; see exact-source records |
| [scripts/capture_audit_mutation_check.py](../../../scripts/capture_audit_mutation_check.py) | Added native audit harness | Six isolated actual-module/generation safeguards | No per-file comparison asserted; see exact-source records |
| [scripts/speaker_mutation_check.py](../../../scripts/speaker_mutation_check.py) | Added/extended safety harness | Profile, settings, encryption, confirmation and enrollment guards | No per-file comparison asserted; see exact-source records |
| [scripts/voice_lock_ui_mutation_check.py](../../../scripts/voice_lock_ui_mutation_check.py) | Added UI safety harness | Eleven effective-state/accessibility safeguards in final recorded run | No per-file comparison asserted; see exact-source records |
| [scripts/settings_window_async_check.py](../../../scripts/settings_window_async_check.py) | Added async contract harness | Actual opener Future contract and synchronous-opener mutation | No per-file comparison asserted; see exact-source records |
| [scripts/neutral_local_mutation_check.py](../../../scripts/neutral_local_mutation_check.py) | Added decoder/dispatch harness | Historical seven mutations; current decoder anchor drift needs review | No per-file comparison asserted; see exact-source records |
| [scripts/speaker_trial_summary.py](../../../scripts/speaker_trial_summary.py) | Added then repaired legacy reporter | Historical CSV descriptions; never qualifying; GLM patch plus supervisor fixes | Matches latest recorded per-file tool hash |
| [scripts/speaker_trial_summary_test.py](../../../scripts/speaker_trial_summary_test.py) | Added/expanded CSV regressions | Twelve final recorded legacy tests | Matches latest recorded per-file tool hash |
| [scripts/speaker_benchmark.py](../../../scripts/speaker_benchmark.py) | Added offline scalar benchmark | Paired trials, split validation, threshold selection and stable resource summaries | Matches latest recorded per-file tool hash |
| [scripts/speaker_benchmark_test.py](../../../scripts/speaker_benchmark_test.py) | Added/expanded benchmark regressions | Nineteen final recorded scalar tests including median overflow | Matches latest recorded per-file tool hash |
| [scripts/speaker_embedding_scores.py](../../../scripts/speaker_embedding_scores.py) | Added offline embedding scorer | Enrollment-only max cosine and vector-free provenance | Matches latest recorded per-file tool hash |
| [scripts/speaker_embedding_scores_test.py](../../../scripts/speaker_embedding_scores_test.py) | Added scorer regressions | Fifteen final recorded embedding tests | Matches latest recorded per-file tool hash |
| [scripts/speaker_embedding_mutation_check.py](../../../scripts/speaker_embedding_mutation_check.py) | Added offline mutation runner | Eighteen benchmark/enrollment/scoring/privacy mutations | Matches latest recorded per-file tool hash |
| [scripts/voice_trial_evidence.py](../../../scripts/voice_trial_evidence.py) | Added richer declared-trial reporter | Qualified/excluded trials, all-locked safety failures and independent fidelity | Matches latest recorded per-file tool hash |
| [scripts/voice_trial_evidence_test.py](../../../scripts/voice_trial_evidence_test.py) | Added trial-accounting regressions | Eighteen final recorded richer-evidence tests | Matches latest recorded per-file tool hash |
| [scripts/voice_trial_mutation_check.py](../../../scripts/voice_trial_mutation_check.py) | Added trial mutation runner | Twenty-one temporary-copy evidence safeguards | Matches latest recorded per-file tool hash |
| [docs/testing/runs/voice-lock-ui-native-probe.mjs](../../../docs/testing/runs/voice-lock-ui-native-probe.mjs) | Added historical device helper | Native/UI readback and confirmed fault-test mechanics; do not auto-run mutation modes | No per-file comparison asserted; see exact-source records |
| [docs/testing/runs/voice-lock-one-clip-analysis.py](../../../docs/testing/runs/voice-lock-one-clip-analysis.py) | Added historical diagnostic helper | One consented in-memory replay comparison; not authorization to reuse audio | No per-file comparison asserted; see exact-source records |

### Current-copy digests for offline evidence tools

These SHA256 values identify files read for this documentation inventory. They are not a new test, signature or executable provenance check.

| File | SHA256 read on 2026-10-28 |
|---|---|
| [scripts/speaker_benchmark.py](../../../scripts/speaker_benchmark.py) | `78e791736a24f74f4a6f73709b2f1ed25ced43d71e34232a3463e8aad260827f` |
| [scripts/speaker_benchmark_test.py](../../../scripts/speaker_benchmark_test.py) | `35db9b93fbc3d9067bc3a5cb8f7cf317cf88f3e157b07bdaf8abcabef14dfe55` |
| [scripts/speaker_embedding_mutation_check.py](../../../scripts/speaker_embedding_mutation_check.py) | `ec3daff0b321c23c010e451232a7fb9eaba3089b70d92c52cba53bb0277ff9b6` |
| [scripts/speaker_embedding_scores.py](../../../scripts/speaker_embedding_scores.py) | `f16380544dceb1af5bfc78788a2f315c88f2aa7517ac5cd6b8d7ad2cf0506195` |
| [scripts/speaker_embedding_scores_test.py](../../../scripts/speaker_embedding_scores_test.py) | `feb7dc3cc19047cb044f4e9465453303237c5cc620fa8f77258d34492ddc0306` |
| [scripts/speaker_trial_summary.py](../../../scripts/speaker_trial_summary.py) | `48be4eb51817103dc121953c280724f7f2c96e8757e7f75d9c82fbcedc157b88` |
| [scripts/speaker_trial_summary_test.py](../../../scripts/speaker_trial_summary_test.py) | `518a8f5e939a991df9701c1d781864956146d9a90e8d15b32597945dd03c4506` |
| [scripts/voice_trial_evidence.py](../../../scripts/voice_trial_evidence.py) | `51b791793dce85c839d1adae069df07b636e443e2aee5475a8712c42792f4111` |
| [scripts/voice_trial_evidence_test.py](../../../scripts/voice_trial_evidence_test.py) | `e3410714c71e66d4fb34596f1e82a885cb0c275542600086627a5e00ece2c0cd` |
| [scripts/voice_trial_mutation_check.py](../../../scripts/voice_trial_mutation_check.py) | `8ecd2970443caecc5076354764a200ed97394bff861a745541b5e97c8dec297d` |

### Documentation and evidence artifacts

| Artifact group | Files / scope |
|---|---|
| Original analysis | [Local Architect-guided review](../elevenlabs-architect-guided-nexus-review.md), [unsubmitted external brief](../elevenlabs-architect-review-brief.md), [candidate audit](../speaker-embedding-candidate-audit-2026-10-26.md) |
| Implementation plan | [Feature152](../../features/152-voice-lock-evidence-and-reliability-plan.md) |
| Live work and acceptance | [Speaker work card](../../testing/speaker-verification-work-card.md), [capture checklist](../../testing/voice-capture-accounting-checklist.md), [phase closeout](../../testing/voice-lock-phase-closeout.md) |
| Analysis format specifications | [Scalar benchmark](../../testing/speaker-verifier-benchmark.md), [embedding workflow](../../testing/speaker-embedding-score-workflow.md), [richer trial protocol](../../testing/voice-trial-evidence-protocol.md), [legacy template](../../testing/speaker-trials-template.csv) |
| Queue and evidence bookkeeping | [Scorecard](../../testing/scorecard-data.json), [dated queue](../../testing/windows1-production-queue.md), [live ledger](../../testing/live-proof-ledger.md), generated cards/process map |
| Frozen/release results | Dated assessment JSON/Markdown, build manifests, source fingerprints and test logs under [runs](../../testing/runs/) |
| Discovery | Main/research/testing/feature indexes and [changelog](../../changes/CHANGELOG.md) |
| This handover | Five documentation files; new cross-index entries; historical testing-table clarification; no functional edits |

Generated work-card changes reflect evidence text and queue regeneration, not new acceptance proof. The work-card generator and later queue additions also serve concurrent programmes; this handover does not claim their entire implementation.

