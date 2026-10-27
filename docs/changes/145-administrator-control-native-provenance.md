# Administrator control: native capture provenance and session policy

**Date:** 2026-10-19. **Status:** first implementation phase. Administrator access is still disabled.

## Why

`orchestrator_process` receives `turn_context` from the WebView. Its presence alone cannot prove that an Administrator command came from live microphone capture. The existing wake-owner filter also rejects an unfamiliar speaker before STT. These are blocking trust boundaries for the requested cross-laptop Administrator mode.

## Built

1. The native STT receiver now records a 60-second, one-use proof for an Administrator-shaped transcript before emitting it. The Rust orchestrator consumes that proof by capture session and **exact** text when an Administrator command arrives. Missing, altered, replayed, stale, or canceled proofs deny capture provenance. No Tauri command mints or submits a verdict. The proof stores at most eight recent Administrator-shaped texts in RAM.
2. Administrator phrases from native STT now bypass the frontend's learned text corrections and local intent executor, preserving the exact text for Rust's strict parser. The capture session number is carried through the existing frontend turn metadata.
3. The session policy distinguishes user-sidebar review from paired-laptop review export. Its absolute and idle clocks remain in Rust. A policy check does not refresh the idle clock; the future executor must record completed actions separately. Credential and general action authorization are still planned and have no executing route. The production session still has **no opener**.
4. Removed the older rotating raw WAV debug writes from command capture and wake verifier paths in source. Audio still lives in RAM for STT. A rebuilt application is needed for this change to take effect. Ten legacy debug clips were found under `%APPDATA%\com.nexus.assistant\verify_debug`; automatic approval review rejected their deletion in this task, so they remain locally until removed manually.
5. [Feature 145](../features/145-administrator-control-implementation.md) records the sequence for OS-gated enrollment, signed device bootstrap, live voice/face/challenge ceremony, action policy, review delivery, and hardware acceptance.

## Verification

- Rust `cargo check --features custom-protocol,admin-brain` passed after the debug-write removal.
- Seven targeted Administrator Rust tests passed on the final policy code after the debug-write removal.
- Frontend TypeScript and production build passed; 369/369 Vitest tests passed, including Administrator phrase routing. Vite still reports its existing Lottie `eval` and Ghost dynamic/static import warnings.
- Turn-end and repository hygiene gates passed. A scoped `git diff --check` on the modified implementation files passed.
- IPC contract gate fails on unrelated existing/concurrent commands and events. The dead-code ratchet fails on unrelated `tts.rs` and `vision.rs` items plus resolved baseline entries; it reports no new item from this work.
- No live voice, face, OS credential, two-laptop, review, or Administrator control walkthrough was performed.

## Remaining release blockers

The native proof establishes capture provenance only. It does not authenticate the speaker. Voice/face enrollment and models, challenge freshness, signed target-device bootstrap, revocation, trusted session opening, action role routing, device-user approval UI, and review execution/delivery remain unbuilt. The ordinary wake-owner gate still blocks an unfamiliar Administrator candidate before transcription. Do not describe Administrator control as operational.

## References

- [NIST SP 800-63B-4 authenticator guidance](https://pages.nist.gov/800-63-4/sp800-63b/authenticators/) explicitly excludes voice biometric comparison from its conforming authentication process. This product-specific design must not claim NIST compliance.
- [Microsoft UserConsentVerifier](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.ui.userconsentverifier.requestverificationasync?view=winrt-26100) and [Apple LocalAuthentication](https://developer.apple.com/documentation/localauthentication/lapolicy/deviceownerauthentication) inform the later trusted enrollment credential gate.
