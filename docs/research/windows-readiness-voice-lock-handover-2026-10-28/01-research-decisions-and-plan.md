# 01 — Research, analysis, decisions and plan

[Handover index](README.md) · [Implementation](02-implementation-and-file-inventory.md) · [Evidence](03-verification-failures-and-release-history.md) · [Remaining work](04-pending-work-and-maintenance.md)

Prepared 2026-10-28 from the dated research and implementation records. This document distinguishes hypotheses, reproduced behavior, implementation choices and unresolved measurements.

## 1. Original problem and agreed objective

The owner first requested understanding of the project documentation, a whole-project testing/readiness table and a worst-first work queue. The resulting Windows1.0 programme made speaker verification the active lowest-rated feature at 41%. It required finishing one feature with real proof before moving to installer/release packaging or later queue entries.

The original audit identified shared release gates that affected every feature: native failures, IPC inconsistencies/comment parsing, NLU split contamination, whitespace and dead-code checks. Passing feature-specific tests while shared release checks failed was insufficient. Concurrent source edits also made a plain branch/HEAD label inadequate as an assessment identity.

The work therefore separated three objectives:

1. Establish a reproducible source/build baseline without discarding concurrent work.
2. Repair the active feature's observable safety, enrollment, settings and command-execution gaps.
3. Build the evidence pipeline needed to decide whether real-device results justify a production claim.

### Original lowest-rated planning queue

The following is the owner's original 65-feature programme excerpt, preserved as planning history. It is not a new2026-10-28 rating or the expanded 68-card queue. Later additions and tie ordering can change positions; use current scorecard data and generated cards before selecting subsequent work.

| Original order | Feature | Raw score | Stars /5 | Main evidence gap in the approved plan |
|---:|---|---:|---:|---|
|1|Speaker verification / owner check|41%|2.05|Enrollment, fail-closed behavior and measured owner/impostor outcomes|
|2|Installer / release pipeline|47%|2.35|NSIS, shortcut and clean install/uninstall proof|
|3|Reverse engineer website/app|48%|2.40|Two real sites, voice, scroll and window restoration|
|4|Solid panel windows|51.5%|2.58|Real windows and display-scaling inspection|
|5|WhatsApp priority people|52.5%|2.63|Real bridge and read-receipt behavior|
|6|Vision-grounded click|54.2%|2.71|Measured real-screen accuracy|
|7|Agent-watch sidecar|55%|2.75|Real CLI sessions and stronger tests|
|8|Proactive-speech policy|56%|2.80|Live timing and suppression behavior|
|9|Leave-by reminders|56%|2.80|Real addressed calendar event|
|10|Wake-up sound|57%|2.85|Audible cue and microphone echo check|
|11|Boot/shutdown/resume greeting|57%|2.85|Fresh-build lifecycle walkthrough|
|12|Track all agents|58.8%|2.94|Real multi-agent completion run|
|13|Memory profile/corrections/routines/review|58.8%|2.94|Real usage over time|
|14|Important-mail watcher|59.5%|2.98|Real Gmail alert run|
|15|BERT-Mini NLU fallback|59.8%|2.99|Data integrity, model and live routing proof|

The initial project-wide report is [the dated feature-readiness scorecard](../feature-readiness-scorecard-2026-10-21.md). Its older classification thresholds do not replace the later90% acceptance contract. This work did not complete the other queue entries merely by fixing shared gates.

## 2. Findings and evolution of the diagnosis

| Finding | Original evidence | What later changed | Remaining uncertainty |
|---|---|---|---|
| Enrollment interfered with waking | Owner said enrollment also woke NEXUS | Whole-session microphone baton and cancellation lifecycle repaired | Repeat exact-candidate attribution remains necessary |
| Only “Nexus” was emphasized in prompts | Owner explicitly requested “Hey Nexus” and “OK Nexus” too | Both enrollment/calibration halves cover all three | Phrase coverage does not measure identity accuracy |
| Ten owner attempts lacked complete accounting | Seven completed captures identifiable; three unmatched | Native attempt/session lifecycle and reconciliation implemented | No qualified ten-start acceptance batch was established |
| UI On/Saved disagreed with native Off | Readback contradicted the owner's UI report | Effective-vs-draft UI, verified Save and fault handling implemented | The original interaction's full cause was not established from the report alone |
| Speaker feature had no unfamiliar distribution | Owner-only calibration and zero measured negative corpus | Offline paired-model and embedding tools implemented | No measured candidate comparison or unfamiliar-live FAR exists |
| Correct owner identity still produced wrong command | Verified captures had duplicated/incorrect commands | One-clip experiment reproduced prompt-dependent duplication; neutral decoding applied | General command fidelity, echo and unseen conditions remain unproven |
| Local native action could be routed without execution | Frontend skipped native execution; Rust branch emitted Done | Existing native mapper/executor invoked after authorization | Repaired device action still needs observation |
| Logs did not prove actions | Transcript delivery/dispatch receipts were visible | Evidence tools separate capture, verification, fidelity and observed success | Human/device confirmation and exact-source logs remain required |
| Legacy CSV could look sufficient | Mixed negative rows could set a numeric flag without qualification | Legacy numeric acceptance disabled; richer metadata format added | Declared metadata cannot authenticate itself |

