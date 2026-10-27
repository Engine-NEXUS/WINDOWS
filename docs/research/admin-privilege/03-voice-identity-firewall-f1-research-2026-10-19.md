# Voice Identity — Firewall 1 (F1) Deep Research

**Date:** 2026-10-19. Status: RESEARCH (nothing built). This doc carries the complete evidence for the *voice* side of the Administrator feature. Architecture and cross-references: [01-admin-privilege-any-device-research-2026-10-19.md](01-admin-privilege-any-device-research-2026-10-19.md). Companion face+enrollment stack: [02](02-face-enrollment-template-only-pipeline-research-2026-10-19.md), [05](05-face-verification-firewall-f2-research-2026-10-19.md).

**Role of F1 (decided in doc 01, re-justified here):** voice = **candidate detector**, never the grant authority. Two reasons: (a) NIST 800-63B-4 forbids voice as an authenticator ("*Biometric comparison based on voice SHALL NOT be used*"), and (b) the anti-spoofing numbers below show no published countermeasure is un-foolable against modern TTS/VC. F1 answers exactly one question cheaply and hands-free: *"is the person who just said the wake word plausibly the Administrator, speaking live?"* Grant authority is F2+F3 (face+challenge); F4 policy bounds privilege regardless.

---

## 1. Acoustics and math — how speaker verification works (30-second recap)

A speaker-embedding network maps an audio window (2-4 s, 16 kHz mono) to a fixed-length vector (128-512 d). Enrollment = averaging L2-normalized vectors over enrollment utterances → a centroid. Verification = cosine similarity between the fresh utterance's vector and the centroid, plus score normalization. The pattern is **identical in spirit to the face pipeline (doc 05 §2)** — one measuring tool, one template of numbers, no user-side training.

```
raw waveform → VAD/energy gate → feature front-end → embedding CNN → L2 norm
                                                                    ↓
enrolled centroid (per device, per profile) ─────── cosine ────────┤
                                                                    ↓
                                              AS-Norm (imposter cohort) → score → τ decision
```

NEXUS already ships the necessary substrate: `wakeword_oww.rs` runs a **sherpa-onnx speaker-embedding extractor** — `speaker_model.onnx` (29.6 MB, 256-d, CPU, 1 thread), integrated with enrollment/verify paths per [docs/wake-word/07-speaker-verification.md](../../wake-word/07-speaker-verification.md). F1 becomes an upgrade + orchestration of existing code, not a new engine from scratch.

---

## 2. Model choice — the toolkit landscape (what NEXUS can ship)

| Toolkit | License | Models of interest | Headline VoxCeleb1-O EER (matched) | ONNX / runtime | NEXUS fit |
|---|---|---|---|---|---|
| **sherpa-onnx (current)** | Apache-2.0 | generic x-vector-class embedding, 29.6 MB, 256-d | (unpublished; measured in V1 below) | already ONNX | **baseline today** |
| **wespeaker** (wenet-e2e) | Apache-2.0 toolkit; pretrained VoxCeleb models follow VoxCeleb license = **CC-BY 4.0** | ECAPA-TDNN-512 + large-margin FT; ResNet34/152/221/293; CAM++; SSL-frontends (wav2vec2/WavLM) | ECAPA512+LM+AS-Norm **0.957 %**; ResNet34+LM+AS-Norm **0.723 %**; deep r-vector 293L **0.447 %** | official ONNX runtime exports; CLI `wespeaker` | **recommended F1 upgrade** |
| SpeechBrain | Apache-2.0 | `spkrec-ecapa-voxceleb` | **0.80 %** (s-norm), 0.90 % without | PyTorch (convert) | prototyping reference |
| 3D-Speaker (modelscope) | Apache-2.0 | CAM++ CN/EN 200k, multimodal extensions | tool-paper benchmarks | ONNX available | multi-modal follow-on |
| NeMo / NVIDIA | Apache-2.0 | TitaNet-class | — | PyTorch | not needed |

Notes on fit: wespeaker models exist as ready ONNX files with modest sizes (ECAPA512 ≈ 6-30 MB class weights), CPU inference in tens of ms — compatible with NEXUS's standing RAM budget culture (features 130-132; unload-after-use pattern like `admin-brain`).

Interpolation caution: all published numbers above are **matched-condition** batches with hundreds of imposters. NEXUS's real 1:1-vs-admin task is much narrower (one enrolled template, far-field laptop mics, room noise), so V-phases *measure* the operating point rather than assume table accuracy.

