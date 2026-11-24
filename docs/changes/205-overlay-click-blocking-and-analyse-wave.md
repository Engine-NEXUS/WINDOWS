# 205 — The full-screen overlay no longer blocks clicks and copy; the "analyse my screen" wave

Date: 2026-11-22. Branch `fix/overlay-click-blocking-and-analyse-wave`. Owner report: NEXUS blocked clicking and copying in Brave.

## 1. Root cause: the orb rectangle stayed interactive

The stage is a full-screen transparent window that is click-through except over registered "hitbox" rectangles (`stage.rs`
polls the cursor and flips click-through). Every wake called `show_orb_interactive`, which registered the orb's rect, a
200x200 logical square (400x400 physical at 200 %) at the top centre of the screen, as an interactive hitbox. Only the
frontend cleared it, and only when the orb state changed to `idle`. A wake with no speech (the log shows several) could leave
the rect in place for good, so the top-centre strip, where Brave's tabs and address bar are, swallowed every click.
The orb has nothing to click: it needs to be interactive only during animation calibration (drag and wheel resize).

## 2. What changed

| Fix | Where |
|---|---|
| The orb is a hitbox only during calibration. `show_orb_interactive` registers nothing; `set_orb_interactive(true)` is refused outside a calibration session; the frontend asks only through a `calibrating` flag | `window_manager.rs`, `OrbFrame.tsx` |
| Every hitbox source expires unless refreshed (60 s; 180 s for `tour`, `spatial`, `annotation`); calibration sources are pinned for the session | `stage.rs` (`HitBook`) |
| A rect covering more than 35 % of the screen, an empty rect, or an off-screen rect is refused; rects are clamped to the screen | `stage.rs` |
| The stage never takes keyboard focus (`WS_EX_NOACTIVATE`), has no Alt+Tab or taskbar entry (`WS_EX_TOOLWINDOW`, no `WS_EX_APPWINDOW`). A click on a hitbox used to activate the overlay, so the next Ctrl+C went to NEXUS. The bits are flipped directly (tao's `set_ignore_cursor_events` rebuilds the style and drops them), and re-checked every loop pass | `stage.rs` (`apply_click_through`, `stage_ex_style`) |
| A panic in one loop pass can no longer leave the overlay interactive: the loop forces click-through and restarts | `stage.rs` (`spawn_hitbox_loop`) |
| `[HITBOX]` log lines: the rect set whenever it changes, expiry, refusals, and a warning if the overlay stays interactive for 20 s | `stage.rs` |

## 3. Also in this branch

- **"Analyse my screen" wave** (`docs/features/195-screen-analysis-wave-animation-plan.md`): one ring leaves the orb and covers the screen, the owner's reviewed settings (Rings, one ring, strength 80, the review page's 0.25x speed = 5600 ms). It fires at NEXUS's acknowledgement (`screen:analyse_wave`, `orchestrator.rs`), is click-through, clears on Esc, "stop" or the end of the tour, and plays a soft fade under reduced motion. `AnalyseWave.tsx`, `analyseWaveModel.ts`, `analyse-wave.css`, dev page `frontend/dev-analyse-wave.html`.
- **Cloud voice default**: a fresh profile saved the Kokoro id `af_sky` as the cloud voice and Edge rejected it, so every reply fell back to the local voice. The setup wizard now accepts only a cloud voice id and the built-in default is `en-US-AvaNeural`.
- **Setup keys**: the "Free AI Keys" card is the first thing in setup step 2 ("Keys & Preferences") instead of below the volume slider.
- **Local speech path**: `lazy_stt::stt_script_path` returned early for an app installed in a shallow folder (for example `C:\NEXUS-Dev`), so the offline speech server was never found.

## 4. Verification

- Frontend: `tsc` clean, vitest 1241/1241 (includes the 9 wave tests and `orbHitbox.test.ts`); IPC, window-opacity, hard-coded-colour, turn-end, hygiene and feature-registry gates pass.
- Rust: 12 `hitbook_tests` pass. The full `cargo test` build fails on `ort-sys` ("could not find native static library onnxruntime") in the test profile on this machine, with or without the contributor environment, so the tests were run by extracting `HitBook`, `stage_ex_style` and the tests from `stage.rs` into a scratch crate. Four mutations (expiry removed, size cap removed, NOACTIVATE dropped, pinned sources expiring) were each killed by a failing test. The one test that needs the real `window_manager` module (`orb_hitbox_allowed`) did not run.
- Live (release build `8f1576f5a63f13af` from `C:\NEXUS-Dev`): the stage window reads transparent + NOACTIVATE + TOOLWINDOW, no APPWINDOW; startup log has no panic and no `[HITBOX]` line (no rect was ever registered by a wake). The owner confirmed in Brave that clicking and copying work after wakes with and without a command.

## 5. Not verified / open

- Other apps than Brave were not tested. Multi-monitor: the stage covers the primary monitor only.
- Keyboard Esc inside the stage (a callout the user clicked) no longer works because the stage is non-activating; the callout's close button and the global Esc still do.
- The offline Moonshine server still does not become ready within 15 s on a cold start.
- Fix the `ort-sys` test-build failure so `cargo test` runs the whole Rust suite here.
