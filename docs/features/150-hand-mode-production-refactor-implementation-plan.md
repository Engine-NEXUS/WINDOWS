# 150 — Hand Mode production refactor: implementation and release plan

**Status: PLAN, 2026-10-21.** No implementation or live verification is claimed here. Read the [research record](../research/hand-mode/02-production-refactor-research-2026-10-21.md) for evidence, open hypotheses, and external API references. The [Hand Mode master record](118-hand-mode-master-compendium.md) remains the interaction contract; [feature 117](117-cross-os-screen-adaptation-and-intent-plan.md) records platform limits.

## Goal, supported scope, and non-negotiable behavior

Deliver dependable **Windows live Hand Mode** so the user can move on to other features with an auditable release gate. Preserve palm→fist→palm→fist arming, fist-hold disarm/exit, 1-minute no-hand exit, Esc disarm, hotkey voice exit, physical-mouse takeover, preview, Hand+Ghost behavior, protected/elevated-window refusal, and camera-off-on-exit. The hand camera must not run before explicit mode entry. Keep raw video local and unsaved. macOS and Linux remain honest preview until platform-specific input and safety work is implemented and verified on those systems.

Priority order is **input release and stale-action safety → truthful feedback → bounded latency → feel tuning**. A faster cursor with a stuck drag or late click is a regression.

### Invariants to encode in tests and in the implementation

1. One active session generation owns all frames and output. Old frames, UIA replies, tick targets, and WebView callbacks cannot affect a replacement or stopped session.
2. Stop, disarm, physical mouse takeover, camera stall, and preview transition cancel pending movement and release every held mouse button/modifier. Release is not blocked behind UIA work or ordinary motion.
3. Moves may be overwritten; click, press, release, scroll, and gesture transitions may not be silently overwritten or reordered. No click executes after its deadline, after focus changes to a protected/elevated target, or after its session ends.
4. An armed cursor may move only with a fresh, valid hand target. It never drifts on stale velocity, a fist, a click lock, a gesture pose, lost tracking, or a paused camera.
5. The HUD reports camera starting/ready/error, disarmed/armed, live/preview, low delivered fps, and known GPU contention. No TTS, UIA query, or long allocation holds the `SESSION` lock across frame processing.
6. Hand Mode alone suppresses ambient wake detection; Ctrl+Space and the existing gesture/UI exit routes remain usable. Hand+Ghost preserves Ghost's voice behavior.

## Phase H0 — Capture a defensible baseline

**Work:** Re-read the touched files and current dirty tree. Verify the release provenance before a hardware comparison; do not test a stale executable. Add bounded monotonic stage timestamps and counters described in the [research measurement contract](../research/hand-mode/02-production-refactor-research-2026-10-21.md), or a temporary local measurement harness that can be removed after the comparison. Label track-derived capture age as a lower bound. Record actual track settings after all constraints, callback fps, intentional skips, inference, IPC turnaround, engine decision, output queue age, snap duration, OS injection return, and session generation.

**Files:** `frontend/src/hand/main.ts`, `payload.ts`; `src-tauri/src/hand/mod.rs`, `engine.rs`; existing `[HAND]` logging. Avoid broad changes to `orchestrator.rs` and `intent_parser.rs`.

**Gate:** A baseline table for bright/dim, charging/battery, light/heavy GPU load, and click/drag; timestamps can explain a visible stall. No camera image is saved. Keep log volume bounded and diagnostics opt-in where they contain landmark data.

## Phase H1 — Repair capability and state feedback

**Work:** Add `hand-cap` to the explicit Tauri capability list. Make `listen("hand:state")` registration awaitable and report failure through the Hand Mode error path instead of silently continuing. Verify the hand window receives `gpu_busy` and other state, and the stage chip receives the same event. Show a clear starting/ready/error state; preserve the existing beep feedback and solo voice isolation. Ensure closing the window ends the camera track and session.

**Files:** `src-tauri/tauri.conf.json`, `src-tauri/capabilities/hand-cap.json`, `frontend/src/hand/main.ts`, `frontend/src/stage/HandOverlay.tsx`, `src-tauri/src/hand/mod.rs`.