---

## 3. Cross-device reality — the honestly relevant numbers

The owner's trial is: admin enrolled (usually in his room / his own laptop), **testing on a stranger's laptop** with an unfamiliar mic in an unfamiliar room. This is *cross-domain* data — exactly where published EERs inflate:

| Condition change | Reported EER | Meaning for F1 |
|---|---|---|
| CNCeleb (unconstrained Chinese, real devices/venues), ResNet | **8.43 % → 6.49 %** after large-margin FT + AS-Norm | real-world camera-style mismatch is a 6-8x degradation vs VoxCeleb toy numbers |
| NIST SRE16 (cross-language, new channels) | **6.6 % pooled** even after PLDA domain adaptation | mismatch costs are structural, not just tuning |
| SdSVC 2020 (text-independent, in-domain Farsi) | 1.63-2.38 %; **1.45-1.7 %** with domain-balanced HPM + language-offset AS-Norm (IDLab, Interspeech 2020) | in-domain adaptation recovers roughly half the loss; the LID-offset trick for language mismatch is a template for our per-device offset |
| Studio-grade fooled trials (anonymous impostor, same studio) | 0.5-1 % | the "matched" reference every other row degrades from |

**Engineering menu that closes part of the gap** (all mature techniques, all implementable server-side of the ORT call):

1. **Per-device calibration enrollment** — on first admin verification on a new device, admin reads 2-3 s fixed sentences; average ≥3 device-local embeddings into a *device-adapted centroid* (enrollment refresh averaging is standard practice from IDLab recipe).
2. **AS-Norm (adaptive score normalization)** with a device-matched imposter cohort — normalizes the score by the mean/std of top-K impostor similarities; offsets percondition (top-40 is the classic choice). Cohort can be device-agnostic + a per-device offset — the Interspeech 2020 trick demonstrated removing language mismatch without retraining.
3. **Score calibration** (logistic regression) once device trials accumulate — the calibration flag in SpeechBrain's recipe; wespeaker added quality-aware calibration 2024-05.
4. **Text-independent scoring over the entire post-wake window** (2-4 s of the admin reading the challenge — see §5) instead of scoring the wake word alone.

---

## 4. "Live voice only" — anti-spoofing evidence and where it breaks

### 4.1 Countermeasure (CM) landscape — ASVspoof 5 (2024) numbers

| System class | Metric | Closed condition | Open condition (SSL features) | Who breaks it |
|---|---|---|---|---|
| RawNet2/AASIST (previous SOTA baselines) | minDCF / EER | **minDCF > 0.7, EER > 29 %** → baseline collapse | same collapse | crowdsourced real-device data + new attacks |
| Best single-model-track ensemble (T32-class) | minDCF / EER | 0.2436 / 8.61 % | — | — |
| **Best overall (wav2vec2-SSL feature ensembles, T45-class)** | minDCF / EER | 0.3948 / 14.33 % | **0.0750 / 2.59 %** | — |
| Codec robustness (same family, per-track EER breakdown) | EER | clean C01 ≈ **1.17 %** | degrades sharply on MP3+Encodec (C07) and Speex-8 kHz (C10); moderate on Opus-8k, Encodec | narrow-band laptop mics are exactly C10-shaped |
| Per-attack outliers | EER | A28 (YourTTS-based spoof) **16.10 %**; adversarial "Malacopula" family A27/A30-A32 ≈ 5 % | same | adversarial + modern VC |
| Calibration | actDCF | top systems ≈ **1.0** (score normalization ≠ LLR calibration) | same | uncalibrated scores are decision-hazardous — NEXUS must calibrate or use DCF-aware thresholds |

### 4.2 Spoofing-robust ASV (fusion of identity + countermeasure)

