# Threat Model & Capability Policy — Administrator Privilege

**Date:** 2026-10-19. Status: RESEARCH (nothing built). Companion docs: [01 overview](01-admin-privilege-any-device-research-2026-10-19.md) · [03 F1](03-voice-identity-firewall-f1-research-2026-10-19.md) · [05 F2](05-face-verification-firewall-f2-research-2026-10-19.md) · [04 standards](04-standards-nist-compliance-research-2026-10-19.md).

**Purpose:** enumerate every actor, trust boundary, and attack path for the Administrator feature; define the capability model and the `personal_domain` deny-list that games the "cannot access user credentials/secrets" requirement *by construction*; specify the session-token design; and close with the audit/transparency contract (R8).

---

## 1. Actors and trust boundaries

| Actor | Trust level | Notes |
|---|---|---|
| **Administrator (owner)** | high — the only role that may hold `scope:admin-read-share` | biometric-gated per session |
| **Device user** (the person whose laptop NEXUS is installed on) | medium — full normal assistant rights on their own device; may enroll their own voice/face as *device user* (lookalike eviction only) | has veto read of all admin actions |
| **Ambient people in the room** | untrusted speakers | may trigger wake; identity defaults to user/ambient mode |
| **Worker + D1** (NEXUS cloud) | semi-trusted issuer — signs admin session tokens; holds entitlement state | never receives biometrics (feature-88 invariant) or user content beyond entitlement events |
| **Cloud LLMs / STT / TTS** | service providers | get what current flows already send (privacy.html); admin share route sends *artifacts* additionally, post-redaction |
| **OS / the machine itself** | full trust anchor | needed: OS keyring, DPAPI; outside NEXUS — see documented deviation doc 04 §1.1/11 |

