# Standards & Compliance — NIST 800-63B-4, ISO 30107-3 PAD, GDPR biometrics

**Date:** 2026-10-19. Status: RESEARCH. Companion docs: [01 overview](01-admin-privilege-any-device-research-2026-10-19.md), [03 voice](03-voice-identity-firewall-f1-research-2026-10-19.md), [05 face](05-face-verification-firewall-f2-research-2026-10-19.md), [07 policy/threat model](07-threat-model-capability-policy-research-2026-10-19.md), [08 UX](08-ux-session-flow-phrasing-research-2026-10-19.md).

**Purpose:** extract the *engineering requirements* hiding inside the standards the research surfaced, map each to a concrete NEXUS design decision (or a documented deviation), and define the exact claim language NEXUS may publish. Nothing here certifies NEXUS; it aligns the design with published authority.

---

## 1. NIST SP 800-63B-4 (July 2025) — the biometrics clause (§3.2.3), verbatim anchors

Direct quotes captured during research (2026-08-26 revision date shown on the page):

> *"Biometric comparison based on voice SHALL NOT be used."*

> *"Biometrics SHALL only be used as part of multi-factor authentication with a physical authenticator (i.e., 'something you have'). The biometric characteristic SHALL be presented and compared for each authentication operation. An alternative non-biometric authentication option SHALL always be provided to the subscriber. Biometric data SHALL be treated and secured as sensitive personal information."*

Supporting rationale bullets (verbatim selections):
- *"...the biometric FMR does not provide sufficient confidence ... because it does not account for active impersonation attacks."*
- *"Biometric comparison is probabilistic, whereas the other authentication factors are deterministic."*
- *"Biometric characteristics do not constitute secrets. They can often be obtained online or otherwise without consent... While presentation attack detection (PAD) technologies can mitigate the risks of these types of attacks, additional trust in the sensor or biometric processing is required..."*
- *"The use of biometric characteristics, especially when stored for central verification, introduces new privacy concerns..."*

### 1.1 Requirements check-list mapped to NEXUS

| # | NIST requirement (paraphrase) | NEXUS decision | Where | Status |
|---|---|---|---|---|
| 1 | Voice SHALL NOT be an authenticator | Voice is F1 *candidate detector* only; never grants; session cannot start on voice alone | [03 §1](03-voice-identity-firewall-f1-research-2026-10-19.md), [01 §0](01-admin-privilege-any-device-research-2026-10-19.md) | ✓ design |
| 2 | Biometrics only WITH a physical authenticator | Feature-88 Worker-signed **device-bound Administrator session token** (stored in OS keyring on the device, TTL, revocable) plays the possession factor; biometric gates its *issuance ceremony* | [08-session-token](07-threat-model-capability-policy-research-2026-10-19.md) | ✓ design |
| 3 | Biometric compared **each authentication operation** — no long-lived unlocks | Each admin session: F1 candidate → F2 face PAD → F3 fresh digits every time; no "verified today, valid tomorrow" | [09 plan U-phases](09-master-implementation-plan-2026-10-19.md) | ✓ design |
| 4 | Alternative non-biometric option SHALL be provided | Spoken alternate path per session: fallback typed authorization on the OWNER'S own device (PIN/passkey scoped to admin console) or, when unattended, admin mode simply unavailable (fail-closed, honest line spoken) — owner decision pending (open Q) | [08 §4](08-ux-session-flow-phrasing-research-2026-10-19.md) | ⚠ owner choice |
| 5 | Biometric data = sensitive personal information | Template files AES-256-GCM + DPAPI; IPC-gated; never cloud; audited access | [02 §1-2](02-face-enrollment-template-only-pipeline-research-2026-10-19.md) | ✓ design |
| 6 | Face verifier SHALL implement PAD | PAD ensemble mandatory before match (MiniFASNet + rPPG + lip-sync + motion) | [05 §4-5](05-face-verification-firewall-f2-research-2026-10-19.md) | ✓ design |
| 7 | FMR ≤ 1/10,000 (zero-effort impostor conformant attack); FNMR < 5 % | *Targets*, measured in V2/F3 phases on real devices; not achievable *claims* | [03 §7 V2](03-voice-identity-firewall-f1-research-2026-10-19.md), [05 §7](05-face-verification-firewall-f2-research-2026-10-19.md) | ⚠ measured target |
| 8 | IAPAR < 0.07 (impostor attack presentation accept rate) | Red-team corpora goals (photo/print/screen/mask ≥ 98 % PAD rejection per attempt class) | [09 red-team corpora](09-master-implementation-plan-2026-10-19.md) | ⚠ measured target |
| 9 | Fixed threshold per system; ≤ 5 consecutive failures then 30 s exponential backoff or disable + alt factor | Backoff state machine in [08 §4](08-ux-session-flow-phrasing-research-2026-10-19.md); τ fixed per device at enrollment calibration | ✓ design |
| 10 | Local comparison preferred; central storage avoided | All biometric comparisons on-device; Worker stores identity/entitlement only, zero biometrics | [02](02-face-enrollment-template-only-pipeline-research-2026-10-19.md), feature 88 | ✓ design |
| 11 | Sensor/endpoint integrity & authenticity (attestation) | Out-of-scope for v1 (consumer laptops, no TPM-attested cameras); documented deviation §3 | — | ✗ documented gap |

**Deviation note (11):** NIST assumes managed endpoints with attestable sensors. NEXUS runs on arbitrary BYOD laptops where it cannot attest hardware. The design therefore pushes the *deterministic boundary* down into policy (F4) and keeps biometrics as ceremony — the honest recognition that without sensor attestation, host-compromise beats every client-side biometric. This is the same boundary doc 140 reached for file operations ("the kernel's path scoping is the v1 boundary, honestly weaker than an OS sandbox").

