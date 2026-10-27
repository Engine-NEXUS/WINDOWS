# 161 — Focus Mode: Hand Mode Background Work Gating (2026-10-25)

## Overview

When Hand Mode (camera pointer and spatial gestures) is active, NEXUS now pauses unneeded background tasks and loops via Focus Mode (`src-tauri/src/focus_mode.rs`). This ensures cursor tracking and gesture recognition remain smooth and jitter-free without CPU/disk contention from background routines.

Nothing is killed, terminated, or dropped. Background loops simply skip their tick while focus is active and continue on their normal intervals afterwards. Spoken Medium proactive alerts are held (not spoken, and not expired) until Hand Mode ends; High and Critical alerts still break through.

## Changes

### 1. Focus Mode Core (`src-tauri/src/focus_mode.rs`, registered in `src-tauri/src/lib.rs`)
- Implemented pure `Controller` with injected monotonic clock (`now_ms: u64`):
  - `acquire(now_ms, reason) -> u64 (epoch)`: Grants a lease epoch; idempotent for same active reason.
  - `release(epoch)`: Releases lease; stale epochs are safely ignored.
  - `heartbeat(now_ms)`: Updates lease timestamp.
  - `tick(now_ms) -> bool`: Watchdog check; if lease held without heartbeat for `WATCHDOG_MS = 10_000`, forces release.
  - `force_release(reason)`: Unconditional cleanup for app shutdown.
  - `active() -> bool`: Read active status.
- Process-global singleton `static CONTROLLER` with thread-safe `AtomicBool` mirror (`PAUSED`) for hot-loop zero-allocation checks via `focus_mode::paused()`.
- Spawned a background watchdog thread checking `tick()` every second to safely recover if a caller drops without explicit release.
- `defer_log(name: &'static str)`: Prints `[FOCUS] <name> deferred` at most once per focus session per job name.
- Helpers `wait_until_clear(name)` (async) and `wait_until_clear_sync(name)` (sync blocking 2s sleep) for one-shot heavy tasks.
- Comprehensive unit tests using a mock clock covering lease acquisition, release, stale epoch rejection, watchdog expiration, heartbeat renewal, force release, and `paused()` atomic mirror synchronization.

### 2. Hand Mode Lifecycle Hooks (`src-tauri/src/hand/mod.rs`)
- Added `static FOCUS_EPOCH: AtomicU64 = AtomicU64::new(0);`.
- In `start()` (~line 1069): Reads setting `handFocusMode` (default `true`). When enabled, calls `focus_mode::acquire("hand")`, stores the epoch, and prints `[FOCUS] on`.
- In `stop()` (~line 1113): Atomically resets `FOCUS_EPOCH`, calls `focus_mode::release(epoch)`, and prints `[FOCUS] off`. Because all Hand Mode exit paths (manual stop, window closure watchdog, ghost termination, and idle exit) call `stop()`, release is guaranteed.
- In `spawn_watchdog()` (~line 1190): Calls `focus_mode::heartbeat()` on each 80 ms tick while the session is active.

### 3. Settings (`src-tauri/src/commands.rs`, `frontend/`)
- Added `pub hand_focus_mode: bool` with `#[serde(default = "default_true")]` to `NexusSettings` and `NexusSettings::default()`.
- Added unit test `hand_focus_mode_defaults_to_true_and_round_trips` to verify deserialization defaults and serde round-trips.
- Added toggle to `frontend/src/settings-sidebar/HandModeSection.tsx` and updated `SettingsSidebarApp.tsx` (`handFocusMode: boolean`, default `true`).

