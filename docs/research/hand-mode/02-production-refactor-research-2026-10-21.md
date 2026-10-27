# Hand Mode production refactor: evidence and research (2026-10-21)

**Status: research only.** No application changes or live tests were made for this document. The [implementation plan](../../features/150-hand-mode-production-refactor-implementation-plan.md) turns these findings into ordered work. The [Hand Mode master record](../../features/118-hand-mode-master-compendium.md) describes earlier field tests and the current interaction contract. For current implementation status, check `AGENTS.md` and source rather than treating an older plan as shipped behavior.

## Objective and scope

Make the camera pointer dependable enough to finish one feature and move on to others: it must never leave a button or modifier held, perform an old click after stopping, fight the physical mouse, or silently appear broken. Motion, clicks, and feedback must be measured on the actual camera and release executable. Windows live output is the first release target. macOS and Linux remain preview-only until their separate output, permission, protected-window, and hardware gates pass; see [feature 117](../../features/117-cross-os-screen-adaptation-and-intent-plan.md).

The attached latency discussion is a useful hypothesis list, not a measurement of the whole path. The old `[HAND]` reports include camera delivery, inference, and capture-to-Rust samples, but do not timestamp the actual OS mouse injection. Its proposed 60–120 Hz output and 320×240 camera mode require an A/B test on real hands before becoming defaults.

## Current path and evidence

```text
camera track or video callback
  -> synchronous MediaPipe detectForVideo in hand WebView
  -> at most one hand_frame invoke in flight
  -> Rust Engine: pose, arm, click, relative motion, filter, lead
  -> unbounded Out channel
  -> one OS output thread: move, click, press/release, gestures
       click may call UI Automation before injecting
  -> operating-system cursor
```

| Observation | Evidence | Verdict / consequence |
|---|---|---|
| `hand-cap.json` is not in the explicit `app.security.capabilities` list. | `src-tauri/tauri.conf.json`, `src-tauri/capabilities/hand-cap.json`, `frontend/src/hand/main.ts` | **Confirmed configuration defect.** The hand window's Tauri event listener is not granted the capability declared for it. Verify `hand:state` delivery, GPU feedback, and the stage HUD in a rebuilt app after the fix. Tauri documents that an explicit list selects only those capabilities. |
| Mouse output uses `mpsc::channel::<Out>()`; every frame may send `Out::Move`. | `src-tauri/src/hand/mod.rs::spawn_output_thread`, `apply` | **Confirmed unbounded backlog.** A blocked output thread can replay obsolete moves and clicks later. The queue must distinguish replaceable motion from ordered button/key actions. |
| The output thread calls `snapped()` before a click. That calls `screen::list_actionables()`. | `hand/mod.rs::snapped`, `screen.rs::list_actionables` | **Confirmed blocking path.** The fast query can wait up to 2,500 ms. On an unavailable fast query, the legacy path can take longer. A hand-click-specific deadline on a separate worker is needed; changing the shared voice/UIA budget is not the right fix. |
| Engine releases a drag after 600 ms of *no-hand frames*, but the no-frame watchdog only warns after 12 s. | `hand/engine.rs::step`, `hand/mod.rs::spawn_watchdog` | **Confirmed safety gap.** If frame delivery stops altogether, `step` is not called and a held drag may remain down. A wall-clock fail-safe must release without another frame. |
| One pending `invoke` causes new frames to be skipped. | `frontend/src/hand/main.ts::process` | **Confirmed behavior; bottleneck unproven.** Measure invoke wait and stale-frame rate. Sending 2–3 invocations concurrently risks out-of-order engine transitions unless sequence/session checks are in place. |
| Capture-to-Rust age drives 70% lead, capped at 90 ms and 4% of camera width/height. | `hand/mod.rs::hand_frame`, `hand/engine.rs::set_predict_ms`, `MAX_LEAD` | **Confirmed limits; tuning unproven.** The engine already consumes `t_capture_epoch_ms` when supplied. Raising the caps without trace and live click trials may overshoot. |
| `getUserMedia` requests ideal 640×480@30, then `applyConstraints` requests only a 24 fps minimum. | `frontend/src/hand/main.ts::openCamera` | **Confirmed request pattern; actual mode is device-dependent.** `applyConstraints` replaces earlier track constraints, so resolution and frame rate must be requested together and verified with `getSettings()` after each change. A lower resolution may improve delivered fps or may harm finger detail. |
| Prior field logs show about 15 camera fps on battery with low inference time, and contention can slow inference while another app uses the GPU. | `AGENTS.md` Hand Mode field records; [change 129](../../changes/129-hand-mode-voice-isolation-and-idle-exit-timer.md) | **Observed on this laptop, not universal.** Run controlled plugged/battery and bright/dim comparisons. Do not attribute every slow period to the camera or every slow period to GPU contention. |
| `detectForVideo` is called synchronously on the WebView page. | `frontend/src/hand/main.ts::process`; MediaPipe web guide | **Confirmed architectural cost.** A worker is an option if measured page-thread blocking or UI feedback latency justifies the extra frame transfer and compatibility work. |

