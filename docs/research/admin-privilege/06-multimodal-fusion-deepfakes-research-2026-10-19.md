# Multimodal Fusion & the Deepfake Landscape — Evidence and the NEXUS Decision Logic

**Date:** 2026-10-19. Status: RESEARCH (nothing built). Companion docs: [01 overview](01-admin-privilege-any-device-research-2026-10-19.md) · [03 voice F1](03-voice-identity-firewall-f1-research-2026-10-19.md) · [05 face F2](05-face-verification-firewall-f2-research-2026-10-19.md).

**Purpose:** (1) document the audio-visual deepfake research that defines the modern threat NEXUS's Administrator gate faces; (2) define exactly *how* the independent verdicts (F1 voice candidate, F2 face/PAD verdict, F3 freshness) combine into one session decision — with failure-mode coverage for every component.

---

## 1. Why fusion is mandatory (complementarity argument)

Each layer's residual risk is disjoint:

| Weak spot of a layer | Killed by another layer |
|---|---|
| Voice CM misses a novel TTS clone (ASVspoof 5: YourTTS-derived attack A28 EER 16.1 %) | F2 face: clone speaks *off-camera* or with the wrong face; camera can't be cloned by audio |
| Photo/video of admin passes face-ID | PAD ensemble (texture/pulse/parallax) + F3 freshness (the video never answers fresh digits coherently while lips move) |
| Replay of yesterday's live recording passes the CM (recordings of *real* admins are genuinely live-clean) | F3: digits do not exist yet when the recording was made |
| In-room imposter speaks the wake word + digits | F1 speaker similarity (identity mismatch) + F2 (face is not the admin) |
| Lookalike/sibling passes SFace at a thin margin | F1 voice (different voice embedding) + F3 (still separate biometric streams) + tightened policy margins |
| Screen/email text containing "delete X" triggers an action | Irrelevant to identity new — provenance gate (`origin = Derived` prose in doc 07 §5) means no destructive tool may run regardless of who a *model* heard |

That disjointness is precisely Anthropic/prompt-injection doctrine transposed to biometrics: no single wall is asked to be unbreakable; walls are arranged so walls fail *independently* (doc 01 §0 "defeat them all, simultaneously, in a stranger's room", is the practical bar).

---

## 2. The audio-visual deepfake literature NEXUS references

### 2.1 Identity-consistency checking (the POI-Forensics line)

Cozzolino et al., *"Audio-Visual Person-of-Interest DeepFake Detection"* (CVPR Workshops 2023; code public, grip-unina/poi-forensics):
- trains contrastive identity embeddings over **real talking-face video only**, so the detector learns each person's audio+visual manifold *without any fake data*
- flags when manipulations push audio/video identity representations out of the person's manifold
- notable SOTA cross-dataset and low-quality-video robustness
- *Relevance:* conceptually this is what NEXUS's F1+F2 jointly implement with classical components — identity consistency is checked, not forgery artifact detection. Worth tracking as a future ONNX candidate for an explicit AV-consistency score.

### 2.2 Cross-modal correspondence learning

- **AVFF** (Oorloff et al., 2024, arXiv:2406.02951): two-stage self-supervised cross-modal feature fusion; **98.6 % acc / 99.1 % AUC on FakeAVCeleb** (+14.9/+9.9 points over prior audio-visual SOTA). Learns phoneme↔viseme etc. dependencies from real videos.
- **SyncNet-lineage** lip-sync correspondence: the mouth-region motion track must correlate with the phoneme track of concurrent audio — the cheap, interpretable version used in NEXUS F2's lip-sync signal (only on challenge phrases, where the transcript is known a priori).
- **AV-Deepfake1M** (Cai et al., ACM MM 2024): > 1 M videos, > 2 k subjects, LLM-driven per-segment manipulations (audio-only, video-only, audio-video); benchmark shows **significant performance drop** of existing detectors — the current worst-case benchmark family; the reason NEXUS assumes attacks *will* exist and plans for layered failure, not detector excellence.
- **MFMS** (ACM MM 2024, AV-Deepfake1M competition winner line): modality-fused and modality-specific branches for detection+localization — evidence that fusion (both branches matter) is the winning recipe.

### 2.3 Multimodal speaker frameworks

- **3D-Speaker-Toolkit** (modelscope, arXiv:2403.19971): acoustic + semantic + visual modules fused (visual diarization with RFB-Net tracks, TalkNet-style active-speaker detection, CurricularFace embeddings; E2CP constraint propagation); 10k-speaker dataset. Documented insight: *real rooms break audio-only speaker systems* (background noise, reverberation, overlapping speech) — the same insight that motivates F2 in NEXUS.

---

## 3. What fusion means for NEXUS (decision semantics)

The output of the ceremony is not "the score" but a **verdict object** consumed by F4:

