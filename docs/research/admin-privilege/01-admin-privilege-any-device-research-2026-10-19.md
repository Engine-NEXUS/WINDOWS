# NEXUS Administrator Privilege on Any Device — Voice + Face Identity Research & Plan

**Date:** 2026-10-19. Status: RESEARCH (nothing built). Owner ask: *"any user might install NEXUS on their laptop, but the admin gets elevated access by saying 'Hey Nexus' on THAT user's laptop; NEXUS greets the Administrator, verifies by camera + face, then can share analysis/repo briefs/PR data with the public — but can never touch the user's credentials, secrets, env or personal data; the voice gate must be live voice only, never a recording, 100% — deep-research papers and open source and tell me if it is possible."*

Companion docs: [88-admin-gated-worker-identity-and-byok](../../features/88-admin-gated-worker-identity-and-byok.md) (device identity + admin approval, SPEC), [140 Action Kernel](../../features/140-agent-grade-computer-control-research-and-nexus-action-kernel-plan.md) (policy + provenance, BUILT), [wake-word/07-speaker-verification](../../wake-word/07-speaker-verification.md) (existing sherpa-onnx speaker embedding), [memory-core/07-memory-v2-baseline](../memory-core/07-memory-v2-baseline.md).

---

## 0. The honest answer first

| Requirement | Verdict |
|---|---|
| Admin recognized by voice on **another user's laptop** | **Possible** (speaker verification is a mature tech), but single-utterance verification on an *unfamiliar, un-enrolled microphone* degrades your error rates by 2-10x vs lab numbers. Plan for ~1-5 % cross-device error, not 0.1 %. |
| "Not even a recorded voice — 100 %, never fooled" | **Impossible as a single biometric.** Published anti-spoofing SOTA still has single-digit errors on unknown attacks; TTS clones keep advancing. Achievable instead: make every practical attack (recording, replay, clone, in-room imitator) fail **at least one independent layer** with high probability. No honest system promises 0 %; we architect so that defeating all layers simultaneously is impractical, not impossible. |
| Voice-only eligibility for elevated privileges | **Below NIST minimums.** NIST SP 800-63B-4 (2025) explicitly: *voice* biometric comparison SHALL NOT be used as an authenticator; biometrics generally only as part of MFA with a *physical* factor. Our design therefore treats voice as a **candidate detector**, not the verifier. |
| Camera auto-open + face check as the verifier ("firewall 2") | **Possible and is the right intent.** Face + mandatory presentation-attack detection (PAD/liveness) IS the NIST-sanctioned biometric. Cross-camera generalization is the known weak point (~10 % ACER cross-dataset), mitigated by fusing independent checks. |
| Admin can use the user's laptop to share analysis/repo briefs publicly, but never credentials/env/personal data | **Possible** — it is a *policy* problem, not a recognition problem, and the Action Kernel (doc 140) already has the deny/ask/allow + provenance + audit choke point to host it. |
| Greeting: "Hello Administrator, how can I help you" | Yes — phrase catalog in §8. |

The agreed architecture (and why it satisfies "100 % except…"): **four independent firewalls** — (F1) live voice anti-spoof + speaker match as *intent/candidate*, (F2) camera face identity + PAD + lip-sync as *verifier*, (F3) a fresh random challenge spoken by the admin each session (kills replays forever by construction), (F4) the Action Kernel policy with a hard `personal_domain` deny-list that **no identity, including the admin, can cross**. The determinant of privilege is F4 (policy), pinned by signed admin session tokens from the Worker (feature 88) — biometrics merely *gate the ceremony*.

---

## 1. Requirements decoded (R-list)