## Corrections to the proposed latency batch

1. **A fixed output ticker is not a faster sensor.** At 15 camera fps a new sample arrives about every 67 ms, before inference and delivery. A 60 Hz output loop can interpolate or briefly predict between samples; it cannot know a new direction, stop, pinch, or hand loss before a fresh sample. Prediction must stop after a bounded age, and stop immediately for click lock, disarm, takeover, loss, or mode end. Measure visual smoothness and control error together.
2. **Use final cursor velocity, not raw landmark velocity, for output prediction.** The engine maps relative palm motion through a filter, ballistic gain, mirror setting, monitor geometry, and clamps. Replaying its normalized `vel` directly in OS pixels would disagree with the engine. An output target needs timestamp, position, screen-space velocity or a bounded interpolation horizon, session generation, and whether movement is permitted.
3. **Keep discrete actions ordered and cancellable.** A latest-wins move slot is appropriate; latest-wins click/press/release/gesture is unsafe. Press and release must be paired by state. Stop/disarm must invalidate pending work and release held input even when snap or frame IPC is late.
4. **Do not pipeline IPC merely because frames are skipped.** First add per-stage timestamps and measure invoke turnaround. If it is material, use a one-slot latest pending frame plus monotonic sequence and session IDs. Never let older inference results or an old WebView session re-enter the state machine.
5. **Do not assume 320×240 preserves click quality.** MediaPipe preprocesses and resizes input, but the camera still supplies the finger detail. Compare actual negotiated modes, hand reacquisition, pinch/curl recognition, click accuracy, CPU/GPU cost, and frame age. Do not change resolution during an active press or gesture sequence.
6. **Calibration and pre-warm are UX experiments.** A shorter delegate trial may pick the wrong backend under intermittent contention. Model-only pre-warm at app start may increase idle RAM/GPU use and complicate camera-on-only-while-active behavior. Try warm-on-entry and persistent reuse only after memory, battery, startup, and teardown measurements. Never open the camera before the user starts Hand Mode.
7. **State feedback belongs outside the frame-critical path.** Preserve the existing arm/disarm beeps, chip, and solo Hand Mode's ambient wake suppression. A ready cue may help, but speech must not block `hand_frame` or become an unsolicited conversational turn.

## Measurement contract for the next implementation session

Record one monotonic timestamp and session/sequence identity at each boundary: frame callback/arrival, inference start/end, IPC enqueue, Rust acceptance, engine decision, output target received, and OS injection return. Keep wall-clock capture timestamps separately and label their quality: browser-reported `captureTime`, track lower bound, or arrival-only. The `MediaStreamTrackProcessor` track timestamp path currently assumes its first frame is fresh, so its reported age is a **lower bound**, not proof of true sensor-to-cursor latency.

Report p50/p95 and counts over a fixed 5-minute window, with a compact per-event record for clicks, presses, releases, stale drops, UIA timeouts, camera stalls, and takeover. Distinguish:

- **Camera delivery:** actual `getSettings()` mode, callback fps, processed fps, intentional skips, and source.
- **Freshness:** best available capture-to-landmarks, landmark-to-Rust, Rust-to-injection, and capture-to-injection. Include timestamp quality and never silently equate arrival age with true capture age.
- **Responsiveness:** OS position versus engine target, frame-to-frame cursor spacing, stale target age, click target versus cursor at injection, and time to release after loss/stall/stop.
- **Cost:** inference p50/p95, main-thread long tasks, CPU/GPU/RAM, battery/charging state, and warm-start time.
- **Behavior:** arm attempts, missed/false clicks, drag completion, protected-window refusals, hand reacquisition, physical mouse takeover, and HUD truthfulness.

