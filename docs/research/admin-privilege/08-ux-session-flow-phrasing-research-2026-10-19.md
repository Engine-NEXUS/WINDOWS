# UX, Session Flow & Phrase Catalog — Administrator Privilege

**Date:** 2026-10-19. Status: RESEARCH (nothing built). Companion docs: [01 overview](01-admin-privilege-any-device-research-2026-10-19.md) · [07 policy/threat model](07-threat-model-capability-policy-research-2026-10-19.md) · [09 master plan](09-master-implementation-plan-2026-10-19.md). Voice/face mechanics: [03](03-voice-identity-firewall-f1-research-2026-10-19.md), [05](05-face-verification-firewall-f2-research-2026-10-19.md).

**Purpose:** turn the four firewalls into a *lived experience* — every state, every spoken line, every retry timing and visual effect — in the voice of NEXUS's existing persona work (tone references: "Working on it, sir" stock acks; proactive-policy spoken lines; offer/confirm windows; caption stack). Final strings live as keys in one JSON so UX changes never touch Rust flow logic (house pattern: settings/voice strings already externalized).

---

## 1. Address style (owner decision recorded as default)

- The Administrator is addressed as **" Administrator"** — capital-A role word, NOT "sir" when the device-user persona owns "sir". Rationale: on a stranger's laptop, "sir" targets the device user's persona register; the admin is a *role*, so role words are less ambiguous and read authoritative. Optional variant pack B keeps "sir" for owners who prefer continuity (setting `adminAddressStyle`: `"role" | "sir"`; default `role`).
- Time-of-day greeting variants reuse the existing 3 time-of-day lines machinery (plans 123/124 boot greeting family).

## 2. Session state machine (complete)

```
S0 idle-device
 └─ wake/hotkey → capture 2-4 s
S1 candidate-evaluation            [F1 runs: identity ⊗ CM]
 ├─ device-user match              → normal user turn (today's path)
 ├─ open/ambient                   → normal open mode (today's path)
 ├─ ADMIN-LIKE + CM pass           → S2
 └─ admin-like but CM suspicious   → S2 (challenge will arbitrate) or decline
S2 verify-celebration              [camera opens; PAD ensemble; F3 digits]
 ├─ all pass                       → S3
 ├─ photo/screen/flat detected     → refuse → S6-fail (audit)
 ├─ digits wrong/missing (≤2 retry)→ re-challenge (S2 loop)
 ├─ no face / wrong face           → refuse → S6-fail
 ├─ camera unavailable/denied      → S7-unavailable
 └─ worker unreachable             → S8-offline (fail-closed)
S3 granted                         [greeting; token; scope:admin-read-share]
 ├─ idle 30 min                    → S4-expiry
 ├─ 15 min privileged activity     → re-challenge (S2-mini)
 └─ "exit admin mode" / Esc        → S5 close
S4 expiry   spoken close → S0
S5 close    spoken close + audit → S0
S6-fail     spoken line + audit → S0 (NIST backoff: 5 fails → 30 s, then exponential)
S7-unavail  spoken line → S0 (device user unaffected)
S8-offline  spoken line → S0
```

State-transition rules that matter to engineering:
- Camera opens **only** after F1's admin-like verdict (never on ambient speech) — doc 03 §8.5 occupancy-gate rule.
- All F2 frames deleted immediately (rejected frames too — the retention rule extends to failures, doc 02 §2).
- Every S2 → S6/S7/S8 exit records an audit line with the failure reason class.
- Backoff counter per device, cleared by a successful full ceremony.

### 2.1 Timings

| Transition | Target | Notes |
|---|---|---|
| wake-word end → S2 camera preview visible | ≤ 700 ms p50 (est.) | overlay respects the ripple/wake-cue timing; not yet measured |
| digits spoken → S3 greeting audio start | ≤ 2 s p50 (face+token issuance) | worker round-trip; measure in P5 |
| whole ceremony candidate→greeting | ≤ 4 s p50 / ≤ 6 s p90 | doc 01 §5 target |
| re-challenge min quiet gap | 1.5 s | avoid double-fire with talk-over |

## 3. Onboarding (first-run enrollment interview)

Voice + hub cards both guide it; spoken script (role style):

```
1. "Welcome, Administrator. Two quick enrollments and this device is yours to command."
2. voice:  "First, your voice. Read the phrase on screen."  → 3 pose phrases on screen ×3 s
3. face:   "Now your face. Look straight… left… right… up… down." (5 poses; framing overlay; quality gates)
4. worker: "Registering this device with the NEXUS cloud…" → success banner (feature-88 profile approve state)
5. "Enrollment complete. Say 'NEXUS', then the digits I show you, on any of your devices. Raw frames were never stored."
```