ASVspoof 5 Track 2 (SASV-style): fused ASV+CM systems reached **t-EER ≈ 2-4 % (in-element)** and min a-DCF ≈ 0.075-0.15 (open condition, T45/T39/BUT). Top submissions all fuse; single integrated models underperform (organizers' own conclusion). The architecture conclusion is stable across challenges: **two-in-one score beats either alone**, so F1's final score should be a fusion of (speaker similarity) ⊗ (spoof likelihood) — implemented as a gated product or stacked logistic back-end, not a single naive threshold.

### 4.3 Why unattended fusion still cannot promise "100 %, never fooled"

1. Every ASVspoof edition publishes **attack families that degrade the previous best**; "unknown/OOD generalization remains unsolved" is the organizers' standing conclusion.
2. Codec + narrowband conditions (laptop microphone reality) eat the robust models that do well wide-band.
3. Calibration and adversarial attacks (Malacopula) are confirmed present in the eval set — voice CM lives with adversarial pressure like every other ML detector.
4. Even with perfection, a *deterministic* authority cannot rest on a probabilistic detector — this is the letter of NIST 800-63B-4 (doc 04).

**The structural fix (not a detector): freshness.** A *fresh random challenge* per session re-frames the adversary's cost — to pass, an attacker must generate the right phrase on the *right day* in the *right room* on camera (F2). Replaying recorded admin audio fails at generation time; TTS cloning fails unless it beats both fusion CM and F2 face-and-PAD, simultaneously, on the admin's distinctANJI voice. "100 % except…" becomes: every published attack class loses to at least one layer with high probability; the residual is the residual of convolution, not of any single model.

---

## 5. Challenge-response design (F3's voice half)

```
nexus → orb/sidebar shows 3 random digits ("4 - 7 - 9")
admin speaks: "…four seven nine…" (optional lead-in: the digits inside one sentence)
 window: the SAME 2-4 s containing identity + liveness + freshness:
   - speaker embedding  → F1 identity score
   - speech CM pass     → F1 spoof score (AASIST-class ONNX)
   - waveform jitter    → replay fingerprints (optional, secondary)
   - transcript timbits → digits match → F3 freshness score
 failure modes:
   - digits mismatch/absent → "please repeat digits" (max 2) → refuse politely
   - impostor speaks digits → identity+CM still gate
```

Design constraints verified against literature posture:
- Text-prompted SV has precedent (SdSV Challenge 2020 — text-*dependent* task where the "pass-phrase + speaker" combine); ASVspoof 5 Track 2 explicitly moved to fused SASV scoring, making this integration point natural.
- The digit phrase must be **non-enumerable** (no fixed "1-2-3" default), **short** (≤ 4 elements for a ≤ 3.5 s window), **numeral-aware in STT** (bias decoder toward digit vocabulary — NEXUS already has vocabulary-bias machinery in `stt_groq.rs` from the wake work).
- Failure UX from doc 08: re-prompt ≤ 2x → "I couldn't confirm the Administrator." Never present partial identity results.

---

## 6. F1 data-lifecycle (voice templates)

| Item | Rule |
|---|---|
| Enrollment | 30-60 s of administrative utterances (varied phrases), recorded **only into RAM**, embedded, dropped; per-device calibration sentence appended |
| Stored | 1 global centroid + N device-adapted centroids (cap 5), L2-normalized 256/192-d floats |
| Protection | AES-256-GCM + DPAPI (same store design as face templates, doc 02 §1-2); never leaves the device; never enters IPC except decisions |
| Refresh | quiet: successful high-margin F1 sessions may append a device centroid (cap 5) |
| Revoke | "delete my voice profile" → centroids shredded → re-enroll required |
| Non-existent path | voice templates never synced to Worker (feature-88 biometrics-never-cloud invariant) |

---

## 7. F1 engineering plan (V-phases; nothing built)

| Phase | Work | Gate |
|---|---|---|
| **V0** | Decide: keep sherpa 256-d extractor vs swap wespeaker ECAPA512 ONNX (or run both: cheap A/B). Pin fusion recipe (score ⊗ CM) and calibration route. | decision record + benchmark harness wired (repo A/B script on fixed corpora) |
| **V1** | Measure matched baseline: same-device impostors (owner + 2 voice doubles), 10 trials/files/head positions; publish measured FMR/FRR and compare with published tool numbers sanity-check | measured EER recorded in `docs/research/ghost-mode/data/`-style JSONL; anomaly flagged if wildly off tool numbers |
| **V2** | Cross-device matrix: enroll on device A, test on laptop B + C + D (different mic brands), collect ≥ 300 trial pairs; tune per-device calibration + AS-Norm cohort; set per-device τ | cross-device FRR < 5 % (with challenge retry) at FAR < 0.1 %; each threshold mutation-safe (≥ 5 mutations killed) |
| **V3** | Anti-spoof CM ONNX (AASIST-class or small SSL-feature model) + replay-jitter heuristics; integrate fusion scoring | recorded-replay corpus (adb-recorded + phone-speaker) detected ≥ 98 %; TTS-clone corpus (3 public TTS systems of admin voice) detected ≥ 95 %; codec-8k sanity test |
| **V4** | Challenge-response loop (STT digit bias + retry + fusion gate), state machine hooks to orchestrator/capture window; admin token issuance only on full pass | red-team: yesterday's recording cannot answer fresh digits in 100 % of attempts; faked live digits by imposter cannot pass identity+CM simultaneously |
| **V5** | RAM/latency: session pool + 20 s unload; wake→candidate-decision ≤ 400 ms p50 on mid-range laptop (est., measure); recording into AGENTS.md with *Verify / Not run live* lines, per house rules | budgets recorded; "not run live" list empty only after live house test (P5/master plan) |

Kill-switches (inherited from doc 01): F1 work must not touch Ghost files (`ghost_center.rs`, `mouse.rs`, `shortcuts/*`, `screen.rs`, `ghostHotMic.ts`); wake-pipeline changes stay additive and flag-gated (`adminVoiceEnable`).

---

## 8. Open questions (not verified)

1. **Wespeaker ONNX on NEXUS's CPU class** (thumbnails: mid-range laptops): confirm real-frame latency and RAM delta before committing (V0 benchmark).
2. **CNCeleb-class degradation on NEXUS devices** — the matrix in V2 decides whether per-device calibration is sufficient or per-*room* re-calibration is needed.
3. **CM model choice under license/size**: AASIST reference implementations vary in license; SSL-feature models (wav2vec2) are heavier (few hundred MB); if the small-CM trade is unacceptable, F1 leans harder on F3 freshness (challenge mandatory on every admin candidate) and F2 — which is the design fallback anyway.
4. **D_outch sensitivity of wake word sequences to admin phrase** ("NEXUS" alone vs "NEXUS, digits…") — wake-buffer reuse vs capture-extension decision in V4.
5. **Room dynamics** (TV speech, other voices) triggering camera-open — occupancy gate: only fire F2 when F1 similarity is confidently admin (no "maybe-admin → camera pops" annoyance path).
6. **Role collision**: if the device user is also enrolled (voice or face), the system must *not* confuse "device user" and "admin" at low margins — tie-break rule: prefer lower-privilege role when ambiguous.

---

## 9. Sources (checked 2026-10-19)

Speech/toolkits:
- sherpa-onnx speaker embedding integration + config: [docs/wake-word/07-speaker-verification.md](../../wake-word/07-speaker-verification.md) (NEXUS internal)
- wespeaker: GitHub wenet-e2e/wespeaker (Apache-2.0); pretrained tables — ECAPA512-LM+AS-Norm 0.957, ResNet34-LM+AS-Norm 0.723, deep r-vector 293L 0.447, ECAPA-SSL-self-sup 2.627, CNCeleb 6.492 (LM+AS-Norm)
- SpeechBrain: `speechbrain/spkrec-ecapa-voxceleb` model card (Apache; 0.80 % with s-norm / 0.90 % without), quality-aware calibration support noted in wespeaker 2024-05 changes
- 3D-Speaker-Toolkit: Chen et al., arXiv:2403.19971 (Apache, multimodal direction)

Papers (voice):
- ECAPA-TDNN: Desplanques et al., Interspeech 2020 / arXiv:2005.07143 (AAM-softmax, architecture, AS-Norm top-K 40)
- Cross-lingual domain-balanced HPM + language-dependent s-norm: Thienpondt et al., Interspeech 2020 (arXiv:2007.07689) — SdSVC fused 1.45 %
- ASVspoof 5: Yamagishi et al., 2024 (arXiv:2408.08739) — Track 1/2 result tables, baseline collapse, a-DCF/t-DCF/t-EER metrics, SSL-ensemble findings, CPU/crowdsourced conditions
- BUT systems for ASVspoof 5 (arXiv:2408.11152) — t-EER 2.0-4.3 open condition; codec conditions in ASASVIcomtech (arXiv:2408.10361) — A28 YourTTS 16.10 % EER, adversarial Malacopula A27/A30-A32 ≈ 5 %, codec C07/C10 degradation
- SASV 2022 challenge (Jung et al.): spoof-aware fused ASV framing motivating two-in-one score fusion
- NIST SP 800-63B-4 (July 2025) §3.2.3 — voice biometric exclusion (full quote treatment in doc 04)
