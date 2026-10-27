# Face Verification — Firewall 2 (F2) Deep Research

**Date:** 2026-10-19. Status: RESEARCH (nothing built). Companion docs: [01 overview](01-admin-privilege-any-device-research-2026-10-19.md) · [02 enrollment & retention](02-face-enrollment-template-only-pipeline-research-2026-10-19.md) · [03 voice F1](03-voice-identity-firewall-f1-research-2026-10-19.md) · [04 standards](04-standards-nist-compliance-research-2026-10-19.md) · [06 fusion](06-multimodal-fusion-deepfakes-research-2026-10-19.md).

**Role of F2:** the **verifier**. It is the only biometric NIST fully sanctions for authentication (with mandatory PAD — doc 04 §1). F2 converts F1's "candidate" into a grantable session by answering: *"a live human face matching the Administrator's template is present, right now, acting naturally."*

---

## 1. Pipeline anatomy (the 5 stages, F2 concrete)

```
camera frames (e.g. 15-30 fps, RS 1280x720 default)
  1. DETECT    YuNet (338 KB ONNX) → box + 5 landmarks; larger-scale alternative SCRFD
  2. ALIGN     5-point similarity warp → canonical 112×112 face chip
  3. NORMALIZE exact pixel preprocessing of the chosen embedder
  4. EMBED     CNN → 128-d (SFace) or 512-d (ArcFace-class) vector, L2-normalized
  5. VERIFY    cosine vs template centroids + PAD ensemble verdict gated first
```

Because NEXUS needs detection at every frame plus embedding only at decision frames, cost is dominated by *detection* (cheap) — embedding runs on 3 sampled decision frames, not per frame (RAM/CPU budget, features 130-132).

## 2. Model comparison — detection & embedding (with licenses)

### 2.1 Detectors

| Model | Size | Accuracy (WIDER Face val) | License | Notes |
|---|---|---|---|---|
| **YuNet** (2023mar) | **338 KB** | 0.830 easy / 0.824 medium / 0.708 hard | Apache-2.0 (OpenCV Zoo) | 5 landmarks incl. eye/nose/mouth — exactly what alignment wants |
| SCRFD (via InsightFace buffalo packs) | ~2-160 MB class | comparable/stronger | **models non-commercial** | only if commercial license is later acquired |

### 2.2 Embedders (the identity engine)

| Model | Dim | LFW | CALFW | CPLFW | AgeDB-30 | CFP-FP | License |
|---|---|---|---|---|---|---|---|
| **SFace (MobileFaceNet backbone, Sigmoid-constrained hypersphere loss)** | 128 | **99.60 %** (τ cos 0.363 / τ L2 1.128) | 93.95 % | 91.05 % | 94.90 % | 94.80 % | Apache-2.0 (OpenCV Zoo) |
| SFace block-quantized / int8 | 128 | 99.42 % / 99.32 % (OpenCV Zoo tool benchmark) | — | — | — | — | Apache-2.0 |
| ArcFace R50-class (buffalo_l) | 512 | ≈ 99.8 % class | higher | higher | higher | higher | **models non-commercial** (code MIT) |

Decision default: **SFace** — commercial-clean, tiny (36.9 MB ONNX), quantized variants available; upgrade path documented if InsightFace commercial license is ever purchased (change record then re-benchmarks thresholds).

Human benchmark for context (DeepFace README): human parity ≈ 97.53 % — modern models exceed it on benchmark sets; real rooms differ (§5).

## 3. Presentation attack detection — the science NEXUS inherits

PAD (liveness) decides *live human* vs *artifact/mockery* before identity is considered. Research-backed signals, ordered as NEXUS ensembles them:

| Signal | Principle | Evidence / source | Known bounds |
|---|---|---|---|
| **MiniFASNet ensemble** (Silent-Face-Anti-Spoofing, minivision-ai) | CNN texture/quality classifier; two models (V2 @ scale 2.7 + V1SE @ scale 4.0), 80×80 BGR input, 3-logit output (index 1 = real), softmax-averaged ensemble | Apache-2.0 repo; deployed by LocalAI as `Antispoofer` with the same ensemble-average reference impl | strong on print/replay/screen; camera-dependent (§4) |
| **rPPG (remote photoplethysmography)** | micro red-channel pulsation from cardiac cycle over 3-5 s | auxiliary-supervision literature (Liu et al., CVPR 2018 — rPPG estimated alongside depth improves cross-dataset generalization) | needs motion-free, decent light; fails on dim rooms → fixed by fallback (§6) |
| **Depth/parallax cue** | 3D head geometry vs flat photo | skewed-motion/multi-pose capture in enrollment script; depth-estimated supervision (CVPR 2018) | cheap approximation in v1 (head-turn prompt), full depth sensor unrealistic |
| **Lip-sync correspondence** | phonemes (audio just captured) ↔ visemes (mouth region) must match the *challenge phrase* | SyncNet family; POI-Forensics (CVPRW 2023) identity-consistency framing; AVFF (98.6 % FakeAVCeleb) | needs clean transcript-window alignment (NEXUS has word-level timings — captions/caption-stack work); degraded by mask |

