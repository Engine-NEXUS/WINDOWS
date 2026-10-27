# Administrator access and Device Links: current data flow

The Administrator implementation sequence is tracked in [Feature 145](../features/145-administrator-control-implementation.md).

**Status 2026-10-19:** Device Links pairing control plane is implemented locally but not deployed or live-tested. Administrator access remains fail-closed. This map distinguishes shipped code from the planned verifier and transport.

```text
Command Hub (React) ──worker_request IPC──> Rust worker_auth
                                          │ attaches device token from OS vault
                                          ▼
                              Cloudflare Worker /v1/links/*
                                │                    │
                                ▼                    ▼
                       D1 link metadata       SQLite Durable Object
                                             signaling WebSockets only

Trusted Admin laptop --future signed verifier package--> target NEXUS laptop
Native STT --one-use exact transcript proof--> Rust Administrator command parser
Target mic + camera --future live verification--> Rust AdminSession
Rust AdminSession --future role/provenance--> central orchestrator / action policy
```

## Boundaries

- The Worker-issued device token identifies an installation. Device Links accepts an active registered device even when its profile lacks the separately approved Workers AI entitlement. Credential/OAuth and AI routes retain their own gates.
- Phone numbers are addresses, not authentication secrets. The verified binding is unique; six-digit SMS and connection codes expire after ten minutes and have attempt limits. The optional Administrator-owned SMS phone polls with its own token. No gateway heartbeat means new phone verification stops.
- D1 stores phone/link metadata and short-lived encrypted SMS code payloads. A daily trigger removes expired proof rows; if Free quotas are exhausted, cleanup retries on the next trigger. D1 does not store activity history, transferred files, voice recordings, face frames, or Administrator templates.
- The Durable Object relays only bounded WebRTC negotiation and presence. Activity reports stay disabled until a local opt-in switch and link-scoped sharing rule are implemented. It does not relay files. Transactional counters persist through reconnects and hibernation; a link revocation closes its room and blocks reconnection.
- Local Windows Activity Manager is opt-in and keeps 30 days of app-only estimated active minutes in an encrypted file. This local switch does not authorize sharing; no activity WebSocket frame is accepted.
- A linked device never becomes an Administrator. The Administrator command parser remains independent of Device Links and still refuses session opening. Native STT now issues only an exact, one-use capture proof; `admin_privilege.rs` expresses timeout and separate credential/export policy but has no production biometric proof source. A WebView `turn_context` alone cannot pass this gate.
- Rust's central orchestrator and action contract remain the sole execution owners. Future Administrator actions must enter that path with captured-voice provenance; neither Worker data, signaling frames, nor a WebView boolean may assert a biometric verdict.

## Cloudflare Free behavior

Workers Free, D1 Free, and SQLite Durable Objects Free have account-wide quotas. A quota failure becomes an unavailable/retry response; the deployment must never switch to paid billing automatically. Group file transport is planned as direct encrypted WebRTC with sender-held retry, since R2 usage beyond its free allowance is billable. Existing local NEXUS commands remain separate from the Worker quota.

See [Feature 143](../features/143-administrator-review-access.md) for the Administrator product contract, [Feature 145](../features/145-administrator-control-implementation.md) for the build sequence, and [changes 144](../changes/144-cloudflare-free-device-links-foundation.md) and [145](../changes/145-administrator-control-native-provenance.md) for current implementation status.
