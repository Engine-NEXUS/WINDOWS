# Face Enrollment Without Storing Raw Images — How Face ID Actually Works + Open-Source Reference Study (Administrator Privilege, doc 02)

**Date:** 2026-10-19. Status: RESEARCH (nothing built). Companion: [01-admin-privilege-any-device-research-2026-10-19.md](01-admin-privilege-any-device-research-2026-10-19.md) (four-firewall architecture; this doc drills into the **F2 face pipeline**: how "professional face ID" works, whether the admin must "train" it, the zero-raw-image retention design the owner wants, and which open-source projects already implement enrollment from images → vectors → verify).

**Owner ask (translated):** *"How does professional face ID work — muscles/molecules? Do I give it a bunch of my photos to train it? I don't want my images saved: use them only for the setup, delete them after, but never stop recognizing me. Find the best open-source projects that already implement this."*

---

## 1. How face recognition actually works (no "training" by you, no "muscles")

Misconception corrected: modern face ID stores **no pixels, no anatomy, no "muscles."** It stores **vectors of numbers** and compares distances. The model itself was trained **once**, years ago, by its authors, on millions of face photos — the admin's photos are never used for training.

What every production system (Apple FaceID, banks, DeepFace, OpenCV, InsightFace) does is the same **5-stage pipeline** (DeepFace's own README lists it verbatim: *detect, align, normalize, represent, verify*):

```
camera frame
  1. DETECT      YuNet/SCRFD finds a face box + 5 landmarks (eyes×2, nose tip, mouth corners)
  2. ALIGN       warp/crop the face to a canonical 112×112 using those landmarks
  3. NORMALIZE   the exact pixel scaling the chosen model expects
  4. REPRESENT   the CNN turns the 112×112 crop into ONE fixed-length vector
                 (e.g. 128-d for SFace/MobileFaceNet, 512-d for ArcFace-class)
  5. VERIFY      cosine distance between the fresh vector and the stored template vector.
                 same person → high similarity; different person → low.
```

Key properties that matter for the owner's requirements:

1. **Stage 4–5 is math, not learning.** Enrollment = creating vector(s). Verification = distance computation. You can enroll or delete vectors at any time, instantly — no model files change.
2. **One image is enough to enroll; more images are better.** CompreFace's docs say exactly this: *"One subject example is enough for face recognition… if you add more examples, the accuracy may be even better."* The professional pattern is **5–15 frames at different poses/lighting** (frontal, slight left/right, up/down, glasses/no glasses) — not thousands. Each becomes a vector; production systems store either the average or a small set of per-pose centroids.
3. **Raw frames have no ongoing value.** After the vectors exist, every frame can be discarded *within the same second it was read* — the pipeline never needs the pixels again. This is exactly what DeepFace's `represent()` does in memory and what scale the owner wants.
4. **Two usages, one gallery:** 1:1 *verification* ("is this the Administrator?") vs 1:N *identification* ("which enrolled person is this?"). NEXUS needs the trivial case: a tiny gallery (Administrator template + optionally the device user's template) — verify-first, which is also more accurate than open-world identification.

### What the vectors are (and what they leak)

The 128/512 numbers encode a learned statistical signature of *facial geometry and texture* (spacing around eyes/nose/mouth, bone shading gradients…) — learned from the **model's** training set, not your anatomy. Helpful framing: your photos are the *input*, the model is a fixed measuring tool, the template is the *measurement*.

**But — the honest caveat, from the papers:** a template is *not* formally "irreversible".

| Finding | Source |
|---|---|
| IdDecoder reconstructs realistic faces from embeddings that are then *fool the system as the same identity* | Le & Carlsson, CODASPY 2023 (ACM) |
| Reconstruction succeeds from **20 % partial leakage** of an ArcFace template (87 % attack success at FMR 0.1 %, LFW) | Shahreza & Marcel, ICASSP 2024 |
| Framework measuring inversion vulnerability across **31 SOTA models** — reconstructed faces can *enter* the system as the user | TIFS 2024 (IEEE) |

Consequence for NEXUS: "images deleted, template kept" **without template protection** is a real data-protection requirement, not a checkbox. Design mandates (see §5):
- templates always **AES-256-GCM encrypted at rest** (same crypto core as memcore; key sealed with Windows DPAPI),
- templates **never sent to the Worker or any cloud** (extends feature-88's "biometrics never leave the device" rule),
- template access limited to the verification module (IPC-gated like every other endpoint),
- optional **cancelable template** (a per-device permutation/secret transform before storage) as a future hardening — the literature shows naive protections (PE-MIU-class soft-biometric privacy) are themselves attackable when weak (arXiv:2310.03005), so this stays *optional*, not shipped as a promise.

---

## 2. Answering the retention question precisely

Owner rule: **"no images saved, only for the setup; after done delete them — never stop recognizing me."**

| Item | NEXUS rule |
|---|---|
| During enrollment | Camera frames live **only in RAM**. Every frame → detect → align → embed → *dropped*. Even the aligned 112×112 crop must not be written to disk (test-gated: a file-watch fuzz test asserts zero `.jpg/.png/.bin` writes in the profile dir during enrollment, plus an ORT callback asserting no serialization). |
| What is persisted | 3–8 prototype vectors ("centroids") + quality metadata (pose label, similarity spread, capture date, device name). A template file is a few KB. |
| Enrollment button photos chosen by the admin ("give it a bunch of images") | Supported: admin can ALSO enroll from a folder of existing photos. The images are used to compute vectors and then the tooling deletes them by default (explicit `--keep-sources` opt-out exists for dev only). |
| After enrollment | Nothing to delete in the normal path — no raw copy ever existed. For the import-path: photos auto-deleted after vector extraction (undo impossible by design; a second confirmation card asks before delete). |
| "Never stop recognizing me" | Templates persist → recognition continues. Two honest caveats below. |
| Appearance drift over months/years | Optional **quiet template refresh**: when a verification succeeds with high margin (and PAD passed), one new centroid can be added (cap 8, cosine-dedupe, never replaces, off `adminFaceRefresh` default ON). This keeps recognition stable as glasses/beard/age change — without ever storing an image. |
| Re-enrollment trigger | Template deleted (explicit revoke), template file corrupted, or a streak of high-margin *failures* suggesting real drift (soft-recommend, never silently) → re-enroll. |
| Revocation (rights-respecting) | "Delete my face template" IPC → files shredded → admin must re-enroll on next session. That is the only irreversible-loss case; accepted. |
| Device-user privacy | The device user's face is **only ever seen transiently** to check *it is not the admin*. If a face is detected and doesn't match the admin template → no vector stored, no verdict saved beyond "not-admin". Device users are NOT enrolled by default. |

---

## 3. Reference study — open-source projects that already do this

Ordered by fitness-for-NEXUS-shaping:

| Project | What it implements | License | Notes for NEXUS |
|---|---|---|---|
| **OpenCV Zoo — YuNet + SFace** (`opencv_zoo/models/face_detection_yunet`, `face_recognition_sface`) | Detect → 5-landmark align → 128-d embed → cosine/gt_l2 `match()`. Single ONNX each (338 KB detector, 36.9 MB embedder). C++ + Python. Model card: LFW **99.60 %** (cosine τ 0.363), CALFW 93.95 %, CFP-FP 94.8 %, quantized variants 99.3-99.4 %. | Apache-2.0 (clean model+code) | **Recommended F2 core.** Free, commercial-OK, tiny, one-image enroll works. Rust integration via `ort` (ONNX runtime crate) or the existing Python sidecar pattern. |
| **agentculture/face-recognition-cli** | Frontend CLI with the *exact* domain verbs: `enroll` (per-name embeddings), `match` (cosine against bank), `forget`, `forget all`, per-consumer "banks", exit-code-clean JSON. Built on YuNet + SFace. | check repo | **Direct behavioral reference** for NEXUS's `admin_identity` verbs (`enroll_admin`, `verify_face`, `revoke_template`). Its "bank" concept maps to NEXUS per-device template store. |
| **Exadel CompreFace** | Self-contained face server (Spring + Python embedding servers + Postgres): `subjects` → `faces` (each POST computes a "faceprint"/embedding), 1:N `recognize`, 1:1 verify endpoints, embeddings-only submission supported. | Apache-2.0 | Useful as an **architecture diagram of a general product**; NOT for embedding (too heavy, server-shaped). One documented foot-gun NEXUS must avoid: *"By default, the image itself is also saved… you can change it using `save_images_to_db`"* — i.e., an industry implementation defaulted to keeping raw images. NEXUS inverts this: zero-raw by construction (§2). |
| **serengil/DeepFace** | The canonical 5-stage pipeline in one Python lib; wraps VGG-Face/Facenet512/ArcFace/SFace/etc., **built-in `anti_spoofing` flag** in represent/verify/analyze; batch represent; database-backed `register/identify` (pgvector, sqlite…); homomorphic embedding encryption via **LightPHE** integration. | MIT (code) — *model licenses inherited per model* (its SFace/OpenCV path is clean; its `Buffalo_L` path is not) | Great for prototyping F2 sidecar quickly and for its anti-spoof wiring reference; in production NEXUS should call the models directly via `ort` to hold the 128-d/512-d stream and avoid inheriting unknown model weights. |
| **InsightFace (buffalo_l / antelopev2)** | Highest-accuracy open embeddings (ArcFace R50-class, 512-d). | Code MIT; **models non-commercial** (2025-11-24 note). | Only if NEXUS later buys the commercial license (then swap-in best accuracy). Default stays SFace. |
| **minivision-ai/Silent-Face-Anti-Spoofing** | MiniFASNet V2+V1SE PAD ensemble, 80×80, ONNX-exportable, onnxruntime-friendly (LocalAI wraps exactly this as `Antispoofer`). | Apache-2.0 | Already selected as F2's liveness layer (doc 01). Pairs in front of SFace. |
| **LightPHE (serengil)** | Partially-homomorphic encryption so similarity is computable on encrypted embeddings. | MIT | Future option if NEXUS ever wants cloud-templated verification with provider-invisible templates; not needed for local-only. |

Pipeline recommendation (unchanged from doc 01, now concrete): **YuNet → SFace (128-d) → MiniFASNet PAD → rPPG → lip-sync**; SFace stage costs ~bytes-per-frame protein and is the lightest model in the comparison; 1:1 verification makes its accuracy ceiling irrelevant for the admin-vs-others question.

### Accuracy shaping for the NEXUS verdict (numbers, honest)

- SFace report (OpenCV zoo benchmark tools): LFW 99.60 % @ cosine τ 0.363; block-quantized 99.42 %; int8 99.32 %. For a **1:1 twin-vs-admin** task this is comfortably strong; the residual risk is short-of-twin lookalikes and cross-camera framing, covered by the PAD ensemble + F3 challenge + policy.
- NIST bar (doc 01 §4): FMR ≤ 1/10,000, FNMR < 5 %, face PAD mandatory, IAPAR < 0.07 — we *target* these, measure in P-phases, and refuse to *claim* certification.
- Per-device camera variance changes τ; calibration step: on enrollment, record the in-gallery max/second-best similarity spread and set the per-device acceptance threshold adaptively (initial τ default: cosine > 0.45 accept, 0.30-0.45 ask again, < 0.30 refuse — to be replaced by measured values).

---

## 4. Proposed NEXUS flow (`admin_identity::face`)

```
ENROLL  (spoken: "enroll my face" — admin voice F1 already passed, or via his own device)
  → camera opens with framing overlay; 5 pose prompts ("look straight / left / right / up / down")
  → per pose: YuNet detect → quality gate (face ≥ 96 px, blur < τ, exposure in-band) → retry
      else re-prompt (max 3 per pose)
  → SFace embed per accepted frame → ALL FLOATS into RAM → frame dropped
  → cluster (cosine linking, 3-8 centroids, keep pose-diverse set)
  → centroids L2-normalized, serialized: N × 128 f32 (≈ 2-4 KB) + metadata
  → AES-256-GCM (DPAPI-sealed key, profile-scoped) → admin_face.bin
  → optional import path: folder of photos → same pipeline → sources shredded
  → spoken: "Face enrollment complete. Raw frames were never stored."  + hub card
VERIFY  (F2, per doc 01)
  → PAD ensemble FIRST (MiniFASNet + rPPG ≥ 3 s + lip-sync on F3 challenge phrase + motion probe)
  → YuNet + SFace on 3 sampled frames → per-centroid cosine → max over centroids
  → decision: pass / ask-again / refuse (NIST-style 5-fail backoff)
REFRESH (silent, flag-gated)  → verified-high-margin sessions append 1 centroid (cap 8, dedupe)
REVOKE  → shred admin_face.bin + audit line + "re-enroll required" spoken
```

Implementation chassis options (decision in E0):
- **Rust-native** with `ort` crate (ONNX Runtime) — no Python at install time; matches the on-demand-ONNX-session pattern NEXUS already uses (`speaker_model.onnx`, admin-brain llama.cpp). Preferred if buffers/CPU stay in budget (YuNet ~1 ms, SFace ~3-6 ms CPU-class, est.).
- **Python sidecar** (the `server/` Moonshine/NLU pattern, `127.0.0.1:392xx`) — fastest to stand up with onnxruntime + numpy; heavier startup/RAM; acceptable fallback. DeepFace not vendored — models called directly.

Both routes must honor the **RAM/CPU standing budget** (features 130-132): sessions are lazily created at first F2 use and unloaded 20 s after last use (rodio stream pattern precedent).

---

## 5. Plan extension (E-phases; nothing built) — folds into doc 01's P1/P3

| Phase | Work | Gate |
|---|---|---|
| **E0** | Model + chassis decision: SFace vs (commercial ArcFace later); `ort` vs sidecar; confirm no license rows added without `THIRD_PARTY_NOTICES.md` entry | written decision record; license check list passes |
| **E1** | Enrollment UX: pose prompts, quality gates, framing overlay, live "scanning" orb state; frames-in-RAM only (no disk writes anywhere during enroll — enforce with a test that fails if the profile dir mtime changes) | 5 poses × 3 users enrollment demo; zero-frame-disk test green; A11y: no camera if F1 voice gate not passed first |
| **E2** | Vector store: centroid clustering + AES-256-GCM template file (DPAPI-sealed), schema + migration; import-from-photos path with auto-shred | corpus fuzz (malformed bin) never crashes; decrypt round-trip; import leaves zero source images |
| **E3** | Verify path: PAD first → embed → per-centroid match → per-device τ calibration captured at enroll | measured per-device accept/refuse margins on 3 laptops; spoof corpus (photo, phone-screen, printed card) rejected ≥ 98/100 by PAD ensemble; 5 mutations killed |
| **E4** | Refresh/revoke/tip: quiet centroid growth flag, revoke IPC, drift-soft-recommend; restart safety (template survives reboot; no credential plane changes) | lifecycle-state machine tests; "delete my face" works from voice AND hub; revoke forces re-enroll on next attempt |
| **E5** | Security hardening: template-at-rest audit (grep-gate: no template bytes in logs, never shipped to JS except boolean decisions), optional cancelable-template research note, worker never sees templates (`privacy.html` text for the biometrics section) | grep/IPC gates clean; privacy.html updated; doc-record of inversion-attack rationale for reviewers |

Verification flow inherits doc-01 F4 policy unchanged: even a perfect face match grants only the `scope:admin-read-share` session — personal-domain deny-list sits below biometrics.

---

## 6. Open questions / risks (not verified)

1. **SFace lookalike margin** for *specific twins/siblings of the admin* — untested; per-device τ from real enrollments decides; the F3 challenge remains the fresh-factor if a lookalike squeaks past.
2. **Camera quality on stranger laptops** (dirty/low-res 480p cams): PAD + SFace FNMR may rise; mitigation: higher-quality pose prompts and generous retry semantics; measured in E3.
3. **Template refresh conflicts with"never stops recognizing"** if owner wants strictly *static* template forever → set `adminFaceRefresh` OFF; documented tradeoff (drift risk slowly rises over years).
4. **Inversion-attack literature moves fast** (partial-leakage inversion at 20 %); DPAPI+AES local-only is our declared baseline; monitor TIFS-style papers yearly.
5. **360°-camera-less laptops** (e.g., no physical camera) — F2 unavailability path: admin mode unavailable on non-camera devices (spoken line), F3+F4 unchanged (safer constraint, since F2 is the NIST-eligible verifier; without a camera the elevated session does not start).
6. **The "user's own face" accidental enrollment** risk — NEXUS denies non-admin voices the enroll verb (provenance = Verified-Owner gate).

---

## 7. Sources (checked 2026-10-19)

- DeepFace README + `represent()`/database module (serengil/deepface; MIT; anti_spoofing flag; LightPHE note)
- OpenCV docs: DNN-based Face Detection and Recognition (FaceDetectorYN/FaceRecognizerSF; SFace accuracy table incl. LFW 99.60 %, τ cosine 0.363)
- OpenCV Zoo SFace/YuNet demo + model card (Apache-2.0; quantized variants ~99.3-99.4 %)
- agentculture/face-recognition-cli (enroll/match/forget verbs on YuNet+SFace)
- CompreFace REST API docs (subjects/faces/-faceprint; `save_images_to_db` default caveat; Apache-2.0)
- IdDecoder: Face Embedding Inversion (Le & Carlsson, CODASPY 2023)
- Otroshi Shahreza & Marcel, Partial Template Inversion (ICASSP 2024; 87 % success @ FMR 0.1 % from 20 % leakage; ArcFace/LFW)
- Vulnerability of SOTA Face Recognition to Template Inversion (TIFS 2024; 31 models)
- Reversing Deep Face Embeddings w/ soft-biometric privacy-protection weaknesses (arXiv:2310.03005)
- NIST SP 800-63B-4 §3.2.3 (face = the sanctioned biometric; PAD SHALL; FMR/FNMR bars) — do's from doc 01 §4
