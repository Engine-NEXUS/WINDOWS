# Master Implementation Plan — Administrator Privilege on Any Device

> **Current status:** The command/session foundation is partially built and remains fail-closed; see [Feature 143](../../features/143-administrator-review-access.md). The phases below are still planning work. Later owner decisions supersede the public-sharing examples and approval assumptions in this original research.

**Date:** 2026-10-19. Status: PLAN (nothing built — this file is the consolidated roadmap). Sources: [01 overview + P0-P6](01-admin-privilege-any-device-research-2026-10-19.md) · [02 E0-E5 enrollment/face](02-face-enrollment-template-only-pipeline-research-2026-10-19.md) · [03 V0-V5 voice](03-voice-identity-firewall-f1-research-2026-10-19.md) · [04 compliance](04-standards-nist-compliance-research-2026-10-19.md) · [05 F0-F5 face](05-face-verification-firewall-f2-research-2026-10-19.md) · [06 fusion/re-verification](06-multimodal-fusion-deepfakes-research-2026-10-19.md) · [07 policy/tokens/audit](07-threat-model-capability-policy-research-2026-10-19.md) · [08 UX state machine + keys](08-ux-session-flow-phrasing-research-2026-10-19.md).

**Ground rules (house style):** every phase ends with *tests, not claims*; every session appends an AGENTS.md entry with Built / Verify / Not-run-live lines; nothing run live before the red-team gate; kill-switches preserved (no edits inside Ghost files; additive flags only).

---

## 1. Sequenced roadmap (dependencies made explicit)

| Order | Phase (source doc) | Deliverable | Depends on | Gate (verbatim from source doc) |
|---|---|---|---|---|
| 1 | **P0 policy/token spec** (01 §8) | Administrator role JSON, session-token shape, `personal_domain` deny-list wording; feature-88 alignment review | — | sign-off; no code |
| 2 | **P1 worker surface** (01 §8) | Worker routes for `admin` flag on profiles + session issuance + revocation (extends 88 suite) | P0 | unit tests; revoke cascades; fail-closed offline behaviors |
| 3 | **V0 voice chassis decision** (03 §7) | sherpa-onnx vs wespeaker ECAPA512 ONNX; fusion recipe baked | — | A/B benchmark harness + recorded decision |
| 4 | **V1 matched baseline** (03 §7) | measured FMR/FRR, thresholds | 3 | measured table; ≥5 mutations killed |
| 5 | **V2 cross-device calibration** (03 §7) | per-device centroids, AS-Norm cohort, per-device τ | 4 | cross-device FRR < 5 % @ FAR < 0.1 % (with retries) on ≥ 3 laptops |
| 6 | **V3 voice CM + optionality** (03 §7) | AASIST-class CM ONNX, replay heuristics, fusion scoring | 4 | replay corpus ≥ 98 %, clones ≥ 95 % |
| 7 | **F0 face chassis** (05 §7) | `admin_identity/face/` skeleton; `ort` vs sidecar decision; session pool | 1 | skeleton compiles; IPC stubs |
| 8 | **E1 enrollment UX + zero-raw gate** (02 §5) | pose-prompt capture, frames-in-RAM, disk-write fuzz test | 7 | 5 poses × 3 users demo; zero-frame-on-disk test green |
| 9 | **E2 template store** (02 §5) | centroid clustering, AES-256-GCM+DPAPI, import-from-photos with shred | 8 | corpus fuzz; decrypt round-trip; zero sources survive |
| 10 | **F1/F2 PAD integration** (05 §7) | MiniFASNet + rPPG + lip-sync ensemble + ordering gate | 9 | PAD-first test ordering; spoof corpus ≥ 98 % (photo/screen/print) |
| 11 | **F3 identity match** (05 §7) | SFace embed + centroids + per-device τ | 10 | FNMR < 5 % on ≥ 20 sessions × 3 cameras; lookalike eviction |
| 12 | **V4 challenge loop** (03 §7) | digits generation/STT bias/retry/fusion | 10 | fresh-attack corpus: yesterday's recording fails 100 % |
| 13 | **E4/F4 lifecycle + permissions UX** (02 §5, 05 §7) | refresh flag, revoke IPC, template-drift re-enrollment guidance, camera-quality failure UX, pool unload | 11 | lifecycle tests; RAM delta ≤ 120 MB; unloaded on idle |
| 14 | **P4 Action Kernel + AdminShare** (01 §8, 07 §3) | `scope:admin-read-share`, deny-list enforcement on canonical paths, artifact provenance, redaction, share route, audit/banners | 2, 11 | red-team prompts 100 % refused; share provenance tests; IPC/dead-code gates clean |
| 15 | **U-phrases + ceremony wiring** (08) | phrase keys externalized; state machine joins F1+F2+F3 (doc 06 cascade); orb/caption/banner choreography | 9, 12, 14 | snapshot tests per transition; latency budget (≤4 s p50 ceremony) |
| 16 | **P5 measurement + live** (01 §8) | full-budget measurement, 6-laptop matrix, base house test, FRR retry UX, red-team controlled day | all | end-to-end demo on ≥ 2 unrelated machines; median wake→greet ≤ 4 s; RAM ≤ 120 MB delta; "not run live" emptied |
| 17 | **P6 hardening** (01 §8) | threat-model doc refresh, THIRD_PARTY rows, privacy.html biometrics section, revocation drill, paper-recheck cadence | 16 | adversarial (owner-run) attempt: no prohibited action completes; flagoff rollback clean |