```
AdminVerdict {
  f1: { speaker_cos, device_centroid_used, cm_pass, cm_margin, calibration_src },
  f2: { pad: { minifas_p, rppg_snr, lipsync_corr, motion_ok }, face_cos_max, tau_device },
  f3: { challenge_ok, retries_used, freshness_ts },
  session: { token_issued?, scope, ttl, device_id, audit_id },
  decision: Granted | AskAgain(reason) | Refuse(reason) | Unavailable(reason)
}
```

Decision policies evaluated in v1 (with the chosen default):

| Policy | Rule | Pros | Cons |
|---|---|---|---|
| **Strict cascade (v1 default)** | F1 candidate + CM pass → F2 PAD-pass + face-match → F3 digits → grant; any fail = stateful retry/divert | interpretable; each layer's margin auditable; matches failure UX | slower feeling (sequential ceremony ~4 s) |
| Weighted risk score | z-normalized fused score ≥ τ | compact; single threshold | un-auditable margins; calibration burden; doc 140 culture prefers explicit per-gate semantics |
| Two-of-three (M-of-N) | any two strong factors grant | friendlier FNMR | weakens the freshness/face guarantee the owner demanded — rejected |

Additional v1 invariants:
1. **PAD precedes identity** (doc 05 §3) — no identity score computed on PAD-failing frames.
2. **Freshness binds the session** — challenge digits and face must be contemporaneous (F2 verdict window within ±3 s of the spoken challenge); stale-face-fresh-digits (video overlay attacks) die on recall via lip-sync + rPPG window agreement.
3. **Ambiguity → lower privilege** — if margins are close (e.g. 0.41 cosine between accept/refuse τs), the verdict degrades to *user mode*, never to *admin mode* (doc 03 §8.6 tie-break rule).
4. **Every verdict object lands in the audit JSONL**, redacted (scores only, no embeddings, no image/audio bytes) — doc 07 §6.
5. **Office visitor asymmetry** — device-user template exists only to *evict lookalikes*; its fail is not an admin-verdict refinement (kept out of the grant path deliberately).

## 4. Re-verification inside a session (the "long session" hole)

A granted session should not last all day on one successful ceremony (occlusion/holdout attacks, doc 05 §8.5). Default policy proposal (owner to confirm in doc 08 §5):
- re-challenge every 15 min of *privileged* activity (cheap, ≤ 3 s total, same ceremony),
- and immediately on any change of "risk class" upward (e.g., admin runs something in the `Destructive`-adjacent categories; doc 140's Risk classes apply verbatim since semantic reuse is a stated goal),
- session hard-expires at 30 min of idle admin activity.

## 5. Open questions (not verified)

1. **Real adversarial testing of F2 against consumer attack tools** (deepfake-livestream generators) — corpus to be built in P5/master-plan red-team; AV-Deepfake1M-style segment-level attacks remain the most concerning class for the lip-sync signal.
2. **Weighted fusion upgrade** — whether calibrated score fusion beats strict cascade *for NEXUS's room distribution* is an empirical question; until measured, cascade wins on auditability grounds.
3. **POI-Forensics ONNX adaptation** for an explicit AV-consistency gate — model sizes/architectures need profiling before considered as F2's fourth signal.
4. **Audio-visual sync drift** on Bluetooth headsets (mic/camera path latency skew) — lip-sync window must be generous under +80 ms path skews; measure first (E3 task).

## 6. Sources (checked 2026-10-19)

- Cozzolino, Pianese, Nießner, Verdoliva. *Audio-Visual Person-of-Interest DeepFake Detection* (CVPRW 2023) — POI-Forensics; identity-consistency method; cross-dataset SOTA claims; code public
- Oorloff et al. *AVFF: Audio-Visual Feature Fusion for Video Deepfake Detection* (arXiv:2406.02951, 2024) — 98.6 % / 99.1 % AUC FakeAVCeleb
- Cai et al. *AV-Deepfake1M: A Large-Scale LLM-Driven Audio-Visual Deepfake Dataset* (ACM MM 2024) — dataset + benchmark; existing detectors degrade significantly
- Zhang et al. *MFMS: Modality-Fused and Modality-Specific Features…* (ACM MM 2024) — competition-winning fusion design
- Chen et al. *3D-Speaker-Toolkit* (arXiv:2403.19971) — multimodal (acoustic/semantic/visual) diarization; real-environment audio-only weaknesses
- SyncNet lineage (Chung & Zisserman, 2016-2017 era) — lip-sync correspondence reference for the F2 lip-sync signal
- ASVspoof 5 results (doc 03 §4.1) — the voice-side attack landscape figures referenced above
- Internal: doc 140 risk classes + provenance gate; feature 88 session-token shapes; caption word-level timing work (caption-stack docs) enabling the lip-sync transcript join