### 4. Background Loop Gating
The following periodic loops now check `if crate::focus_mode::paused()` and skip their tick:
- `src-tauri/src/app_registry.rs`: Window-cache enumeration loop (line 803; `try_focus_existing` retains live fallback). Daily app scan waits via `wait_until_clear_sync` (lines 248, 260).
- `src-tauri/src/memcore/session.rs`: Periodic snapshot recorder loop (line 678).
- `src-tauri/src/memcore/resume.rs`: Periodic state recorder loop (line 237).
- `src-tauri/src/device_activity.rs`: Window activity sampler loop (line 283).
- `src-tauri/src/memcore/curator.rs`: Daily memory curator loop (line 152).
- `src-tauri/src/memcore/review.rs`: Daily conversation reviewer loop (line 367).
- `src-tauri/src/memcore/routines.rs`: Routine suggester loop (line 412).
- `src-tauri/src/auth_vault.rs`: Vault token refresh monitor loop (line 387).
- `src-tauri/src/stt_diagnostics.rs`: STT diagnostic retention cleanup sweep (line 77).
- `src-tauri/src/tts_network.rs`: Network probe spawn check (line 239; probe state machine preserved).
- `src-tauri/src/google/sentinel.rs`: Sentinel background poller loop (line 288).
- `src-tauri/src/architect.rs`: Foreground window tracker loop (line 73).
- `src-tauri/src/tts.rs`: TTS cache refresh spawns (lines 216, 819) and `pregenerate_cache` (line 456) wait until focus clears.
- `src-tauri/src/tts_swap.rs`: Offline voice download worker loop (line 392) waits until focus clears.
- `src-tauri/src/lib.rs`: Startup/deferred heavy tasks (`tts::pregenerate_cache` at line 784, offline voice sync at line 808, NLU update check at line 981) wait until focus clears.

### 5. Proactive Spoken Alert Gating (`src-tauri/src/proactive_policy.rs`)
- Added `pub focus: bool` to `Signals` struct.
- In `collect_signals`: Populated `focus: crate::focus_mode::paused()`.
- In `evaluate`: Medium urgency arm treats `s.focus` as busy (`Verdict::Wait`), holding speech. High and Critical urgency bypass focus gating.
- In `accrue`: Focus intervals do not accrue against the alert's active speech deadline, preventing alerts from timing out or expiring as stale during extended hand gesture sessions.
- Added unit test `focus_mode_holds_medium_alerts_without_expiring_while_high_still_speaks`.

## Deliberately NOT Gated (and Rationale)

- **Wake word audio stream & silence recovery** (`vad.rs`, `wake.rs`): User must still be able to say "Nexus" or issue voice commands while using hand gestures.
- **Meeting detection** (`meeting_detect.rs`): Passive, low-overhead mic volume metric needed for audio state transitions.
- **Stage hitbox thread & watchdog** (`stage.rs`): A starved stage watchdog closes the Tauri stage window; hitbox detection is required for UI interaction.
- **Keep awake** (`keep_awake.rs`): Essential to prevent the OS from sleeping while user gestures in front of the camera.
- **Webhook & agent watch** (`webhook.rs`, `agent_watch.rs`): Passive network event receivers for developer tooling.
- **Return review** (`return_review.rs`): Triggers only on user presence return after absence.
- **Watch runner** (`watch_runner.rs`): Manages unattended shutdown observation.
- **Proactive policy loop / ticker**: Loop continues ticking to process signals; only the spoken Medium evaluation verdict is gated.
- **Mail / WhatsApp / leave-by / calendar poller loops**: Ingestion and triage continue so incoming notifications and sidebar cards are updated; only spoken output of Medium alerts is held.

## Verification Commands for Reviewer

```bash
# Rust syntax and unit tests
cargo check --lib
cargo test --lib focus_mode
cargo test --lib proactive_policy::tests::focus_mode_holds_medium_alerts_without_expiring_while_high_still_speaks
cargo test --lib commands::nexus_settings_tests::hand_focus_mode_defaults_to_true_and_round_trips

# Frontend typecheck and settings tests
npm --prefix frontend run typecheck
npm --prefix frontend run test -- src/settings-sidebar
```

## Not Run Live

- Physical camera pointer operation with active MediaPipe hand tracking.
- Subjective latency/jitter benchmark of hand tracking with background loops concurrently active vs paused.
- Real-world voice alert un-holding after closing Hand Mode.