## 3. ElevenLabs Architect request: what actually happened

The owner asked to obtain an Architect opinion through ElevenLabs' own MCP, offered to log in, showed the plugin as connected and invoked the Agents Platform skill. The work checked available capabilities and attempted the connection paths documented in the [external review brief](../elevenlabs-architect-review-brief.md).

Recorded connection outcomes:

- The hosted MCP endpoint's automatic client-registration flow did not support dynamic registration.
- Retrying with the official plugin's public client identifier reached an `Invalid redirect_uri` result for the CLI loopback callback.
- The official plugin/skill could be installed/enabled while its required agent-management tools were not exposed in the active chat.
- A signed-in browser Architect composer was reached, but prompt entry timed out repeatedly. No confirmed conversation submission or received opinion exists.
- No hosted agent, deployment, subscription or account permission was changed. No API/OAuth secret was copied into the documents.

The fallback was a **local review using installed ElevenLabs playbooks**, not a vendor consultation. The [local review](../elevenlabs-architect-guided-nexus-review.md) applied ideas from agents-platform, production security and turn-taking guidance to the source and measured failures. Its findings are local recommendations; they are not ElevenLabs approval.

Applicable ideas were layered authorization, conversion of observed failures into regressions, repeated evaluation, controlled rollout, privacy and stage-level timing. Provider-specific configuration or hosted-agent authentication was not treated as a drop-in local speaker authenticator.

## 4. Identity, recognition, authorization and execution are different questions

The analysis established separate boundaries:

| Boundary | Question | Evidence needed |
|---|---|---|
| Wake detection | Did the activation model detect an intended wake? | Held-out wake recall and false fires per hour |
| Speaker verification | Does the captured speaker match the enrolled owner? | Independent owner/impostor distributions and calibrated threshold |
| Speech recognition | Were the intended words recognized faithfully? | Held-out command labels and controlled decoding comparison |
| Authorization | Is this action allowed for this verified capture/context? | Fail-closed policy and regression/mutation tests |
| Routing | Did the command reach the proper subsystem? | Correlated session/dispatch evidence |
| Execution | Did the executor run once and report its real result? | Injected-boundary tests plus actual device observation |
| User-visible success | Did the intended app/window/action occur? | Independent observation of the requested result |

One success cannot substitute for another. A 0.960 owner score with an incorrect command is not an owner rejection. A delivered transcript is not app-opening proof. A failed action does not establish an acoustic echo cause.

## 5. Speaker-model research and candidate boundary

The current baseline reused openWakeWord features and maximum cosine over stored owner vectors. Wake-activation features may help reject some voices, but the work did not establish their suitability for arbitrary-command identity. Owner-only calibration cannot define an unfamiliar-speaker distribution.

The [preliminary candidate audit](../speaker-embedding-candidate-audit-2026-10-26.md) recorded official sources on 2026-10-26:

| Candidate | Reason to investigate | What was not established |
|---|---|---|
| Existing OWW pipeline | Already integrated; baseline with existing preprocessing | Adequate identity discrimination or replay rejection |
| SpeechBrain ECAPA-TDNN VoxCeleb | Dedicated embedding/verification reference; model card describes cosine verification and16kHz mono input | Windows footprint, ONNX export parity, short-command accuracy, exact artifact/legal clearance |
| WeSpeaker VoxCeleb ResNet34 ONNX | Official pretrained catalogue provides ONNX runtime variants | Exact pinned weight/license/export verification, target-device resource use and microphone-domain accuracy |

Research distinctions retained:

- Repository code terms and pretrained weight terms are separate checks. The WeSpeaker source audit did not infer weight rights solely from the repository's Apache-2.0 code license.
- Sherpa's referenced enrollment example averages vectors; NEXUS uses the maximum cosine across individual enrolled vectors. They are different scoring policies and must not be interchanged without an explicit experiment.
- Published benchmark performance is not NEXUS performance under noise, distance, short phrases, TV replay or local microphone conditions.
- No artifact was downloaded, candidate selected, new model executed or threshold installed by this research phase.

These statements summarize the dated audit. They are not a renewed2026-10-28 model/license verification or recommendation to migrate providers.

## 6. Experimental design decisions

### 6.1 Separate enrollment, calibration and acceptance

Enrollment vectors define the owner reference. Calibration selects a provisional threshold. Acceptance evaluates the frozen choice. A recording/group cannot cross these partitions, and unfamiliar calibration speakers must not silently reappear as held-out identities.

The existing ten-clip UI stores five profile clips and withholds five owner clips. Those clips do not become owner acceptance trials. A future dedicated-model comparison needs independent owner and impostor calibration data, plus a separate held-out acceptance set.

### 6.2 Use paired trials for model comparisons