**Gate:** Fresh built app shows the chip at entry, ready/error after camera/model start, arm/live/preview transitions, and GPU/covered states. A denied camera permission produces an actionable error and no active cursor. Capability tests or a config check fail if `hand-cap` is removed.

## Phase H2 — Make release and stop independent of frame arrival

**Work:** Add a wall-clock watchdog for the last accepted valid hand frame and for all frame silence. At a short, measured threshold, stop motion and release a held drag; at a longer threshold, disarm and show the camera failure. Preserve the engine's existing 600 ms no-hand release and 5 s disarm for correctly delivered *no-hand* frames. Track actual injected button/modifier state in the output owner, so stop can release it even if the engine frame path stalled. Use session generation to invalidate queued actions. Do not rely solely on `Out::Stop` sitting behind an unbounded FIFO.

**Files:** `src-tauri/src/hand/mod.rs`, `engine.rs`; add focused fake-clock/output-state tests. Keep OS injection behind a small testable interface rather than trying to unit-test Enigo directly.

**Gate:** Inject a camera freeze while dragging, window destruction, stop, preview transition, Esc, and mouse takeover. In every case the held button/modifier releases within the specified deadline, and no later move or click occurs. The output thread exiting unexpectedly leaves a visible error and a best-effort release path.

## Phase H3 — Bound motion and isolate click snapping

**Work:** Replace the unbounded all-purpose `Out` FIFO. Use a newest-only motion slot and a small ordered control-action queue with explicit overflow behavior: fail closed by disarming and releasing if a discrete action cannot be accepted. Give stop/release priority. Run UI Automation lookup off the output thread with a hand-click deadline (initial experiment: 100–150 ms), no legacy slow fallback on this path, and at most one outstanding lookup. Tag the result with session, click ID, and foreground identity. The sole OS injector may use a timely snap, otherwise the original point after rechecking focus/guard; discard any late reply. Freeze or coordinate motion around the click so injected position and click target agree.

**Files:** `src-tauri/src/hand/mod.rs`, `hand/snap.rs`, a hand-specific screen lookup API in `screen.rs` if needed. Leave the shared 2,500 ms voice lookup budget unchanged.

**Gate:** A deliberately hung UIA provider cannot freeze cursor updates, delay stop/release, or produce a click later. Bursts of move targets use bounded memory and never replay a path after the stall. Protected/elevated-window checks still hold at injection time. Discrete events remain ordered.

## Phase H4 — Preserve newest frames and measure camera modes

**Work:** Give `FramePayload` a session ID and monotonic frame sequence. Accept only increasing sequences in Rust and impose a measured maximum age. Keep inference serialized unless the MediaPipe backend is moved to a dedicated worker; maintain at most one pending latest result while IPC is busy. Record rejected/stale counts separately from intentional camera/backoff skips. Correct the camera constraint call to specify desired resolution and frame rate together; inspect `getCapabilities()` and `getSettings()` rather than assuming support. Compare 640×480, 480×360, and 320×240 on real hardware. Adapt only after sustained low delivered fps with normal inference/IPC timing; use hysteresis and do not reconfigure during a press, drag, or gesture. Reinitialize timestamp estimates after a mode change.

**Files:** `frontend/src/hand/main.ts`, `payload.ts`, `src-tauri/src/hand/mod.rs`; tests for out-of-order and prior-session frames, camera mode decisions, hysteresis, and fallback.

**Gate:** No frame from an older session or lower sequence changes cursor/gesture state. Actual delivered fps and landmark/click accuracy are recorded for each selected mode. If a lower resolution does not improve usable tracking, the algorithm keeps the higher mode. Track and `requestVideoFrameCallback` paths are exercised on their supported platforms.

## Phase H5 — Smooth output without inventing hand movement

**Work:** A/B test an output ticker at the display-relevant rate (start at 60 Hz) against the existing frame-driven output. Have the engine supply timestamped **screen-space** targets and a bounded velocity estimate after filtering, gain, and monitor mapping. Interpolate or extrapolate only for a short measured horizon; decay to zero and hold at the last safe target when it expires. Clip to monitor geometry. Disable prediction during click lock, drag transitions, gesture poses, loss, disarm, or takeover. Do not run OS movement at 120 Hz unless measurements show a benefit that exceeds CPU/power cost. Keep stage preview behavior aligned with the real target.

