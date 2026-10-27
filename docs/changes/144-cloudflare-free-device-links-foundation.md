# Cloudflare Free Device Links foundation and Administrator session policy

**Date:** 2026-10-19. **Status:** partial implementation; do not advertise Administrator access, file transfer, or activity sharing as ready.

## Product decisions this record implements

- Cloudflare Worker + D1 remain the control plane. The Worker uses a SQLite-backed Durable Object with WebSocket hibernation only for signaling. No R2 bucket or paid TURN relay is introduced.
- Device Links authenticate a registered, active device token without requiring the separate Workers AI entitlement. A link is direct between two explicit devices and cannot grant Administrator access.
- One verified E.164 phone number belongs to one laptop. To move it, revoke its old binding first. A recipient types the six-digit connection code shown on the sender's laptop. Links can be revoked by either member.
- The Administrator session's maximum duration is 15 minutes, with a five-minute idle limit. Review export to the paired Administrator laptop requires a separate device-user approval state. **No production code can create this session yet.**
- Strict $0 Cloudflare use is a deployment constraint, not an unlimited-capacity claim. Worker, D1, Durable Object, and Workers AI Free limits can stop cloud operations until their reset. The spare-phone SMS gateway may incur carrier fees.

## Built in this pass

1. `server/worker/src/route_auth.ts` adds `authenticateLinkedDevice`, which validates the Worker-issued device token and active device row independently of the cloud-AI grant. OAuth and credential routes retain their stricter entitlement check.
2. `server/worker/schema.sql` adds phone binding and OTP challenge tables, direct link requests and links, plus SMS gateway heartbeat state. A daily Free-plan Worker trigger deletes expired SMS challenges and pair requests, plus links 30 days after revocation. D1 stores no transferred file contents or activity history.
3. `server/worker/src/device_links.ts` provides phone verification, a short-lived encrypted OTP outbox, rate/attempt limits, connection-code issue/accept/decline, list, and revoke. A missing or stale SMS gateway fails closed. Phone and link codes expire after ten minutes.
4. `server/worker/src/link_signal.ts` adds a hibernating two-device signaling room. It forwards only bounded WebRTC negotiation and presence; file bytes and activity reports are rejected. Transactional Durable Object counters limit connections and messages across reconnects; quota failure stops forwarding. Link revocation closes connections.
5. `server/sms-gateway/termux-gateway.mjs` is an optional bridge for an Administrator-controlled Android phone with Termux:API. It heartbeats, polls the authenticated outbox, sends an SMS with `termux-sms-send`, and acknowledges the job. This bridge has **not** been run on a phone.
6. Rust's `worker_request` proxy now permits an exact Device Links control-route allowlist; gateway and WebSocket routes remain inaccessible from the web view. Command Hub has a Device Links page for number verification, direct pairing, listing, and revocation.
7. `admin_privilege.rs` now owns an expiring session state and denies paired-laptop review export without the session-scoped export approval. It still has **no production session opener**; this is a policy foundation, not biometric access.
8. `device_activity.rs` adds an opt-in Windows sampler that stores only foreground app names and estimated minutes in a locally encrypted 30-day rollup. Expiry pruning continues while recording is off. The Device Links page can enable it, show its history, and clear it. Sharing remains disabled.

## Deployment requirements before using Device Links

1. Apply the additive `server/worker/schema.sql` definitions to D1, then deploy the Worker **only after review**. Configure three distinct Worker secrets: `NEXUS_LINK_CODE_PEPPER`, `NEXUS_SMS_GATEWAY_TOKEN`, and `NEXUS_SMS_CODE_KEY` (32 random bytes, standard base64). Never put these values in source, logs, the web view, or screenshots.
2. Install Termux, Termux:API, the `termux-api` package, Node.js, and grant SMS permission on the gateway phone. Set `NEXUS_WORKER_URL` and `NEXUS_SMS_GATEWAY_TOKEN` locally on that phone and run the bridge. Verify an actual SMS on that device before allowing public pairing.
3. Confirm the account is on Workers Free. Do not activate a paid Worker plan or R2. Check Worker, D1, and Durable Object usage as adoption grows. The gateway's 30-second polling consumes about 2,880 Worker requests/day, plus roughly 360 heartbeat requests/day; this is a fixed account-wide cost in Free-plan capacity. Each signaling frame also reads and writes one SQLite Durable Object quota row, so signaling may exhaust its separate Free storage allowance.

## Verification performed

- Worker TypeScript: `npx tsc --noEmit` passed.
- Worker Vitest: 145/145 passed, including registered pending-device access without AI entitlement, revoked/suspended/wrong-token denial, phone validation, absent-gateway refusal, blocked activity/file signaling frames, and a reconnect-resistant signaling limit.
- Wrangler's local Worker + D1 simulation passed `server/worker/scripts/smoke-device-links-local.ps1`: both phones verified through the test gateway, sender issued a code, recipient accepted it, the pair appeared in the list, and one member revoked it. A second request was refused after the recipient removed their phone binding, and the recipient declined it. No production SMS was sent.
- Frontend Vitest: 368/368 passed; production frontend build completed with pre-existing Vite warnings after the Activity Manager UI was added.
- Rust `cargo check --tests --features custom-protocol,admin-brain`, five `admin_privilege::tests`, the targeted `device_activity::tests` test, and seven `worker_auth::tests` passed. Turn-end and repository hygiene gates passed. IPC and dead-code ratchets failed on concurrent unrelated files; neither listed a new item from these changes.
- Wrangler dry-run bundled the Worker and found the D1, KV, AI, and SQLite Durable Object bindings. It did **not** deploy anything. The 30-statement schema also applied to local Wrangler D1.
- Schema parsed with in-memory SQLite.
- Frontend TypeScript: `npx tsc --noEmit` passed.
- An earlier Rust test link attempt failed with Windows `LNK1104` while other Cargo builds held the artifact directory. A later targeted Activity Manager test linked and passed.

## Not yet built or verified

- Administrator voice and face enrollment, OS reauthentication, signed per-device template provisioning, liveness/PAD, fresh-challenge ceremony, revocation leases, Administrator action routing, fresh review production, and encrypted review delivery.
- Direct WebRTC file transfer, recipient file approval/save flow, durable sender retry queue, opt-in activity sharing, and macOS capture parity. The Device Links page currently manages pairings and local Windows activity only.
- D1 migration on the live account, Worker deployment, Worker secret configuration, live SMS, two-laptop connection, audible/visual walkthrough, and free-tier capacity measurements.
- The Wrangler dry-run created an untracked `server/worker/.wrangler-dry-run/` output directory. Automatic approval review rejected both cleanup attempts, so it remains for manual cleanup.

## Research and platform references

- Cloudflare [Workers Free limits](https://developers.cloudflare.com/workers/platform/limits/), [D1 Free pricing and failure behavior](https://developers.cloudflare.com/d1/platform/pricing/), [Durable Object Free pricing](https://developers.cloudflare.com/durable-objects/platform/pricing/), and [WebSocket hibernation](https://developers.cloudflare.com/durable-objects/best-practices/websockets/).
- Cloudflare [R2 activation and usage billing](https://developers.cloudflare.com/r2/get-started/) is why the strict-$0 design does not use R2.
- The [webrtc-rs data-channel examples](https://github.com/webrtc-rs/webrtc/blob/master/examples/README.md) inform the later direct transfer stage; no WebRTC transfer has been implemented yet.
- The [Termux:API project](https://github.com/termux/termux-api) supplies the optional local SMS command. SMS delivery and carrier pricing require a real-phone check.