## 2. ISO/IEC 30107-3 — PAD vocabulary adopted

Vocabulary NEXUS uses so that future certification/targeted evaluation has consistent semantics:

| Term | Meaning | NEXUS mapping |
|---|---|---|
| **Presentation attack (PA)** | presentation of an artifact or human characteristic to spoof the sensor | recording replay, phone-screen photo of admin, printed mask, voice clone |
| **PAD** | presentation attack detection | the F2 ensemble; the F1 CM |
| **IAPAR / IAPMA** | impostor attack presentation acceptance rate (composition of FMR and PAD misses) | the compound metric doc 09's red-team corpora target |
| **Species** | attack category (print, replay, mask, makeup...) | corpus taxonomy in [09 red-team table](09-master-implementation-plan-2026-10-19.md) |

Reference implementations used as efficacy ceiling: Azure Face Liveness (gated SDK) — **ISO 30107-3 iBeta Level 1 AND Level 2 PAD, 0 % penetration rate**; passive and passive-active modes; nothing stored post-session. Not usable in NEXUS (cloud/gated), but defines the state-of-the-art bar an offline ensemble is judged against (doc 05 §6).

## 3. GDPR / privacy-law notes (EU-flavored, implementable globally)

- **Biometric data = special-category personal data (Art. 9)** when processed to identify a natural person → **explicit consent** at enrollment (enrollment screen asks the Administrator; a device-user consent banner if a user template is ever enrolled — default: never enroll device users).
- **Purpose limitation & minimization**: template-only design (doc 02) — no images retained, centroids only, on-device only. The "minimization" is belt *and* braces: even a full disk leak exposes vectors, not photos (with the inversion-attack caveat of doc 02 §1 handled by encryption-at-rest).
- **Right to erasure / portability**: revoke templates by voice command + hub page; export = none (vectors meaningless outside the pipeline — and *that* is a feature; the claim is truthful).
- **Transparency (Art. 13/14)**: `privacy.html` biometrics section must state: what is stored (3-8 vectors), where (encrypted, device-local), for how long (until revocation), who can trigger (the Administrator only), and what happens on admin action (R8 visibility contract, doc 07 §6).
- **Data protection by design** (Art. 25): the template-only zero-retention architecture *is* the design argument; record it in the privacy doc.
- Non-EU sanity: CCPA-class rights satisfied by the same erasure path; nothing is inferred about demographics (age/gender plugins in CompreFace-class servers are **explicitly not used** — templates only compute identity).

## 4. The claim ladder (what NEXUS may say, in ascending order of caution)

| Tier | Claim | Allowed? |
|---|---|---|
| 0 | "NEXUS uses voice + face biometrics for an owner role" | ✓ (fact) |
| 1 | "Engineered toward NIST 800-63B-4 biometric requirements (multi-signal session verification + PAD + possession token + policy boundary)" | ✓ with "engineered toward", never "compliant" |
| 2 | "Face + PAD pipeline follows ISO/IEC 30107-3 vocabulary" | ✓ (vocabulary only) |
| 3 | "Certified / NIST-compliant / iBeta-tested" | ✗ NEVER |
| 4 | "Impossible to fool / 100 % secure" | ✗ NEVER (doc 01 §0 honesty) |
| 5 | "Administrator actions on your device are always spoken, visible, and auditable — even for the Administrator" | ✓ (is the R8 contract; see doc 07 §6) |

**Suggested README language (draft):** *"Administrator recognition is a layered, probabilistic gate (live voice + camera face/liveness + fresh spoken challenge) leading to a session token; the hard boundary is a policy layer that denies personal data to every role. It is deliberately not marketed as a certified identity system."*

## 5. Compliance artifact checklist (P0 gate in doc 01 / master plan)

1. privacy.html — biometrics section (above).
2. QUICKSTART.md — admin enrollment instructions + what is never stored.
3. THIRD_PARTY_NOTICES.md — rows for wespeaker (Apache-2.0; VoxCeleb CC-BY 4.0 models), OpenCV Zoo YuNet+SFace (Apache-2.0), Silent-Face-Anti-Spoofing (Apache-2.0), plus any AASIST-class CM model license before vendoring.
4. Feature-flag defaults documented (`adminVoiceEnable`, `adminFaceEnable`, `adminFaceRefresh`).
5. Revocation drill script (E4/V5 evidence) recorded.
6. Template-inversion literature note (doc 02 §1) captured as the risk rationale for encryption-at-rest; annual re-check instruction.

## 6. Sources

- NIST SP 800-63B-4 (July 2025), §3.2.3 Use of Biometrics + Biometric Accuracy + PAD subsections (pages.nist.gov/800-63-4/sp800-63b.html; also SP 800-63B (v3, withdrawn 2025-08) for PAD-consideration note)
- ISO/IEC 30107-3 terminology (as summarized in Azure Face Liveness docs and NIST text)
- Azure AI Face Liveness — iBeta Level 1 & 2, 0 % penetration, ISO 30107-3, passive/passive-active, no data retention (Microsoft Learn, checked 2026-10-19)
- GDPR Arts. 9, 13/14, 25 and rights framework (implementation-level mapping, not new case law)
- IdDecoder (CODASPY 2023), Shahreza & Marcel (ICASSP 2024), TIFS 2024 — inversion-attack evidence supporting encryption-at-rest claims (full treatment doc 02 §1)
