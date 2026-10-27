# NEXUS review using ElevenLabs Architect guidance

Date: 2026-10-25. **Local review by Codex using installed ElevenLabs playbooks and NEXUS source/evidence. No remote Architect consultation or ElevenLabs MCP call was completed.** The Agents Platform skill was explicitly invoked, but none of its required agents_* tools is exposed in this chat. Plugin installation and skill availability do not establish an authenticated tool connection.

Implementation: [Voice Lock evidence and reliability plan](../features/152-voice-lock-evidence-and-reliability-plan.md). The owner authorized implementation; P1 capture accounting is being built and validated first. Findings below remain evidence-based recommendations, not vendor approval.

## Scope and sources

NEXUS is an existing Rust/Tauri Windows assistant, not a discovered ElevenLabs-hosted agent. No ElevenLabs agent ID, branch, deployment, or conversation was retrieved. No cloud resource was changed.

Installed guidance examined: agents-platform, architect-secure-for-production, and architect-turn-taking-latency, from official plugin version 0.4.0. The applicable ideas are layered authorization, converting real failures into tests, repeated evaluation, controlled rollout, privacy, and measuring turn timing. Provider configuration instructions are not assumed to apply directly to NEXUS.

## Findings and suggested order

| Order | Finding | Evidence / uncertainty | Narrow next action | Required cross-check |
|---:|---|---|---|---|
| 1 | Trial accounting is incomplete | Ten attempts reported; seven completed captures identifiable. Logs reuse session labels. The cause of the three unmatched attempts is unknown. | Assign a monotonic capture/attempt identifier and log start, cancellation, verification, transcript filtering and dispatch terminal states against it. Associate the running executable identity with each session. | Every scripted start has exactly one terminal outcome. Delayed responses cannot attach to a newer capture. Real ten-attempt run reconciles all ten, including failed/cancelled attempts. |
| 2 | Effective Voice Lock state is unclear to the owner | Owner reported on/Saved while native queries returned false. Draft settings are only applied by Save. We have not reproduced the precise UI interaction. | Show native effective state separately from unsaved draft state; reread native state after save, surface mismatches/errors, and refresh on reopen. | Slow/failed save, stale window, unavailable profile and backend refusal cannot display an effective locked state. Verify native state in the live test before speaking. |
| 3 | Speaker identity discrimination has not been established | Current code pools OWW audio features and uses maximum cosine similarity across profile vectors. Calibration uses held-out owner clips only; no impostor distribution exists. | Benchmark the present verifier against a dedicated speaker-verification model on independent recordings; retain the existing fail-closed action gate. Do not change the threshold based only on additional owner successes. | Separate owner rejection, unfamiliar-speaker acceptance and executable actions. Freeze training/calibration/acceptance sets. Measure RAM, latency and Windows feasibility before selecting a replacement. |
| 4 | Verified identity does not guarantee a faithful command | The observed batch includes duplicated targets, repeat filtering and an unrelated agent-hook transcript. The owner decoder prompt contains complete executable command examples. Prompt influence, microphone timing and echo are possible causes, not proven diagnoses. | With explicit audio-diagnostic consent, compare the same retained failed clips under neutral decoding and vocabulary hints. Preserve guards and the failed results. Inspect capture boundaries and TTS overlap before choosing a fix. | A fresh held-out corpus of commands, silence, ambient speech and playback measures unintended actions, not just word accuracy. No broad deduplication or fuzzy repair that silently changes executable intent. |
| 5 | Replay resistance is unmeasured | There are zero measured negative trials. Owner acceptance does not establish rejection of recordings, synthetic speech or another person. | Keep separate playback and live unfamiliar-speaker protocols. Include owner recording replay and owner-wake/non-owner-command handoffs. | Rejected/uncertain speech must produce zero executable actions; rejected answers cannot approve pending actions. Never label 300 repetitions of one clip as 300 independent speaker observations. |
| 6 | Release evidence spans different builds | Automated passes belong to frozen source 50dcda2; later diagnosis ran D: release 3d1932b68b2c0f15. | Pick one assessment revision and make its identity visible in the live-test instructions/logs. Preserve unique logs across restarts. | Complete gates and provenance pass on that revision; repeat source-sensitive owner checks there. A successful check on another build stays separate. |
| 7 | Turn latency needs stage-level evidence | Prior work measured Edge prewarming benefits; current end-to-end acceptance conditions remain incomplete. | Measure capture endpoint, verification, STT, routing and first audible playback separately. Evaluate speculative preparation only after cancellation semantics are proven. | Report p50/p95 on the real Windows device, network transitions and interruptions. Speculative work must never execute actions or emit stale speech after cancellation. |

These are review recommendations, not implemented fixes. The existing feature score remains 41% (2.05/5), and the queue stays on speaker verification.

## Why compare a dedicated verifier?

openWakeWord documents voice-specific custom verifiers as second-stage wake-activation filters. Its reference verifier uses labeled positive/negative examples and a trained classifier; NEXUS currently uses a cosine-profile comparison instead. That documentation does not validate NEXUS's arbitrary-command identity check. [Official openWakeWord repository](https://github.com/dscripka/openWakeWord), [reference verifier implementation](https://github.com/dscripka/openWakeWord/blob/main/openwakeword/custom_verifier_model.py).

SpeechBrain publishes an ECAPA-TDNN model for speaker verification. It is a candidate for a comparative experiment, not an approved Windows runtime dependency or a guarantee of replay resistance. Export compatibility, licensing of code/weights, footprint and microphone-domain performance still need verification. [Official model card](https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb).

## Solo developer protocol

1. Confirm the executable identity and native effective lock state before the session. Avoid rebuilding or switching apps during a measured batch.
2. Allocate an attempt ID before each start. Record normal position, distance/noise condition and keyboard/wake path. Keep failed attempts, cancellations and no-capture outcomes visible.
3. Measure owner acceptance on new speech excluded from enrollment/calibration. Check recognition and requested action success separately.
4. Use independently played recordings or synthetic voices as labeled playback tests. Preserve source/speaker/clip grouping. NEXUS's own muted TTS output is not a valid negative trial if it never reaches verification.
5. Owner replay, profile deletion/corruption, restart, unavailable model and keyboard recovery are separate security/recovery cases. Changing one's own voice is not a substitute for unfamiliar speakers.
6. Live unfamiliar-speaker performance remains pending until suitable participants are available. Solo testing cannot settle that claim.

## Statistical and deployment limits

With zero failures in 300 independent negative trials, the exact one-sided 95% upper bound is about 0.994% per trial, not proof of a zero underlying rate. Reused recordings and correlated conditions limit generalization. The existing speaker_trial_summary.py computes the bound; preserve condition-level counts and the trial-to-log mapping.

The target of at least 95% owner acceptance needs the owner sample size and uncertainty alongside the observed rate. Seven verified captures with three unmatched attempts do not establish a ten-attempt acceptance rate. No model or threshold should be tuned on the final acceptance set.

The default-off rollout and keyboard recovery remain appropriate while the evidence is incomplete. Speech recognition, identity, replay resistance and native action authorization require separate checks. No ElevenLabs TTS/STT or hosted-session authentication capability was established as a replacement for local speaker authentication in this review.

## MCP limitation

The invoked agents-platform skill documents managing agents, tests and conversations in an ElevenLabs account. It does not document a tool for submitting an arbitrary local repository to the dashboard Architect for an independent review. Discover and inspect actual tool schemas before promising such a capability. Earlier CLI authentication failed with Invalid redirect_uri. No remote review result is being claimed here.