Sizing note (honest): this plan assumes several real-work weeks across sessions (the V-matrix alone is a multi-day benchmark exercise); phases are deliberately separable — any phase can ship independently behind `adminVoiceEnable`/`adminFaceEnable` flags with the *next* phase's footage-gating always fail-closed.

## 2. Module map (where code lands — file-level intent)

```
src-tauri/src/admin_identity/
  mod.rs                 flag-gated mountpoint wired from orchestrator candidate path
  voice/                 F1: enrollment centroid store, AS-norm, CM ONNX, fusion score
  face/                  F2: yunet/sface/minifas sessions, rPPG, lipsync, matching, τ store
  challenge.rs           F3: digit generation, window binding, retry budget
  session.rs             F4: Verdict{..}, token verify (pinned pub key), TTL/backoff
  templates.rs           AES-256-GCM + DPAPI blob store (face + voice templates together)
  audit.rs               JSONL lines per doc 07 §6 (reuse mcp_audit pattern)
server/worker/           admin flag routes + session issuance + admin_share egress (P1/P4)
frontend/src/admin/      phrases.ts, banner, AdminActivity list, enrollment wizard screens
frontend/src/sidebar/    orb verification ripple + banner choreography (growth of existing anim family)
```

Explicit non-touch list preserved everywhere (doc-derived): `ghost_center.rs`, `ghost_learning.rs`, `ghost.rs`, `mouse.rs`, `shortcuts/*`, `screen.rs`, `ghostHotMic.ts`; orchestrator ownership unchanged (12-command-center contract — candidate path inserts *before* `prepare_action`, never beside it).

## 3. Budgets (mirrors house RAM/latency culture, features 130-132)

| Item | Budget | Basis |
|---|---|---|
| Idle RAM delta | **0 MB** (all sessions unloaded; no warm pools) | standing rule — memory measured per-state |
| Active F1 delta | ≤ 40 MB peak during ceremony | embedding model class sizes (29.6-30 MB ONNX) + buffers |
| Active F2 delta | ≤ 80 MB peak during ceremony (all 4 models + 3 frame buffers) | sizes: detector 338 KB, SFace 36.9 MB, MiniFASNet small 80×80, rPPG pure math |
| Wake→candidate decision | ≤ 400 ms p50 | F1 runs inside the existing post-wake capture window |
| Ceremony (S1→S3 audio) | ≤ 4 s p50 / ≤ 6 s p90 | doc 01 §5 |
| Model unload | session pool closes ≤ 20 s after last use | rodio stream precedent (wake_cue) |

