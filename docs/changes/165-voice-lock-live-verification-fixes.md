# Voice Lock live verification — failures and repairs

Date: 2026-10-25. Speaker feature remains 41% (2.05/5), in progress. This follows [P2](164-voice-lock-native-state-ui.md).

## Real first-candidate result: failed

Frozen revision 9c3f0320a5c7df359c6a9769e8492ed48baf8fcd produced release 021cba2bac4e4bed after 42m51s. Source, bundled frontend and executable hashes verified before launch, PID 31624. Full shared checks passed: Rust 2,532 library + 19 integration, frontend 678, Worker 145, Python 60; repository guards and native/UI mutations.

On the real laptop, UI Automation plus native IPC confirmed:

- Starting effective and saved lock were both On; the encrypted five-clip profile was valid.
- Draft Off left native On until Save.
- Save Off produced native/saved Off; Save On produced native/saved On.
- Escape and Ctrl+Shift+S reopened Audio with effective/native On.

Two live failures prevent completion:

1. The footer exposed Saved to accessibility before any Save. It hid the text with opacity only; the accessibility tree still announced it.
2. Opening the legacy settings window through open_settings_window never completed. The app became Responding=false, and the stage watchdog logged a blackout. The installed Tauri 2.11.5 WebviewWindowBuilder docs explicitly warn that synchronous commands/event handlers can deadlock window creation on Windows and recommend async commands/separate threads.

The original On state was restored before the failed legacy-open attempt. No profile was changed or deleted; no owner/impostor voice trials were counted. The unresponsive assessed process was stopped after checking its exact executable path. [Ledger](../testing/live-proof-ledger.md), [first assessment](../testing/runs/windows1-20261025-voice-lock-ui-frozen-assessment.json).

Native screenshots did not represent the target UI because these NEXUS windows intentionally use WDA_EXCLUDEFROMCAPTURE. UIA/native evidence is kept distinct from visual proof. The replacement protocol uses browser-rendered WebView captures without changing that native capture-exclusion policy.

## Narrow repairs

- VerifiedSaveStatus keeps the footer layout but removes unverified message text and marks its status region hidden from accessibility until a matching native Save is confirmed. Refresh invalidates old Save confirmations.
- Untouched Voice Lock drafts now follow fresh native state, including a delayed initial settings load. Explicit local edits remain dirty and are preserved. Code review found the previous stale-draft/dirty-set gap; new failing tests exposed it before its fix.
- The legacy opener is async, and its tray entry dispatches and awaits it on Tauri's async runtime. A real-function Future contract regression and an isolated synchronous-opener mutation guard the dispatch contract; they do not substitute for a real-window smoke test.
- Voice Lock switches have accessible names; the legacy switch also exposes checked/disabled state.

No native authorization, profile threshold, default-off rule or voice command safety rule was relaxed. The primary checkout's concurrent edits were preserved.

## Replacement assessment

Frozen revision 3e5b4a803b2a22bc28986189377942f127234fa7, input 342c6f390461e0c69d0c6aa6b66536d84d9cd2c16423f0f8e35604d206dc9124. Frontend TypeScript/build and 687 tests pass; 31 focused tests, all 11 UI mutations and the one async-opener mutation pass. The frozen full shared rerun passes: Rust 2,533 library + 19 integration (38 ignored), frontend 687, Worker 145, Python 60, NLU and all repository guards; dead-code remains 148 known, none new. All 33 UI/window/audit/speaker mutations pass. The optimized replacement release is still compiling; repaired-source live proof is pending. [Replacement status](../testing/runs/windows1-20261025-voice-lock-ui-v2-assessment.json).

Repeat the real settings checks after fresh provenance verification. Also check a bounded legacy open with continuing stage heartbeat, cross-window state changes, no premature accessible Saved message, controlled Save refusal/fake acknowledgement/delay and profile-status failure, then restoration. The legacy Save's unrelated startup-task call must be isolated during this Voice Lock test. Fault injection is labelled explicitly and never changes the owner's profile. Owner voice/replay/recovery gates remain pending; no queue advancement.

## Replacement device result — 2026-10-26 IST

Release `0b10fd3a1416a540` finished in 61m37s, launched as PID12064, and passes provenance before/after the real Windows checks. Shipping Hub and standalone settings pass native draft isolation, Save Off/On/reopen, accessible acknowledgement, cross-window refresh, explicit draft preservation and bounded settings open (640ms, process responsive, no blackout in run logs). Confirmed fetch-intercept simulations pass false acknowledgement, refused/delayed Save and profile-status error/recovery. The initial invoke wrapper could not replace Tauri's immutable function and is excluded from fault proof. Original native/saved On, five-clip profile and wake listening restored; all interceptors removed.

The native standalone window renders Command Hub by default; the separate legacy prototype was not device-tested. Actual profile corruption/deletion/recovery and owner/non-owner/replay/capture trials remain pending. [Detailed evidence and qualifications](../testing/runs/windows1-20261025-voice-lock-ui-v2-assessment.md). This supersedes the compiling/live-pending checkpoint above for shipping settings only; feature stays 41%.