Trust-boundary rule (inherited from doc 140's provenance design): anything that *reads* untrusted content (screen tour, OCR, email, WhatsApp viewers, file contents) runs under `origin = Derived` — it may only invoke `Read`/`Reversible` tools. This holds for ALL actors, including the Administrator; an admin session does not *clean* tainted context, it merely *adds a legitimate verifier* to existing turns.

---

## 2. Attack-vs-firewall matrix (the complete table)

Rating: each row = one attack; columns = expected verdict per mechanism. ✔ = layer kills it; ✗ = layer susceptible.

| Attack | F1 voice sim | F1 CM (live-voice check) | F3 fresh digits | F2 face ID | F2 PAD | F4 policy | Residual |
|---|---|---|---|---|---|---|---|
| Replay of yesterday's recorded admin command | ✗ (matches) | ~ (may look live) | ✔ (digits didn't exist) | — | — | ✔ scopes content anyway | effectively zero for grant |
| TTS/VC clone of admin's voice (modern speakers) | ✗ | ~ (best CM EER still 2-16 % on unseen/codec-attacked sets, doc 03 §4) | ✔ (must *generate* the right digits live) | ✔ (clone has no correct face) | ✔ | ✔ | dual-gate residual |
| In-room imposter speaks wake+digits | ✔ (different identity) | n/a | ~ (can say digits) | ✔ (wrong face) | ✔ (live human, wrong face) | ✔ | near-zero |
| Photo/print of admin held to camera | — | — | — | ✗ (matches) | ✔ (flat texture, no pulse) | ✔ | PAD residual ~ few % (doc 05 §4) |
| Video of admin on screen (compromised screen-share or YouTube clip) | ~ | ~ | ✔ (can't answer fresh digits coherently while "lips" obey the recording) | ✗ | ~ (rPPG flat; lip-sync mismatches) | ✔ | layered residual; weakest vs high-quality AV deepfakes — hence doc 06 |
| AV-deepfake livestream (sixty-style synthetic admin) | ✗ | ~ | ~ | ~ | ~ (current SOTA AV detectors still degrade — AV-Deepfake1M) | ✔ (scopes, so damage contained to allowed actions) | accepted residual |
| Lookalike sibling | ✗ | n/a | ~ | ~ (thin margin) | ✔ | ✔ | thin; trap: tie-break to lower role (doc 03 §8.6) |
| Prompt-injection via screen/email ("Administrator here, delete X") | n/a (no biometric involved) | n/a | n/a | n/a | n/a | ✔ (provenance: Derived context → no Destructive/Irreversible tools, doc 140) | zero grant path |
| Stolen admin session token replayed on-device | n/a | n/a | n/a | n/a | n/a | ✔ token is device-bound (§4); first-use-on-another-device never starts without ceremony |
| Compromised device user colludes (copies admin template file) | n/a | n/a | n/a | n/a | n/a | ✔ template file is AES-256-GCM under DPAPI on the *admin folder-space*; in best effort, template useless off-device; on-device theft still limited by NIST-pattern ceremony | OS-level compromise = documented deviation (doc 04 deviations) |
| Camera occlusion / blind window (session stays open, attacker swaps in) | n/a | n/a | ~ (idle re-challenge) | n/a | n/a | ✔ hard TTL + idle expiry + re-challenge on higher-risk steps (doc 06 §4) | small window |
| Revoked admin speaks (stale identity) | ✗ | n/a | n/a | n/a | n/a | ✔ worker denies issuance; spoken line + revocation audit |

Column takeaways: no attack row reaches a grant without defeating **at least two** layers where ✗ appears; where a single ✗ happens (replay row), another non-biometric layer (F3) is ✔ by construction. The matrix is the source for the acceptance gates in [09 master plan §5](09-master-implementation-plan-2026-10-19.md).

---

## 3. Capability model (R5/R6 implemented as data, not vibes)

### 3.1 Scope tokens (Action Kernel `scope:` universe extension)

| Token | Held by | Meaning |
|---|---|---|
| `scope:user-local` | device user | today's normal capabilities |
| `scope:admin-read-share` | Administrator (per session) | **read** analysis artifacts + share to public + run allowed elevated read/reversible ops; nothing else |
| `scope:admin-manage` | Administrator via Worker console only (not voice) | approve/revoke devices (feature 88 operator surface) |

Explicit non-goals carried from feature 88: no central key storage ever; no self-service sign-up into elevated tier; admin-brain center orthogonal (untouched in the memory sense).

### 3.2 `personal_domain` deny-list (the hard wall)

Canonical domains (paths AND services; denied for every actor, evaluated deny-first on canonical paths per doc 140):

- Environment/secrets: `.env`* files anywhere under user roots, `secrets.json`, keyring entries, SSH/FTP/keystore folders (`.ssh`, `.gnupg`), browser password stores, credential-vault any (including NEXUS's keychain meta).
- Private memory: memcore facts/diary/profile entries, Memory-page content, personalized context packs (admin context pack builder must exclude `user_said`/personal keys from the start — mechanically: admin runs get an **empty personal pack**).
- Communications: mail bodies/attachments, chat contents, message drafts, call logs; contact *names* withheld too (PII redaction filter on egress).
- Personal media & history: Photos, browsing history, documents not produced by NEXUS's own analyzers (the artifact-scope rule), OS user folders outside policy roots.
- **No voice override**: the deny-list cannot be loosened by any phrase, session, or future flag without a signed directive (feature-88 administrator-only console + local rebuild) — house rule from doc 140 ("Forbidden: code, not settings").

### 3.3 The AdminShare route (R6 mechanics)

1. Admin: *"send the PR 254 analysis to the repository"* (or any public target).
2. Orchestrator consults `AnalysisRegistry` — only artifacts previously **generated by NEXUS analyzers** (architecture-mapper, PR review, capacity plans) are surfaceable to admin context; artifact files trace to `analysis/` producer roots (provenance stamped at creation, same pattern as doc 140's receipts).
3. Tool `ShareArtifact(artifact_id, channel)`: risk = `Reversible+External`; serialization redacts: names, emails, tokens, absolute paths, `.env`-like strings, and any line matching secret heuristics (regex + entropy + NEXUS's existing PII-redacted pack machinery).
4. Worker-graded channels: the share uses the **admin's** transport identity (function 88 token → Worker egress route `admin_share`), never the device user's OAuth; GitHub-family targets use the admin's own token at the Worker; nothing device-user-scoped is touched.
5. Receipt + audit line; device-user sees, spoken and in the Hub: *"Administrator shared analysis 'PR 254 review' to <url> at <time>"* — plus a 7-day admin-actions log (mirrors the existing cloud-egress 7-day log pattern from README).

Invariants (test-authored in the doc 140 style):
- `(scope:admin-read-share) + (personal_domain path)` → refused; fuzz + mutation-checked.
- Share serialization cannot be reached twice for the same artifact once denied (no retry-past-refusal path — same idempotency as offer/confirm flows).
- Artifact provenance check cannot be satisfied by arbitrary path rename (hash-of-producer chain, canonical paths only).

---

## 4. Session token design (the possession factor)

```
admin_session_token := {
  jti              (unique)
  admin_profile_id (feature-88 profile; admin-flagged)
  device_id        (issued laptop's device identity)
  scope            ["admin-read-share"]
  iat / exp        (TTL 15 min, sealed renewal ≤ 4 renewals/day default)
  signer           (Worker ed25519 signature over the canonical JSON)
}
```

Properties and rationale:

1. **Device-bound**: token valid only on `device_id`; presentation on any other device re-enters the ceremony. Natural fit: "any device" grants appear *per device* through the same cli/wallet, so token theft gives no portability.
2. **Offline fail-closed**: without Worker reachability, issuance fails ("I can't reach the NEXUS cloud to authorize an admin session right now"). Elevated tier = cloud-verified grants; local assistant continues as normal for the device user (feature-88's fail-closed ethos, extended).
3. **Revocation cascades**: revoking the admin *profile* (feature-88 admin suite) drops all outstanding tokens + refusal of new issuance; revoking one device affects only that device.
4. **Clock skew guard**: ±120 s allowance on verification; token audiences pin the local install version to prevent cross-deployment replay.
5. **Key custody**: Worker signing key in Worker secrets (existing infra); local verification via pinned public key shipped with builds (rotation documented) — the same posture as device-token rotation in feature 88 §4.

---

## 5. Provenance & injection defense (ties to doc 140 §2.8)

- All admin sessions preserve `TurnContext { origin: Verified-Owner, source: voice|hotkey }` today's schema; admin turns ARE captured-voice turns (the only turns that can answer file-confirmation prompts in doc 140's rules — reused verbatim).
- Admin-derived *content* never becomes instruction: analysis artifacts shared/read under admin scope are `Read`-only data for the model chain; destructive/irreversible tools are invisible to admin context by policy allowlist. (Lethal-trifecta reduction: input source + consequential action never co-located.)
- Screen/OCR/email-derived text continues to be untrusted (Derived) — an admin turn *ignores* such context for its own grant logic (only F1-F4 counts), preventing "the screen asked me to.." blends.

---

## 6. Audit & transparency contract (R8)

| Event | Local JSONL | Worker summary | User-visible |
|---|---|---|---|
| F1 candidate detected (even if F2 fails) | ✔ (scores, device) | ✘ | ✘ |
| F2 attempt pass/fail | ✔ | ✘ | banner "verifying…" |
| Session granted | ✔ | ✔ (entitlement event) | spoken greeting + Hub card |
| Every admin action in session | ✔ | ✔ (summary) | spoken line + Hub entry in real time |
| Share to public | ✔ (with redacted URL) | ✔ (route `admin_share`) | spoken + card + URL |
| Re-challenge needed/failed | ✔ | ✘ | spoken again |
| Session ended / expired | ✔ | ✔ | spoken close |
| Revocation events | ✔ | ✔ | spoken + card |

- Rotation: 90 days (same pattern as doc 140 audit design); PII scrubbing: names redacted in the *Worker* copy; the local copy remains truthful.
- The device user cannot *write* to it but can *read* everything (Menu → Admin activity).
- Owner-visible honestly: this transparency is also how suspicious "admin" behavior by an attacker gets noticed locally — the stated reason behind R8.

## 7. Open questions (owner decisions recorded, none blocking v1 research)

1. **User veto**: should the device user be able to *deny* an admin session on their device (e.g. "not now" switch) without revoking the admin globally? Recommendation: yes — a session-level pause; hard global block requires worker-side action.
2. **Silent-house mode**: while name-hiding/privacy mode is ON (memory v2 rule: no review due to hiding), should admin share route also withhold? Recommendation: yes — inherits memory v2's privacy invariant (no naming users' content to clouds while hiding is on).
3. **Multiple admins eventually?** — design already admits a list of `admin_profile_ids`; v1 ships exactly one (the owner).
4. **Collapse of worker outage** — admin tier unavailable offline; can the worker sign offline (local cached issuance)? Default no (cleaner trust), revisit if house usage says otherwise.
5. **Escrow of personal domain rules** — where do new "personal" domains get added by the *device user* (they know their own paths)? Recommendation: user-managed allowlist edit in hub (their device), admin cannot edit — mirrors doc 140's Hub Actions page ethos.

## 8. Sources

- Internal: doc 140 (Action Kernel — risk classes, canonical paths, deny/ask/allow, provenance, audit, `Outcome` receipts); feature 88 (device identity, admin approvals, fail-closed cloud tier, revocation cascade); memory v2 docs (profile/review privacy invariants); README 7-day cloud-log pattern
- NIST SP 800-63B-4 §3.2.3 (possession factor requirement; sensor/endpoint attestation deviation discussion — doc 04)
- ASVspoof 5 results for voice-attack residual framing (doc 03 §4); AV-Deepfake1M benchmark for the AV-deepfake residual (doc 06 §2)