Test bright and dim light; charging and battery; quiet desktop and a heavy GPU app; 640×480, 480×360, and 320×240 *only when the camera actually offers them*; track and video-callback source where supported; slow/fast moves, small targets, repeated click/drag, hand loss, camera unplug, and a hung UIA target. Use the same landmarks for offline A/B comparisons. Keep raw camera frames local and unsaved; retain only opt-in bounded landmark traces already supported by `handTrace`.

## Research work plan and decision gates

| Study | Method | Decision it must resolve |
|---|---|---|
| R0: Reproduce the baseline | Fresh, provenance-checked executable; paired `[HAND]` log and observed cursor/video; inspect a normal click and a deliberately slow UIA target. | Identify whether the dominant delay is camera delivery, inference, IPC, engine/output, or snap in each condition. Record the current failure rather than assuming the pasted diagnosis covers every case. |
| R1: Camera format | On the same laptop and lighting, request combined resolution/fps constraints for each supported mode; read `getSettings()`; use the same hand motion and click script. | Select a mode only if delivered fps or frame age improves **without** worse landmark reacquisition, missed clicks, or false clicks. If the driver stays at 15 fps, do not claim an application-side fps fix. |
| R2: Queue and UIA fault injection | Delay the actionables lookup, burst move targets, stop mid-lookup, unplug camera during a drag, then check actual OS button and cursor state. | Define the output-owner and hand-click deadline that prevent a freeze, stale replay, and stuck input. This study is a safety gate regardless of its effect on median latency. |
| R3: Motion model | Replay identical landmark traces through frame-driven output, bounded interpolation, and bounded prediction; then do blind real-hand A/B use. | Keep the simplest model that reduces visible stepping without increased overshoot, resting drift, click displacement, or power draw. Do not infer true sensor fps from OS update count. |
| R4: Startup and delegate | Compare current 4+25 calibration with shorter trials across cold/warm, charging/battery, and contention; measure model-only warm-on-entry versus reuse. | Change calibration/pre-warm only if delegate choice remains stable and first-ready time improves without unacceptable idle RAM/GPU/battery cost. |
| R5: Platform boundary | Run the video-callback fallback on a real MacBook and verify camera permissions, fps, model startup, and HUD. Review feature 117 before any live-input port. | State exactly which platform behavior is proven. A Windows output pass does not certify macOS/Linux live control. |

Each study needs dated hardware, power and lighting conditions, build ID, raw counts, p50/p95, failure examples, and a decision to adopt, reject, or defer the hypothesis. Put successful live runs in the proof ledger during implementation; leave unrun studies marked pending.

## Production claim boundary

The existing [feature 115 targets](../../features/115-camera-hand-mouse-plan.md) include ≤80 ms median camera-frame-to-OS output, still-hand jitter within 2 px over 3 s, ≥95% click placement within 12 px, and fewer than one false click per 10 minutes. Treat these as candidate release targets to measure, not achieved facts. A 24 fps camera target cannot be a universal pass condition on hardware that delivers 15 fps; define supported lighting/device conditions and report a clear degraded state when hardware cannot meet them. “Zero frames dropped” is incompatible with intentional idle/GPU backoff. “60 fps at any inference rate” is physically unprovable as hand-tracking quality.

Windows live behavior must pass a fresh release build and a recorded real-hand walkthrough. macOS/Linux camera-preview behavior and live-input parity are separate claims. A written plan, passing unit tests, or a smooth-looking interpolated cursor alone does not close the production gate.

## Primary external references

- [Tauri v2 capabilities](https://v2.tauri.app/security/capabilities/) — explicit capability lists and window permission boundaries.
- [MediaPipe Hand Landmarker for Web](https://developers.google.com/edge/mediapipe/solutions/vision/hand_landmarker/web_js) — `detectForVideo` is synchronous on the calling page and preprocessing includes resize/normalization.
- [MediaStreamTrack constraints and settings](https://developer.mozilla.org/en-US/docs/Web/API/Media_Capture_and_Streams_API/Constraints), [applyConstraints](https://developer.mozilla.org/en-US/docs/Web/API/MediaStreamTrack/applyConstraints) — requested versus actual camera mode and replacement of existing constraints.
- [MediaStreamTrackProcessor](https://developer.mozilla.org/en-US/docs/Web/API/MediaStreamTrackProcessor), [requestVideoFrameCallback](https://developer.mozilla.org/en-US/docs/Web/API/HTMLVideoElement/requestVideoFrameCallback) — compatibility and timestamp limits for the two frame-source paths.