**PAD-first ordering (hard rule):** the identity match is only computed on frames/chips whose PAD ensemble passed. A photo of the admin with pixel-perfect embedding similarity must still fail the ensemble (flat/texture/no-pulse/no-sync) — this ordering is what makes F2 a verifier and not a recommender.

## 4. Datasets and the honest generalization gap

| Dataset | Size | Spoof types | Role |
|---|---|---|---|
| **CelebA-Spoof** (2020) | 625,537 images, 10,177 subjects, >10 sensors, 10 spoof types | print/replay/3D/paper-cut | modern training/diversity reference |
| CASIA-SURF (2019) | 1,000 subjects, RGB-D-IR | paper-cut | multimodal benchmark |
| SiW / Oulu-NPU / Replay-Attack | classic PAD eval sets | print, replay | cross-dataset protocol source |

**The number that governs expectations (Liu et al., CVPR 2018):** cross-dataset testing (SiW→Oulu protocols 1-4) lands at **ACER ≈ 10.0-14.1 %** for the era's best supervised models — i.e., generalized PAD is NOT saturated; intra-dataset saturation (ResNet-18 reaching ~99-100 % recall @ 0.5 % FPR on SiW/Oulu per CelebA-Spoof paper) is misleading in the wild. The camera-invariance work (Chen et al., arXiv:2101.10075) frames camera divergence as the *dominant* domain-gap factor, proposing high-frequency decomposition to de-couple camera signature from spoof cues.

**Consequence encoded into F2 design:** never trust a single PAD model across unknown laptop cameras → ensemble ≥ 2 signals (MiniFASNet + rPPG + lip-sync) AND capture-side quality gates (blur/exposure/face-size) AND per-enrollment threshold capture (doc 02 §E3), AND gracefully-fallback behavior when signal quality is low (§6).

## 5. Per-device camera reality on BYOD

1. **Camera origins vary** (built-in, USB, virtual OBS cams) — quality gate + detector confidence thresholds measured at enrollment (E3), not global constants.
2. **OS permission dialog** will appear on first F2 use per device — the hands-free claim is weakened on first use only (doc 01 §9 open Q5); scripted into UX (doc 08 §3 state machine).
3. **Privacy-mode laptops** (camera shutter/kill-switch) — F2 unavailability path: spoken "no camera available on this device; Administrator elevated features are unavailable here", fall back to normal mode (fail-closed).
4. **No-camera devices:** same path — elevated features simply cannot run (F2 is the NIST-eligible verifier; no F2 ⇒ no grant).

## 6. Decision logic (per-frame cascade)

```
for each frame w:
  detect → quality gate (size, blur, exposure)
  if quality OK:
    PAD ensemble (MiniFASNet + rPPG-over-window + lip-sync when challenge active)
    if all signals pass (per-margin rules):
       sample decision frame → SFace embed
       score = max cosine over template centroids
       if score ≥ τ_accept → F2 PASS
       elif τ_refuse < score < τ_accept → re-sample (≤ 3) → ask-again
    else: PAD fail → record reason → attempt offline with retry budget
```