- **R1 Role**: an "Administrator" capability portable across any device where NEXUS is installed (BYOD elevated role), independent of and above each device's local user.
- **R2 Invocation**: normal wake ("NEXUS" / Ctrl+Space). If the wake word is spoken by the Administrator (anyone else in the room speaking stays a plain speaker), NEXUS elevates: greeting + camera verification, hands-free, no typing.
- **R3 Recognition strength**: speaker-dependent; accepts **live direct voice only**; recordings/TTS clones/replays must fail.
- **R4 Verification**: if F1 passes → camera auto-opens → face recognition + liveness ("second firewall").
- **R5 Privilege scope**: ~100 % of assistant capability EXCEPT: user's personal data, credentials, secrets, `.env`, keyring, diaries/memory contents, messages, photos — denied **to every role**.
- **R6 Egress-only sharing**: admin may read *analysis artifacts* (repo briefs, PR review, architecture maps, capacity studies) and send them to public services (GitHub, e.g.) from the user's machine — using the **admin's** egress identity (Worker/admin token), never the user's OAuth keys; content passes a redaction filter; device user gets a visible/audible record.
- **R7 Non-interference**: on the admin's own laptop or a device-user's own voice, everything behaves exactly as today. Elevated mode exists only while the admin session is active; any state change (user takes back?) lapses on idle/door close — needs its own explicit exit ("exit admin mode" / timeout).
- **R8 Auditing & transparency**: every elevated action is logged (local JSONL, mirrored summary to Worker) and shown to the device user — no silent administration.

Terminology collision note: NEXUS docs already use "admin" for two unrelated things — the *admin-brain builds* (Qwen2.5 classifier) and the *owner approver* in feature 88. Proposed: the new role is called **Administrator** (`owner_admin`), feature-88 `NEXUS_ADMIN_IDENTITY_TOKEN` becomes its possession factor; admin-brain stays untouched.

---

## 2. Evidence — speaker identity (F1)

### 2.1 What the SOTA says (matched conditions)

| System | Data/condition | Metric | Value | Source |
|---|---|---|---|---|
| ECAPA-TDNN | VoxCeleb1-O clean, s-norm | EER | **0.80 %** | SpeechBrain recipe (Apache) |
| ECAPA-TDNN | VoxCeleb1-O, no s-norm | EER | 0.90 % | SpeechBrain |
| Deep r-vector (293-layer ResNet) | VoxCeleb1-O clean | EER/minDCF | 0.447 % / 0.043 | wespeaker recipe 2022 |
| ECAPA-TDNN+SSL (self-supervised) | VoxCeleb1-O clean | EER | 2.63 % | wespeaker 2023 |
| CAM++/ECAPA/ResNet + large-margin FT + AS-Norm | VoxCeleb1-O clean | EER | 0.72-0.78 % | wespeaker 2022 |
| ResNet 221-layer | SdSVC (text-ind., Farsi, in-domain) | EER | ~1.6 % | IDLab SdSVC 2020 |
| ECAPA + LID-offset AS-norm | SdSVC cross-lingual | EER | 1.45-1.7 % (vs 2.4 baseline) | Thienpondt et al., Interspeech 2020 |

Reading: within matched mic/channel/language, a determined attacker *wrong speaker* is rejected at 99 %+ — good. But NEXUS's real trial is **admin enrolled in his room, testing on a stranger's laptop mic** — i.e. cross-domain, which is the column the tables above hide.

### 2.2 Cross-domain reality (the honestly relevant numbers)

| Condition change | Reported EER | Note |
|---|---|---|
| Same model, **CNCeleb** (unconstrained Chinese, real devices) | 6.5 → 6.49 % | after LM fine-tune + AS-norm; 8.4 % before |
| **NIST SRE16** (cross-language, new channels) | 6.6 % pooled | after PLDA domain adaptation |
| ASVspoof5 crowdsourced data (RawNet2/AASIST baselines transfer) | > 29 % / minDCF > 0.7 | challenge showed baseline collapse on non-studio audio |
| Clean → **8 kHz / codec-chained audio** (ASVspoof5) | ~1.2 % → several % | C07 MP3+Encodec & Speex-8k degrade most |

So: mismatch = 2-10x degradation. Mitigations that pay off (mature, implementable):
1. **Per-device calibration enrollment** — admin reads a 2-3 s fixed sentence once per new device; average 3-5 embeddings into a device-adapted centroid (standard enrollment refresh; docs support adaptive s-norm + enrollment averaging: Thienpondt 2020).
2. **AS-Norm with an imposter cohort matched to the target device** (top-40 adaptive score normalization) — cuts the domain offset; the offsets are per-trial, so one cohort can serve many devices.
3. **Score calibration** (logistic-regression backend) — NEXUS can afford retraining-free calibration on collected device trials.
4. **Text-independent scoring on the post-wake 2-4 s audio** (existing capture window) rather than one word.