**Files:** `src-tauri/src/hand/engine.rs`, `mod.rs`, `filter.rs`; overlay model only if its prediction display diverges.

**Gate:** Recorded traces show less visible stepping without more overshoot, stop drift, click displacement, or battery cost. A stale 10 fps stream holds or degrades honestly; it is never described as true 60 fps tracking.

## Phase H6 — Tune model startup and pointer feel from data

**Work:** Replay the same opt-in landmark traces and compare One Euro cutoff/beta, lead cap, 150 ms click rewind, and ballistic gain one variable at a time. Keep release/click accuracy higher priority than apparent speed. Trial a shorter GPU calibration only if it chooses the same delegate across cold/warm and contended runs; fall back to a longer trial if uncertain. Measure optional model-only warm-on-entry/reuse for startup, idle RAM, GPU and battery before enabling it. Never pre-open the camera. A worker-based MediaPipe path is conditional on measured main-thread blocking and browser compatibility, not an automatic rewrite.

**Gate:** Real-hand trials meet or improve baseline for small-target click placement, false clicks, still-hand jitter, fast movement overshoot, drag stability, arm time, and first-ready time in each supported condition. Rejected tunings are documented instead of accumulating constants.

## Phase H7 — Release proof and production sign-off

**Automated gates:** Focused Rust `hand::` and safety/output tests, frontend `payload`/camera-mode tests, TypeScript, frontend production build, Rust build/tests for the release feature set, IPC/turn-end/hygiene ratchets, and a fresh `nexus build` with provenance verification. Mutation-check safety invariants: remove stale-session rejection, release-on-stall, priority stop, protected-window recheck, or snap deadline and a test must fail. Preserve unrelated working-tree edits; inspect the diff before and after touching shared files.

**Live Windows matrix:** At least one 20-minute real-hand session on the release binary and a recorded checklist covering bright/dim, charging/battery, heavy GPU app, small controls, 100 deliberate clicks, quick repeated pinches, 20 drags, scroll, tab/window gesture, arming and every exit, camera cover/unplug, no-frame stall, protected/elevated foreground, physical mouse takeover, restart, and multi-monitor/DPI layouts actually supported for release. Record misses, false actions, p50/p95 age/injection times with timestamp quality, RAM/CPU/GPU, and whether the HUD matched reality. Repeat any failing case after repair; do not average it away.

**Acceptance:** Zero stuck buttons/modifiers, zero late actions after stop/takeover, zero stale-frame state regressions, zero click/drag/scroll/gesture injections into protected windows, and zero output-thread freezes caused by UIA during the matrix. Measure the [feature 115](115-camera-hand-mouse-plan.md) targets (≤80 ms median camera-frame-to-OS output, ≤2 px still-hand jitter over 3 s, ≥95% click placement within 12 px, fewer than one false click per 10 minutes) with the caveat that actual camera capture age may be a lower bound on some frame sources. If a target is missed, either improve it or explicitly narrow the supported hardware/lighting condition and rerun; do not call it passed by estimate. Record p95 and the worst visible stall as well as p50. A final owner feel check is required for click and cursor behavior on their laptop.

**Status update:** Only after these gates pass, write the real build identifier, hardware, conditions, measurements, remaining limits, and live proof into `docs/testing/live-proof-ledger.md`, the change record, scorecard, and `AGENTS.md`. Until then the status is “refactor planned/in progress,” not production ready. macOS and Linux live support must be reported separately, never implied by the Windows pass.

## Implementation review checklist

- Is each new queue bounded? Are moves replaceable while button and key transitions retain order?
- Can stop/release complete while UIA hangs, camera frames stop, the WebView closes, or IPC finishes late?
- Do all delayed callbacks compare session generation, event ID, freshness, and foreground safety before acting?
- Are wall-clock, performance-clock, and browser capture timestamps labeled rather than mixed silently?
- Does a lower camera mode demonstrably improve the *measured* result without lowering click reliability?
- Does prediction stop on stale input, and does the physical mouse still win immediately?
- Do preview, Ghost coexistence, ambient wake suppression, and all exit paths remain correct?
- Did the release executable, not only Vite/debug, pass the live hardware matrix?