- **PAD margins** are per-signal (not one threshold): e.g. MiniFASNet p(real) ≥ 0.5 robust floor, rPPG SNR ≥ gate, lip-sync correlation ≥ gate — exact values *calibrated per device* at enrollment and re-calibrated only via deliberate re-enrollment.
- **Failure accounting**: 5 consecutive F2 failures → 30 s backoff then exponential (NIST pattern, doc 04 §1.1 #9); each failure writes an audit line (what was seen, which signal failed — no image bytes).
- **Mixed-signal fusion options** (doc 06 §4) define AND-vs-weighted choices; v1 = strict AND with margins, upgrade path to calibrated weighted risk score.

## 7. F2 engineering plan (F-phases; nothing built) — extends doc 01 P3 and doc 02 E-phases

| Phase | Work | Gate |
|---|---|---|
| **F0** | Chassis decision: Rust `ort` (ONNX Runtime) vs Python sidecar (doc 02 §4); session pool + unload budget; module skeleton `admin_identity/face/` | decision record; skeleton compiles; IPC surface stubbed |
| **F1** | Detection + quality gates + framing overlay integration (orb/sidebar .ts plumbing, no new runtime beyond models) | live demo enrollment aid works on 3 dev machines; quality-gate unit tests |
| **F2** | PAD ensemble integration (MiniFASNet ONNX export; rPPG estimator function; lip-sync join with word timings from transcript) | PAD-first unit test ordering (mock embedder proves frames skip identity when PAD fails); spoof-corpus ≥ 98 % rejection (photo, screen, print) on ≥ 200 attempts |
| **F3** | Identity: SFace embed + per-centroid match + calibrated per-device τ; integration with F3 challenge phrase (doc 03 §5) and F1 candidate state | FNMR < 5 % measured over ≥ 20 real sessions across ≥ 3 cameras; lookalike eviction test (device users must never pass as admin) |
| **F4** | Camera-permission + privacy-mode + no-camera failures UX; memory hooks (session pool unload ≤ 20 s) | device-failure taxonomy tests; RAM delta ≤ 120 MB target while active; no leak while idle |
| **F5** | Live end-to-end: real house test on stranger laptop (position: wake → greet with challenge → face → session open → exit) | the standing "not run live" line removed only after this demo (master plan P5) |

## 8. Open questions (not verified)

1. **SFace vs buffalo_l on the real 1:1 margin** — the ten-percent lookalike/sibling question (doc 02 §6.1) resolves only with F3's measured per-device τ; if margins thin, revisit the commercial-license path (doc 02 table).
2. **rPPG in dim rooms** (dorms/hotel rooms after midnight): SNR threshold may flip into "ask-again" loops — fallback ordering (drop rPPG, raise MiniFASNet+lip-sync margins) must be decided by measurement, not vibes.
3. **Lip-sync alignment robustness** under compressed STT caption timing (word-level pipeline exists, but challenge phrases with digits emit short tokens — need a viseme map for digit words; added to E3 task list).
4. **MiniFASNet ONNX export quality** — repo serves PyTorch; ONNX conversion effort/accuracy delta untested (LocalAI wraps it via ONNX directly per research, but NEXUS must re-verify at import).
5. **Occlusion adversarial timing attacks** (attacker waits for the admin mid-session → camera open pre-session)? — F3 re-challenge policy inside a *long* session (re-verify after N minutes of active elevated mode) — owner decision pending; default proposal: re-challenge every 15 min or on new high-risk action.

## 9. Sources (checked 2026-10-19)

- OpenCV docs `DNN-based Face Detection And Recognition` (FaceDetectorYN / FaceRecognizerSF accuracy tables incl. SFace LFW 99.60 % τcosine 0.363; detector 338 KB; recognizer 36.9 MB)
- OpenCV Zoo model cards: `face_recognition_sface` (architecture, loss, demo, quantized variants 99.32-99.42 % tool accuracy; Apache-2.0) and `face_detection_yunet` (WIDER Face val numbers)
- InsightFace licensing (code MIT; *models non-commercial*, 2025-11-24 note) — buffalo_l rows and accuracy claims class
- minivision-ai/Silent-Face-Anti-Spoofing: MiniFASNet V2 (scale 2.7) + V1SE (scale 4.0), 80×80, 3-logit ensemble (Apache-2.0; reference impl details from LocalAI `Antispoofer` port)
- Liu, Jourabloo, Liu, "Learning Deep Models for Face Anti-Spoofing: Binary or Auxiliary Supervision," CVPR 2018 — auxiliary depth+rPPG supervision; cross-dataset (SiW→Oulu) ACER 10.0-14.1 %
- Chen, Yang, Li, Wang, Kwong, "Camera Invariant Feature Learning for Generalized Face Anti-spoofing," IEEE (arXiv:2101.10075) — camera as dominant domain gap
- CelebA-Spoof (Zhang et al., ECCV 2020, arXiv:2007.12342) + Challenge 2020 (arXiv:2102.12642) — scale/saturation statistics; benchmark protocols
- CASIA-SURF "A Dataset and Benchmark for Large-Scale Multi-Modal Face Anti-Spoofing" (CVPR 2019)
- Azure Face Liveness docs — passive/passive-active; iBeta L1+L2, 0 % penetration; ISO 30107-3; privacy posture (no image/video retention)
- NIST SP 800-63B-4 §3.2.3 — face verifier SHALL implement PAD; FMR/FNMR/IAPAR bars (doc 04)