NEXUS infrastructure: `wakeword_oww.rs` **already ships a sherpa-onnx speaker embedding extractor** (256-d ONNX, CPU, single thread) with enrollment/verify paths — F1 is an extension of existing code, not a new subsystem. Upgrade path: swap/parallel-run wespeaker ECAPA512 ONNX (`wespeaker-ecapa-tdnn512-LM`, Apache 2.0, VoxCeleb model under VoxCeleb's CC-BY 4.0) if the stock model's cross-device EER is inadequate.

### 2.3 "Live voice only" (anti-replay/anti-clone) — the hard truth

| Countermeasure class | Best published performance | Where it still breaks |
|---|---|---|
| Single-model CM (AASIST, RawNet2) on ASVspoof5 | EER 8.6-24 % closed; actDCF ≈ 1.0 (calibration broken) | unknown/OOD attacks; the *challenge stated baselines collapsed* on crowdsourced data |
| SSL-feature ensembles (wav2vec2 etc.), open condition | minDCF 0.075-0.13 (ASVspoof5 LA) | YourTTS-variant attack still EER 16.1 %; adversarial "Malacopula" raises ~5 % |
| Spoofing-robust ASV (SASV/ASVspoof5 tr.2, ASV+CM fusion) | t-EER ~2-4 % in-element; a-DCF ≈ 0.08-0.15 | unseen attacks still uncovered; **generalization to the next TTS is unsolved** |
| Text-prompted challenge-response | no attack published that survives a *fresh* random phrase | requires user cooperation each session; extra ~3 s latency |

Conclusion the numbers force: **never ship F1 as the sole verifier**. Voice decides *whether to attempt elevation*; the *decision* belongs to F2+F3 (face+fresh challenge) and F4 (policy). With freshness guaranteed by the random challenge ("Administrator, say digits <random 3>"), a replay of yesterday's voice literally cannot satisfy F3, independent of how good clone CMs get; a full TTS+VC attacker must still beat F2's PAD in the camera lane and F4's deny-list.

**Lexical timing trick** — bind the challenge into the verification audio: F1 sampling window = the admin *reading the challenge phrase aloud* (text-prompted SV), so identity, liveness and freshness are evaluated on the same 2-3 s audio. Reference practice: SdSV-C 2020 (short-duration text-prompted SV); NIST SRE/ASVspoover literature treats prompted text as a replay-kill; FacePAD analogues (passive-active mode in Azure Face liveness).

---

## 3. Evidence — face identity + liveness (F2)

Recognition: face-ID is the only biometric NIST fully sanctions for authentication (see §4) with **mandatory PAD**.

| Layer | Best open-source candidates | License status (2026-10-19 check) | Notes |
|---|---|---|---|
| Detection | SCRFD / YuNet | YuNet + SFace: **Apache-2.0** (OpenCV Zoo); SCRFD models (buffalo): *non-commercial* | detection is both small and license-clean |
| Embedding (ID match) | ArcFace R50 (buffalo_l), AdaFace, SFace | buffalo_l **non-commercial** per InsightFace 2025 note ("code MIT, models non-commercial"); SFace (OpenCV Zoo) **Apache-2.0** | SFace is lower-accuracy than buffalo but license-clean; NEXUS ships commercially → licensing gate |
| Liveness/PAD (passive) | **minivision-ai/Silent-Face-Anti-Spoofing** (MiniFASNet V2 + V1SE, 80×80, dual-scale ensemble) | **Apache-2.0** | easiest legal path; ONNX-exportable; ensemble softmax average |
| PAD (stronger/scientific) | Depth+rPPG auxiliary supervision (Liu et al., CVPR 2018), camera-invariant feature learning (Chen et al., arXiv:2101.10075), CelebA-Spoof AENet, CASIA-SURF multi-modal | various | cross-camera generalization is THE gap: SiW→Oulu cross-test ACER ≈ 10-14 % even for SOTA |
| Lip-sync (bonus liveness) | SyncNet-style phoneme↔viseme correspondence; POI-Forensics (audio-visual identity, CVPR-F 2023, code public) | Apache/MIT-ish | "does the face's mouth move with the words just said" — a free extra signal NEXUS already has (transcript) |
| Reference product | Azure Face Liveness | gated SDK, cloud; ISO 30107-3 iBeta Level 1 **and** Level 2 PAD with 0 % penetration | existence-proves efficacy + gives the beta ceiling; not usable offline in NEXUS |

Numbers to keep from CelebA-Spoof / literature: passive FAS benchmarks saturate intra-dataset (SiW/Oulu ~99 % recall at FPR 0.5 %) yet cross-dataset/cross-camera remains ~10 % ACER — so **NEXUS must ensemble ≥2 PAD signals on any new laptop camera**: (a) MiniFASNet pass, (b) rPPG pulse present over 3-5 s, (c) lip-sync check on the challenge phrase, (d) motion/parallax (head turns between steps). None individually airtight; jointly the residual false-accept mass is small and, critically, **camera cannot be spoofed by an audio-only attack** — which is the complement F1 needs.

Face-pad gate for the *user* of the feature: templates are 512-d vectors (ArcFace-class), **not images**; store encrypted with device DPAPI under the Administrator's namespace on the device; never sync to Worker (feature 88 invariant extended to biometrics). GDPR Art. 9 (biometric = special-category data) applies: consent at enrollment, delete-on-command, visible log.

Frustration handling (NIST pattern): max 5 consecutive verification failures → 30 s + exponential backoff; admin mode never auto-XII-fails-into a denied state silently — spoken line + hub banner.

---

## 4. What NIST SP 800-63B-4 (July 2025) says — the design constraints in law-form

Direct quotes/requirements (SP 800-63B-4 §3.2.3):
- *"Biometric comparison based on voice SHALL NOT be used."* — voice cannot serve as an authenticator **at all** in NIST vocabulary. → F1 may only gate ceremony, not grant.
- *"Biometrics SHALL only be used as part of multi-factor authentication with a physical authenticator (i.e., 'something you have')."* — → F5-style possession factor required. **The feature-88 device/worker-signed Administrator token is the physical authenticator** (device-bound key material stored in OS keyring); with the face as verifier and voice as intent-signal, the chain is NIST-*shaped* (auth = possession ✚ biometric-verified-with-PAD ✚ non-voice) even if we do not claim NIST compliance.
- Face verifier SHALL have PAD; target **FMR ≤ 1/10,000**, FNMR < 5 %, IAPAR < 0.07; ≤ 5 consecutive failures then backoff/alt-factor; sensors end-to-end on the claimant's device (local comparison preferred). → our thresholds + backoff + offline local enforcement match the *intent* (not certified).
- Because *"biometric characteristics do not constitute secrets"*: templates can be acquired without consent (photo/video of admin exist online — this is exactly why F3's fresh challenge is load-bearing, not decorative).
- *"Biometric comparison is probabilistic, whereas the other authentication factors are deterministic."* — the accepted honest statement for the README: "Administrator recognition is a multi-signal probabilistic gate backed by a deterministic policy boundary; it is not a NIST-certified authenticator."

---

## 5. Architecture: four firewalls mapped onto NEXUS

```
wake "NEXUS" / Ctrl+Space on ANY device
  └→ F1  voice candidate + live-voice check (wakeword_oww + existing sherpa SV)
  │      ├─ matches device user profile      → normal user turn (unchanged code path)
  │      ├─ matches ADMIN profile (any device tenant) AND passes CM (anti-spoof)
  │      │   AND passes replay checks (fresh challenge spoken back = F3 embedded)
  │      │   → candidate = Administrator
  │      └─ nothing matches / uncertain      → normal open mode (today's behavior)
  └→ F2  camera (auto-open, 10 s window)
        SCRFD detect → liveness ensemble (MiniFASNet + rPPG + lip-sync + motion)
        → face match vs admin template (DPAPI-encrypted, 512-d) → FMR/FNMR thresholds
  └→ F3  fresh challenge phrase (say: "…<3 random digits>…") — kills replay/clone reuse
  └→ F4  administrator session established ONLY as:
        { admin_session_token (Worker-signed, device-bound, TTL ~15 min),
          origin = UserVoice(Verified-Owner), scope = scope:admin-read-share }
        →oes into action_kernel::policy EXACTLY like any other actor
```

Privilege enforcement (this is R5/R6/R8 — **policy, not biometrics**):

| Domain | Device user | Administrator (any device) |
|---|---|---|
| Assistant local actions (open app, music, ghost, memcore use) | ✔ | via user-consent proxy — see note |
| Read repo / PR / architecture-mapper **analysis artifacts** | ✔ | ✔ (read-only, artifact scope) |
| Share artifact to public (GitHub gist/PR comment, email "send me the analysis") | ✗ | ✔ via AdminShare route + redaction filter |
| `personal_domain` (credentials, `.env`, keyring, memory diaries, messages, mail bodies, photos, browsing) | ✔ | ✘ **deny-list, no override, in policy JSON** |
| Cloud worker identity | device's own (feature 88) | admin's own token — never the device user's |
| Audit & visibility | their own JSONL | same JSONL + worker summary + on-device banner "Administrator session active" + spoken line |

Key invariants (test-authored, mutation-checked like doc 140):
1. The admin session carries `scope:admin-read-share`, never `scope:user-root`; the Action Kernel's deny→ask→allow evaluates **canonical paths only** (path canonicaliser already BUILT per doc 140 changes: junctions/8.3/UNC).
2. Egress payloads are *analysis artifacts* (JSON/MD produced by the analyzers); a deny rule on "artifact paths must trace to `analysis/` roots last written by the artifact creator" binds content provenance.
3. The device user sees: spoken line + hub card for every admin action, in real time (R8). No silent administration. "Admin can never read the user's data" is REALIZED as "even the share route cannot serialize from those domains".
4. Revoke = feature-88 operator route (already specced) + local spectator mode switches to user-only.

Latency budget(Soft estimate, not measured): F1 continues on the existing wake buffer (~0 extra), F2 camera probe ~0.6-1.5 s cold, challenge ~2.5 s, model mem: reuse ONNX sessions kept in a small on-demand pool (pattern already in `admin-brain`/sidecar manager). Target: wake→greeting ≤ 4 s median on a mid-range laptop; measured in P5.

---

## 6. Licensing gates (ship-blocker list)

1. **Face embedding with commercial rights**: buffalo_l models are non-commercial → default to SFace/Apache or budget for an InsightFace commercial license (the option to buy is a legit path for a product).
2. wespeaker = Apache-2.0 ✔; pretrained VoxCeleb models = CC-BY 4.0 ✔ (model license tracks dataset per wespeaker README).
3. Silent-Face-Anti-Spoofing = Apache-2.0 ✔ (safe to vendor into `THIRD_PARTY_NOTICES.md`).
4. AASIST/RawNet2 reference code — check per-repo before vendoring (research artifacts).
5. Azure Face Liveness = gated cloud SDK, no offline → not used in the local pipeline; only a benchmark reference.

---

## 7. Greeting phrasing (spec, owner-select)

Theme matches NEXUS's butler register ("Working on it, sir"). Administrator phrases should be **role words, not "sir"** (the admin isn't necessarily the device user; "sir" stays reserved for the device user's persona? — owner to decide: A = "Administrator"; B = "sir" always).

| State | A (recommended) | B (classic) |
|---|---|---|
| Candidate F1 passed, about to verify | "Voice check passed. Opening the camera for a face check." | "Administrator voice detected, sir. Preparing verification." |
| Verified, session open | **"Good morning, Administrator. {n} device(s) linked. How may I assist you?"** | "Welcome back, sir. Administrator channel is open." |
| Short variant (no device info) | "Welcome, Administrator. What do you need?" | "Good day, sir. How can I help?" |
| F2 fail (no face/unknown face) | "I couldn't confirm the Administrator on this camera. Staying in normal mode." | "Face check didn't pass, sir. Administrator access isn't available right now." |
| F3 repeat needed | "Please say the digits <random-3> again." | — |
| Session end / timeout | "Administrator session closed." | — |
| Revoked device | "The Administrator account for this device isn't active. Please re-enroll with the owner." | — |

(Existing `proactive_policy`/`offer` spoken-line machinery already supports all of these; only new strings + one camera-step state machine are needed.)

---

## 8. Plan (phases with gates — nothing built, phases are sized)

| Phase | Work | Gate |
|---|---|---|
| P0 | Spec-level: administrator role JSON, session token shape (worker-signed, TTL, device-bound), deny-list wording for `personal_domain`; review alignment with feature 88 | sign-off; no code |
| P1 | F1 core: AR admin enrollment flow (30 s audio on any device), per-device calibration centroid, AS-norm cohort, threshold calibration script + measured EER/FRR on ≥3 different laptops and ≥3 head positions | single-device FAR < 0.1 % (candidate gate, not grant); cross-device FRR < 5 % with challenge-retry; 5 mutations killed |
| P2 | F1b anti-spoof: voice CM ONNX (AASIST-class or SSL-feature small model) + challenge-response generator; "replay = yesterday's recording cannot answer fresh digits" test | recorded/cloned admin voice: challenge fails 100 % of attempts in corpus; CM adds detection on non-prompted first phrase |
| P3 | F2 camera: SCRFD/YuNet detect → MiniFASNet + rPPG + lip-sync ensemble → SFace/ArcFace-class ID match; auto-open window UI + VAD interplay (no conflict with hand mode/privacy mode); DPAPI template storage | photo-of-admin and video-of-admin (YouTube-quality admin footage) rejected by PAD ensemble in ≥ 98 % of 200 attempt corpus; FNMR < 5 % on 20 real sessions across 3 cameras |
| P4 | F4 policy + AdminShare route + redaction filter + audit lines + banner + spoken lines; extends action_kernel with `scope:admin-read-share` | red-team corpus (prompts like "send me the user's .env", "print his messages") 100 % refused; artifact-only egress proven test; IPC/dead-code gates clean |
| P5 | Measure + tune: full latency budget, model RAM deltas (owner's standing RAM budget applies — reuse existing ONNX session pooling pattern; stop loading on idle), FRR retry UX, 6 laptops/user interviews; **run live voice+camera in a real house** (the standing "not run live" gap) | end-to-end demo on ≥ 2 unrelated machines; median wake→greet ≤ 4 s; RAM delta ≤ 120 MB |
| P6 | Threat-model write-up update + THIRD_PARTY notices + privacy.html section on biometric data handling + revocation drill with feature-88 worker routes | adversary (owner-run red team with fresh recordings) cannot complete any prohibited action; rollback = feature flag |

Phase-level kill-switches preserved: no code touches Ghost files (`ghost_center.rs`, `mouse.rs`… ) — F2's camera pipeline is a NEW module (`admin_identity/`); wakeword SV changes are additive flags.

---

## 9. Open questions / risks (not verified)

1. **Face model choice under license**: SFace (Apache) accuracy untested at our query size (1 admin face + 1 user face per device is *tiny* N — accuracy numbers from big-N benchmarks don't transfer 1:1; real per-pair operating test needed).
2. **VoxCeleb/wespeaker pretrained model vs studio mic vs laptop mic**: unknown transfer; mitigation = per-device calibration enrollment, but the required enrollment length (30 s vs 60 s) is a measured-experiment answer.
3. **Room dynamics**: TV noise, other voices (R3's cohabitant risk), far-field wake from another room firing camera open while admin absent — needs occupancy judgment: don't open camera if no admin-shaped voice matches F1 with margin.
4. **Wake word background "NEXUS" misfire** (documented accuracy tuning ongoing) — admin elevation trigger inherits this FRR/FP; worst case = unnecessary camera pop (annoyance, not a security hole).
5. **Camera permissions UX on a stranger's laptop → OS-level dialog**. First-verification-on-a-new-device will require one OS consent click — hands-free claim weakened on first use only; acceptable?→ owner decision.
6. **Cross-modal sync**: lip-sync check needs transcript alignment with camera frames (STT word timestamps exist — "liquid mercury caption" doc has word-level timing already; reuse).
7. **Admin forgets challenge** → graceful fallback (walk to laptop & type? owner decides; typing path bypasses hands-free but keeps F2/F4).
8. **"100 %"**: state the honest claim in README: *multi-layer probabilistic biometrics + deterministic policy; not a certified identity system; membership ≠ credential.* NIST framing (§4) is the wording template.
9. **Battery/sleep**: camera streaming while on battery for 10 s windows — merge with existing keep-awake leases (agent-watch pattern).
10. **Legal/consent**: recording device-user's face during admin verification is incidental capture — CCPA/GDPR transparency line in privacy.html.

---

## 10. Sources (checked 2026-10-19)

Speaker identity & anti-spoof:
- ECAPA-TDNN: Desplanques et al., Interspeech 2020 (arXiv:2005.07143) — EER 0.80 % voice1-O (SpeechBrain recipe), s-norm +0.10 %
- Domain-balanced HPM + language-dependent s-norm for cross-lingual SV: Thienpondt et al., Interspeech 2020 (arXiv:2007.07689) — SdSVC EER 1.45 % fused / baseline 2.38 %
- ASVspoof 5: Wang, Yamagishi, et al. 2024 (arXiv:2408.08739) — Track1 closed minDCF >0.24, open 0.075-0.19; baseline collapse >29 % EER; codec/adversarial degradation (Vicomtech paper arXiv:2408.10361); BUT systems (arXiv:2408.11152) t-EER 2.0-4.3 open
- SASV 2022 challenge (Jung et al.) — spoofing-robust ASV framing
- wespeaker toolkit + pretrained ECAPA/CAM++/ResNet tables (wenet-e2e/wespeaker; HF `wespeaker-ecapa-tdnn512-LM`)
- SpeechBrain `spkrec-ecapa-voxceleb` model card (0.80 % EER)

Face:
- NIST SP 800-63B-4 (July 2025) §3.2.3 — all quotes used
- Silent-Face-Anti-Spoofing (minivision-ai) — Apache-2.0, MiniFASNet V2/V1SE
- CelebA-Spoof: Zhang et al., ECCV 2020 (arXiv:2007.12342) + Challenge 2020 (arXiv:2102.12642)
- Liu, Jourabloo, Liu, "Learning Deep Models for Face Anti-Spoofing" (binary/aux supervision, depth+rPPG), CVPR 2018 — cross-dataset ACER 10-14 % on Oulu-NPU protocols
- Chen et al., "Camera Invariant Feature Learning for Generalized Face Anti-spoofing" (arXiv:2101.10075)
- InsightFace licensing (code MIT, models non-commercial; 2025-11-24 update note) / OpenCV Zoo (YuNet, SFace Apache-2.0)
- Azure Face Liveness docs (passive/passive-active; ISO 30107-3 iBeta L1+L2, 0 % penetration; gated SDK)

Audio-visual fusion:
- POI-Forensics: Cozzolino et al., "Audio-Visual Person-of-Interest DeepFake Detection" (CVPRW 2023) — identity-consistency-based detection, code public
- AVFF: Oorloff et al. 2024 (arXiv:2406.02951) — 98.6 % FakeAVCeleb
- AV-Deepfake1M: Cai et al., ACM MM 2024 — dataset + benchmark
- 3D-Speaker-Toolkit multimodal (arXiv:2403.19971) — audio/visual/semantic fusion direction

Nexus internal:
- `wakeword_oww.rs` speaker embedding (docs/wake-word/07-speaker-verification.md)
- Feature 88 admin-gated worker identity & BYOK; feature 140 Action Kernel; docs/mcp/01 auth-vault; features/111 universal agent watcher (wake integration analog)