Failure UX during onboarding is identical taxonomy as §2 with gentler copy. Device-user template offers a *separate*, post-admin flow ("Enroll this user for faster recognition" — optional, default off; synonym for lookalike eviction note of doc 07 §1).

## 4. Phrase catalog (ready-to-embed string keys)

Style rules: role-register, no make-believe apology, no AI-theater, one line per state, no "as an AI". Digits format: "four seven nine".

### 4.1 Style A — `role` (default)

| Key | State | Line |
|---|---|---|
| `admin.candidate` | S1→S2 | "Voice check passed. Opening the camera for a face check." |
| `admin.challenge.show` | S2 | "For freshness, say the digits: <digits>." |
| `admin.challenge.retry` | S2 | "I need those digits again: <digits>." |
| `admin.granted.full` | S3 | "Good <time-of-day>, Administrator. <n> devices linked. How may I assist you?" |
| `admin.granted.short` | S3 | "Welcome, Administrator. What do you need?" |
| `admin.granted.devices0` | S3 | "Good <time-of-day>, Administrator. You're set up. How may I assist?" |
| `admin.face.fail.flat` | S6 | "That looks like a photograph. Face check refused." |
| `admin.face.fail.wrong` | S6 | "I couldn't confirm the Administrator on this camera. Staying in normal mode." |
| `admin.digits.fail` | S6 | "The freshness check didn't pass. Administrator session not opened." |
| `admin.backoff` | S6 (5 fails) | "Verification is cooling down for <n> seconds." |
| `admin.camera.denied` | S7 | "The camera isn't available on this device, so I can't verify an Administrator here." |
| `admin.offline` | S8 | "I can't reach the NEXUS cloud to authorize an admin session right now." |
| `admin.expiry` | S4 | "Administrator session expired." |
| `admin.close` | S5 | "Administrator session closed." |
| `admin.reauth.required` | revoke path | "The Administrator account for this device isn't active. Please re-enroll with the owner." |
| `admin.action.spoken` | per action | "<Action summary, past tense>." (e.g. "Shared the PR 254 review to the repo.") |
| `admin.share.ask` | confirm | "Share <artifact> to <target>? Say yes to confirm." |
| `admin.user-veto.hint` | S6 + config | "(The device user can pause admin sessions in Settings → Admin.)" |

### 4.2 Style B — `sir` (legacy register)

| Key | Line |
|---|---|
| `admin.candidate` | "Administrator voice detected, sir. Preparing verification." |
| `admin.granted.full` | "Welcome back, sir. Administrator channel is open." |
| `admin.granted.short` | "Good day, sir. How can I help?" |
| (all other S-states) | same as A |

### 4.3 Campaign defaults

- Greeting picks `full` when device-count data loads in ≤ 1 s, else `short`.
- Digits spoken by assistant use TTS with *numeral expansion* already in the caption stack (word timings reused for lip-sync check).
- All lines are ≤ 12 words where possible (latency register — "never silent, never late" family).

## 5. Visual layer (orb, captions, hub, banner)

| State | Orb | Caption | Hub/Windows |
|---|---|---|---|
| S2 | verification ripple (reuse wake-up ring anim family) | "verifying Administrator…" | none yet |
| S3 | coral→emerald pulse | greeting caption | **banner "Administrator session active"** persistent; session TTL shown |
| action in S3 | per existing action choreography | per line | Hub Admin activity list entry in real time |
| S6-fail | amber shake | failure caption | attempt logged |
| S7/S8 | grey idle | line | settings hint |
| S4/S5 | ripple-out | close line | banner clears |

Banner copy: "Administrator session — expires <hh:mm> · <n> devices linked". Esc cancels session (also cancels Ghost per current hotkey semantics — doc change record 128's symmetric-hotkey model).

## 6. Open questions (owner)

1. Variant `sir` vs `role` — decision recorded above as `role` (flag exists).
2. Device-count line: which source (Worker `me` route) — confirm cache TTL so greeting doesn't need a network round trip (est. acceptable: cached from last poll).
3. Should S2 show a *camera-consent first-time* helper card (OS dialog) — recommend yes with one-line spoken helper.
4. Does the banner need user-veto pause button embedded? — open in doc 07 §7.1.

## 7. Test-ready strings

Every key above lands in `frontend/src/admin/phrases.ts` (or Rust-side catalog file) with:
- per-key max-length lint (≤ 140 chars),
- digit placeholder formatting test (`<digits>` never renders as numerals in TTS path),
- snapshot test per state machine transition (doc 02's tests would cover once built — U-phase gate).