Each candidate must receive the same trial IDs and equivalent metadata. A model tested only on easier conditions must not be compared with one tested on the entire corpus. Artifact and preprocessing hashes identify the declared candidate; authentic extraction still needs review.

### 6.3 Preserve failures and dependencies

Keep blocked starts, cancellations, missing captures, no speech, verifier errors, filtered transcripts and wrong actions visible. Repeated clips/groups/speakers are disclosed, not treated as automatically independent observations. Changing an ID does not make a recording independent.

### 6.4 Calibrate without looking at acceptance

The scalar benchmark's stated objective chooses the lowest observed calibration score strictly above every live non-owner calibration score. If none qualifies, it uses a next-representable threshold above the calibration maximum, rejecting all calibration rows. This is a measured-data selection rule, not proof of population security or a shipping configuration write.

Acceptance must not tune the threshold, model, preprocessing, prompt or template. A new choice after looking at results requires a newly held-out acceptance experiment.

## 7. Statistical interpretation

The programme targets at least 95% observed owner acceptance and zero observed locked non-owner executable actions across at least 300 qualified negatives. Report sample counts and uncertainty separately from the point estimates.

For zero failures in `n` independent trials, the exact one-sided95% upper bound is `1 - 0.05^(1/n)`. At300 independent trials this is approximately0.994%, not a zero underlying failure probability. Correlated replays weaken the population interpretation;300 repeats of one clip cannot establish broad unfamiliar-speaker performance.

Wilson95% intervals accompany acceptance rates. Owner false rejection and live non-owner false acceptance are reported separately. Owner replay, other-speaker playback and synthetic playback are separate negative classes. A solo developer can measure their own acceptance and labelled playback behavior but cannot establish unfamiliar live-speaker performance alone.

## 8. Important decisions and rejected shortcuts

| Decision | Rationale / rejected shortcut |
|---|---|
| Require a valid sealed profile before enable | A toggle alone must not imply available identity verification |
| Fail closed when enabled but missing/corrupt/unavailable/uncertain | Do not silently promote an unverifiable voice to owner |
| Retain keyboard/settings recovery | A failed voice gate must not lock the owner out of recovery |
| Use a profile-specific persisted Windows key | Memory wipe must not strand a valid voice profile |
| Pause native wake/capture through the entire enrollment session | Browser recording must not race normal activation |
| Track native attempts and sessions separately | Request count, accepted capture count and action count differ |
| Verify effective native settings after Save | Patch acknowledgement is not proof that routing state changed |
| Remove inaccessible premature Saved text | Opacity does not remove text from the accessibility tree |
| Make window creation async on Windows | Preserve responsiveness and avoid the reproduced synchronous-open deadlock |
| Neutral decoding after controlled reproduction | Do not keep executable command examples in the owner prompt without independent evidence |
| Preserve repetition/correction guards | Broad deduplication or fuzzy repair could silently change executable intent |
| Reuse the existing executor after native authorization | Fix the missing path without adding a second action system or bypass |
| Use offline analyzers without a production-ready verdict | Declared metadata and fixture passes cannot certify a release |
| Keep the feature score unchanged | Tool implementation and isolated successes do not close the missing live gates |

## 9. P1–P6 plan with measurable completion

| Phase | Intended implementation | Automated acceptance | External/live acceptance |
|---|---|---|---|
| P1 accounting | Unique requests/sessions; lifecycle packet; provenance linkage; reconciliation | Every request has a valid terminal or explicit unfinished outcome; stale/superseded work cannot attach to a later capture | Reconcile ten physical starts and ten requested outcomes on one exact release |
| P2 native state | Effective state vs draft; verified Save; cross-window refresh; error/profile handling | Slow/refused/fake Save, stale reads and profile loss do not display confirmed protection | Exact-candidate draft/Save/reopen/fault walkthrough and real profile recovery |
| P3 verifier comparison | Frozen paired corpus and model/preprocessing identity; offline score/embedding tools | Split, template, metadata and numeric validation; no acceptance tuning; resource/uncertainty reports | Actual corpus, artifact/export/license audit and measured candidate selection |
| P4 fidelity | Same-audio diagnosis; neutral decoding; native local executor boundary | Wrong prompt policy and removed dispatch/cancellation/legacy guards fail regressions | Held-out commands/silence/ambient/replay and intended observed actions; stage health |
| P5 acceptance/recovery | Qualified metadata; separate identity and action outcomes; all locked failures retained | Bad denominators, missing qualification and false success claims cannot pass tooling checks | Owner/live unfamiliar/replay trials; restart/profile/model/capture/confirmation/keyboard recovery |
| P6 acceptance | Freeze source; all shared gates; owner-built fresh executable; second review | Complete library/integration/frontend/Worker/Python/NLU and repository checks on that source | Fresh provenance, device proof, no contradictory failure and rubric thresholds |

The source-side evidence tools for P3/P5 do not complete their measurement phases. The full plan is [feature152](../../features/152-voice-lock-evidence-and-reliability-plan.md); the operational handoff is in document04.