## 4. Red-team corpora (each adversarial phase owns a measured bar)

| Corpus | Contents | Bar |
|---|---|---|
| replay-voice | ≥ 200 recordings of admin phrases (varied rooms, phone-speaker re-play, BT re-broadcast) | 100 % challenge-fail; CM ≥ 98 % |
| clone-voice | ≥ 3 public TTS/VC systems rendering admin voice from public samples | CM ≥ 95 %; fusion bar 100 % |
| imposter-live | ≥ 5 people speaking wake+digits on 3 devices | F1 reject 100 % @ tuned τ |
| photo-face | printed + phone-screen + laptop-screen admin photos | PAD ≥ 98 % |
| video-face | admin video + talking-head render | PAD+rPPG+lipsync ≥ 95 % compound; share of *grants* = 0 |
| av-deepfake | real-time synthetic admin (boundary test) | no grant predicted; policy scope caps damage; document every layer's margin |
| prompt-injection | 100 prompts embedding commands in screens/emails/artifacts | 100 % refusal (provenance gate) |
| lookalike | sibling/close-resemblance users | never granted; τ tie-break rules defined by measured margins |

Corpus ethics rule: all corpora built from the owner's own materials plus volunteer family members/friends with consent; nothing leaves the devices; recorded under the privacy-modes/leases ethics the repo already uses.

## 5. Testing infra (aligned with existing repo tooling)

1. **Unit gates**: Rust `cargo test --lib` (features `custom-protocol,admin-brain`); template/crypto fuzz like doc 140's path fuzz.
2. **Mutation checks**: ≥ 5 mutations killed per security decision like doc 140 changes (identity scoring, τ boundaries, deny-list, provenance, challenge binding).
3. **IPC + dead-code gates**: no new public IPC without hub mirror and gates clean (existing gate list).
4. **Zero-retention fuzz**: enrollment/verify directory-watcher asserts no raw frame/template byte ever persists outside templates+audit (E1/E2).
5. **Grep-gates**: no biometric vectors in logs/IPC payloads/shipped bundles; no CM/chat model endpoints receive templates (regex like the no-shell-strings test in doc 140).
6. **Frontend**: vitest + tsc clean; phrase-snapshot tests; banner TTL timer tests.
7. **Measurement harness**: JSONL runner under `docs/research/admin-privilege/data/` (pattern matches ghost-mode p0 runs), each phase's thresholds recorded with device specs.

## 6. AGENTS.md entry template (paste-ready per session)

```
## Administrator privilege — <phase names> (YYYY-MM-DD)
- **Built**: <modules, flags; nothing run live>
- **Verify**: <test suites, counts; mutation survivors fixed; budget deltas>
- **Not run**: <anything real-hardware; outstanding>
- **Docs**: research suite 01-09; change record <NN-...>
```

## 7. Dependencies/ordering risks

1. Feature 88 (worker identity) must land before P1: token issuance rides its profile/device model; feature flags make admin optional on pending profiles.
2. Memory v2 interplay: admin context pack builder must plug the memcore exclusion invariant (doc 07 §3.2) — small surface, but ordering after memory M-phases is cleaner.
3. `dur` exhaustion: passes on V2's device matrix degrade if NEXUS mid-refactor swaps capture path — plan P5 re-run after any capture/STT engine change.
4. Caption-stack timing reuse (lip-sync join) inherits caption-stack fixes; rerun its tests alongside F2.

## 8. Acceptance summary (what "done" means, one line)

*"On any unrelated laptop, the owner says 'NEXUS' + fresh digits on camera; NEXUS greets them as Administrator within ~4 s; they can read/share analysis artifacts publicly with redaction; and they cannot read a single personal-domain byte of the device user — verified by tests, red-team corpora, and a recorded live run."*
