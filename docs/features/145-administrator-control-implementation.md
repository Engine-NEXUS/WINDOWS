# Administrator control — build plan and gate ledger

**Date:** 2026-10-19. **Status:** implementation in progress; Administrator access remains disabled.

This plan continues [Feature 143](143-administrator-review-access.md) and the [identity research](../research/admin-privilege/README.md). Device Links pairing is separate. The goal is for the Administrator, physically at another NEXUS laptop, to pass separate voice and face checks plus a fresh spoken challenge, open one short session, and use that laptop's normal NEXUS capabilities through their existing execution gates. The laptop's owner does not approve session start. Credential access and export to the Administrator's laptop do require the device user's separate approvals once per session. The user accepted that a laptop owner who alters the local application can forge a local verdict; this design does not claim resistance to that attack.

## Trust and execution path

```text
native mic/camera capture
  -> Administrator candidacy before ordinary wake-owner rejection
  -> native STT proof + voice/replay + face/PAD + fresh spoken challenge
  -> signed, device-bound authorization lease (online revocation checked)
  -> Rust Administrator session (15 min maximum, 5 min idle)
  -> main center preparation + action-specific policy/OS/confirmation gate
  -> existing executor -> audited result -> existing speech/orb timeline
```

The WebView may display the ceremony and relay a transcript, but cannot submit a voice/face verdict, open a session, grant credential/export approval, or extend a session. A registered Device Link is not an Administrator credential. A captured transcript alone is not an identity check.

## Sequenced implementation

| Phase | Work | Gate before enabling |
|---|---|---|
| A0 — native provenance | Bind an exact STT transcript to a short-lived, one-use Rust capture proof; bypass frontend corrections for Administrator syntax; invalidate on cancellation. | Forged `turn_context`, altered text, replay, expired turn, and canceled STT cannot pass. **Started in this pass.** |
| A1 — trusted identity | Give each installation a device signing key. Separate Enroll Voice and Enroll Face on the trusted Administrator laptop after a native OS credential check. Store only encrypted templates and quality metadata; no retained raw media. Provide replacement/revocation. | Enrollment cannot complete with either factor missing or low-quality; no WebView-only enrollment; at-rest inspection finds no raw recordings. |
| A2 — first-device bootstrap | Target displays a short code. Trusted laptop signs an encrypted package bound to the target public key and device ID. Worker/D1 carry only a short-lived bootstrap record and revocation metadata. Target pins the Administrator public key and caches a signed lease for at most 24 hours. | Wrong device, reused/expired code, invalid signature, revoked identity, or unavailable first-use network denies. Offline reuse expires no later than 24 hours. |
| A3 — live ceremony | Branch Administrator candidacy before the ordinary wake-owner filter drops a non-owner. Require live speaker match, replay/spoof rejection, live face match and PAD, and a fresh phrase bound to this ceremony. Combine verdicts in Rust; no score or boolean arrives from UI. | Each factor's failure/unavailability denies; challenge freshness and cross-device error rates measured on actual Windows/macOS hardware. Only this phase may add a production session opener. |
| A4 — session and actions | Rust owns the 15-minute/5-minute clock and role. Every captured Administrator command runs through `prepare_action`, existing action executors, confirmation and OS gates. Separate once-per-session device-user approval for credential access and review export. Show the session and elevated actions on the device; audit denial and completion. | A denied action cannot refresh idle time or start a later compound step; cancellation and expiry close queued work. No route obtains privileges from a Device Link or a frontend role flag. |
| A5 — fresh review and delivery | Resolve local repository or PR from the target laptop, including its existing GitHub connection for read-only fetch. Generate a provenance-stamped review. Deliver to the user sidebar, or encrypt it for the paired Administrator laptop after an immediate session/export check; expire undelivered artifacts at session end. | GitHub token never leaves target; private PR read-only test; failed delivery is not reported complete; sensitive-content and recipient tests pass. |
| A6 — release evidence | Rebuild Windows and macOS; record genuine/impostor/spoof error rates, RAM and latency, offline/online revocation behavior, normal/Ghost/compound/confirmation/barge-in walkthroughs, and free-tier quota failure. | No platform ships as supported without its live hardware matrix and privacy review. |

## Current code boundary

`admin_privilege.rs` parses explicit commands and keeps sessions closed in production. The new native capture ledger is only a provenance foundation: it does **not** verify voice identity, face identity, liveness, or device authorization. Its implemented policy distinguishes sidebar review from paired-laptop export; normal actions and credential access need A4's real execution boundary. The ordinary wake-owner gate still rejects a different speaker before transcription, so A3 is required before the Administrator can use another laptop.

## Research and platform checks

- Windows desktop enrollment should use the desktop-appropriate OS credential API described by [Microsoft's UserConsentVerifier documentation](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.ui.userconsentverifier.requestverificationasync?view=winrt-26100); macOS can use [LocalAuthentication device-owner policy](https://developer.apple.com/documentation/localauthentication/lapolicy/deviceownerauthentication). Exact bindings and user prompts must be verified in the rebuilt apps.
- [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b/authenticators/) says biometric comparison based on voice **shall not be used** for its conforming authentication process. NEXUS's requested voice-plus-face design is a product-specific access rule and must not be presented as NIST-compliant.
- Cloudflare [D1](https://developers.cloudflare.com/d1/platform/pricing/) and [Durable Objects](https://developers.cloudflare.com/durable-objects/platform/pricing/) Free quotas can interrupt bootstrap and revocation checks. First bootstrap must fail closed when unavailable; offline leases have a hard expiry and cannot be silently extended.
