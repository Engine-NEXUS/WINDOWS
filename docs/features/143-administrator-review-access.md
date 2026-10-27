# Administrator review access — implementation record

**Date:** 2026-10-19. **Status:** partial, fail-closed foundation. The feature is not usable yet. The latest owner decisions and Cloudflare Free constraint below supersede conflicting older research.

The active build sequence and trust-boundary gates are in [Feature 145](145-administrator-control-implementation.md).

## Product contract

- Administrator access to another person's NEXUS laptop requires one live voice verification and one live face verification for each short session. A fresh spoken challenge belongs inside the voice check. Passing both opens the session immediately; the device user does not approve each session. Sending a review during that session does not repeat either biometric check.
- The installer packages the verification software, while Administrator voice and face enroll separately on the trusted laptop after an OS credential check. Administrator access remains disabled until both are complete. Raw enrollment media is not kept.
- A first-time device needs a one-time online trusted bootstrap from the Administrator's own laptop. Later sessions may use a signed cached lease for at most 24 hours when offline. The Administrator is physically using the target laptop's microphone and camera.
- An active Administrator session may use the laptop's existing NEXUS controls through their normal Action Kernel and OS gates. The device user approves credential access once for that session. The session lasts at most 15 minutes or five minutes idle.
- The Administrator may request a new analysis of any repository or pull request available to that laptop, including a private repository accessible through the device user's existing GitHub connection. The connection must be used for read-only repository/PR fetches; its token must never be copied to the Administrator or used for writes in this route.
- The Administrator chooses one of two destinations: the Administrator's paired laptop, or the device user's NEXUS sidebar. The paired-laptop route is an encrypted review-artifact transfer, not chat. The device user approves review export once for the current Administrator session. A queued review expires when that session ends.
- Device Links is independent: linked users may send arbitrary files directly after recipient approval, but a link never grants Administrator privileges. Its partial implementation is described in [change 144](../changes/144-cloudflare-free-device-links-foundation.md).
- Repository content can contain secrets or personal data; filtering and explicit disclosure handling remain required before external delivery. The blanket `personal_domain` deny-list in older research 01/07 is historical where it conflicts with the later owner decision to allow existing laptop controls with specific credential and export approvals.
- Voice and face alone cannot defend against the owner of the laptop modifying their local NEXUS installation and forging a local verdict. The owner accepted that limitation. Do not describe this design as tamper-proof, NIST-compliant, or guaranteed to detect deepfakes.

## Built in this pass

- `admin_privilege.rs` defines separate Administrator commands, strict review destinations, named repository/PR and current-workspace targets, captured-voice provenance, and a paired-laptop policy gate. No production code can open its session until real verification is integrated. These commands are independent of `admin_config::is_admin()` and the ordinary `voice_profile` wake-owner check.
- The Rust orchestrator claims explicit Administrator commands before generic NLU or Worker routing. Unavailable actions get a spoken refusal through the existing result/turn-close path, so a cloud model cannot claim to have sent a review.
- The parser rejects unsupported Administrator actions, missing destinations, non-repository targets, malformed repository names, and PR number zero.
- The session policy now measures the 15-minute absolute and five-minute idle limits and requires export approval for paired-laptop reviews. No production biometric verifier can create a session yet, so access remains closed. See [change 144](../changes/144-cloudflare-free-device-links-foundation.md).

## Required before enabling

1. Real Administrator voice enrollment, live speaker and replay checks, and fresh challenge response; per-device calibration and held-out attack measurements. Add a session controller with a short TTL that requires both checks from one ceremony. The current `wakeword_oww.rs` speaker gate and `verify_turn_audio` discard a non-owner before transcription, so this candidate branch must be added at capture time. The existing wake-owner verifier can fail open on feature-extraction errors and is unsuitable for Administrator grants. Disable raw command-audio debug dumps for Administrator candidates.
2. Real camera capture, face enrollment and matching, PAD/liveness checks, encrypted template storage, and a live Windows 11/macOS device matrix. Raw frames stay in RAM and are never stored.
3. Trusted first-device bootstrap, revocation, session audit, device-user transparency, and a pairing state for the Administrator laptop. Windows 10 export remains disabled for the first release.
4. A typed, read-only review route through the main center's `prepare_action` contract that resolves local and GitHub repositories without exposing the user's token, generates a fresh provenance-stamped artifact, and filters sensitive content before export.
5. End-to-end encrypted paired-laptop relay, delivery acknowledgement/retry, and a local-sidebar delivery route. Recheck the live session immediately before each delivery. No arbitrary file-send endpoint.
6. Rust/TypeScript/IPC/turn-end/hygiene gates, biometric red-team corpora, and rebuilt live voice walkthroughs. The current parser/session tests do not replace these gates.

## Spoken command grammar currently recognized

The following grammar is implemented for **recognition and refusal only** until the above gates pass. The accepted review prefix is `administrator`, `admin`, `as administrator`, or `as admin`.

| Intent | Examples |
|---|---|
| Start access ceremony | “Start administrator session”; “Open admin session”; “Verify me as administrator”; “Authenticate me as administrator”; “Administrator access” |
| End session | “End administrator session”; “Close admin session”; “Exit admin mode” |
| Check session | “Administrator session status”; “Am I in admin mode?” |
| New repository review to paired laptop | “Administrator, analyse owner/repo and send it to my laptop”; “Admin, review owner/repo and deliver the review to my paired laptop” |
| New PR review to paired laptop | “Administrator, analyse PR 24 in owner/repo and send it to my laptop” |
| New review to device-user sidebar | “Admin, review owner/repo and deliver the review to the user sidebar”; “As administrator, analyze the latest PR in owner/repo then send it to the user sidebar” |
| Current coding repository | “Administrator, review this repo and send it to the user sidebar”; “Admin, analyse the current repository and send it to my laptop” |

The parser also recognizes `to the administrator laptop`, `to the admin laptop`, `to the device user sidebar`, `in the user sidebar`, and `on the user sidebar` as destination endings. A review request without an explicit destination is refused.

This is an exact documented grammar, not a claim that every natural-language paraphrase is supported. Expanding it requires parser tests and review of the security boundary.
