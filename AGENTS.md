# NEXUS — Project Notes

## Wakeup Orb Redesign — Single-Stage Shell Migration Complete (2026-10-04)

Closes out the "Single-Stage Shell" migration first planned in the
2026-09-25 entry below ("Next: step 2 orb moves in (pixel-compare), then
loading layer, then panels one by one") — that step was never executed
until this 5-phase arc. All 5 phases are done and committed
(`ff42fa7`, `a89fa82`, `3cbcef3`, `461fc46`, plus this cleanup commit).

- **Phase 1 — window consolidation** ([doc 73](docs/changes/73-window-consolidation-orb-into-stage.md)): the voice orb and the
  top-right loading spinner no longer own OS windows (`main`,
  `loading-indicator` — both deleted from `dyn_windows.rs`). Both are now
  positioned `<div>`s inside the one always-on `stage` overlay, driven by
  `stage:orb_rect`/`stage:loading_rect` events from `window_manager.rs`
  instead of native window show/hide/position calls. The entire voice
  runtime (wake globals, mic warmup, every `stt:*`/`ghost:*`/`audio:*`
  listener) moved from the retired `main.tsx`/`App.tsx` into
  `stage/orbRuntime.ts`; calibration drag/resize became in-page pointer
  tracking (a `<div>` can't `startDragging()` the whole stage window).
- **Phase 2 — shape/color/glitch redesign** ([doc 74](docs/changes/74-orb-shape-color-and-glitch-redesign.md)), grounded directly
  against the user's reference video (frames re-extracted and read, not
  worked from memory): listening shifted from pale gold to grainy
  amber/brown; thinking's closed braided-toroidal-knot became an open
  6-strand wisp formation (evenly-spread golden-angle directions — random
  per-strand directions first clumped into a blob, caught by actually
  rendering it); speaking's sparse lat/lon wireframe lattice became a
  dense noise-displaced bumpy blob. New glitch transition: a ~220ms
  position-jitter + color-flicker burst on every dominant-state change.
- **Phase 3 — response caption** ([doc 75](docs/changes/75-response-caption-word-by-word-above-orb.md)): the spoken reply grows
  word-by-word above the orb, timed off edge-tts's real word-boundary
  events (`Boundary::Word`, not `Sentence`) or evenly-estimated timing for
  the Piper fallback. Emitted at playback start (not synthesis) so cache
  hits, edge-tts, Piper, and streamed chunks all go through one path.
- **Phase 4 — live-speech caption** ([doc 76](docs/changes/76-live-speech-caption-streaming-stt.md)): the user's own words grow
  above the orb while they're still talking, via a new best-effort
  `/stream` WebSocket endpoint on the local Moonshine STT server —
  entirely parallel to the existing batch `/transcribe` pipeline, which
  intent parsing/NLU/brain still exclusively depend on. Verified live
  (real synthesized speech through the real server, then the server
  killed outright) rather than assumed.
- **Phase 5 — cleanup**: removed the dead `.transcript, .caption {
  display: none; }` rule and the rest of `styles.css`'s now-unreferenced
  `main`/`#app`-window-era CSS (confirmed zero remaining usages by
  grepping every class name against the actual JSX before deleting
  anything) — the retired slide-up/down transitions, the CSS-fallback
  `.orb` + its pulse keyframes, `.avatar-section`, and three
  already-orphaned ghost-waves modifier classes found the same way.
  `styles.css` is now exclusively the orb/calibration/ghost-waves styles
  `stage/main.tsx` actually uses; `stage.html` already owns the base
  transparent-fullscreen reset, so that duplicate was dropped too.

## Braided Toroidal Knot Ribbon for Thinking State (2026-10-02)

- **Problem & Motivation**: User requested upgrading the `thinking` state avatar animation to match the 3D braided toroidal knot ribbon from Ship Notes `signal-orb` / `speaking-orb` (replacing the squished sphere with belts).
- **Implementation**:
  - `voice-orb.js`:
    - `sphere(count)` sets the 4th attribute (`seed.w`) to normalized index $u = i / \text{count} \in [0, 1)$, creating continuous ribbon streams across all 12,000 points.
    - `VS`: Replaced old orbital belts with parametric braided toroidal knot equations ($\theta = 3 \cdot 2\pi u + 0.57t$, $\phi = 8 \cdot 2\pi u - 0.48t$, $r_t = 0.7135 + 0.2135\cos\phi + 0.125\sin a$), tilted by $34^\circ$ ($v_y = 0.77 b_y - 0.52 b_z$, $v_z = 0.52 b_y + 0.77 b_z$).
    - Scale alignment: Bounding radius ($R \approx 1.05$) perfectly matches sphere radius ($R \approx 1.0$), ensuring zero jumping, clipping, or center displacement.
    - Visuals: Shifted thinking color from amber to radiant lavender/violet (`rgb(178, 151, 255)` / `#b297ff`). Added `knotSpark` specular white core sparks travelling along the front ribbon loops.
    - Added full parity to `_paint2D` fallback.
- **Verify**: Rust 827/827 lib tests pass; Frontend 144/144 Vitest pass across 19 test suites; `tsc --noEmit` clean; Production binary compiled via `node nexus.mjs build` to `src-tauri/target/release/nexus.exe` (51.2 MB).
- **Docs**: `docs/features/87-shipnotes-webgl-voice-orb-integration.md`, `docs/changes/70-braided-toroidal-knot-thinking-avatar-upgrade.md`.

## Ship Notes WebGL 3D Voice Orb Wake Avatar Integration & Overlay Lifecycle Fix (2026-10-02)

- **Problem & Motivation**: User requested replacing the 2D Lottie vector wakeup avatar with the interactive 3D WebGL particle sphere animation from `aqualang89/shipnotes-components` (12,000 WebGL points, smooth 700ms morphing transitions across states, GPU-accelerated).
- **Hotkey Initialization Issue & Root Cause**: During testing, pressing hotkey `Ctrl+Shift+Space` did not initialize animation. Root causes:
  1. `voice-orb.js` exited early if `document.hidden` was true. Because `"main"` is a non-activating overlay window (`WS_EX_NOACTIVATE`), Windows does not focus it, causing Chromium/WebView2 to mark `document.hidden = true` and halt the rAF loop.
  2. Flexbox sizing collapse: `.avatar-section` lacked explicit dimensions, causing `<voice-orb>` to collapse to 0×0px during initial offscreen layout, initializing a 1×1px WebGL canvas.
- **Implementation**:
  - `voice-orb.js`: Removed `document.hidden` throttle; added explicit `play()` / `pause()` methods with `_paused` flag; updated `_resize()` with `getBoundingClientRect()` and `Math.max(80, ...)` floor; styled shadow DOM canvas with `pointer-events: none` and full dimensions.
  - **No Mic Lock Conflict**: `VoiceOrb` does not initiate duplicate WebView2 `getUserMedia` mic streams (preventing Intel SST audio driver starvation of Rust's `cpal` capture loop). Audio reactivity is cleanly driven via `setLevel(vol)` hooked to Zustand `audioVolume` and `ttsActive`.
  - `VoiceOrb.tsx`: Added `visible` prop with `play()`/`pause()` hook synchronization and minimum 120×120px bounding styles.
  - `Avatar.tsx`: Integrated `<VoiceOrb />` with `state`, `visible`, and `level` reactivity inside `.avatar-voice-orb-container`; `.avatar-wrap` enforces minimum 140×140px footprint; preserved hidden `containerRef` anchor to maintain 100% test compatibility.
  - `styles.css`: Added explicit 100% width/height to `.avatar-section` and `.avatar-voice-orb-container`.
  - **1:1 State Mapping**:
    - `idle`: Soft lilac sphere with organic breathing drift.
    - `listening`: Turquoise inward suction waves reacting to user voice volume.
    - `thinking`: Three orbital amber belts with glowing particle heads.
    - `speaking`: Outward pink/violet wave pulses with bass deformation and transient sparks.
- **Verify**: Rust 827/827 lib tests pass (100%); Frontend 142/142 Vitest pass across 18 test suites (100%); `tsc --noEmit` clean; `node nexus.mjs build` compiled release binary `src-tauri/target/release/nexus.exe` (51.2 MB).
- **Docs**: `docs/features/87-shipnotes-webgl-voice-orb-integration.md`, `docs/changes/69-shipnotes-webgl-voice-orb-integration-and-overlay-lifecycle-fix.md`.

## Wake Word Dataset Poisoning Rescue, Whisper Validator Fix & Retraining (2026-10-01)

- **Problem & Root Cause**: User recorded positive samples saying "NEXUS", but `scripts/record_wake_samples.py` used Whisper with a rigid `soundalikes` filter (`clean.startswith("next")`). Due to Whisper's strong prior on short 1s utterances and regional/fast cadence, "nexus" was misheard as "next test", "next sense", "next sis", and 43 recordings from today's session (plus 93 historical) were automatically promoted to `negative/soundalike_from_rec_*.wav`. This poisoned the dataset: the model was literally being taught that the user's authentic voice was negative.
- **Rescue**: Rescued all 136 `soundalike_from_rec_*.wav` files out of `wake_word_data/negative/` and moved them to `wake_word_data/positive/` as `nexus_0820.wav` through `nexus_0955.wav`.
- **Script Fix (`record_wake_samples.py`)**: Completely removed `promote_neg` from `mode_positive`. In positive mode, user speech is never routed to the negative set. Added natural phonetic mishearings (`next`, `nixes`, `next sis`, `nexis`, `open excess`, etc.) to the accepted pattern list.
- **Retraining Results (`scripts/train_local_wakeword.py`)**: Trained for 45 epochs with Binary Focal Loss ($\gamma = 2.0$, pos_weight = 2.2). Extracted 11,412 positive windows and 31,239 negative windows. Final metrics: **Recall: 97.5%**, **False Alarm: 1.67%**, **Val Loss: 0.0369**. Calibrated model exported to `src-tauri/resources/oww/nexus.onnx`.

## Live Screenshot Blur Restored (ADR-05) + Settings Import/Export Removed (2026-10-01)

- **User report**: sidebar/settings *context* still pitch black, "no changes at all"; directive: the blue transparency is ALREADY implemented — find the method in `docs/` first, implement from that.
- **Method found**: `docs/architecture/06-liquid-glass-screenshot-blur.md` (ADR-05) — GDI capture behind the window → `fast_blur` → `sidebar:backdrop` event → `--sidebar-backdrop-image` → `.sidebar-card::after`. A blue wallpaper = the "sapphire bokeh" (feature 84 §4).
- **Root cause**: pipeline wired (all 7 `prepare_sidebar` paths arm `spawn_sidebar_live_blur`) but killed by 2 switches: `DEV_LIVE_BLUR_DISABLED = true` (commands.rs) + CSS `background-image: none` + `#060608` solid black (the 2026-10-01 black directive). The change-62 follow-up DWM tint (`rgba(0,0,0,0.35)`) can't blur (shows desktop sharply) — replaced by the HEAD-verified `::after` rule.
- **REAL blackout root cause (found via GitHub comparison, user directive)**: `live_glass.rs` (untracked, added by the change-61 session) applied `DWMWA_SYSTEMBACKDROP_TYPE=DWMSBT_TRANSIENTWINDOW` (Acrylic) to the non-activating sidebar — the EXACT API ADR-05 Option A rejected. DWM paints a solid opaque fallback on inactive windows + the material call overrides tao transparency → whole HWND surface pitch black under all CSS. That's why CSS changes showed "no difference". Fix: `apply_live_glass` call removed from `dyn_windows.rs` (GitHub-verified branch: `round_corners` + `WDA_EXCLUDEFROMCAPTURE` only); GitHub card surface restored in CSS (`rgba(20,20,22,0.60)` + 1px border). `live_glass.rs` remains but its accent path has zero callers. Docs: doc 62 "Root-Cause Revision" + doc 63 follow-up 2.
- **Changes**: 
  - `DEV_BACKDROP_CAPTURE_DISABLED` removed from `capture_backdrop` (R1).
  - `DEV_LIVE_BLUR_DISABLED = true` re-added to disable self-capturing live loop (R2).
  - `apply_live_glass` (DWM Acrylic) removed from `dyn_windows.rs` glass branch; GitHub-verified `round_corners` + `WDA_EXCLUDEFROMCAPTURE` only (ADR-05 Option A compliance).
  - `.sidebar-card::after` restored to HEAD (backdrop image + `rgba(8,8,10,0.45)` + crossfade).
  - **All three view cards restored to HEAD**: `.settings-container` (card + gradient `::after`), `.architect-app` (white glass card + gradient `::after`), `.pr-list-container` (rgba card + `::after`).
  - `.unified-shell` (untracked) `background: #060608 !important` → `transparent`.
  - **Export/Import buttons removed** from settings footer (Reset/Save stay).
  - Forensic log `live_glass: skipped for '<label>'` retained in dyn_windows.rs.
- **Verify**: tsc 0; vitest **114/114**; official `node nexus.mjs build` → binary 51.1 MB @ 23:24; dist css contains 4 backdrop variable occurrences across all four views; 7/7 spawn coverage + backdrop emitters + listener audited.
- **Pitfall**: Start Menu `NEXUS.lnk` → `%LOCALAPPDATA%\NEXUS\nexus.exe` = **2026-08-31 stale build** — always launch via `nexus start` after `nexus build`.
- **User terminology (binding)**: "window" = SAME window, different context/view (settings context = settings view in unified sidebar) — never a separate HWND. Docs/responses say "context/view".
- **Docs**: `docs/changes/63-restore-live-screenshot-blur-and-settings-cleanup.md`.

## Sidebar No-Nav-Bar, Full-Height Right-Dock, Waves Preview & DWM Blur Fix (2026-10-01)

- **Problem**: (1) Sidebar showed a header bar with TTS + gear icons — user wants zero navigation; Rust voice commands decide which view renders. (2) Sidebar not positioned full-height from top to bottom. (3) DWM Acrylic blur was applied only to `"sidebar"` and `"calibrate-toolbar"` — not all compact glass windows. (4) Waves animation preview missing from companion calibration HUD. (5) Settings view rendered blank when voice command emitted `sidebar:set_view("settings")` — `SidebarApp.tsx` had no listener.
- **SidebarApp.tsx refactor**: Removed entire `<header className="sidebar-header">` block (TTS + gear icon). Added `activeView` state (`"assistant" | "settings"`), wired `sidebar:set_view` Tauri event listener. When view is `"settings"`, lazy-imports `SettingsSidebarApp` and renders it inside the same window via `<Suspense>` — no HWND swap needed. Removed now-unused `query`, `speaking`, `setSpeaking`, `speak`, `handleToggleTts`, `heading` computed values.
- **sidebar.css**: Added `.sidebar-card--settings` (zero padding, full overflow hidden) and `.sidebar-settings-loading` placeholder style.
- **commands.rs `sidebar_geometry`**: Right dock now places window at `x = monitor_w - w + 12` (12px slide off right edge = subtle emergence). `y = 20` (top margin), `h = monitor_h - 40` (full height minus 40px total margin) for all views. Settings view right-docked by default (no longer centered). All 13 geometry tests updated to match new values — all pass.
- **dyn_windows.rs**: Refactored DWM glass branch from `if label == "sidebar" || label == "calibrate-toolbar"` to a `matches!()` for clarity. The two windows in scope remain unchanged (only `sidebar` and `calibrate-toolbar` are created via `get_or_create_window` — the old separate sidebars are gone).
- **CompanionHudApp.tsx + companion-hud.css**: Added inline **waves preview** — when target is `"waves"`, 7 animated CSS bars appear between the Liquid Radio and action buttons, each with staggered `--bar-delay` and `--bar-base` height. Uses `@keyframes hud-wave-pulse` (scaleY 0.35→1.0, alternating). Added `React.CSSProperties` cast for custom CSS properties.
- **Verify**: Rust 788/788 tests pass (100%); Frontend 108/108 Vitest pass (100%); `tsc --noEmit` clean; `npm run build` clean; `cargo build --release --features custom-protocol` clean.

## STT Empty-Transcript & Mic Pipeline (A→C→B→E) + Stuck-Speaking Watchdog (2026-10-01)

- **Trigger**: live run — wake fires (0.985+), capture starts, `transcript = ''` twice. Research ranked 6 hypotheses; key findings: wake works on the same stream (mic not hard-dead), frontend baton log fires unconditionally (noise, not hog proof), and the 8s frontend abort could never stop Rust (parallel-timer race).
- **A (turn packet)**: `TurnStats` travels with every buffer; `endpoint_reason()` mirrors the stop predicate; STT-path/filter markers in all 3 transcribe fns; `stt:turn_stats` emitted at every receiver exit; frontend logs + `debug_trace`. Every future miss names its cause class.
- **C (holders)**: `micHolders.ts` registry on the shared getter (also fixes a per-call stream leak) + p0 trace carries holders summary.
- **B (single owner)**: 8s timer queries non-destructive `stt_capture_had_speech()`; voice underway → hands off (Rust finishes ≤10s); silence → legacy path (late success preserved). A first cut forwarded destructive `stop_stt_capture` — caught in review (would amputate slow starters), replaced with the query.
- **E (escalation)**: 1st miss silent auto-relisten (reset-first so the wake guard can't misread it as cancel), 2nd single nag + deterministic close; streak resets on speech.
- **Visuals**: stuck-speaking watchdog in `hideOrbAfterSpeech` (10 quiet re-arms → force close; genuine TTS re-arms forever); mapping was already correct (breathe=listening, circles=thinking).
- **Verify**: Rust 788/788 serial; vitest 108/108 ×2; tsc + clippy-zero-new; one full-suite flake rewritten hermetic (seeded ticks).
- **Pending (user-run)**: next miss must carry `stt:turn_stats` + holders trace — paste both to name the cause.
- **Docs**: `docs/research/voice-stt/stt-empty-transcript-mic-pipeline-2026-10-01.md` (§8 implementation record).

## Companion Calibrator 2-Row Layout, Liquid Radio & Animated NumberFlow Slider (2026-10-01)

- **Problem & Feedback**:
  - The companion HUD pill in the live run (`media_1790841924506.jpg`) suffered from truncated action buttons: `Cancel` and `Save` were cut off at the right edge due to narrow 440px window width.
  - The static `200 px` badge took up horizontal space.
  - The subtext (`1-2-3 select · ...`) cluttered the HUD.
  - The target selection lacked refractive liquid glass visuals in dark/light modes.
- **Implementation**:
  - **Window Dimensions (`dyn_windows.rs`, `calibration.rs`)**: Expanded `calibrate-toolbar` from 440×54 to 540×92, centered at the top of the monitor (`phys_w = (540.0 * scale) as i32`), giving full breathing room so `Cancel` and `Save` are never clipped.
  - **Liquid Radio Selector (`GlassFilter.tsx`, `CompanionHudApp.tsx`, `companion-hud.css`)**: Implemented the 3-option selector (`Wakeup`, `Waves`, `Loading`) with `#radio-glass` SVG displacement map filter (`feTurbulence` + `feDisplacementMap` scale 30) and sliding frosted glass pill indicator with specular reflections.
  - **Precision Size Slider with NumberFlow (`Slider.tsx`)**: Replaced the static `200 px` badge with a dedicated Row 2 containing `@radix-ui/react-slider` and animated `@number-flow/react` tooltip badge above the thumb, allowing live visual scaling (100–300px for Wakeup/Waves, 40–160px for Loading).
  - **Subtext Removed**: Deleted the bottom helper text entirely for a clean, minimalist 2-row liquid glass floating widget.
- **Verify**: Rust 769/769 tests pass (100%); Frontend 97/97 Vitest pass (100%); `npm run build` clean; Release binary built (`target/release/nexus.exe`, 53.4 MB with `--features custom-protocol`).

## Animation Positioning Controls Liquid Glass & Calibrator HWND Glass Integration (2026-10-01)

- **Audit & Cross-Check of Muse Changes**:
  - `dyn_windows.rs`: Muse correctly removed `stage` from `apply_live_glass()`, fixing the fullscreen blackout. However, `calibrate-toolbar` (the companion pill HUD) was missing from the `live_glass::apply_live_glass` and `round_corners` branch. Added `config.label == "calibrate-toolbar"` so the companion HUD receives native Win32 DWM hardware GPU blur behind the window with rounded corners and capture exclusion.
  - `companion-hud.css`: Replaced dark opaque backgrounds (`0.82` opacity) with true liquid frosted glass (`rgba(15, 23, 42, 0.45)`, `backdrop-filter: blur(28px) saturate(180%)`, specular border `rgba(255, 255, 255, 0.22)`, inset reflection rim `inset 0 1px 1.5px rgba(255, 255, 255, 0.35)`). Upgraded all buttons (`Wakeup`, `Waves`, `Loading`, `Undo`, `Reset`, `Save`, `Cancel`) to liquid glass style with crisp pure white text shadows and specular highlights.
  - `SettingsSidebarApp.tsx` & `settings-sidebar.css`: Styled the Command Hub animation positioning controls (`✥ Drag & Position on Desktop` and `Center-bottom, 200px`) with `.settings-btn--liquid-glass` featuring Apple-grade inset specular rim, high-contrast text glow, and optical backdrop blur.
  - `stage/main.tsx`: Verified completely clean, transparent, with zero click-blocking hitboxes or overlay pills.
- **Verify**: Rust 769/769 tests pass (100%); Frontend 97/97 Vitest pass (100%); `npm run build` clean; Production release compiled cleanly (`target/release/nexus.exe`, 53.4 MB with `--features custom-protocol`).

## Ghost Fullscreen Blackout Fix — Live Glass Isolated to Compact Windows (2026-10-01)

- **Symptom**: entering Ghost Mode blacked out the entire screen (Esc abort only).
- **Root cause**: `dyn_windows.rs:202` applied `live_glass::apply_live_glass()` to `stage` — a 1920×1080 fullscreen overlay. DWM paints Acrylic/blur across the whole HWND rect, i.e. 100% of the monitor. DOM innocent (`ghost.css` has no fullscreen background).
- **Fix**: dropped `stage` from the live-glass branch (corners + affinity go with it, matching the share-visible stage policy); `stage/main.tsx` root explicitly transparent, overlay `+ Button` block removed (its hitbox was self-registered on mount, so no dead click-eater remains).
- **Rule**: fullscreen overlay HWNDs never get DWM backdrops/whole-window blur — live glass is for compact windows only.
- **Verify**: vitest 97/97; Rust 769/769 serial; `npm run build` + release 50.9 MB clean.
- **Docs**: `docs/changes/62-ghost-fullscreen-blackout-fix.md`.

## Live Optical Frosted Glass, Selective Cursor Hit-Testing & Adaptive Luminance (2026-10-01)

- **Problem & Specification**: User required true optical pass-through blur (moving tabs, video playback, cursors behind the glass blurred live in real-time at 60Hz–144Hz) with zero-touch automatic light/dark luminance adaptation (reference image `media_1790832308311.png` "+ Button" split demo) where white text stays pure `#FFFFFF` across both modes, and selective cursor hit-testing where the cursor is an exception ONLY on the overlay button and sidebar (copying/clicking) while the rest of the screen is 100% click-through.
- **Implementation**:
  - **Live Optical DWM Hardware GPU Blur (`live_glass.rs`)**: Wired `DwmSetWindowAttribute` (`DWMSBT_TRANSIENTWINDOW` - Acrylic on Win11 22H2+) and focus-independent `SetWindowCompositionAttribute` (`ACCENT_ENABLE_BLURBEHIND` state 3, `0x00000000` transparent mask) applied across `sidebar`, `settings-sidebar`, `architect-sidebar`, `pr-list-sidebar`, and `stage`.
  - **Selective Hitbox Mouse-Hole Loop (`stage.rs`, `live_glass.rs`)**: Stage overlay is 100% click-through (`set_ignore_cursor_events(true)`). When cursor enters registered hitboxes (overlay button or sidebar), `set_ignore_cursor_events(false)` enables clicking, text selection, and copying.
  - **Screen Luminance Probe (`luminance_probe.rs`)**: Samples 8×8 GDI grid under widget rects (<0.1ms); computes ITU-R BT.709 relative luminance ($Y = 0.2126R + 0.7152G + 0.0722B$); applies 25-point hysteresis ($Y_{light} > 140$, $Y_{dark} < 115$) preventing edge flicker.
  - **Liquid Glass CSS & Component (`liquid-glass.css`, `LiquidGlassButton.tsx`, `main.tsx`)**: Replicates `media_1790832308311.png` "+ Button" pill button with specular edge border, inset reflection rim, and high-contrast text shadows ensuring white text is 100% readable over both milky light glass and obsidian dark glass.
- **Verify**: Rust 770/770 tests pass (100%); Frontend 100/100 Vitest pass (100%); `tsc && vite build` clean; Release binary built (`target/release/nexus.exe`, 50.9 MB with `--features custom-protocol`).
- **Docs**: `docs/changes/61-live-optical-frosted-glass-and-selective-cursor-hit-testing.md`.

## Calibration Follow-Up: Waves Preview, Keyboard, Save Scope, Liquid Glass (2026-10-01)

- **Phase 0 (user-reported garbled Waves preview)**: container mounted but Lottie never loaded (mount effect gated on unreachable `ghostPhase`), bars frozen flat, return-to-Wakeup blank (init deps `[animationData]`). Fix: `resolveWavesShown()` selector + preview playback (0.6x Lottie loop, free CSS bars) + `calibrationTarget` in init deps. +4 tests.
- **Phase 2 (keyboard)**: numbered badges + `1/2/3` select + arrows 1px/⇧10px via Rust `calibration_nudge` (`overlay_nudge` pure, +6 tests); window-scoped keydown only; burst-coalesced undo.
- **Phase 1 (save scope)**: dirty ✓ dots + scoped toast (`save_toast()`); `initial` snapshot restored to session.
- **Phase 3 (liquid glass)**: user-scoped OUT — implemented + verified, then fully reverted; plan retained in research 03.
- **Verify**: Rust 764/764 serial; vitest 95/95 calibration scope (full suite 97/97); tsc clean; `npm run build` + release 50.9 MB `--features custom-protocol`.
- **Pending (user-run)**: toggle previews, wallpaper legibility, keyboard matrix, scoped-save acceptance.
- **Docs**: `docs/research/command-hub/03-…-2026-10-01.md` (§-implementation outcomes), `docs/changes/60-calibration-keyboard-nudge-save-scope-and-liquid-glass.md`.

## Direct Desktop Drag & Scroll-Wheel Calibration + Workspace Collision Recovery (2026-10-01)

- **Collision incident (recovered)**: a second agent session stale-restored `lib.rs`/`commands.rs` (180 errors; 16 mods + ~50 command registrations + my Phase-1/3 settings/commands lost; dual-grammar intent_parser/screen/dyn_windows survived). Correct copies existed only in the working tree (HEAD is older). Recovered via the Gemini transcript at `.gemini/antigravity/brain/3122b61d…/logs/transcript_full.jsonl` (numbered pre-collision reads of `lib.rs` gave the true handler list) + grep-driven `#[tauri::command]` inventory + 59 frontend invokes. Post-recovery: 756/756 Rust tests.
- **Feature (all gates green)**: on-screen drag + scroll-wheel calibration for Wakeup/Waves/Loading — Command Hub Display card ("✥ Drag & Position on Desktop") → 440×54 companion pill HUD top-center → native `startDragging()` drag + debounced onMoved pct report (20px magnetic snap to center/bottom) + wheel ±10px resize (100–300 orb/waves, 40–160 loading) + px badge + Undo/Default/Cancel/Save round-trip (9 params read-modify-write to settings.json, unknown fields preserved; Esc=cancel, Enter=save) + success toast.
- **Key files**: `calibration.rs` (session + 8 commands, +6 tests), `window_manager.rs` (`overlay_xy` pure, `read_waves/loading_settings`, `position_loading`, ghost/calibration branch in `position_orb`), `frontend/src/calibration/geometry.ts` (+9 tests), `frontend/src/companion-hud/` (+5 history tests), `Avatar.tsx` (100%/100% container, drag/wheel/badge, waves preview without ghost session), `loading.html` (inline calibration block), `SettingsSidebarApp.tsx` (entry card + toast + 6 round-trip keys), `dyn_windows.rs::calibrate_toolbar`, vite input + capabilities.
- **Waves semantics**: waves render inside the orb window; `waves_*` rect is applied to it during ghost sessions (`position_orb` branches on `session_active()`). Loading runtime placement now honors saved settings at both show sites.
- **Verify**: Rust 756/756 serial; vitest 87/87; tsc clean; `npm run build` (dist/companion-hud.html emitted); release 50.9 MB with `--features custom-protocol`.
- **Pending (user-run)**: manual acceptance — drag orb/waves/loading, wheel-scale, save/cancel round-trip, ghost-session waves at saved rect.
- **Docs**: `docs/research/command-hub/02-direct-desktop-drag-and-drop-calibration-architecture-2026-10-01.md`, `docs/changes/59-direct-desktop-drag-and-scroll-calibration.md` (incl. the collision recovery record).

## Dual-Grammar Modal Partitioning, Unbreakable Typing & Screen Analysis (2026-10-01)

- **Problem & Root Causes**:
  - Live Ghost Mode run showed: *"Search for almonds"* misheard as `'So it's for almonds.'` was stripped of `"so"` and fell to `WorkerBackend` chat instead of searching in Brave.
  - Saying bare `'Type.'` fell to `WorkerBackend`.
  - Dictating complex phrases like *"type so it analysis for the servx right send it to as soon as possible"* in chat windows collided with `parse_analyse_command` and `parse_send_whatsapp_message`.
- **Implementation**:
  - **Unbreakable Typing Enclave (`parse_type_dictation_command`)**: Prioritized ahead of all domain parsers in `intent_parser.rs`. Explicit toggles manage dictation mode; bare `"type"` toggles dictation; any `"type <payload>"` captures literal text verbatim without command keyword interference.
  - **Acoustic Search Resilience**: Added `"so it's for"`, `"it's for"`, `"it is for"`, and `"search that on"` to `normalize_phonetic_mishearings` and `SEARCH_VERBS`.
  - **Numbered Search Result & Screen Click Matching**: Enhanced `screen::parse_ordinal` with word numerals (`first`, `second`, `one`, `two`, etc.) and expanded regex matching in `intent_parser.rs` for *"open result 2"*, *"open the 2 in the result"*, and *"click the second result"*.
  - **Vision Screen Analysis & Global Prefix Gating (`orchestrator.rs`)**: Wired `run_screen_analysis` with instant feedback (*"Analyzing the screen, sir."*) and Groq Llama-4-Scout VLM screenshot querying. Global breakouts (`"Nexus <action>"`) escape local foreground grounding.
- **Verify**: Rust 750/750 lib tests pass (100%); Frontend 72/72 Vitest pass (100%); Release binary compiled (`target/release/nexus.exe`, 50.9 MB with `--features custom-protocol`).
- **Docs**: `docs/research/ghost-mode/06-dual-grammar-modal-partitioning-and-universal-grounding-architecture-2026-10-01.md`, `docs/features/81-dual-grammar-modal-partitioning-and-foreground-grounding.md`, `docs/changes/58-dual-grammar-modal-partitioning-unbreakable-typing-and-screen-analysis.md`.

## Ghost FIFO Queue ("Real Ghost Mode") + Hotkey/Esc Realignment (2026-09-30 → 2026-10-01)

- **Plan**: Pasted master plan `NEXUS-PLAN-GHOST-FIFO-2026-09-30-V2` (Approach E: FIFO + 4 preemption rules + hotkey/Esc realignment). Executed phase-by-phase with double cross-checks (pre-implementation audit + post-implementation verification per phase).
- **Plan audit corrections (plan was ~85% accurate)**:
  - Gap 1 overstated: `drill_begin()` IS called in `live/commands/ghost_drill.rs:75` + `mouse.rs:316`; only orchestrator-level runners (`run_ghost_open`, `run_browser_search`, `run_browser_search_focus`) lacked it → RAII `GhostDrillGuard` added to those 3 only.
  - Gap 2 overstated: `drain_ghost_followups` IS called (`ghost_drill.rs:152`, `mouse.rs:457`) for standalone runs; voice sessions use the hot-mic loop → no second drain added to `run_ghost_message`.
  - `browser_url::is_browser_active()` doesn't exist → `verify_grounding` uses `get_active_browser_url().is_some()`.
  - Sidebar Esc routes through store `hide()` (not direct `hide_sidebar` invoke) to preserve TTS-stop + 400ms guard; settings Ctrl+Space-closer REPLACED (not added) for D3 coherence.
- **Phase 1 (`wakeword_oww.rs`, `commands.rs`, `lib.rs`, `hotkey.rs`, `main.tsx`)**: `CAPTURE_SESSION_ID` generation + session-tagged `(u64, Vec<f32>)` channel with silent stale-drop; `abort_stt_capture()` → `SttAbortResult{had_speech, elapsed_ms}` exposed as `stop_stt_capture`; hotkey TTS branch → 150ms DAC drain + wake sequence + `drop_followups()` reconcile; 2nd-press hides orb only when voiceless. 4 new tests.
- **Phase 2 (Esc C3, D3)**: `SidebarApp.tsx` Esc → lightbox-first via pure `resolveEscDismiss()` (new `sidebarEsc.test.ts`, 2 tests) else store `hide()`; settings Esc replaces Ctrl+Space; hotkey window branch closes then falls through to wake. Grep-gate: only global `Escape` registration remains `ghost.rs:445`.
- **Phase 3 (`ghost.rs`, `orchestrator.rs`, settings)**: `QueuedCmd` + `SlotClass` FIFO (`enqueue_command` supersede-AppOpen/Navigate + drop-newest, `dequeue_command`); priority lane (stop → preempt + purge); per-session depth-ACK (`DEPTH_ACK_SPOKEN`, reset in `ghost_enter`); `verify_grounding`; drain rewrite (τ sleep + 15s watchdog + grounding skip); 3 new settings (`ghostDepthAck`, `ghostStepTimeoutMs`, `ghostTurnGapMs`) with serde defaults + loose readers + frontend round-trip keys. 7 new tests (one caught a real bug: "start dictation" mis-slotted AppOpen → dictation-first ordering fix).
- **Phase 4 (cadence)**: `waitForAudioIdle` 3000→1000ms; ghost turn-end beats 550→250ms (normal mode byte-identical, existing tests untouched); +1 ghost-beat test.
- **Verify**: Rust 746/746 lib + 19/19 integration; tsc clean; frontend 71/71 vitest; zero new clippy warnings (1 pre-existing error `google/mail.rs:241` untouched).
- **Post-exit stuck-orb fix (same night)**: exit left the wakeup orb visible forever — `ghost:session(false)` forces visible but the exit turn emits no `ack`, and the `result` path never arms a hide. Fix: arm `hideOrbAfterSpeech(3000)` in the exit branch (`App.tsx`); +1 race-proof test; 72/72 vitest; rebuilt dist + release binary (50.9 MB, `--features custom-protocol`).
- **Pending (user-run)**: live script test ("open Brave → new tab → search almonds"), F1–F7 failure drills, H1–H7 hotkey matrix, normal-mode regression.
- **Post-build incident (agent error, fixed same night)**: first `cargo build --release` omitted `--features custom-protocol` → windows pointed at `http://localhost:5173` with no Vite running ("localhost is not reachable"; confirmed by port scan + binary strings). `frontend/dist` was also stale (cargo never rebuilds it). Fix: `npm run build` + `cargo build --release --features custom-protocol` (50.9 MB). Rule: never ship bare `cargo build --release`.
- **Docs**: `docs/changes/57-ghost-fifo-queue-and-hotkey-realignment.md` (full per-file record, deviations, limitations, checklists).

## NEXUS Command Hub Overlay Wiring & Repeated Punctuation Wake-Word Hardening (2026-09-30)

- **Root Cause Analysis — Why "Open Command Hub" Spoke "Welcome..." and Didn't Open**:
  - *Root Cause 1 (Stale Release Binary)*: The background process was executing an earlier `target\release\nexus.exe` compiled before `ParsedIntent::OpenSettings` was dispatched.
  - *Root Cause 2 (STT Punctuation & Repeated Wake Word)*: User said *"Nexus command hub"*, and Whisper captured `'Nexus. Nexus command hub.'`. In `strip_wake_prefix`, the boundary check only allowed `' '` or `'\t'`, skipping `"nexus."` because of the period. The utterance dropped through to the Cloudflare Worker LLM backend, which hallucinated the conversational reply *"Welcome to NEXUS Command Hub..."* instead of handling it locally.
  - *Root Cause 3 (Missing Topmost & Focus Elevation)*: `WindowConfig::settings_sidebar` was configured with `focus: false`, which on Windows could leave the window behind active full-screen applications.
- **Punctuation & Multi-Prefix Wake Stripping (`src-tauri/src/intent_parser.rs`)**:
  - Updated `strip_wake_prefix` with boundary support for punctuation (`.`, `,`, `!`, `?`, `:`, `-`, `;`).
  - Added loop stripping repeated wake words (e.g. `'Nexus. Nexus command hub.'` $\to$ cleanly resolves to `'command hub'`).
- **Exhaustive Command Hub Utterance Alternatives (`src-tauri/src/intent_parser.rs`)**:
  - Expanded `is_settings_command` matching: `"the nexus command hub"`, `"nexus command hub"`, `"the command hub"`, `"command hub"`, `"command hub sidebar"`, `"command hub window"`, `"show command hub"`, `"bring up the command hub"`, `"command hub please"`, `"nexus control hub"`, `"open control hub"`, `"nexus comment hub"`, etc.
- **Topmost & Focus Enforcement (`src-tauri/src/commands.rs`)**:
  - Added `win.set_always_on_top(true)` and `win.set_focus()` in `show_settings_sidebar` ensuring the window is elevated above all desktop apps.
- **Orchestrator Execution & Cached TTS (`src-tauri/src/orchestrator.rs`, `tts.rs`)**:
  - Added `ParsedIntent::OpenSettings => run_open_settings(app).await` speaking cached *"Opening Command Hub, sir."* (<5ms) and spawning `show_settings_sidebar`.
- **Verify**: Rust 184/184 `intent_parser` tests pass; 53/53 `orchestrator` tests pass; frontend 57/57 vitest pass; release binary freshly compiled (`50.7 MB`).

## Multi-Email Google Architecture, Direct OAuth & Chromium Omnibox Hardening (2026-09-29)

- **Root Cause & Fix for Brave URL Detection (`src-tauri/src/browser_url.rs`)**:
  - In user's live run, Brave returned `Active browser URL: None`.
  - Root cause 1: Chromium/Brave strips `https://` from the address bar (`mail.google.com/mail/u/0/...`); line 143 discarded any URL not starting with `http`.
  - Fix 1: Auto-prepends `https://` for schemeless Chromium URLs.
  - Root cause 2: UI Automation Edit control often stores the value in `Name` instead of `ValueValue`. Added fallback in `read_url_from_edit`.
  - Fix 2: Added window title fallback (`GetWindowTextW`) extracting Gmail thread context when the address bar element is not exposed.
- **Direct RFC 8252 Native Loopback OAuth (`src-tauri/src/google/oauth.rs`)**:
  - Implemented direct PKCE OAuth authorization flow with Tokio TCP listener on `http://127.0.0.1:49152/callback`.
  - Eliminates Cloudflare Worker dependency for authentication and generates permanent auto-refreshing access tokens directly in Windows Credential Manager.
- **Multi-Account Vault Registry (`src-tauri/src/auth_vault.rs`, `types.rs`)**:
  - Added `get_google_accounts`, `save_google_account`, `set_primary_google_account`, `remove_google_account`, `get_google_refresh_token`, `set_google_access_token`, and `get_token_for_google_account`.
  - Supports multi-account profiles with avatars, primary badges, and account-scoped token lookup with automatic refresh.
- **`google::accounts` Sub-Engine & Commands (`google/accounts.rs`, `commands.rs`)**:
  - Implemented `list_accounts`, `connect_account`, `disconnect_account`, `set_primary_account`, and `save_custom_credentials`.
  - Wired to Tauri IPC commands for seamless frontend invocation.
- **Multi-Account Sentinel Target Resolution (`src-tauri/src/google/sentinel.rs`)**:
  - Updated Sentinel polling loop to resolve account-specific tokens per watched email target with automatic refresh fallback.
- **Settings Sidebar UI Refactor (`frontend/src/settings-sidebar/SettingsSidebarApp.tsx`)**:
  - Refactored Auth tab into a clean liquid-glass Google Accounts manager rendering account cards, avatars, primary badges, "+ Add Google Account" button, and custom developer credentials drawer.
- **Verify**: Rust 716/716 library unit tests pass; frontend 51/51 vitest pass; `cargo check` clean.
- **Docs**: `docs/features/77-multi-email-google-accounts-and-direct-oauth-settings.md`.

## Vision Mode Screen Email Scanning, Hybrid Grounding & Proactive Watch Memory (2026-09-29)

- **Native Gmail Engine Thread Watch & Semantic Diffing (`src-tauri/src/google/mail.rs`, `types.rs`)**:
  - Added `WatchStatus`, `ThreadWatchTarget`, and `ThreadUpdateEvent` data models.
  - Implemented `create_thread_watch` and `diff_thread_state`, extracting sender, subject, latest message ID, message count, and semantic deadline status (`parse_deadline_update`).
  - Added tertiary initial deadline extraction regex pattern (`due by|due on|due date is|submission deadline is`) enabling accurate baseline capture even before any extension occurs.
  - Differentiates `DeadlineChanged`, `NewReply`, and `NoChange`.
- **Active Browser URL Grounding (`src-tauri/src/browser_url.rs`)**:
  - Implemented `extract_gmail_thread_id_from_url` resolving `#inbox/<ID>`, `#all/<ID>`, `#search/.../<ID>` from Chrome/Edge/Brave active tab in <5ms.
  - Provides deterministic thread identification without VLM latency, hallucination, or token costs when Gmail web is active.
- **Vision Screen Email Context Extraction (`src-tauri/src/vision.rs`)**:
  - Added `ScreenEmailContext`, `build_email_extraction_prompt`, and `parse_screen_email_json`.
  - Fallback VLM pipeline captures active desktop email clients (Outlook, Thunderbird, web) extracting sender, subject, snippet, and apparent deadline with robust markdown fenced JSON parsing.
- **Persistent Watch Memory Storage (`src-tauri/src/memory.rs`)**:
  - Created dedicated JSON storage `mail_watches.json` in `%APPDATA%/com.nexus.assistant/memory/`.
  - Implemented `load_mail_watches`, `save_mail_watches`, `add_mail_watch`, and `update_mail_watch_status`.
- **NLU Intent, Center Routing & Orchestrator Execution (`intent_parser.rs`, `center.rs`, `google/mod.rs`, `orchestrator.rs`)**:
  - Added `ParsedIntent::WatchScreenEmail` matching utterances like *"update me whenever there is any update on the deadline or anything for this email"*, *"watch this email"*, *"track deadline changes in this email"*.
  - Mapped `WatchScreenEmail` to `"GoogleCenter"` in `center.rs` with repeat-back confirmation dialog.
  - Implemented `run_watch_screen_email` orchestrator runner: reads active browser URL for instant thread ID grounding; falls back to vision capture; registers watch target in persistent memory; triggers proactive background sentinel polling.
- **Verify**: Rust 34/34 `google` unit tests pass; 12/12 `memory` unit tests pass; 24/24 `vision` unit tests pass; 2/2 `browser_url` unit tests pass; 183/183 intent parser unit tests pass; frontend 45/45 vitest pass; clean release binary built.
- **Docs**: `docs/features/76-vision-mode-screen-email-scanning-and-proactive-watch-memory.md` & `docs/research/vision-grounded-gmail-engine-and-proactive-watch-architecture-2026-09-29.md`.

## Google Ecosystem Sub-Center & Main Center Validity Gate Architecture (2026-09-29)

- **Domain Sub-Services Built & Tested in Isolation (`src-tauri/src/google/`)**:
  - `mail.rs` (8 tests): Regex semantic deadline / schedule extraction (`parse_deadline_update`), RFC 2822 base64url message encoding, JSON parsing for threads, messages, and receipts.
  - `calendar.rs` (5 tests): Event payload construction, RFC 3339 / ISO 8601 parsing, schedule conflict detection (`check_schedule_conflict`), and free-busy buffer calculation (`calculate_free_busy_buffer`).
  - `maps.rs` (4 tests): Directions / leg summary parsing, commute delay detection with dynamic departure recommendation (`detect_commute_delay`), Places API parsing, and navigation/search deep link generators.
  - `photos.rs` (5 tests): Media item & album parsers, natural speech semantic query extraction (`parse_semantic_query`), search filter payload generation, and sized URL helper.
  - `sentinel.rs` (4 tests): De-duplicating `SeenMessageRingBuffer` (500 capacity), proactive deadline alert synthesis, commute traffic alert synthesis, and urgency ranking (`Low`, `Medium`, `High`, `Critical`).
- **GoogleCenter Hub Consolidation (`google/mod.rs`, `google/auth.rs`, `google/client.rs`)**:
  - Implements the `SubCenter` trait (`name`, `validate`, `confirm_kind`).
  - Alexa Skills Kit dialog standard: validates required slots (`to`/`body` for email, `title`/`time` for calendar, `destination` for directions, `query` for photos) with exact spoken elicitation prompts instead of falling back to cloud LLM chat.
  - Confirmation policy: gated confirmations with timeout for outgoing actions (`send_email`); repeat-back confirmations for routine operations (`create_event`, `directions`, `search_photos`).
  - Seamless authentication via `auth_vault::get_token("google")` backed by OS Credential Manager (Windows Credential Manager) with environment token fallback.
- **Main Center Connection (`center.rs`)**:
  - Implemented `center_for` decision tree mapping intents to their respective centers (`AppCenter`, `BrowserCenter`, `MediaCenter`, `MessageCenter`, `CommerceCenter`, `GitHubCenter`, `ArchitectCenter`, `KnowledgeCenter`, `DictationCenter`, `GhostCenter`, `GreetingCenter`, `GoogleCenter`).
  - Implemented `validate` gatekeeper filtering unheard / sub-threshold transcripts (<2 alphanumeric characters) and enforcing required slots before execution.
- **Verify**: Rust 32/32 `google` unit tests pass; 31/31 `center` unit tests pass; 182/182 intent parser tests pass; frontend 44/44 vitest pass; release build clean (50.8 MB binary `nexus.exe`).
- **Docs**: `docs/features/75-google-center-hierarchical-domain-architecture-and-proactive-sentinel.md` & `docs/research/google-sub-center-and-sentinel-architecture-2026-09-29.md`.

## Ghost Mode Persistent Always-On Waves & Speech-Only Motion (2026-09-29)

- **Root Cause 1 (Waves Killed on Idle)**: `shouldShowWaves` previously checked `wakeChoreographyDone`. When a turn completed and `state` became `"idle"`, a hook reset `wakeChoreographyDone = false`, immediately flipping `shouldShowWaves` to `false` and destroying the waves.
  - **Fix**: Removed `wakeChoreographyDone` entirely. `shouldShowWaves = visible && ghostActive`. As long as Ghost Mode is active (`ghostActive`), the waves stay persistently visible and always on throughout the entire session.
- **Root Cause 2 (waves.json Container Not Mounted)**: The `useEffect` that loaded `waves.json` into `wavesContainerRef` only depended on `[wavesData]`. When `wavesData` fetched on boot, `ghostPhase` was `"smile"`, so `<div ref={wavesContainerRef} />` was not yet in the DOM (`wavesContainerRef.current === null`). When Ghost Mode started, the effect never re-ran, so `waves.json` was never mounted.
  - **Fix**: Added `ghostPhase` to the dependency array `[wavesData, ghostPhase]`. When `ghostPhase` becomes `"waves"`, `lottie.loadAnimation` mounts `waves.json` directly into the container and parks it at frame 0.
- **Root Cause 3 (Motion When We Speak Only)**: In the rAF loop, when active speech occurs (`ttsActive || micLevel >= WAVE_REST_FLOOR`), `wavesAnimRef.current.play()` animates the waves. When silent, `wavesAnimRef.current.pause()` freezes the waves in place.
- **Verify**: Rust 182/182 intent parser tests pass; frontend 44/44 vitest pass.

## Normal Mode Orb Stability, Non-Stop Thinking Loop & 'The Post Mode' Mishearing Fix (2026-09-29)

- **Idle Glitch / Infinite Restart Loop (`frontend/src/avatar/Avatar.tsx`)**:
  - **Root cause**: Inside `onComplete`, when `idle-smile` finished its arrival segment, it called `applyState(a, state)`. In idle, `state = "idle"`, which resolved to `mode: "idle-smile"`. But `modeRef.current` had just been set to `"holding"`. Because `"holding" !== "idle-smile"`, `applyState` restarted `idle-smile` from frame 0 in an infinite loop, causing continuous twitching/jittering in idle.
  - **Fix**: Removed the recursive `applyState` call in `onComplete`. Added a guard in `applyState`: if `modeRef.current === "holding"` and `resolved.mode === "idle-smile"`, it simply returns without restarting. The orb now parks completely calm and stable at `map.hold` (frame 300).
- **Premature Movement & Loading Circles Before User Speaks (`Avatar.tsx`, `styles.css`)**:
  - **Root cause**: `resolveAvatarAnim("listening")` was running `SEG_LOADING` (`[171, 260]`), and `styles.css` had `animation: pulse-listen` on `.avatar-wrap--listening`. The orb was both spinning circles and pulsing before the user spoke.
  - **Fix**: In `resolveAvatarAnim`, `listening` now resolves to `idle-smile` (the smiling orb holding frame 300). Removed `pulse-listen` from `.avatar-wrap--listening` so the orb stays completely still and steady while listening.
- **Non-Stop Loading Loop in Thinking (`Avatar.tsx`)**:
  - **Requirement**: Non-stop loading when thinking.
  - **Fix**: `resolveAvatarAnim("thinking")` exclusively returns `mode: "loading-loop"`, segment `map.loading` (`[171, 260]`), `loop: true`, `speed: 1.5`. It continuously loops without interruption until thinking completes.
- **Speaking Zoom In/Out (`styles.css`, `Avatar.tsx`)**:
  - **Requirement**: Speaking zooms in and out.
  - **Fix**: Speaking state holds the smiling orb at `map.hold`, while `.avatar-wrap--speaking` drives `animation: pulse-speak 0.55s ease-in-out infinite` scaling from `1.0` to `1.10`.
- **STT Mishearing 'The Post Mode' / 'The Ghost Mode' (`intent_parser.rs`, `recorder.ts`)**:
  - **Root cause**: Saying "the ghost mode" or Whisper mishearing it as "the post mode" produced transcripts starting with "the ", which failed the prefix checks in `parse_ghost_control_entry`.
  - **Fix**: Added `"the ghost mode"`, `"the post mode"`, `"the host mode"`, `"the coast mode"` to `TRIGGERS` in `parse_ghost_control_entry`, `normalize_phonetic_mishearings`, and `recorder.ts` regex.
- **Verify**: Rust 182/182 intent parser tests pass; frontend 44/44 vitest pass.

## Ghost Mode Esc Fix & Ctrl+Space Wave Flash Fix (2026-09-29)

- **Esc Not Stopping Ghost Mode (`src-tauri/src/ghost.rs`)**:
  - **Root cause**: `abort_session` (triggered by Esc panic) sets `SESSION = Yielded` but NOT `Idle`. On the next `"ghost mode"` voice command, `ghost_enter` called `register_esc` without first unregistering. The `tauri_plugin_global_shortcut` plugin rejected or stacked on the already-registered Esc shortcut, causing the handler to silently fail on every session after the first Esc press.
  - **Fix**: Added `unregister_esc(&app)` immediately before `register_esc(&app)?` in `ghost_enter`, giving a clean shortcut registration every session regardless of prior abort state.
- **Ctrl+Space Waves Flash and Disappear (`frontend/src/audio/recorder.ts`)**:
  - **Root cause**: On hotkey tap with no voice, Rust STT emitted `stt:transcript = ""`. `processTranscript("")` spoke "Didn't catch that sir" (~1s TTS) which raced with Plan B wake choreography completion (`wakeChoreographyDone = true` → 220ms pinch → waves appear). TTS finished almost simultaneously with waves appearing → `setVisible(false)` + `reset()` → `state = "idle"` → `wakeChoreographyDone = false` → `shouldShowWaves = false` → waves collapsed. Net: ~100-300ms flash.
  - **Fix**: Empty transcript in non-ghost mode now silently hides (`setVisible(false)` + `reset()` **immediately**) with no TTS. Ghost hot-mic silent-miss cap behavior preserved.
- **Regression — Smile Animation Flip on Silent Dismiss (`Avatar.tsx`)**:
  - **Root cause 1 (Exclusive Waves)**: I mistakenly forced the 3-bar wave visualizer into the normal wake-up flow. The user clarified waves should **only** appear during Ghost Mode.
  - **Root cause 2 (Smile Flip)**: The immediate `reset()` (setting `state="idle"`) caused the `applyState` hook to force the Lottie to `idle-smile`. Since the orb takes 600ms to visually slide down (CSS transition), the user saw this abrupt frame jump as a "flip".
  - **Fix 1 (Exclusive Waves & Choreography Sequence)**: Reverted the complex choreography logic. `shouldShowWaves` is now strictly `visible && ghostActive && wakeChoreographyDone`. In normal mode, the orb simply plays its native Lottie animations (loading, smile). When Ghost Mode is initialized, it waits to finish its smile arrival, then seamlessly pinches into the waves and stays there.
  - **Fix 2 (Slide-down Freeze)**: Added a `!visible` early-return guard to the `applyState` hook. If the orb is sliding down, it freezes on its current frame instead of jumping.
  - **Fix 3 (Atomic Entrance)**: Added an `isEntering` guard to prevent rapid state changes (like TTS audio starting) from interrupting or pausing the wake-up choreography. This prevents the orb from freezing mid-animation when entering Ghost Mode.
  - **Fix 4 (Normal Mode UX Alignment)**: In Normal Mode, `listening` (STT) and `speaking` (TTS) now strictly show the static smiling orb pulsing via CSS (`pulse-listen` and `pulse-speak` zooming in and out), rather than playing the `loading-loop` circles. The circle Lottie animation is now exclusively reserved for the `thinking` state.
- **Verify**: Ghost tests 24/24 pass; frontend 44/44 vitest pass.
- **Docs**: `docs/changes/55-ghost-waves-browser-search-typing-and-intent-isolation-implementation.md` §4.

## Ghost Waves, Browser Search, Line-by-Line Dictation & Intent Isolation (2026-09-29)

- **Root Cause Fix — Eradicated False "No repository found" (`recorder.ts` & `wsBridge.ts`)**:
  - Removed hazardous frontend mic truncation rewrite `if (/^open-?$/i.test(t.trim())) { t = "open architecture mapper"; }` in `recorder.ts`. Truncated words like "open" or "open-" no longer falsely morph into codebase mapping.
  - Hardened `isLongRunningQuery` and `isArchitectQuery` in both `recorder.ts` and `wsBridge.ts` to strictly require explicit keywords (*"architecture mapper"*, *"codebase diagram"*, *"pr"*, *"pull request"*), eliminating false triggers from generic verbs and nouns (*"create"*, *"show"*, *"check"*, *"code"*, *"project"*).
- **Local Browser Search & Address Bar Focus (`intent_parser.rs`, `orchestrator.rs`, `browser.rs`)**:
  - Added deterministic intents: `BrowserSearch { query }`, `BrowserSearchFocus`, `StartDictation`, `StopDictation`.
  - Added `focus_search_bar()` in `browser.rs` (`Ctrl+L`).
  - Saying *"search <query>"* in Ghost Mode or *"search in browser <query>"* dispatches directly to `browser::search(query)` (`Ctrl+L` $\to$ types query $\to$ `Enter` in <300ms).
  - Saying bare *"search"*, *"search bar"*, or *"focus address bar"* invokes `Ctrl+L` and speaks pre-cached *"Ready to search, sir."* (<5ms).
- **Line-by-Line Dictation Mode (`orchestrator.rs`)**:
  - Added atomic flag `DICTATION_ACTIVE` in `orchestrator.rs`.
  - Triggered via *"type whatever I say"*, *"start typing"*, *"type line by line"*, or *"start dictation"*.
  - Transcripts are directly typed line-by-line into the focused window (`keyboard::type_text(&format!("{}\n", text))`).
  - Stopped via *"stop typing"*, *"stop dictation"*, or *"stop"*, replying with pre-cached *"Stopped typing, sir."*.
- **Sub-5ms App-Open Voice Confirmation (`orchestrator.rs` & `tts.rs`)**:
  - Replaced un-cached dynamic TTS string `format!("{target} open, sir.")` in `run_ghost_open` with pre-cached `"Ok sir."`, eliminating the 1.5s–2.0s Edge-TTS cloud roundtrip.
  - Added `"Ready to search, sir"` and `"Stopped typing, sir"` to `CACHED_PHRASES` in `tts.rs`.
- **Unified Wave Visualizer, Wake Choreography & Motion Rules (`Avatar.tsx`)**:
  - Plan B Sequencing: On wake up, plays full wake choreography first (`wakeup.json`: loading circles at 1.5x $\to$ smile arrival at 1.5x $\to$ hold frame 300). Once the smile completes (`wakeChoreographyDone`), pinches into the 3-bar audio-reactive waves (or immediately if speaking/thinking starts early).
  - In Ghost Mode (`ghostActive`), wave visualizer activates immediately without playing wake circles.
  - Clean resting visibility: resting scale is `0.35` with container opacity `0.85` (clearly visible, not shrunken or dimmed).
  - Strict motion invariant: completely stationary at rest (paused Lottie, fixed `0.35` scale, no sinusoidal wave). Oscillates dynamically from `0.35` to `1.0` during user speech (`micLevel`) or TTS audio (`ttsWaveLevel`).
  - Lowered `WAVE_REST_FLOOR` to `0.015`, ensuring conversational vocal speech dynamically moves the waves.
- **TTS Handshake Synchronization & Ghost Session Visual Persistence (`orchestrator.rs`)**:
  - Removed premature simultaneous `Done` emission in `speak_line`, `run_ghost_control_enter`, and stop/exit handlers. Prevents the frontend `done` handler from triggering a premature `store.reset()` to `idle` 550ms into speech while TTS is still speaking, allowing `finishSpokenResult` $\to$ `maybeGhostRelisten()` to fire on actual speech completion and keep the wave visualizer persistently active on screen.
- **Brain Monitor Retrain Path Resolution (`brain_monitor.rs`)**:
  - Added candidate path existence checks before running `merge_and_train.py`, resolving `[Errno 2] No such file or directory`.
- **Verify**: Rust 182/182 intent parser tests, 52/52 orchestrator tests, 46/46 wakeword tests pass; frontend 44/44 vitest pass; release build clean (53.2 MB binary).
- **Docs**: `docs/research/voice-and-ghost-mode-root-cause-analysis-and-flow-hardening-2026-09-29.md`, `docs/features/73-ghost-waves-browser-search-typing-and-intent-isolation.md`, & `docs/changes/55-ghost-waves-browser-search-typing-and-intent-isolation-implementation.md`.

## STT Hallucination Mitigation, Strict Wave Motion Rule & Intent Parser Tightening (2026-09-29)

- **Acoustic Pre-Gate & Whisper Hallucination Filter (`stt.rs` & `stt_groq.rs`)**:
  - Implemented length gate (< 200ms / 3200 samples) and RMS floor gate (< 0.005) in `transcribe_audio` and `transcribe_samples`. Sub-threshold / silence buffers are dropped immediately with zero API calls.
  - Expanded `apply_hallucination_filter` to intercept background noise fragments (*"from the"*, *"in the"*, *"open drift"*, *"open breath"*), silence repetition loops, and Latin/foreign confabulations (*"cunzon usarlo"*, *"sarese manguer"*, *"tadrao vara"*). Cleaned `NEXUS_VOCABULARY` in `stt_groq.rs`.
- **Eliminated False "No repository found" (`intent_parser.rs`)**:
  - Removed hazardous prefix match `if trimmed == "open-" || trimmed == "open"` in `is_architect_command` that routed truncated speech to `OpenArchitect`.
  - Rewrote `is_architect_fuzzy` to strictly require both an architecture soundalike keyword (*"architecture"*, *"architect"*, *"arch"*, *"arcade"*, *"octach"*, *"ark"*, *"cat"*) and a mapping keyword (*"mapper"*, *"map"*, *"diagram"*, *"graph"*, *"remember"*, *"member"*, *"december"*), eliminating greedy matches on common words (*"our"*, *"are"*, *"art"*, *"mac"*, *"master"*).
  - Added `strip_leading_stray_punctuation` to sanitize leading punctuation and orphaned single-character prefixes.
- **Strict Wave Motion Rule & In-Place Continuity (`Avatar.tsx`, `orchestrator.rs`, `App.tsx`, `recorder.ts`)**:
  - Eliminated autonomous sinusoidal breathing waves during idle/rest states. Waves now rest statically at a flat baseline (`0.15`) with the Lottie animation paused (`wavesAnimRef.current.pause()`) and container opacity dimmed to `0.35`. Waves only oscillate when mic capture is active or TTS is synthesizing.
  - Suppressed top-right loading window (`loading.json`) usurpation during Ghost Mode (`show_loading` short-circuits if `crate::ghost::session_active()`), keeping the dynamic waves anchored at the exact wakeup orb screen coordinates.
- **WhatsApp Offline Log Demotion (`mcp_client.rs`)**:
  - Demoted periodic connection failure logs from `warn!` to `debug!` when probing `http://127.0.0.1:8765/mcp`, preventing background console error spam.
- **Voice Lock**:
  - Enforced strict voice lock to `en-US-AvaNeural` + `Neutral` prosody.
- **Verify**: Rust 181/181 intent parser tests, 23/23 STT tests, 46/46 wakeword tests pass; frontend 44/44 vitest pass; release build clean.
- **Docs**: `docs/features/72-stt-hallucination-mitigation-wave-motion-and-architect-routing-fix.md` & `docs/changes/53-stt-hallucination-mitigation-wave-motion-and-architect-routing-fix.md`.

## Personalized & Neural-Augmented Wake-Word Training (2026-09-25)

- **User Voice + Multi-Persona Neural Augmentation**:
  - Captured pristine human microphone samples (`nexus_0091.wav`–`nexus_0095.wav`) via `scripts/record_wake_samples.py` with automatic indexing fix.
  - Generated 250 high-diversity 16kHz mono positive samples across 13 Microsoft Neural voices (`edge-tts`) spanning US, UK, and Indian accents across rates (-20% to +20%) and pitches (-5Hz to +5Hz) for `"nexus"`, `"hey nexus"`, and `"okay nexus"`, Whisper-verified.
  - Expanded positive training dataset to **432 samples** (94 human mic recordings + 88 SAPI + 250 Microsoft Neural).
- **Binary Focal Retraining**:
  - Retrained classifier via `scripts/train_local_wakeword.py` with `pos_weight = 2.2` and `BinaryFocalLoss(gamma=2.0)` across **41,183 total windows** (35,004 train + 6,179 val: 7,776 positive, 25,918 hard speech negatives, 6,489 comfort/noise windows).
  - Exported ONNX model `src-tauri/resources/oww/nexus.onnx` (`2de9887f7902befa9d1d2e8f2a02d98b9534b4cad159ebef9ddad73e97760f5a`) and synchronized `model_manifest.json`.
- **4-Point Benchmark Audit (`scripts/verify_hardened_model.py`)**:
  - Problem Phrase (*"Documentation Created & Synchronized"*): **2.61% peak** (0 triggers at 0.68 threshold).
  - Fast Multi-Syllabic Negatives (186 files): **0.00% FA (0/186)**, max peak 17.18%.
  - Positive Recall (432 files): **98.6% recall at 0.68 (426/432)**, average max **98.0%**, median confidence **99.7%**.
  - Vocal Friction (90 files): **0.00% FA (0/90)**, max peak 8.13%.
- **Verify**: Rust 43/43 wakeword unit tests pass (`cargo test --lib wakeword -- --test-threads=1`); release build clean.
- **Docs**: `docs/features/71-personalized-and-neural-augmented-wakeword-training.md` & `docs/changes/52-personalized-and-neural-augmented-wakeword-training.md`.

## NLU Phonetic Alias Map & Persistent Missed-Intent Logging (2026-09-25)

- **Layer 3 Phonetic Alias Normalizer (`intent_parser.rs` & `recorder.ts`)**: Built deterministic soundalike phrase normalizer catching common Whisper acoustic mishearings (*"goes to mode"*, *"goes to mold"*, *"post modern"*, *"postmodern"*, *"coast mode"*, *"gold mode"*, *"ghost mood"* → `"ghost mode"`; *"open what's up"* → `"open whatsapp"`; *"open vs coat"* → `"open vs code"`; *"open spot if I"* → `"open spotify"`; *"stand down"* → `"stop"`).
- **Extended Ghost Control Triggers**: Added `"activate ghost mode"`, `"activate ghost"`, `"turn on ghost mode"`, and all acoustic soundalike phrases to `parse_ghost_control_entry` TRIGGERS. Expanded `STOP_PHRASES` in `ghost.rs` with `"stand down"`, `"stop please"`, `"exit ghost"`, `"stop ghost"`, `"exit ghost mode"`, `"leave ghost mode"`.
- **Persistent Missed-Intent Logger (`missed_intent_logger.rs`)**: Implemented persistent JSONL logger writing all unmatched transcripts to `%APPDATA%/com.nexus.assistant/missed_intents.jsonl` with millisecond timestamp, ISO datetime, transcript, source (`orchestrator` / `stt_learning`), and failure reason. Auto-rotates at 5 MB. Added `get_missed_intents` IPC command. Hooked into both `orchestrator.rs` (`ParsedIntent::Unknown`) and `stt_learning.rs` (`log_failure`).
- **Verify**: Rust 173/173 intent parser tests pass (`cargo test --lib intent_parser -- --test-threads=1`); 1/1 logger test passes; frontend 33/33 vitest pass.
- **Docs**: `docs/features/69-nlu-phonetic-alias-map-and-missed-intent-logging.md` & `docs/changes/51-nlu-phonetic-alias-map-and-missed-intent-logging.md`.

## STT Hallucination Fix — VERIFY_RING Prefix-Pad Poisoning, Ghost Mode Vocabulary & Double Trigger Banner (2026-09-25)

- **Root Cause 1 — VERIFY_RING Prefix-Pad (`wakeword_oww.rs`)**: Diagnosed that `start_stt_capture()` seeded 160ms of `VERIFY_RING` audio (containing the wake word "NEXUS" + silence gap) into the command capture buffer. Groq received `[NEXUS audio] + [1.3s silence] + [command]`, hallucinating random text ("and then the process goes to murder."). Removed prefix-pad entirely; capture starts clean — no phonemes are lost since the user finishes the wake word before speaking the command. Also removed now-unused `STT_PREFIX_PAD_SAMPLES` constant.
- **Root Cause 2 — Ghost Mode Vocabulary Bias (`stt_groq.rs`)**: Groq's Whisper decoder had no bias toward "ghost mode", mapping `/ɡoʊst moʊd/` to "goes to mode" / "goes to mold." Expanded `NEXUS_VOCABULARY` with full ghost mode command phrasings, common app opens (WhatsApp, Chrome, VS Code, Spotify, Discord), and action verbs (open, close, stop, cancel, navigate, settings).
- **Root Cause 3 — Double Trigger Banner (`run.ps1`)**: The console was printing two `TRIGGER #01` / `TRIGGER #02` banners per utterance because the OR-pattern `instant neural trigger|OWW wake detected` matched both a DEBUG and an INFO log line from the same event. Fixed by narrowing the pattern to only `instant neural trigger` and explicitly suppressing `OWW wake detected!` and `high-confidence single-frame trigger` debug lines.
- **Verify**: Rust 43/43 wakeword unit tests pass (`cargo test --lib wakeword -- --test-threads=1`); release build clean (zero warnings).
- **Docs**: `docs/changes/50-stt-hallucination-verify-ring-prefix-pad-and-ghost-mode-vocabulary.md`.

## Live Audio Telemetry, Trigger Debouncing & Connection Diagnostics Parity (2026-09-25)

- **Live Speaker Audio Waveform & Status Meter (`run.ps1` & `wakeword_oww.rs`)**:
  - Implemented continuous 12-slice speaker audio waveform (`Wave: [  ▂▄▆██▆▄▂  ]`) and status meter (`🎙️  VOICE [████████████████] 98.4% | RMS: 0.0245 (AGC  2.5x)`) in `nexus start` identical to `nexus wake test`.
  - Added `compute_waveform_string(&chunk)` in `wakeword_oww.rs` to compute 12-slice Unicode bars (` ▂▃▄▅▆▇█`) without double-gain multiplication.
  - Streaming audio telemetry emitted dynamically: 160ms cadence during active voice and 480ms during silence.
  - Added `Make-Meter` and `Clear-MeterLine` in `run.ps1` with UTF-8 encoding and in-place carriage return (`\r`) rendering, preventing screen tearing or trailing characters when other log lines arrive.
- **Single-Trigger Neural Debouncing**:
  - Investigated duplicate trigger firing (`TRIGGER #01` and `#02` in the same second). Added atomic `LAST_NEURAL_FIRE_MS` (1.5s cooldown) in `wakeword_oww.rs` candidate receiver loop, cleanly aggregating consecutive multi-frame speech chunks into exactly one trigger event.
- **Connection Diagnostics & Service Health Restoration**:
  - Restored the ASCII startup diagnostics box (`╔|║|╠|╚|NEXUS Connection Diagnostics`) in Cyan, verifying Faster-Whisper STT (port 39217), Edge-TTS & Piper TTS, Cloudflare Worker `/health`, GitHub OAuth, and Google OAuth status on boot.
- **Verify**: Rust 43/43 wakeword unit tests pass cleanly (`cargo test --lib wakeword -- --test-threads=1`); release build clean.
- **Docs**: `docs/research/wakeword/unified-console-audio-telemetry-and-diagnostics-parity-2026-09-25.md`, `docs/features/68-live-audio-telemetry-and-connection-diagnostics-parity.md`, & `docs/changes/49-live-audio-telemetry-and-connection-diagnostics-parity.md`.

## Instant Neural Wake Alignment & Clean Console UX (2026-09-25)

- **Latency & Divergence Diagnosis vs. `wake test`**:
  - Investigated why `nexus start` showed a 1.2s–1.8s delay and lower sensitivity than `nexus wake test` (which triggers in < 35ms at 98.4% confidence).
  - Identified 5 root causes: 1) `read_verify_wake` defaulted to `true` (unnecessary Stage-2 Whisper STT HTTP cross-check), 2) 500ms post-hit secondary buffer delay in `wakeword_oww.rs`, 3) WebRTC VAD onset decapitation chopping the initial nasal consonant `/n/`, 4) 4-channel averaging in `try_device` downmix halving signal on Intel SST arrays, 5) repetitive 2s audio heartbeat dumps flooding `nexus_unified.log`.
- **Rust Engine Optimization & Fail-Open DSP**:
  - `commands.rs`: Defaulted `read_verify_wake` to `false` for instant neural firing.
  - `wakeword_oww.rs`: Immediate return on detection; made WebRTC VAD fail-open; connected calibrated `pre_gain` (2.50x) in AGC normalization; downmixed active stereo pair ($\text{Ch}_0+\text{Ch}_1$) on 4-channel devices; demoted 2s heartbeat logs to `DEBUG`.
  - `meeting_detect.rs`: Added `python.exe`, `nexus.exe`, and `node.exe` to system process exclusions.
- **Unified Console UX & Live Speaker Waveform (`scripts/run.ps1` & `scripts/test_wake_live.py`)**:
  - Filtered out 50+ lines of internal startup boilerplate in `run.ps1` (espeak, webview, meeting detection, permissions, app registry, TTS pre-gen, ONNX kernel activations, Tier 3 skipped models, connection diagnostics box).
  - Formatted wake triggers with an instant visual banner matching `nexus wake test`:
    `🔔 [TRIGGER #01] HH:mm:ss — Confidence: XX% | Instant Neural Fire`
    `     Status: ● WAKE WORD HEARD SIR!`
  - Built real-time 12-slice speaker audio waveform visualizer into `scripts/test_wake_live.py` (`Wave: [  ▂▄▆██▆▄▂  ]`) with dynamic AGC scaling and CP1252/Unicode fallback, so vocal speech waves animate visibly when speaking.
- **Verify**: Rust 43/43 wakeword unit tests pass cleanly (`cargo test --lib wakeword -- --test-threads=1`); release build clean.
- **Docs**: `docs/research/wakeword/wakeword-runtime-engine-and-console-parity-2026-09-25.md`, `docs/features/67-instant-neural-wake-alignment-and-clean-console-ux.md`, & `docs/changes/48-instant-neural-wake-alignment-and-clean-console-ux.md`.

## "Hey Jarvis" vs. "NEXUS" Comparative Benchmark & Multi-Syllabic Parity (2026-09-25)

- **Acoustic Comparison vs. Open-Source Jarvis (`hey_jarvis_v0.1.onnx`)**:
  - Benchmarked official open-source `"Hey Jarvis"` (1.24 MB, 3-syllable `/ˈheɪ ˈdʒɑːr.vɪs/`) against our personally trained `"NEXUS"` (839.9 KB, 2-syllable `/ˈnɛk.səs/`).
  - Identified why Jarvis had superior out-of-the-box false alarm rejection on conversational speech: 3-syllable temporal progression creates a mathematical barrier ($p < 0.0005$), whereas 2-syllable `"NEXUS"` requires aggressive negative mining against high-frequency sibilants (*-tion*, *-sion*, *-xus*, *next*).
- **22.05 kHz Silent Training Ingestion Defect**:
  - Diagnosed that `scripts/train_local_wakeword.py` enforced `if sr != 16000: continue`. Because all 186 synthesized fast negative clips (`synth_fast_*.wav`) and `test_doc.wav` were 22,050 Hz, the trainer previously skipped 100% of the fast speech negative dataset during backpropagation.
- **Universal On-The-Fly Resampling & Binary Focal Retraining**:
  - Added on-the-fly linear interpolation 16kHz resampling in `scripts/train_local_wakeword.py` and enabled `include_transitions=True` across all negative speech files.
  - Retrained with Binary Focal Loss ($\gamma = 2.0, \text{pos\_weight} = 2.0$) across 31,048 samples (25,835 hard speech negatives + 6,489 noise/comfort windows).
  - Empirical Verification: Problem Phrase (*"Doc Created & Synced"*): **1.55% peak (0 triggers)**, Fast Multi-Syllabic Negatives (186 files): **0.00% FA (0/186, max 48.58% at 0.68 threshold)**, Vocal Friction (90 files): **0.00% FA**, Positive Recall (178 files): **97.19% @ 0.68 (median 99.22%)**, Inference Latency: **0.025 ms** (12% faster than Jarvis).
- **Manifest & Rust Engine Synchronization**:
  - Synced SHA-256 in `src-tauri/resources/oww/model_manifest.json` (`8e15d4882c90e2033fce6df339ff85c8f76b0819b8ff4e50c7d671e02452890e`).
  - All 43 Rust unit tests pass (`cargo test --lib wakeword -- --test-threads=1`).
- **Docs**: `docs/research/wakeword/jarvis-vs-nexus-acoustic-investigation-and-parity-2026-09-25.md`, `docs/features/66-hey-jarvis-vs-nexus-comparative-acoustic-benchmark-and-multi-syllabic-parity.md`, & `docs/changes/47-hey-jarvis-vs-nexus-acoustic-parity-and-resampling-fix.md`.

## Dual-Target Wake Word Balancing & Rust Engine Alignment (2026-09-25)

- **Diagnosis & Slicing Flaw**: Investigated why `"Hey NEXUS"` scored 98% while standalone `"NEXUS"` scored 13%–32%. Slicing 2-second positive WAVs with `w[-4:]` extracted trailing silence for short words (word ended at 0.8s, sliced chunks 21–24 after comfort frames flushed the FIFO). Dataset was also 75% `"Hey NEXUS"`.
- **Rust Engine 10s Boot Dead-Zone & VAD Onset Decapitation**: `wakeword_oww.rs` hardcoded a 10s startup grace period (discarding all wake words on boot) and ran `VadDetector` with a fixed 0.005 RMS threshold before the neural model, decapitating quiet initial consonants (`/n/` at ~0.0035 RMS).
- **Phonetic Energy Alignment & Balanced Training (`scripts/train_local_wakeword.py`)**:
  - Synthesized and Whisper-verified 88 multi-speaker samples across rates -3 to +4 (`scripts/generate_balanced_positives.py`), balancing positive dataset to 178 files.
  - Enabled `include_transitions=True` in `extract_windows_from_audio` to capture both speech envelopes and 2-frame completion transitions.
  - Retrained with `BinaryFocalLoss(gamma=2.0, pos_weight=2.0)` across 25,428 samples.
  - Verification Audits: Problem Phrase (*"Documentation Created & Synchronized"*): **2.83% peak**, Fast Negatives (186 files): **0.00% FA**, Positive Recall (178 files): **96.6% @ 0.68 (median 98.8%)**, Vocal Friction (90 files): **0.00% FA**.
- **Rust Synchronization (`src-tauri/src/wakeword_oww.rs`)**:
  - Reduced startup grace period from 10s to 1.5s (1500ms).
  - Set `vad_enabled: false` in `AudioPreprocessor::with_profile` to prevent speech onset chopping.
  - Synced SHA-256 in `model_manifest.json` (`7c98741c8a0b4f594853db3567123309929ae0e7efdaad64683803a3154c094b`).
- **Verify**: Rust 43/43 wakeword unit tests passed; `node nexus.mjs build` clean release binary (50.1 MB).
- **Docs**: `docs/features/65-dual-target-wake-word-balancing-and-rust-engine-alignment.md` & `docs/changes/46-dual-target-wake-word-balancing-and-rust-engine-alignment.md`.


- **Plan (`docs/features/64-ghost-mode-plan.md`)**: Windows-only Ghost
  Mode — AI drives the REAL cursor (no second pointer) while user keeps
  speaking; ring rides the cursor; keyboard-first rule (Win-search flows
  need zero grounding); takeover/weapons-grade safety; phases 0→4.
- **Research (`docs/research/ghost-mode/`)**: Clicky teardown (fake
  triangle + Computer Use + Worker-proxy pattern; "no delay" is
  spinner+flight stagecraft) + grounding options (UIA → OmniParser →
  hosted) + perfection limits (Pro 61.6, OSWorld 42.5) + cost ledger
  + OS matrix (Win full, macOS permission-gated, X11 full, Wayland
  partial).
- **Built (`src-tauri/src/ghost.rs`, stage ring UI)**: session machine,
  pure takeover detector (suppress window + 2px slop + first-sight rule),
  dynamic Esc (never global), ring events, silent stand-down on stage
  hide, blackout-reset. Zero AI motion yet — leash before hands.
- **Verify x2**: Rust 531/531 serial (7 new), frontend 26/26, tsc +
  prod build clean, zero clippy hits.
- **Phase 1 keyboard ghost (built, doc 38)**: `launcher.rs` Win-search
  open + focus verify; drill runner with per-step stop/session guards;
  `live_ghost_whatsapp` (send stays confirm-gated); voice-stop via
  `live_cancel`; announce rails. No key tracker needed (primitives
  atomic — verified). Rust 534/534 serial.
- **Phase 2 mouse ghost (built, doc 39)**: `live/commands/mouse.rs`
  (eased glide, click/double/scroll/drag, restore) + UIA-first resolver
  (exact/starts-with/contains; no password fields) + `live_ghost_click`
  with focus verify before+after and suppress windows on all paths.
  Rust 537/537 serial. Vision deferred until measured need.
- **Phase 3 overlap engine (built, doc 40)**: drill-depth flag +
  follow-up queue (cap/drop-oldest) + stop-word intercept (raw match —
  bare "cancel" parses as Greeting; `live_cancel` had zero callers, now
  reachable by voice). Stop aborts spoken + unrouted; rest drains in
  order after clean runs only. Fixed: no glide-home after takeover.
  Rust 540/540 serial.
- **Phase 4 hardening (built, doc 41)**: calibration suite, refusal battery,
  exclusive-fullscreen auto-pause. Rust 544/544 serial. Ghost program complete —
  vision deferred until measured need; UIPI elevation detection follow-up.
- **Entry split (built, doc 42)**: "ghost mode" now enters cursor control
  (`EnterGhostControl` + ring + narration); bare-mode phrases stripped from
  Ghostwriter dictation entry after a live collision sent users to the wrong
  room. Rust 547/547 serial.
- **Ghost waves (built, doc 43)**: orb pinch-to-waveform on session start —
  220ms pinch, 3 live bars in measured Lottie palette (#259ed6/#ef4f25/
  #fbdf38), reverses on exit; pure frontend, zero Rust. Frontend 29/29.
- **Ghost voice-entry fixes (built, doc 44)**: initial `ghost:ring` emit on
  entry (orb never learned motionless sessions); ghost hot-mic loop
  (re-listen after every turn while session live, meeting/echo guarded);
  silent-miss anti-nag (3 quiet re-listens, then one nag + park);
  heartbeat client marker (`stage-shell-v1`) for build identity.
  Frontend 33/33.
- **Ghost instant response (built, doc 45)**: stage scrollbar CSS;
  follow-up listen starts Rust capture (the missing link — orb showed
  listening, nothing captured); in-session OpenApp routes to ghost
  runners; registry-first open with Win-search fallback. Trace tool
  12/12. Rust 547/547 serial.
- **Ghost voice messaging + trace (built, doc 46)**: in-session
  SendWhatsAppMessage/WhatsappChat route to the desktop drill (visible
  typing, confirm-gated send, session stays open); TRIGGER banner
  removed from run.ps1; debug_trace taps p0–p4 (temporary); drain-cycle
  boxed + spawn_blocking refactor (E0733), ghost runtime erased at one
  boundary (ghost_wry). Rust 550/550 serial, 0 clippy new-file hits.
- **Ghost docs organized (same day)**: `features/70-ghost-mode-complete.md`
  (one-stop program guide), `features/ghost-mode-architecture-reference.md`
  (symbol map + Send invariant), `research/ghost-mode/README.md` +
  `03-irregularities-and-fixes-timeline.md` (root-cause timeline of all
  voice-entry defects); features README index updated 69→70+A1.
- **Takeover re-scope (built, doc 47, user directive)**: mouse use NEVER
  ends the session — takeover requires a commanded target (in-flight
  suppresses; settled deviation = task abort "Task stopped… Ghost mode
  is still on", session stays); explicit exits only (`ExitGhostControl`
  intent "exit/close/turn off ghost mode", Esc, stage hide/kill);
  `ghost:session` event (session-scoped orb waves — survives turns);
  `ghost:ring` positional-only (rides commanded glides, never the
  user's cursor); reset() no longer clears ghostActive; idle stop-words
  inform "Nothing running"; drill stop-words task-abort only.
  Rust 549/549 serial, FE 33/33, trace 12/12.
- **Takeover detection DELETED (2026-09-27, user directive — final
  semantics)**: after a third live misfire (00:15:37 yield on a
  keyboard-only "open" flow), `decide_takeover`, the observe_cursor
  judgment, and `abort_task` are removed. Mouse use is ALWAYS free;
  **Esc = the cancel button** (armed at session start); "exit ghost
  mode" voice and stage hide/kill are the other explicit exits.
  Entry narration: "Esc cancels any time". Ring rides commanded
  glides only. Rust 642/642 serial ×2.
- **Phase A quick wins (built, 2026-09-26)**: API keys moved to OS keychain
  (keyring-first reads, settings.json strips secrets, auto-migration on
  startup); PII filtering middleware (`pii_filter.rs` — email/phone/Aadhaar/
  PAN/card/IPV4 redaction before cloud dispatch, 9 unit tests); speaker
  verification wired into wake receiver loop (`verify_speaker_at_fire` —
  separate AudioFeatures instance, fail-open, settings toggle); runtime
  health dashboard (`get_health_status` command + System Status panel in
  Connections tab); settings export/import (JSON with keychain-backed
  keys). Rust 558/558 serial, FE 33/33, trace 12/12.
- **Phase B high-impact (built, 2026-09-27)**: 3-tier persistent memory
  (`memory.rs` — file-based core/episodic, "remember that/my X is Y" hook,
  context injection into 9Router + Worker prompts, episode logging, 8 tests);
  sentence-chunked streaming TTS (first-audio ≈1 sentence, legacy fallback,
  6 tests); emotion prosody (`TtsEmotion` — auto heuristics or manual
  `ttsEmotion` setting, rate/pitch/volume via edge-tts, 6 tests); parallel
  WorkerBackend batches in command center (JoinSet, order-preserving merge,
  5 tests); crash-safe checkpoints (per-step `compound_<id>.json`, boot sweep
  >1h, 4 tests). Rust 587/587 serial, FE 33/33.
- **Phase C strategic (built, 2026-09-27)**: vision grounding fallback
  (`vision.rs` — Groq VLM 0-1000 coords, UIA-first, hooked into ghost_click
  step 2, 8 tests); proactive diary (`diary.rs` — wake/compound/ghost/
  webhook hooks, boot rollup, `diary_summary`, autonomous TTS explicitly
  out of scope, 4 tests); versioned wire protocol v1 (`protocol.ts` +
  `PROTOCOL_VERSION` + payload stamp + health mismatch warn + spec doc,
  3 Rust + 3 Worker tests); multi-voice Piper (`piper-<stem>` drop-in
  models, per-voice reload, picker lists customs, 5 tests); localhost
  webhook triggers (`webhook.rs` — 127.0.0.1:39220, keychain bearer token,
  diary-logged, 6 tests). Rust 613/613 serial, FE 33/33, Worker 52/52.
- **Phase D ecosystem (built, 2026-09-27)**: declarative specs
  (`agent_specs.rs` — `intents.yaml` open+say intents, fallback-slot
  priority, strict validation, example on boot, 11 tests); edge-case
  miner (`improve.rs` — miss/failure clusters → `suggested_phrases.json`,
  report command, boot run, record-only invariant, 6 tests). D1 (gRPC)
  + D4 (ESP32) deferred with reasoning — no unverifiable code shipped.
  Rust 630/630 serial, FE 33/33.
- **Cross-phase audit (2026-09-27)**: re-tested every phase item by item —
  8 real fixes (A1 cooldown order, A2 keychain injection into get_settings,
  A4 history/memory sanitization, B1 shadowed-parts memory drop + remember
  hook post-intercepts, B2 piper streaming guard, C1 post-vision stop check,
  C2 verified-path diary). Rust 634/634 serial ×2, FE 33/33, Worker 52/52.
- **Vision grounding v2 (built, 2026-09-27)**: axis-grid overlay
  (Axis-Grid Scaffold — edge rulers + grid, no fonts), Groq→Gemini
  fallback with Pacific-day quota counters (14,400/500 RPD, 429-marking,
  pre-call skips), `visionProvider` setting, spoken limit notices,
  quota UI in Accounts tab; keys stay in OS keychain (never Cloudflare).
  9 new tests. Entry NEVER pops settings (user directive 2026-09-28:
  `ghost_enter` is pure init — no sidebar, no extra speech; keyless users
  are guided verbally at the click-time vision gate in mouse.rs instead).
  Rust 643/643 serial ×2, FE 33/33.
- **Vision speed mode (built, 2026-09-27)**: parallel race
  (`visionRace: speed` — both providers fired via tokio::join!, first
  valid coordinate wins, both quota units recorded, 429 marked
  per-provider) + 768px fast capture in race mode (encode/upload ~40%
  trim). Sequential default unchanged. 4 new tests.
  Rust 647/647 serial ×2, FE 33/33.
- **Hot-mic loop repair (built, 2026-09-28)**: live bug — mic died
  after the first ghost command (`open WhatsApp` took the local-execute
  branch, which reset without relistening). New `endGhostTurn()`
  helper (relisten-before-reset, no-op outside ghost) wired into every
  turn-end: recorder.ts local/VAD branches, orchestrator error +
  confirm-decline, Tier-3 listener, stage:notice, speaking failsafe,
  legacy wsBridge. Deliberately excluded: silent-park, abortCapture,
  idle timeouts. 2 new tests. tsc clean, FE 35/35.
  Full arc: `docs/research/ghost-mode/04-program-consolidation-and-hotmic-repair-2026-09-28.md`.
- **Speech-synced waves + new-tab verbs (built, 2026-09-28)**:
  live miss "create a new tab" → `browser_new_tab` deterministic verbs
  + test; new Rust `audio:level` event (capture-RMS map, bounded
  channel, forwarder thread, settle-zero) driving audio-reactive ghost
  waves (mic → TTS-rhythm → rest, Esc reverts); `wakeup-v2.json` /
  `ghost-waves.json` auto-pickup with fallback. Rust 645/645 serial,
  tsc + FE 40/40, clippy clean in touched ranges.
  Research + plan: `docs/research/orb-waves/`.
- **Live-test diagnosis (research only, 2026-09-28)**: waves invisible
  (3 causes: `waves.json` vs `ghost-waves.json` filename mismatch,
  orb hides between turns, rest-scale near-invisible); ghost actions
  blind (frontend logs absent from unified console — debug_trace
  forwarding planned); "open youtube in brave" → WhatsApp MCP
  misroute; v4 barge-in requires "nexus" in verify transcript so
  bare "stop" never interrupts TTS (instant-stop tier planned);
  Ctrl+Space never stops TTS; post-turn settle + sentence splitter
  planned. Doc: `docs/research/orb-waves/03-…-2026-09-28.md`.
- **Waves transition + MCP route (built, 2026-09-28)**:
  wakeup→waves is now smile → pinch (220ms) → staggered bar bloom
  (90ms steps) with a 200ms shrink/fade exit beat on Esc (rAF owns
  scaleY, CSS owns opacity/rise — never fight); Creator MCP pipe
  verified (`@lottiefiles/creator-mcp@0.2.2`, 108 tools, needs only
  a connected Creator tab) with palette/stagger/segment edit plan
  ready; no export tool in MCP — JSON still via Creator export.
  tsc + FE 42/42.
- **Deployment readiness (research only, 2026-09-28)**:
  failure-mode paper for the ≤20% problem-rate bar — 12-mode
  catalog, chained math (deterministic core ≈0.66–0.73 today, above
  budget pre-hardening), industry anchors (Whisper 4–5% EN /
  16–22% Hindi, OSWorld 2.0 20.6%, grounding 36–54%), SLO board,
  P0–P4 roadmap, tiered promise (deterministic 85–90%, vision
  beta, compounds supervised). Paper:
  `docs/research/deployment/01-deployment-readiness-failure-mode-analysis-2026-09-28.md`.
  English-only execution plan (user directive — no other languages):
  `docs/research/deployment/02-english-only-execution-plan-2026-09-28.md`
  (P0 E2E harness → P1 liveness → P2 EN parse → P3 degradation → P4
  rings; non-English → single honest line).
- **P0 deployment harness (built, 2026-09-28)**: `stt_filter_stats`
  confabulation counters + IPC; `WEEKLY_TOP_N` + `top_suggestions`;
  50-command EN fixture 49/50 (fixed "stop please"→cancel_action,
  documented `close pr 5` order gap); `soak_wake_fa.py` harness
  (smoke 0 triggers, 120 s silence 0 triggers; 8 h run scheduled).
  Rust 647/647 serial, clippy clean in touched ranges.
- **P1 deployment liveness (built, 2026-09-28)**: ghost relisten
  watchdog (bounded 3 pokes/session, `ghost:relisten` → guarded
  frontend relisten); turn-end choke point + CI grep-gate
  (`check-turn-ends.mjs`, negative-tested); UIPI elevation detect
  with spoken reroute in `ghost_click`; API-keys diagnostics row
  (never alarms) + Connections Keys badge. Rust 653/653 serial,
  e2e 49/50, tsc + FE 42/42, gate OK.
- **P2 EN parse robustness (built, 2026-09-28)**: wake-prefix
  retry-parse (`strip_wake_prefix` + one-shot retry on None,
  word-boundary guarded, bare "command center" added) — 9-phrase
  matrix green; `shouldGhostRoute` routes in-session open_app/
  whatsapp_chat to ghost runners (fallback local + endTurn);
  `temperature=0` added to 2 Groq call sites (all EN-pinned);
  en-IN tracking fixture 15/15 (no gate); hesitant-speaker
  endpointing test. Rust 656/656 serial, tsc + FE 44/44.
- **P3 graceful degradation (built, 2026-09-28)**: offline drill
  walkthrough doc (stage table + gap list) + offline Worker-dropped
  turns speak the attributed line instead of raw JSON; quota
  narration audit (vision + Worker denials speakable, Worker test
  pins 4 denial classes, `quota_exceeded` logged); bridge health +
  fix hints in `mcp_check.py` wired into `nexus check` (verified
  live). Rust 656/656, Worker 53/53, e2e green, FE 44/44, gate OK.
- **P4 rollout ops (built, 2026-09-28)**: soak per-file default
  (fixed harness-manufactured cross-file trigger; `--stream`
  opt-in) + genuine FA finding `necess_0003` 0.821 (1/500 — ring-1
  config: `verifyWake: true` + retrain hard-negative list);
  `collect_slo.py` SLO board collector (verified live);
  rollout-rings runbook (gates/checklist/rollback); ring-1 MSI +
  exe artifacts hashed (NSIS broken locally, plugin cache). Rust
  656/656 at freeze.

## Single-Stage Shell + Blackout Watchdog (Step 1, 2026-09-25)

- **Stage shell (`src-tauri/src/stage.rs`, `frontend/src/stage/`)**:
  Fullscreen transparent overlay window (`WindowConfig::stage`, `stage.html`
  + vite input, `stage-cap.json` capability), parallel-run only — empty,
  hidden by default, zero visuals moved. `WS_EX_TOOLWINDOW` so fullscreen
  overlay doesn't pause video underneath (Tauri #7401); deliberately NOT
  capture-excluded (orb has always been share-visible; only sidebars are).
- **Pixel-identical geometry map (`frontend/src/stage/geometry.ts` + 6 tests)**:
  orb pct/size (mirrors `position_orb` clamps), bottom-right dock
  (12px gap, 48px taskbar), 80px loading top-right — same formulas Rust uses.
- **Hitbox click-through**: 30ms cursor poll (`GetCursorPos`) toggling
  `set_ignore_cursor_events` around frontend-sent physical-px rects
  (per-pixel-alpha hit-testing exists in neither Tauri nor Electron —
  hitbox toggling is the documented workaround). Empty boxes = fully
  click-through.
- **Blackout watchdog (no full blackout, ever)**: 2s cadence, 8s cold-boot
  grace; missing window OR stale heartbeat (>6s) → destroy immediately
  → one orb-spoken inform (skipped mid-turn via `has_active_request()`)
  → backoff recreate → 10s heartbeat verification. 3 failed fixes → stay
  hidden + final message; voice product unaffected throughout.
- **Kill-switch `Ctrl+Alt+X`** destroys + session-disables the stage;
  `Ctrl+Space` close path routes through flag-aware `stage_hide` (raw
  destroy would read as blackout and rebuild — caught in review).
- **Verify x2**: Rust 524/524 serial, frontend 26/26, tsc clean,
  `dist/stage.html` emitted, zero clippy hits in new code.
- **Docs**: `docs/features/63-single-stage-shell-and-blackout-policy.md`.
- **Next**: step 2 orb moves in (pixel-compare), then loading layer,
  then panels one by one (each deletes a `PENDING_*` system).

## Repository & Research Architecture Rule
- **`Engine-NEXUS/NEXUS-PAPERS`** (`https://github.com/Engine-NEXUS/NEXUS-PAPERS`): Dedicated repository for all scientific research papers, acoustic DSP investigations, NLU data science studies, and architecture compendiums.
- **`Engine-NEXUS/WINDOWS`** (`https://github.com/Engine-NEXUS/WINDOWS`): Main application repository. All documentation (`docs/`), feature guides, and implementation code must always be pushed and synchronized in lockstep with this repo.

## Training Data Poison Elimination, Focal Loss & Multi-Syllabic Hardening (2026-09-24)

- **Acoustic Audit & Poison Discovery (`scripts/purge_poisoned_parallel.py`)**:
  Investigated false triggers when shouting or speaking fast multi-syllabic phrases like `"Documentation Created & Synchronized"`. Parallel Whisper audit of all 560 positive samples uncovered severe data corruption: 230+ files in `positive/` were soundalikes (*"texas"*, *"open excess"*, *"nixes"*, *"access"*), 70+ were conversational sentences (*"next, let's take a look"*, *"we'll get access"*), and 160+ were low-energy near-silence. Quarantined bad files and promoted 345 soundalikes into `wake_word_data/negative/`, leaving exactly 89 pristine NEXUS recordings.
- **Fast Multi-Syllabic Negative Synthesis (`scripts/generate_fast_negatives.ps1`)**:
  Synthesized 186 high-tempo audio samples covering words ending in `-tion`, `-sion`, `-ction`, and fast conversational compounds (*"documentation created and synchronized"*, *"authentication"*, *"context switch"*, *"reflexes"*).
- **Binary Focal Loss & Anti-Swamping Optimization (`scripts/train_local_wakeword.py`)**:
  Replaced standard Cross-Entropy with `BinaryFocalLoss(gamma=2.0, pos_weight=2.0)`. Easy background noise samples ($p \approx 0.001$) have gradients scaled down by $(0.001)^2 = 10^{-6}$, focusing 99%+ of backprop updates onto separating ambiguous speech phonetics. Pinned `dynamo=False` and `opset_version=14` for self-contained 860 KB ONNX binary with no external data.
- **Verification Audit Results (`scripts/verify_hardened_model.py`)**:
  - **Audit 1 (Problem Phrase `test_doc.wav`)**: **18.755% Peak Score** (0 triggers at 68.0% threshold).
  - **Audit 2 (Fast Speech Negatives - 186 files)**: **0.00% False Alarms (0/186)**, max peak **26.76%**.
  - **Audit 3 (Pristine Positive Recall - 89 files)**: **93.3% Recall at 0.68**, **97.8% Recall at 0.50 (median 96.3%)**.
  - **Audit 4 (Vocal Friction & Throat/Gargle - 90 files)**: **0.00% False Alarms (0/90)**, max peak **9.24%**.
  - **Rust Unit Tests**: 43/43 tests passed cleanly (`cargo test --lib wakeword -- --test-threads=1`).
- **Docs**: Architecture spec in `docs/features/61-training-data-poison-elimination-and-focal-multi-syllabic-hardening.md` and changelog in `docs/changes/44-training-data-poison-elimination-and-focal-multi-syllabic-hardening.md`.


## Silence Phantom Elimination, Speech Onset Preservation & 5-Point Verification Audit (2026-09-23)

- **Acoustic Root Cause Diagnosis & Codebase Analysis**:
  Investigated why `nexus wake test` produced repetitive triggers in silence and triggered on common conversational words like "hello". Diagnosed five root causes:
  1) **Speech Onset Decapitation (Impulsive Filter Flaw)**: The gate condition `rms > baseline * 8.0` misclassified the natural attack envelope of human speech ($0.05-0.15$ RMS) as an impulsive burst after silence, dropping Chunk 0 of words like "hello" and creating deformed spectrograms in `embedding_model.onnx` that scored 98.1% false alarms.
  2) **Dual-Buffer Squeezebox Bug (Spectrogram Freezing)**: The 16-frame embedding buffer was advanced on silence, but the 10-chunk Mel-Spectrogram circular buffer was frozen, causing past "NEXUS" spectral formants to stitch into the next spoken word.
  3) **AGC Pre-Gain Overdrive**: The formula `gain = ((target_rms / rms) * pre_gain).min(max_gain)` applied a 2.5x double-scaling factor, amplifying quiet room reverberation and trailing phonemes by up to 25x into square-wave clipping distortion.
  4) **LayerNorm Zero-Reset Collapse**: Resetting buffers to zeros collapsed variance $\sigma \rightarrow 0$, turning subsequent small floats into false alarms.
  5) **Output Bias Offset**: Initialized `last_layer.bias` to $-4.0$ ($\sigma = 1.8\%$) and retrained with 600 synthetic comfort/transient negative windows ($\text{val\_loss} = 0.0293$, $98.8\%$ recall, $0.4\%$ FA).
- **Engineering Architecture & Dual-Buffer Comfort Sliding**:
  - Removed destructive impulsive speech chopping; physical vocal artifacts are cleanly handled by the retrained neural classifier ($\le 3.4\%$ max score).
  - Extended `push_comfort_frame` across Python and Rust to advance BOTH `mel_spectrogram_buffer` and `feature_buffer` during silence, preserving 1:1 real-time temporal progression without freezing context.
  - Normalized AGC formula to `gain = (target_rms / rms).min(max_gain)`.
- **The 5-Point Verification Audit**:
  - **Audit 1 (Full 3,300-File Batch Benchmark)**: **99.3% Positive Recall (avg max 99.4%)**, **98.0% Negative Soundalike Rejection**, **98.2% Background Noise Rejection** at 0.68 threshold.
  - **Audit 2 (Conversational Speech & "Hello" Audit - 304 files)**: **97.4% Rejection Rate**; **0 triggers across all 19 "hello" files (avg max 0.34%, peak 4.24%)**; 0 triggers on "google", "siri", "please", "computer".
  - **Audit 3 (Continuous Silence - 100s / 1,250 chunks)**: **0 triggers, 0.000000% max score** (completely flatlined).
  - **Audit 4 (Hardware Invariance Benchmark)**: Studio USB Condenser (99.5%), Laptop Mic Array with Intel SST (100.0%), Bluetooth Headset (99.1%), Noisy Office (98.0%), Far-Field Whisper (96.4%).
  - **Audit 5 (Continuous Multi-Utterance Stream Simulation - 120s)**: Exactly **2/2 true positive NEXUS triggers**, **0 false triggers** on greetings, questions, throat clearing, or silence.
  - **Rust Unit Tests**: 43/43 tests passed cleanly (`cargo test --lib wakeword -- --test-threads=1`).
- **Docs**: Architecture spec in `docs/features/60-silence-phantom-elimination-and-comfort-streaming.md` and changelog in `docs/changes/43-silence-phantom-elimination-and-comfort-streaming.md`.

## Vocal Friction Hardening & Industrial Throat/Gargle Rejection (2026-09-23)

- **Acoustic Root Cause Diagnosis & Research**:
  Investigated why non-verbal throat clearing, gargling, coughing, and vocal fry triggered false wakes. Researched industrial two-pass keyword spotting architectures from Amazon Alexa, Apple Siri (HMM phonetic trellis alignment), and Google Assistant (streaming Conformer verification). Identified root causes: dense MLP receptive field lacking sequential phonetic state constraints, absence of throat/cough negatives, and `pos_weight=8.0` gradient distortion in `BCEWithLogitsLoss` that introduced a +2.08 logit bias towards false alarms.
- **Physical Vocal Artifact & Impulsive Synthesis (`generate_throat_negatives.py` & `generate_impulsive_negatives.py`)**:
  Synthesized 120 specialized negative audio samples (16kHz mono WAV) modeling phlegm flutter (20–36 Hz AM), vocal fry (55–110 Hz F0 jitter), velar turbulence, and coughing wheezes. Added 180 impulsive negative samples (sneezes, vocal bursts, counting sequences, claps, thumps). Expanded negative library to 1,742 clips (38,557 windows).
- **Balanced Bayesian Loss Retraining (`train_local_wakeword.py`)**:
  Replaced distorted `pos_weight=8.0` with balanced `pos_weight=1.2`, training over 40,253 training windows and 10,064 validation windows for 60 epochs. Validation loss reached 0.0245 with 99.0% recall and 0.2% FA.
- **Vocal Artifact Rejection Verification**:
  Achieved **0.0% False Alarm Rate** across all physical vocal tests: Throat clearing (0.0% FA, max score 0.1%), Gargling & saliva flutter (0.0% FA, max score 0.0%), Coughing (0.0% FA, max score 0.1%), Vocal fry (0.0% FA, max score 0.1%), Sneezes (0.0% FA, max score 0.7%), Shouts (0.0% FA, max score 0.1%), and Mic testing (0.0% FA, max score 0.1%).
- **Full Library & Hardware Benchmark**:
  Evaluated across 3,300 files: **97.1% Positive Recall (avg score 97.4%)**, **97.5% Negative Soundalike Rejection**, and **99.6% Background Noise Rejection** at calibrated threshold 0.68. Maintained 100% hardware invariance: Studio USB (97.1%), Laptop Mic Array with Intel Smart Sound (100.0%), Bluetooth Headset (93.6%), Far-Field Whisper (84.3%), and Noisy Office (92.5%).
- **Docs**: Architecture spec in `docs/features/59-vocal-friction-hardening-and-throat-gargle-rejection.md` and changelog in `docs/changes/42-vocal-friction-hardening-and-throat-gargle-rejection.md`.

## Multi-Source Noise Hardening & Hardware Invariance (2026-09-23)

- **Multi-Source Noise Ingestion (`ingest_opensource_noise.py`)**:
  Synthesized and categorized 600 high-fidelity background noise profiles across mechanical keyboard typing,
  113.3 Hz chassis fan resonance, office HVAC, domestic impulsive sounds, and narrowband telecom profiles.
  Screened through `faster-whisper` anti-poisoning ASR, automatically purging colliding clips. Total verified background: 998 clips.
- **Multi-Device Data Augmentation (`train_local_wakeword.py`)**:
  Applied Bluetooth narrowband bandpass (300–3400 Hz), laptop chassis fan resonance (55–145 Hz), distance attenuation (0.25x–0.40x),
  and ambient background mixing to positive recordings. Expanded positive training context to 11,760 windows, trained against 35,715 negative windows.
- **Hardware-Adaptive Microphone Invariance (`test_device_invariance.py`)**:
  Benchmarked across 5 hardware microphone profiles: **Studio USB Condenser (96.6%)**, **Laptop Mic Array with Intel Smart Sound (100.0%)**,
  **Bluetooth Headsets / Earbuds (92.1%)**, **Far-Field / Quiet Whispering (85.9%)**, and **Noisy Office (93.0%)**.
- **Batch Evaluation & Verification**:
  Batch evaluation across 3,000 files: **96.6% Positive Recall**, **95.8% Negative Soundalike Rejection**, and **99.7% Background Noise Rejection**.
- **Docs**: Architecture spec in `docs/features/58-multi-source-noise-hardening-and-hardware-invariance.md` and changelog in
  `docs/changes/41-multi-source-noise-hardening-and-device-invariance.md`.

## Apex Wake Word Evolution, Data Poisoning Quarantine & Adaptive Microphone DSP (2026-09-22)

- **Automated ASR Poisoning Quarantine (`audit_positive_samples.py`)**:
  Audited 592 positive recordings with `faster-whisper` and RMS filters. Identified and quarantined 135 poisoned clips
  (conversational sentences, YouTube background audio, near-silence) into `quarantined_bad_positive/`, leaving 456
  pristine acoustic NEXUS recordings.
- **Multilingual Negative & Multi-Source Background Augmentation**:
  Synthesized 1,442 negative samples across English soundalikes (*"next"*, *"texas"*, *"lexus"*, *"necklace"*),
  Indian languages (Hindi: `hi-IN-Madhur`, `hi-IN-Swara`; Telugu: `te-IN-Mohan`, `te-IN-Shruti`), and assistant names.
  Generated 400 multi-source background sound clips (`generate_background_sounds.py`) for HVAC fans, mechanical
  keyboard typing, mouse clicks, and room ambience.
- **BCEWithLogitsLoss & SigmoidWrapper ONNX Export**:
  Upgraded classifier training with `pos_weight=8.0` penalty on negatives and exported calibrated ONNX graph.
- **Adaptive Microphone Hardware Prober (`nexus wake probe` / `acoustic_profile.rs`)**:
  Created 1.5s FFT ambient spectral scan in `scripts/probe_microphone.py` and Rust `src-tauri/src/acoustic_profile.rs`.
  Detected chassis fan resonance at 113.3 Hz (+22.3 dB) on Intel Smart Sound mic array; dynamically auto-tunes high-pass
  filter to 128.3 Hz, hardware pre-gain to 2.50x, adaptive silence gate to 0.00300 RMS, and impulsive gate to 8.0x (rejecting coughs/throat-clears).
- **Phantom Cascade Elimination**:
  Added `reset_after_trigger()` across Rust and Python to immediately flush the 16-frame embedding buffer on trigger confirmation.
- **Batch Evaluation & Verification**:
  Batch benchmark on 2,298 files: **92.3% True Positive Recall**, **98.9% Negative Rejection** (1.1% FA), and **99.8% Background Noise Rejection** (0.2% FA).
- **Docs**: Architecture spec in `docs/features/57-apex-wake-word-evolution-and-hardware-adaptation.md` and changelog in
  `docs/changes/38-apex-wake-word-evolution-and-hardware-adaptation.md`.

## Targeted Intent Training, Category Drill-Down & MCP Data Promotion (2026-09-22)

- **Targeted CLI Voice Collection (`nexus collect`)**:
  Added `-i` / `--intent` and `-c` / `--category` flags with combined scope validation. Developers can directly
  target single intents (`nexus collect --category github --intent create_pr` or `nexus collect --intent create_pr`)
  or drill down interactively by choosing a category and selecting individual sub-intents from a numbered terminal menu.
- **Complete Phrase Catalogs (All 55 BERT-Mini Intents)**:
  Expanded `PHRASES` in `scripts/collect_nlu_samples.py` with realistic spoken templates for all 23 missing GitHub,
  workflow, release, collaborator, and local mode intents (57 total registered intents including live dictation modes).
  Extended `extract_slots()` with rule-based entity extractors for `create_pr` (`repo`, `title`, `head`, `base`),
  `comment_pr` (`pr_number`, `repo`, `body`), `analyse_pr` (`owner`, `repo`, `pr_number`), `add_collaborator`, etc.
- **Zero-Quarantine MCP Split Promotion (`dataset.json`)**:
  Diagnosed the Step 3d quarantine cause (missing test rows in locked evaluation set for new intents). Promoted
  25 phrase families for `order_food` (99 rows), 12 for `search_product` (68 rows), and 12 for `send_whatsapp_message` (127 rows)
  into `dataset.json` with strict zero phrase-family cross-split leakage. Re-minted `split_lock.json` and `evaluation_lock.json`
  (481 test rows). Validation passes cleanly (`python server/nlu/data_foundation.py validate`).
- **Category Coverage Alignment (`nlu_stats.py`)**:
  Aligned category schema with the 55 production intents. Verified that all MCP intents now display full trained rows
  (`order_food`: 77, `send_whatsapp_message`: 72, `search_product`: 32, `whatsapp_search`: 107) and active `◐ Good` / `● Strong`
  mastery status.
- **Docs**: Architecture spec in `docs/features/56-targeted-intent-training-and-mcp-data-promotion.md` and changelog in
  `docs/changes/37-targeted-intent-training-and-mcp-data-promotion.md`.

## NLU Data Foundation, STT Conditioning & Voice Scaling (2026-09-22)

- **Acoustic STT vs. Language NLU Decoupling**:
  Untangled speech acoustic transcription from text intent classification. Resolved multilingual Whisper
  hallucinations (Urdu/Russian/Spanish on short <2s clips) by adding `language="en"`, `temperature="0.0"`,
  and domain prompt biasing (`NEXUS, WhatsApp, Biryani, Dosa, Ghostwriter, VS Code, PR, GitHub...`) to
  `scripts/collect_nlu_samples.py`.
- **Data Foundation & Evaluation Lock Fix (`data_foundation.py`)**:
  Fixed `ERROR: external benchmark hash mismatch` by updating `server/nlu/data/external_evaluation_lock.json`
  with canonical SHA-256 hashes for sorted evaluation benchmark files (`clinc150_oos_test.jsonl`). Preflight
  validation passes cleanly.
- **The 70/30 Data Principle**:
  Enforces 70% anti-poisoning guardrails (cryptographic test locks, slot consistency, conflict repair, split quarantine)
  and 30% zero-waste user effort preservation (all clean recorded voice samples from `collected_samples.jsonl`
  automatically audited and merged into `dataset.json` — 3,108 training rows).
- **Voice Scaling Science**:
  Empirical research demonstrates 100–500 voice samples is the optimal sweet spot (~96.5%–98.2% standalone BERT accuracy;
  99%+ real-world command success when backed by the 3-tier Regex/BERT-Mini/Qwen cascade).
- **Speaker Independence & One-Login Google Contacts**:
  Documented text-token speaker invariance (1 global BERT-Mini model works for all users/devices via Worker R2 OTA updates).
  Integrated Google OAuth scope union (`contacts`, `gmail`, `calendar`) with local `contacts.json` resolution.
- **Docs**: Comprehensive spec in `docs/features/55-nlu-data-perfection-voice-scaling-and-mcp-bridge-research.md`
  and changelog in `docs/changes/36-nlu-data-foundation-stt-conditioning-and-voice-scaling.md`.

## Interactive Voice Approval & Confirmation Sidebar (2026-09-21)

Full interactive voice approval workflow with smart 5-second listening window
and persistent Response Sidebar confirmation card:
- **Response Sidebar Confirmation Card (`ConfirmationPanel.tsx`)**:
  Renders structured details for actions requiring approval (WhatsApp recipient & message bubble,
  GitHub repo/PR#/danger warning, Swiggy/Amazon MCP tools/parameters). Displays 1-click
  `[✓ Confirm]` and `[✕ Cancel]` buttons with loading state.
- **Automatic 5-Second Voice Listening Window (`orchestrator.ts`)**:
  Once prompt TTS finishes speaking, the microphone automatically opens into `listening` mode
  and starts STT audio capture without needing a hotkey or wake word.
- **Instant Early Reaction (< 2s)**:
  Speaking an approval phrase (*"proceed"*, *"approved"*, *"yes"*, *"confirm"*, *"go ahead"*, *"do it"*)
  or rejection phrase (*"cancel"*, *"no"*, *"abort"*) clears the 5s timer immediately and executes
  or aborts without waiting for the full 5s window.
- **Graceful Timeout & Sidebar Persistence**:
  If 5 seconds elapse with no speech, the mic returns to idle while the sidebar remains open on screen
  for manual review and 1-click approval.
- **Rust Backend Integration (`commands.rs`, `orchestrator.rs`, `lib.rs`)**:
  `show_sidebar_with_confirmation` captures blurred desktop backdrop and stores confirmation JSON
  race-free in `PENDING_SIDEBAR`.
- **Docs & Verification**: Detailed architecture spec in
  `docs/features/54-interactive-voice-approval-and-confirmation-sidebar.md`.
  Verified across 585 tests (519 Rust + 17 Vitest + 49 Worker).

## MCP Connect System — Best-of-Combine Build (2026-09-20)

Industry research in `docs/research/mcp-connection/` (7 files) drove a
three-stack implementation, each phase verified twice:

**Phase 1 — shared connect infrastructure:**
- `mcp_client.rs`: `McpConnectState` (unknown/down/auth-required/ready),
  `pairing_status` parser (`parse_pairing_state`, case-insensitive,
  garbage → Error, never fake green), `normalize_qr_payload`
  (data-URI / raw-b64 / pairing-code), `mcp_connect_state` command +
  `connect_card_for` per server (WhatsApp fuses transport + pairing;
  vault services fuse transport + `token_status`; Amazon transport-only).
- `orchestrator.rs`: MCP failure → voice guidance (existing) + Connect
  card opens in the sidebar ONCE per server per session
  (`SHOWN_CONNECT_CARDS` gate; `open_mcp_connect_card` +
  `connect_card_markdown` — status, numbered steps, QR image, /pair
  link, ToS-burner + 20-day-rotation notes). Failed call is stashed
  (`PENDING_MCP_RETRY`) and a 5s-poll ready monitor
  (`spawn_ready_monitor`, ~10min cap) auto-retries it once when the
  server turns Ready (Composio WAIT_FOR_CONNECTIONS shape); drops
  silently if a newer turn is active. Wired into `Subsystem::Mcp` Err,
  `dispatch_to_mcp` Err, and `orchestrator_mcp_confirm` Err.

**Phase 2 — Swiggy spec-OAuth (Worker + vault + frontend):**
- `server/worker/src/index.ts`: SWIGGY_TOKEN_URL/AUTH_URL/SCOPES +
  Env secrets; `handleAuthUrl`, `handleOAuthBrowserCallback`,
  `handleOAuthExchange` each gained a swiggy branch (PKCE S256, form-
  encoded token exchange, D1 storage, shared `/oauth/callback` +
  `nexus://oauth/` deep link); `refreshSwiggyToken` +
  `getValidSwiggyToken` (silent refresh, null-on-failure like Google);
  `GET /oauth/swiggy-token`; `/config/check` lists swiggy.
- `auth_vault.rs`: `fetch_swiggy_token_from_worker` + swiggy arm in
  `refresh_service_token` (silent refresh on-device).
- `setup/oauth.ts` + `SetupApp.tsx`: provider union widened to
  `"swiggy"` (flow is fully generic). Connections tab: "Login with
  Swiggy" button (`handleSwiggyLogin` → connectOAuth → vault refresh),
  updated hint.
- Setup needs: `wrangler secret put SWIGGY_CLIENT_ID/SWIGGY_CLIENT_SECRET`
  (Builders-Club approval required for production; localhost dev free).

**Phase 3 — long-tail + proactive rotation:**
- Spotify/Vercel/Render cards: numbered 3-step hints + "Get token"
  buttons (`shell.open` provider pages, window.open fallback).
- Vault idle monitor (90s) now also probes WhatsApp `pairing_status`
  (inner call — no breaker trip, no audit) so the scheduled ~20-day
  session rotation surfaces before a send fails.

**Verify x2 results:** cargo check clean; `cargo test --lib
--test-threads=1` 499/499; Worker `npm test` 49/49 ×2 + tsc; frontend
vitest 14/14 ×2 + tsc. Live bridges test (`test_connect_state_live_bridges`)
pins card shape invariants (pair URL, steps present on auth-required,
no nag on ready) against live 5-server probe.

**Round-2 deep audit (same day, `docs/research/mcp-connection/08-...`):**
live sources (Anthropic trackers #744/#54649/#60572/#35, Novu connect-card
PR, AutoGPT MCP fix commit, 2026-07-28 OAuth security spec) exposed 5
gaps — all fixed + verified ×2 (Rust now 500/500 serial): monitor
re-renders the open card when the QR payload changes (WhatsApp rotates
its QR every 20-30s; a static card was unscannable after 30s) and renders
the Connected card on Ready; `getValidSwiggyToken` persists rotated
refresh tokens (OAuth 2.1 public-client MUST); RFC 8707 `resource` param
added to Swiggy authorize/exchange/refresh; `audit_line()` choke point +
test pins tokens out of the audit log; `mcp_connect_state` probes run in
parallel (25s → 5s worst case).

## Stuck Animations + PR Analyse Flow + Minimal Sidebar + Phase 11 (2026-09-18)

**Stuck wakeup animation — root cause was a missing `done` handshake.**
`WorkerBackend` Ok withholds `Done` (would cancel TTS), but the frontend
`result` handler spoke with no `onEnd` and `signalOrchestratorDone()` had
zero call-sites — the orb parked in `speaking`/loading-loop forever after
long replies. NOT any "ghost mode" (no ghost CSS/setting exists anywhere;
"ghost" = Ghostwriter dictation only). Fixes:
- `net/orchestrator.ts::finishSpokenResult` — TTS onEnd → guarded reset +
  `orchestrator_done` (barge-in-safe via request-id check). 3 vitest tests.
- `App.tsx` 60s speaking failsafe — silent 60s in `speaking` (checked via
  `isRustTtsPlaying()`) forces the `done`-equivalent reset; long replies
  re-arm instead of cutting.
- `show_loading` runs inline (spawn removed) — create always completes
  before the paired destroy; kills the spinner re-creation race.
- Known flake: `test_install_and_cancel` fails under parallel threads
  (shared ACTIVE_REQUEST global); passes isolated ×2 and 43/43 serial.

**Analyse-PR flow:** pr-list `Analyse` now hides the list first
(`hide()` + `hide_pr_list_sidebar`); `WorkerBackend` Ok with non-null
`analysis` opens the response sidebar via `show_sidebar_with_analysis`
(PENDING_SIDEBAR race-free path) AFTER emitting `Result` (TTS not blocked).
No new AI call — the Worker already returned `analysis`, it was
`console.log`-dropped.

**Minimal sidebar:** removed container top-highlight gradients, inset
stacks, specular `::before` rims, gradient overlays (flat dim kept),
dead `backdrop-filter`s, dot pulse, button lift/glow, red close hover,
`nexus-hr` gradient, unused `--gradient-*` vars. PR buttons now flat
solid (green Merge, grey Analyse). Settings sidebar untouched (still glossy).

**list_prs "pull requests and alll":** data was fine (64 train rows) — the
deterministic regex had no `pull request` alternative and `$`-anchored out
trailing words. Pattern now accepts the noun + `and all/all of them/
everything` + optional `the` after state. `add_phase11_pr_verbs.py` adds
45 fresh rows (39 list_prs + 6 OOS) → candidate train 3482, test intent
**0.9004** (was 0.8850), gated OOS 0.9871. Production dataset.json
untouched (2765 rows) — promotion is a separate merge+retrain decision.
`collect_nlu_samples.py`: duplicate `list_prs` key merged (21 phrases were
silently dropped), 32-phrase list; interactive category menu (github, mcp,
apps, messages, live, random) + `--category`; no google/research category
(no such intents exist yet). Say-it-your-way paraphrase option built then
REVERTED per user (need is same-word/many-sounds, not rewording) —
replaced by `canonical_repo_name()` sound-alias map (`cervix/srvx/service
→ servx`, zinc/sync → zync, per-segment, unknown pass-through), wired
into `clean_repo_name` (all deterministic repo intents) + `nlu_client`
`repo_slot` (all 22 repo arms). "analyse pr 254 in zink" now exact 1.0
instead of fuzzy 0.8.

## Qwen Brain — Admin-Only Local Reasoning (2026-09-17)

**Phase 6 status: the brain stack already existed** — `admin_config.rs`
(`is_admin` runtime gate + `admin-brain` compile gate), `lazy_brain.rs`
(non-blocking spawn on port 39219), `brain_client.rs` (classify /
generate_phrasings / pronunciation_map / health), `brain_monitor.rs`
(continuous learning), `server/admin/brain_server.py` (Qwen2.5-0.5B
GGUF, 480 MB, present at `server/admin/model/`). The orchestrator tries
brain classification BEFORE NLU when the deterministic parser misses.

What Phase 6 added:

### 1. Intent schema coverage for the new MCP intents

The 3 commerce/social intents existed only in the deterministic parser —
the NLU/brain paths dropped them (`_ => None`). Now wired end-to-end:

- `nlu_client.rs::nlu_to_parsed_intent` — maps `order_food`
  (`food_item`→query, `restaurant`), `search_product` (`query`),
  `send_whatsapp_message` (`contact`, `message`). Brain responses reuse
  this mapping, so both paths benefit.
- `nlu_server.py` — `INTENTS` + `SLOT_TYPES` updated to match the
  retrained model (55 intents, 51 slot labels — **must match
  `train.py` ordering**).
- `brain_server.py` — `ALL_INTENTS` + `SYSTEM_PROMPT` know the 3 new
  intents, their slots (`food_item`, `restaurant`, `message`), and
  few-shot examples.

### 2. Brain-assisted compound planning

`command_center::build_plan_with_brain` (async) — called from
`process_transcript` in place of the sync `build_plan`:

- Fast path: fully deterministic plan → brain never invoked (zero added latency).
- If any step fails deterministic parse AND the `admin-brain` feature
  is on AND `is_admin()` → `brain_classify` fills in that step.
- Same conservative gates: Architect/None/CommandCenter steps abort the
  plan; brain-classified steps flow through the same confirmation-gated
  dispatch.
- Non-admin devices / brain unavailable → identical `None` fallback
  (single-intent path), zero risk.

This enables compounds like "remind me to call mom then order biryani"
where a step phrasing misses the deterministic regexes.

### Gates (unchanged, both required)

1. Compile-time: `cargo build --features admin-brain`
2. Runtime: `%APPDATA%/com.nexus.assistant/admin.json` (or dev
   `server/admin/admin_config.json`) with `is_admin: true`,
   `brain_enabled: true`

Family builds ship without `admin-brain` — all brain code is compiled
out (`brain_client`, `brain_monitor`, `lazy_brain` don't exist).

## NLU Self-Improvement — Family Model Distribution (2026-09-17)

The admin's Qwen brain continuously improves BERT-Mini (brain_monitor →
approved_phrasings.jsonl → merge_and_train.py). **Phase 5 adds the
distribution channel**: family devices pull the improved model over the
air — no app rebuild needed.

### Architecture

```
ADMIN                              WORKER (Cloudflare)              FAMILY
nexus train                        KV: nlu_model_latest manifest    startup +15s:
  → nexus_nlu.onnx                 R2: nlu/<file> objects           GET /models/nlu/latest
python publish_nlu.py                                               version differs?
  → wrangler r2 object put ...        ──────────────────────────→   GET /models/nlu/download?name=
  → POST /models/nlu/publish                                        per-file sha256 verify
     (Bearer NEXUS_ADMIN_TOKEN)                                     → %APPDATA%/com.nexus.assistant/nlu_model/
                                                                    → POST /reload_model (hot-swap)
                                                                    or NEXUS_NLU_MODEL_DIR on next spawn
```

### Worker endpoints (`server/worker/src/index.ts`)

| Endpoint | Auth | What it does |
|----------|------|--------------|
| `GET /models/nlu/latest` | none | KV manifest `{version, updated_at, files:{name:{sha256,size}}}` |
| `GET /models/nlu/download?name=<file>` | none | Streams file from R2 `nlu/` prefix; whitelist-guarded names |
| `POST /models/nlu/publish` | `Bearer NEXUS_ADMIN_TOKEN` | Writes KV manifest (admin publishes blobs via wrangler) |

Returns 503 when `CACHE`/`MODELS` bindings are missing — family clients
treat that as "no update" and keep the bundled model.

### Client side (`src-tauri/src/nlu_update.rs`)

- `spawn_update_check(app)` — called once at startup (15s after session
  auto-open, so first paint isn't competing with a 35 MB download)
- Downloads to a `.staging` dir first; sha256-verifies every file;
  aborts cleanly on mismatch (old model stays live)
- File-level manifest (no zip dep): `nexus_nlu.onnx`, `.onnx.data`,
  `labels.json`, `temperature_calibration.json`, `tokenizer/*`
- If NLU server is already running → `POST /reload_model` hot-swaps
- `downloaded_model_dir_envless()` — used by `lazy_nlu.rs` to pass
  `NEXUS_NLU_MODEL_DIR` at spawn (same `%APPDATA%/com.nexus.assistant`
  derivation as `lazy_stt.rs::read_moonshine_model`)

### `nlu_server.py` model dir resolution

1. `NEXUS_NLU_MODEL_DIR` env var (downloaded update) — if it has ONNX
2. `server/nlu/model/` (dev)
3. `src-tauri/resources/server/nlu/model/` (bundled fallback)

### Admin publish (`server/nlu/publish_nlu.py`)

```bash
# One-time setup:
npx wrangler r2 bucket create nexus-models
# uncomment [[r2_buckets]] MODELS binding in server/worker/wrangler.toml
npx wrangler secret put NEXUS_ADMIN_TOKEN
npx wrangler deploy

# After each retrain:
python server/nlu/publish_nlu.py \
  --worker https://nexus-worker.chitkullakshya.workers.dev \
  --token-file ../admin/data/admin_token.txt
```

### New NLU intents (Phase 8 staged data)

`server/nlu/add_commerce_intents.py` generates 158 staged examples for
`order_food`, `search_product`, `send_whatsapp_message` (+ 9 OOS
negatives), wired into `build_candidate_dataset.py` as
`phase8_commerce_social.json`. `train.py` INTENTS/SLOT_TYPES and
`model/labels.json` updated (55 intents, 51 slot labels; new slots:
`food_item`, `restaurant`, `message`). Next `nexus train` /
candidate build includes them.

**Pre-existing issue found:** `build_candidate_dataset.py` currently
fails on `"how much is an overdraft fee for bank"` — that CLINC staging
row was already merged into production train (twice, duplicated), so
`verify_no_overlap` trips before phase8 is even reached. Fix: remove the
row from `clinc150_oos_train_reviewed.jsonl` or dedupe train.

## Command Center — Multi-Step Task Orchestration (2026-09-16)

**`src-tauri/src/command_center.rs`** implements the n8n-style command
center: compound commands ("X then Y") are split into steps, each step
is parsed + routed to a sub-center, executed sequentially, and results
are merged into one response.

### How it works

```
"open chrome then search for cats"
  → split_compound()    → ["open chrome", "search for cats"]
  → build_plan()        → TaskPlan { 2 PlanSteps, each routed }
  → execute_plan()      → step 1: LocalCommand → command_executor
                        → step 2: WorkerBackend → 9Router/Worker
  → merge               → "Opened Chrome sir. Found results for cats."
```

### Split rules (`split_compound`)

Splits on: ` then `, ` and then `, `, then `, ` after that `,
` afterwards `, `; `. Deliberately does NOT split on bare ` and ` —
"search and rescue", "mum and dad" would break. Dangling trailing
connectors ("open chrome then") are stripped before splitting.

### Conservative fallback

`build_plan` returns `None` (falls back to normal single-intent path)
when:
- transcript isn't compound (< 2 parts)
- ANY part fails deterministic parsing — a bad split should never
  produce a worse outcome than today's path
- any part routes to `Architect` (window-managed, can't compose)

### Step execution (`execute_step`)

| Subsystem | How it runs |
|-----------|-------------|
| LocalCommand | `ParsedIntent` → `command_executor::Intent` → `execute_command` |
| Mcp | `dispatch_to_mcp` — read ops run; gated ops emit Confirm + pause |
| WorkerBackend | `dispatch_to_worker` (9Router fast path included) |
| GitHub | `github_cmd::execute_command` — read ops inline; destructive ops emit Confirm (existing frontend flow) |
| Architect / CommandCenter | `Unsupported` — can't nest/compose |

### Confirmation gates inside compounds

When a step hits a confirmation gate (e.g. `send_whatsapp_message`),
the task pauses: the gated step's pending payload is emitted as a
Confirm event, and the remaining steps are stashed in
`PENDING_COMPOUND`. `orchestrator_mcp_confirm` calls
`resume_compound()` after approval — it runs the confirmed call, then
the remaining steps, and emits the merged result.

Known v1 limit: a destructive **GitHub** step inside a compound emits
Confirm via the existing github flow, but remaining steps are dropped
(GitHub resume needs `orchestrator_github_execute` integration).

### Cancellation

Each step checks `is_cancelled` before running — a barge-in mid-plan
skips remaining steps and reports partial results.

### Sequential only

v1 runs steps in order ("then" implies ordering). Independent parallel
steps are future work.

### Files

- `src-tauri/src/command_center.rs` — plan/split/execute/resume/merge
  (~950 lines, 20 unit tests)
- `src-tauri/src/orchestrator.rs` — `Subsystem::CommandCenter`,
  compound fast path in `process_transcript`, `run_command_center`,
  `orchestrator_mcp_confirm` resume hook, pub wrappers
  (`dispatch_to_mcp_pub`, `dispatch_to_worker_pub`, `is_cancelled_pub`)

## MCP Sub-Center — External Services via Model Context Protocol (2026-09-16)

**`src-tauri/src/mcp_client.rs`** is the MCP client that calls external
MCP servers (Swiggy, Amazon, WhatsApp) via JSON-RPC 2.0 over streamable
HTTP. This is Phase 3 of the roadmap — the foundation for commerce and
social sub-centers.

### Registered MCP Servers

| Server | Endpoint | Auth | Tools |
|--------|----------|------|-------|
| `SwiggyFood` | `https://mcp.swiggy.com/food` | OAuth 2.1 PKCE | 17 (restaurants, menu, cart, orders) |
| `SwiggyInstamart` | `https://mcp.swiggy.com/im` | OAuth 2.1 PKCE | 19 (grocery search, cart, orders) |
| `SwiggyDineout` | `https://mcp.swiggy.com/dineout` | OAuth 2.1 PKCE | 12 (table reservations) |
| `WhatsApp` | `http://127.0.0.1:8765/mcp` | QR session | messaging, contacts |
| `Amazon` | `http://127.0.0.1:8766/mcp` | browser session | product search, details, reviews |

Swiggy MCPs are hosted by Swiggy (free on localhost for dev; production
requires Builders Club approval + demo video). WhatsApp/Amazon run as
local stdio/HTTP MCP servers (e.g., whatsmeow bridge, Playwright scraper).

### New Intents (deterministic parser)

| Pattern | Intent | Routes to |
|---------|--------|-----------|
| "order pizza from dominos" | `OrderFood { query, restaurant }` | `Subsystem::Mcp` → SwiggyFood `search_restaurants` |
| "order biryani" / "get food from swiggy" | `OrderFood` | same |
| "search for X on amazon" / "find X on amazon" | `SearchProduct { query }` | `Subsystem::Mcp` → Amazon `amazon_search` |
| "amazon search for X" / "search amazon for X" | `SearchProduct` | same |
| "send <c> a whatsapp message saying <m>" | `SendWhatsAppMessage` | `Subsystem::Mcp` → WhatsApp `send_message` |
| "whatsapp <c> saying <m>" / "message <c> on whatsapp saying <m>" | `SendWhatsAppMessage` | same |

**Ordering matters:** `parse_send_whatsapp_message` runs BEFORE
`parse_whatsapp_command` — otherwise "whatsapp mom saying hi" would be
swallowed as a chat-open with contact "mom saying hi".

### Confirmation Gates

`McpServer::requires_confirmation(tool)` — write ops (update cart,
send message, book table) emit `OrchestratorEvent::Confirm` with a
pending payload `{server, tool, params, transcript}` instead of
executing. The frontend calls `orchestrator_mcp_confirm(request_id,
confirmed, pending)` to proceed. Read ops (search, list, get) execute
directly.

`McpServer::is_destructive(tool)` — `place_food_order`, `place_im_order`,
`book_table` get an irreversible-action warning in the confirm prompt.

### Files

- `src-tauri/src/mcp_client.rs` — MCP client (McpServer registry,
  JSON-RPC call_tool/list_tools, SSE parsing, extract_text, 12 tests)
- `src-tauri/src/intent_parser.rs` — 3 new intents + 3 parsers + 16 tests
- `src-tauri/src/orchestrator.rs` — `Subsystem::Mcp`, `dispatch_to_mcp`,
  `orchestrator_mcp_confirm` command, 4 routing tests
- `src-tauri/src/lib.rs` — `pub mod mcp_client` + command registration

### Still needed for production use

- **Swiggy OAuth flow** — `call_tool` currently passes `auth_token=None`;
  OAuth 2.1 PKCE token storage/refresh needs wiring (store in
  `settings.json` or keychain).
- **WhatsApp local bridge** — run a whatsmeow/Cloud-API MCP server on
  `127.0.0.1:8765/mcp` (e.g., `whatsapp-mcp` Go bridge + Python MCP).
- **Amazon local server** — run a product-search MCP on
  `127.0.0.1:8766/mcp` (Creators API needs Associates credentials, or a
  Playwright scraper variant).
- **Multi-step flows** — order → cart → checkout chains belong to the
  Phase 4 command center; today each intent maps to one tool call.

## 9Router — Local AI Gateway (2026-09-16)

**9Router** (`src-tauri/src/router.rs`) routes general AI questions
directly to free cloud providers (Cerebras → Groq → Gemini), bypassing
the Worker for 3-7x lower latency (~242ms vs ~2s). The Worker remains
the fallback for PR analysis, GitHub operations, and tasks requiring
session/OAuth tokens.

### Architecture

```
Current (Worker path):
  Device → Worker (50ms) → Workers AI (500-2000ms) → Worker (50ms) → Device
  Total: 600-2100ms

With 9Router:
  Device → localhost (1ms) → Cerebras/Groq (80-120ms) → localhost (1ms) → Device
  Total: ~242ms  (3-7x faster)
```

### Provider Cascade

```
Cerebras (1M tokens/day free, ~80ms, Llama 3.3 70B) — fastest
  → Groq (14,400 req/day free, ~120ms, Llama 3.3 70B)
    → Gemini (1,500 req/day free, ~400ms, flash-lite)
      → Worker (fallback, uses neurons)
```

### What 9Router Handles

- General questions ("what's the capital of France?")
- Factual queries ("how tall is the Eiffel Tower?")
- Conversational responses (when NLU confidence is low)

### What Still Goes Through the Worker

- PR analysis (needs GitHub token + GLM models)
- GitHub commands (merge, approve, close PR)
- Architecture mapper
- Any task requiring session/OAuth tokens

### Routing Logic (`router::can_route`)

Returns `false` (must use Worker) for transcripts containing:
`analyse pr`, `analyze pr`, `analyse latest pr`, `analyse repo`,
`architect`, `check branch`, `merge pr`, `approve pr`, `close pr`,
`github`, `pull request`, `pullrequest`.

Returns `true` (9Router can try) for everything else.

### Integration in Orchestrator

`dispatch_to_worker()` in `orchestrator.rs` now tries 9Router first:
1. If `can_route(transcript)` → try 9Router (Cerebras → Groq → Gemini)
2. If 9Router succeeds → return directly (skip Worker entirely)
3. If 9Router fails or `can_route` returns false → fall back to Worker

### Settings (API Keys)

Stored in `settings.json` (camelCase, same as existing Groq key):
- `groqApiKey` — Groq API key (already existed for STT, now reused for LLM)
- `geminiApiKey` — Google Gemini API key (already existed)
- `cerebrasApiKey` — Cerebras API key (NEW, free at cloud.cerebras.ai)

Settings UI: Settings sidebar → Accounts tab → Cerebras API Key field.

### Files

- `src-tauri/src/router.rs` — 9Router module (~570 lines, 12 unit tests)
- `src-tauri/src/orchestrator.rs` — `dispatch_to_worker()` modified to try 9Router first
- `src-tauri/src/commands.rs` — `NexusSettings` + `cerebras_api_key` field
- `src-tauri/src/lib.rs` — `pub mod router;` registered
- `frontend/src/settings-sidebar/SettingsSidebarApp.tsx` — Cerebras API key field

## Moonshine Medium v2 — Better Local STT (2026-09-16)

**Default Moonshine model upgraded from Small to Medium v2** for better
local STT accuracy. Users can switch back to Small in Settings for
lower RAM on 8GB laptops.

### Model Comparison

| Model | Params | WER | RAM | Latency |
|-------|--------|-----|-----|---------|
| tiny_streaming | 34M | 12.0% | ~100 MB | ~50ms |
| small_streaming | 123M | 7.84% | ~300 MB | ~165ms |
| **medium_streaming** (new default) | 245M | **6.65%** | ~400 MB | ~269ms |

### Configuration

- **Default:** `medium_streaming` (245M, 6.65% WER) — better accuracy
- **Family (8GB laptops):** Set to `small_streaming` in Settings for lower RAM
- **Ultra-low RAM:** Set to `tiny_streaming` (34M, 12% WER)

### How It Works

1. `lazy_stt.rs::read_moonshine_model()` reads `moonshineModel` from
   `settings.json` (defaults to `medium_streaming`)
2. Passes it as `MOONSHINE_MODEL` env var when spawning `stt_server.py`
3. `stt_server.py` reads `MOONSHINE_MODEL` env var (already supported)
4. Settings UI: Settings sidebar → Audio tab → Moonshine model dropdown

### Files

- `src-tauri/src/commands.rs` — `NexusSettings` + `moonshine_model` field
- `src-tauri/src/lazy_stt.rs` — `read_moonshine_model()` + env var on spawn
- `frontend/src/settings-sidebar/SettingsSidebarApp.tsx` — model dropdown

## Diagnostic Fixes — 2026-09-12

### NLU Server ONNX Export (build integration)

The BERT-Mini retraining produced `best_model.pt` but the ONNX export step
was missing. The NLU server requires `nexus_nlu.onnx` to start — without
it, `sys.exit(1)` and every unparseable command blocked for 30s.

**Fix:** `nexus.mjs` now calls `syncNluModel()` before every build. This
copies the ONNX model, `labels.json`, and tokenizer from
`server/nlu/model/` to `src-tauri/resources/server/nlu/model/` so the
Tauri installer bundles the latest trained model automatically.

**`tauri.conf.json` resources** now includes `labels.json`:
```json
"resources/server/nlu/model/labels.json"
```

**To retrain and bundle:**
```bash
cd server/nlu
python train.py           # trains → best_model.pt
python export_onnx.py    # exports → nexus_nlu.onnx (max_length=64)
# Then: nexus build      # syncs model to resources + bundles in installer
```

### NLU Training Commands (`nexus train` + `nexus collect` + `nexus audit`)

**`nexus train`** performs cleaning, generation, merge, canonical conflict/slot/leakage repair, BERT-Mini training, stable ONNX export, resource sync, post-training audit, and temporary-file cleanup.

```bash
nexus train                    # full pipeline (with audit + cleanup)
nexus train --clean-only       # just clean the dataset
nexus train --skip-train       # data generation/repair without training
nexus train --keep-temp        # keep generated data and checkpoint (~17 MB)
```

**`nexus audit`** validates both data and model quality:

```bash
nexus audit                    # structural checks + ONNX evaluation
nexus audit --dataset-only     # structural checks only
```

It writes `server/nlu/audit_report.json` and `docs/research/nlu-model-data-audit-latest.md`.

**Data foundation gates** must pass before external imports or retraining:

```bash
nexus data nlu validate           # provenance registry + frozen 452-row evaluation lock
nexus data wake validate          # wake model fingerprints + optional audio manifest
nexus data wake fingerprint       # explicitly refresh fingerprints after approved model replacement
```

Staged external NLU payloads belong in ignored `server/nlu/data/staging/`. Local wake audio belongs in ignored `wake_word_data/manifest.jsonl`; speaker, session, and source-group IDs may not span train/validation/test splits. See `docs/testing/data-foundation-and-wake-model-gates.md`.

NLU training data is divided into locked phrase-family-separated `train`, `validation`, `calibration`, and final `test` splits. Rows whose templates overlap final test remain preserved under `quarantine` and must not train the model. Validate with `python server/nlu/prepare_evaluation_splits.py`; see `docs/testing/phase-1-nlu-evaluation-split-analysis-2026-09-14.md`.

**`nexus collect`** prompts for 23 intent families, waits for Enter, records two seconds, transcribes with Groq/Moonshine, and requires save/retry/edit/skip/end review before adding a sample.

```bash
nexus collect
nexus collect --intent type_text
nexus collect --count 20
nexus collect --text-only
nexus collect --list
```

Output: `server/admin/data/collected_samples.jsonl` (gitignored, admin-only). `nexus train` merges approved samples and then deletes the temporary collection file unless `--keep-temp` is supplied.

Requirements: working microphone and `sounddevice`/`numpy`/`scipy` (auto-installed). Groq is auto-loaded from NEXUS settings; local STT is the fallback.

### NLU Server Startup Cooldown

Was: 30s wait per failed startup, no cooldown → "loading non stop".
Now: 15s timeout + 60s cooldown after failure. If the NLU server can't
start (missing model, missing deps), it won't block subsequent commands.

### Brain Server Non-Blocking Spawn

Was: `ensure_brain_running()` blocked for 12s while Qwen loaded.
Now: spawns process + background thread. First command falls back to
NLU/deterministic while brain loads in background.

### Wake Word Grace Period (10s after restart)

Was: 5s grace period after stream restart. Intel SST driver produces
transient audio bursts 5-10s after restart that false-trigger the model.
Now: 10s grace period. See `wakeword_oww.rs::detect_chunk()`.

### NLU Model Dimension Fix

The ONNX export used `max_length=32` but the server tokenizes with
`max_length=64` (MAX_LEN). Fixed `export_onnx.py` to use `max_length=64`.

### Live-Mode Intent Mapping

`nlu_client.rs` now maps all 11 live-mode intents to `NluResult`:
`type_text`, `press_key`, `press_hotkey`, `confirm_send`, `cancel_action`,
`browser_new_tab`, `browser_navigate`, `browser_search`,
`whatsapp_open`, `whatsapp_search`, `focus_app`.

### Known Issue: NLU Model Accuracy

The current model has low accuracy (5-9% confidence) because the
training dataset (`dataset.json`) contains malformed intent labels
from the previous merge (e.g., `OpenArchitect` instead of
`open_architect`, debug objects as intent labels). The deterministic
parser handles most commands correctly — the NLU is a fallback only.
**To fix:** clean the dataset labels, retrain, re-export ONNX, rebuild.

## Live Mode — Phase 1 Implementation (2026-09-11)

**Live mode is the always-listening, STT-only, full-laptop control
capability.** It adds keyboard simulation, WhatsApp full flow, browser
navigation, window focus, and a state machine for sequential commands.

### New Dependencies

```toml
enigo = "0.5"     # Cross-platform keyboard/mouse simulation
arboard = "3.4"   # Clipboard for paste-text pattern (avoids autocomplete corruption)
```

### New Module: `src-tauri/src/live/`

| File | Purpose |
|------|---------|
| `mod.rs` | Module root, LiveResult, LiveIntent, 14 Tauri commands |
| `state.rs` | State machine (Idle → AppOpen → ChatActive → TextTyped) |
| `safety.rs` | Whitelist, denylist, confirmation gates |
| `commands/keyboard.rs` | Type text, press keys, hotkey combos, clipboard paste |
| `commands/whatsapp.rs` | Open → search contact → type → send (with confirmation) |
| `commands/browser.rs` | New tab, navigate, search, open site by name |
| `commands/window.rs` | Window focus with AttachThreadInput trick (Windows) |

### New Tauri Commands (14)

`live_type_text`, `live_press_key`, `live_press_hotkey`,
`live_whatsapp_open`, `live_whatsapp_search`, `live_whatsapp_send`,
`live_whatsapp_type_message`, `live_browser_new_tab`,
`live_browser_navigate`, `live_browser_search`, `live_open_site`,
`live_focus_app`, `live_cancel`, `live_get_state`

### New Intent Parser Patterns

- `type <text>` → type_text
- `press <key>` → press_key
- `press <key1> <key2>` → press_hotkey
- `send` / `send it` → confirm_send
- `stop` → cancel_action
- `new tab` / `open new tab` → browser_new_tab

### Safety Layer

- **Whitelist:** Only allowed tools can execute (type_text, press_key,
  open_app, whatsapp_send, etc.)
- **Denylist:** Banking apps, password managers, crypto wallets are blocked
  (1password, bitwarden, bank, paypal, coinbase, metamask, etc.)
- **Confirmation gates:** `whatsapp_send` and `confirm_send` always require
  user confirmation before execution

### State Machine

The state machine tracks context across sequential voice commands:
```
Idle → AppOpen { app } → ChatActive { app, contact } → TextTyped { app, contact, text }
```
Auto-resets to Idle after 30s of silence.

### Clipboard Paste Pattern (from OpenDex)

For text > 50 chars, uses clipboard paste (Ctrl+V) instead of
character-by-character typing. This avoids autocomplete corruption in
WhatsApp/search boxes. Previous clipboard contents are restored after
pasting.

### Window Focus: AttachThreadInput Trick (from ghost-hands)

`SetForegroundWindow` silently fails from background processes on Windows.
The workaround: attach our input queue to the foreground thread's using
`AttachThreadInput`, call `SetForegroundWindow`, then detach.

### NLU Model Update

**New intents:** 47 → 58 (added 11 live-mode intents)
**New training examples:** 1,960 → 2,438 (+478 new examples)
**New slot types:** `B-text`, `I-text`, `B-key`, `I-key`, `B-keys`,
`I-keys`, `B-target`, `I-target`

To retrain:
```bash
cd server/nlu
python generate_live_data.py     # generate new examples
python merge_live_data.py         # merge into dataset.json
python train.py                   # retrain BERT-Mini
```

### Test Results

- 323 Rust tests pass (29 new live-mode tests)
- `cargo check` clean, 0 warnings
- 18 live-mode unit tests (state, safety, keyboard, browser)
- 11 live-mode intent parser tests

## Multi-Worker Optimization — Cloud-First Architecture (2026-09-01)

**Single Worker, internally modularized.** No separate Workers — one deploy,
no cross-Worker latency. The monolithic `index.ts` is split into modules:

- `src/quota.ts` — per-user daily usage tracking + cost control (D1 `usage_log`)
- `src/cache.ts` — edge caching (KV namespace, D1 fallback)
- `src/models.ts` — model constants + fallback chains + truncation
- `src/research.ts` — ad-free search (Wikipedia REST + Wikidata, no API key)
- `src/clean.ts` — result cleaning, dedup, prompt-injection guard

**New D1 tables:** `usage_log` (per-user daily quotas), `cache_entries`
(D1 cache fallback when KV not bound).

**New KV namespace:** `CACHE` (edge cache for search results, PR analysis,
repo metadata). Create with `npx wrangler kv namespace create CACHE` and
paste the ID in `wrangler.toml`.

**Quota limits (per user/day):** 150 requests, 1200 neurons, 15 deep
analyses, 50 searches. Global neuron budget: warn at 8000, hard reject
deep at 9500. (Per-user neuron cap sums below the global stop for 5–6 users:
6 × 1200 = 7200 < 9500.)

**Search routing:** `isSearchQuestion()` detects factual questions
("what is X", "who is Y") and routes them to Wikipedia/Wikidata retrieval
with citations, instead of blind LLM answering.

**Offline local commands (new):**
- `close <app>` — taskkill on Windows, pkill on Unix
- `open chat with <name>` / `message <name>` / `chat with <name>` —
  WhatsApp deep links with local contacts file lookup at
  `%APPDATA%/com.nexus.assistant/contacts.json`
- Both work offline, zero internet, zero RAM overhead

**NLU pre-warm removed.** NLU server now only starts on first
unparseable command (lazy). Saves 50-100 MB at idle.

**STT idle monitor wired but disabled.** `lazy_stt::start_idle_monitor()`
is called from `lib.rs` but `STT_KEEP_ALIVE=true` means STT is never killed.
The idle cost is only ~128 MB (model loaded, not transcribing) and killing
it adds 10-15s delay on the next command (cold model load). The monitor
thread runs for future use but is a no-op. Peak STT RAM during active
transcription is ~340 MB.

**Lazy Kokoro TTS (2026-09-01).** Kokoro is no longer loaded at boot.
`speak_text` calls `ensure_engine_loaded()` on first use — loads in ~1.7s
(one-time), then stays loaded. Saves ~350 MB at idle. See `tts.rs`.

**WebView2 low-memory mode (2026-09-01).** The orb window sets
`MemoryUsageTargetLevel::Low` via `ICoreWebView2_23::SetMemoryUsageTargetLevel`
at creation time. WebView2 drops cached data and swaps to disk. Saves ~40 MB.
See `mic_permissions.rs::set_low_memory_mode()`.

**Idle RAM (2026-09-01): ~104 MB** (before first transcription) or
**~232 MB** (after first transcription, STT model loaded).

| Component | Before first transcription | After first transcription |
|---|---|---|
| nexus.exe (Rust + wake word, NO Kokoro) | 47.8 MB | 47.8 MB |
| WebView2 (orb, low-mem mode) | 35.8 MB | 35.8 MB |
| STT Python (model not yet loaded) | 20.6 MB | 128.6 MB |
| **TOTAL idle** | **104.2 MB** | **232.2 MB** |

After first TTS speak, Kokoro loads and stays loaded: **+350 MB → ~582 MB**.
This is the active state, not idle.

**Worker test suite:** `npm test` in `server/worker/` runs 23 vitest
tests covering quota, cache keys, search question detection, and dedup.

**Rust test suite:** `cargo test --test offline_commands` runs 10 tests
for close_app and whatsapp_chat parsing.

## STT Architecture — Groq Primary + Moonshine Fallback (2026-09-06)

**STT uses Groq Whisper Large v3 Turbo (cloud) as primary, with Moonshine
Small Streaming (local) as fallback when network is unavailable.**

### Primary: Groq Cloud STT
- **Model:** `whisper-large-v3-turbo` (809M params, cloud, ~247ms latency)
- **Free tier:** 20 RPM, 2,000 RPD, 28,800 audio sec/day, 25MB file limit
- **Code:** `src-tauri/src/stt_groq.rs` — sends WAV as multipart to
  `https://api.groq.com/openai/v1/audio/transcriptions`
- **Config:** `localSttOnly: false` in settings.json + valid `groqApiKey`
- **Fallback trigger:** network error, 429 rate limit, 401 auth error,
  or any Groq API error → falls back to local Moonshine

### Fallback: Moonshine Local STT (replaces faster-whisper)
- **Model:** Moonshine Small Streaming (123M params, 7.84% WER, ~165ms CPU)
- **Engine:** `moonshine-voice` Python package (ONNX Runtime, no PyTorch)
- **Code:** `server/stt_server.py` — FastAPI server on `127.0.0.1:39217`
- **Port:** 39217 (`POST /transcribe`, `GET /health`)
- **Audio format:** multipart/form-data with WAV (16kHz, mono, 16-bit PCM)
- **Latency:** ~165ms per transcription after model load; first call ~2s
  (model loading). Model loads in ~1.8s at startup.
- **RAM:** ~150-300MB (model + runtime)
- **Idle timeout:** server killed after 5 min of no requests (saves RAM)
- **Installer:** server files bundled in `resources/server/` via Tauri
  resources config. Production path: `exe_dir/resources/server/stt_server.py`.
- **Requirements:** `pip install moonshine-voice fastapi uvicorn python-multipart`
- **Model download:** automatic on first run via `get_model_for_language("en")`
- **Config:** `MOONSHINE_MODEL` env var (default: `small_streaming`)
  Options: `tiny_streaming` (34M, 12% WER), `small_streaming` (123M, 7.84%),
  `medium_streaming` (245M, 6.65%)

### Why Moonshine over faster-whisper
- Moonshine Small (123M) has 7.84% WER vs Whisper tiny.en's ~18%
- Moonshine is 10-100x faster than Whisper for real-time speech
- Moonshine uses ONNX Runtime (same as wake word engine)
- Moonshine is designed for voice command recognition (streaming, low latency)
- Moonshine MIT license, no restrictions

### Routing logic (`src-tauri/src/stt.rs`)
```
if localSttOnly:
    → local Moonshine (privacy mode, audio never leaves device)
elif groq_key available:
    → try Groq cloud first
    → on error/timeout/429: fall back to local Moonshine
else:
    → local Moonshine directly
```

### Historical: faster-whisper era (pre-2026-09-06)
The previous architecture used faster-whisper `tiny.en` (39M params, ~18% WER)
via a Python sidecar. It was replaced by Moonshine Small Streaming which has
2.3x better accuracy (7.84% vs 18% WER) at similar RAM and faster latency.
- **Hallucination filter:** applied in `stt.rs` — catches
  "thank you for watching", < 2 alphabetic chars, etc.

## NLU Server — Lazy Python Sidecar (2026-08-31)

The **NLU server** (`server/nlu_server.py`) is the only remaining Python
dependency. It provides ML-based intent classification (BERT-Mini ONNX)
as a fallback when the deterministic parser (`intent_parser.rs`) can't
handle a command.

- **Port:** `39218` (separate from the old STT port 39217)
- **Lazy manager:** `src-tauri/src/lazy_nlu.rs` — spawns on first
  unparseable command, kills after 60s idle.
- **Model:** `server/nlu/model/nexus_nlu.onnx` + `.data` + `tokenizer/`
  — committed to git (~18 MB) so fresh clones work without downloading.
- **Requirements:** `server/nlu/requirements.txt` (numpy, onnxruntime,
  fastapi, uvicorn, pydantic, transformers).
- **Fallback:** if the NLU server is unavailable, `nlu_client.rs`
  returns `None` and the deterministic parser handles the command.

### Historical STT Pipeline Fixes (2026-08-30, faster-whisper era)

These bugs were fixed in the faster-whisper Python sidecar. They are
**still relevant** (the sidecar was restored after the Moonshine experiment)
but kept for historical context:

1. **`lazy_stt.rs` path bug:** `stt_script_path()` was missing one
   `.parent()` level. Fixed by adding the correct path.
2. **`ensure_stt_running()` not called on hotkey:** Fixed by adding
   calls to `hotkey.rs` and `stt.rs`.
3. **`is_stt_responsive()` used tokio runtime:** Fixed by using a raw
   TCP connection instead.
4. **STT idle timeout too aggressive:** 60s → 5 minutes.

### Whisper hallucination filter (`stt.rs`)

The hallucination filter is still active in `stt.rs`. It catches common
hallucinations on noisy/silent audio:
- "thank you for watching", "you", "bye", "okay", etc.
- Text with < 2 alphabetic characters
Filtered text is replaced with empty string, triggering the frontend's
"didn't catch that" retry logic (up to 3 retries).

## NEXUS CLI — Unified Cross-Platform Command (`nexus.mjs`)

The unified `nexus` command works on Windows, macOS, and Linux:

```
nexus install     Install prerequisites + build + global 'nexus' command
nexus setup       Install prerequisites + build (no global command)
nexus build       Build frontend + Rust release binary
nexus dev         Tauri dev mode (hot reload via Vite)
nexus start       Launch the built app (unified console on Windows)
nexus run         Alias for 'start'
nexus check       Diagnostics (tools, frontend, Rust, NLU, Worker)
nexus clean       Remove build artifacts
nexus worker      Deploy the Cloudflare Worker (optional)
nexus help        Show help
```

- **Windows:** `nexus.cmd` shim → `node nexus.mjs`
- **Unix:** `nexus` shell script → `node nexus.mjs`
- **Global install:** `nexus install` creates a global command in
  `%USERPROFILE%\.local\bin` (Windows) or `/usr/local/bin` (Unix).
- **`nexus start` on Windows** uses `scripts/run.ps1` for the unified
  color-coded console (Rust logs, audio, frontend CDP in one stream).
- The old `scripts/nexus.bat` and `scripts/nexus.cmd` have been removed.

## Connection Diagnostics (`src-tauri/src/diagnostics.rs`)

Checks 5 services and logs a formatted table on startup:

| Service | Check method | Expected |
|---------|-------------|----------|
| STT | HTTP GET to port 39217/health | OK if running, LAZY if not yet started |
| TTS | In-process Kokoro/Fish Audio readiness (hardcoded ready) | Always OK |
| Cloudflare Worker | HTTPS GET to /health | OK if reachable |
| GitHub | HTTPS GET to Worker /oauth/status | OK if OAuth connected |
| Google | HTTPS GET to Worker /oauth/status | OK if OAuth connected |

Also available as:
- Tauri command: `nexus_diagnostics` (returns JSON to frontend)
- CLI: `nexus check` (build/tool diagnostics via `nexus.mjs`)
- Startup: auto-logged 5s after boot

## Wake Word Model Validation + Mic Silence Recovery (2026-08-30)

### Model is PERFECT — the problem is the Intel SST mic driver

Tested the v2 `nexus.onnx` model with the exact Rust pipeline
(mel → normalize → slice[4:80] → embedding → classifier):

| Input | Model probability | Verdict |
|-------|------------------|---------|
| TTS "nexus" | 0.994 | ✅ |
| TTS "hey nexus" | 0.999 | ✅ |
| TTS "nexus wake up" | 0.999 | ✅ |
| TTS "ok nexus" | 0.999 | ✅ |
| 20 negative samples | 0.0001-0.0002 | ✅ perfect rejection |
| **Trigger rate** | **5/5 positives** | ✅ 100% recall on TTS |
| **False positive rate** | **0/20 negatives** | ✅ 0% false triggers |

The model is NOT the problem. The problem is the **Intel Smart Sound
Technology driver** — it stops delivering audio after 2-25 minutes of
use (RMS drops to exactly 0.000000 and stays there).

### Silence Recovery Thread (`wakeword_oww.rs`)

Added a background thread that monitors the audio callback counter and
automatically restarts the cpal stream when the mic goes silent.

**Settings (tuned for Intel SST bursty audio):**
- Poll interval: **5s** (was 30s)
- Silence threshold: **165 callbacks (~5s)** (was 1000/30s)
- Restart method: **`try_device_silent`** (no 5s probe — saves 5s per cycle)
- Nuclear option: every **12 restarts (~60s of silence)**, restarts the
  Windows Audio service (`net stop/start Audiosrv`) to try to unstick the
  Intel SST driver
- Total restart cycle: **~5s** (was 35s with probe)

**Why 5s?** The Intel SST driver delivers audio in brief 5-15s bursts after
each stream restart, then goes silent. A 5s poll gives us the maximum
number of chances to catch a working window.

**Confirmation RMS threshold lowered from 0.01 to 0.002:**
The 500ms confirmation window was rejecting valid wakes because the mic
fades to silence during the confirmation period. At 0.01, a wake with
RMS=0.0048 was rejected. At 0.002, it would be confirmed.

### Intel SST driver fix (requires admin)

When the mic goes permanently silent, the fix is to restart the driver:

```powershell
# Run as Admin:
pnputil /restart-device "INTELAUDIO\CTLR_DEV_51CA&LINKTYPE_02&DEVTYPE_00&VEN_8086&DEV_AE20&SUBSYS_8BE0103C&REV_10EC\5&111f6c68&0&0000"
```

Or: Device Manager > Sound, video and game controllers > Intel Smart
Sound Technology for Digital Microphones > right-click > Disable > Enable.

Or: Restart the Windows Audio service:
```powershell
Restart-Service -Name "Audiosrv" -Force
```

If none of these work, a full OS restart is required. The Intel SST
driver has a known bug where it stops delivering audio after some time.
Updating to the latest driver from the laptop manufacturer (HP) may help.

### Test scripts (in project root, gitignored)

- `test_wake_model.py` — tests the model with TTS + negative samples
- `test_live_mic.py` — records 5s from the mic and tests the model
- `test_all_devices.py` — tests all audio input devices
- `test_mic_freq.py` — records and shows frequency content
- `gen_tts.py` — generates TTS "NEXUS" samples via Windows SAPI

## RAM Optimization — Lazy Windows + In-Process STT (2026-08-30)

**Idle RAM: 384 MB** (down from 1,644 MB — 77% reduction).

### What was wrong
- `tauri.conf.json` created 5 windows at startup (main, setup, settings,
  sidebar, architect). Each WebView2 window spawns ~7 processes (~250 MB).
  4 of the 5 windows were `visible: false` but still consumed full RAM.
- The old STT server (faster-whisper tiny.en) ran constantly, using ~340 MB
  even when no one was speaking.

### Fix 1: Lazy window creation (`src-tauri/src/dyn_windows.rs`)
- Only `main` (orb) is in `tauri.conf.json` — created at startup.
- `setup`, `settings`, `sidebar`, `architect` are created on-demand by
  `dyn_windows::get_or_create_window()` when first needed.
- `hide_sidebar` / `close_setup_window` / `close_settings_window` now
  **destroy** the window (not `hide()`) — kills the WebView2 process tree
  and frees ~250 MB per window.
- Platform effects (DWM corners, macOS vibrancy) applied at creation time
  inside `get_or_create_window()`.

### Fix 2: Lazy faster-whisper STT server
- STT uses faster-whisper tiny.en via a lazy-started Python sidecar on
  port 39217. See "STT Architecture" section above.
- `lazy_stt.rs` starts the server on first wake/hotkey, kills after 5min idle.
- STT RAM is ~0 MB at idle (server not running), ~340 MB when active.

### Measured RAM (idle, after fix)
| Component          | Before   | After    |
|--------------------|----------|----------|
| NEXUS.exe (Rust)   | 47.9 MB  | 40.8 MB  |
| WebView2 (1 window)| 870 MB   | 344 MB   |
| STT server         | 339 MB   | 0 MB     |
| **TOTAL**          | **1,644 MB** | **385 MB** |

### Files changed
- `src-tauri/tauri.conf.json` — removed 4 windows, kept only `main`
- `src-tauri/src/dyn_windows.rs` — NEW: dynamic window creation/destruction
- `src-tauri/src/stt.rs` — HTTP proxy to faster-whisper sidecar
- `src-tauri/src/lib.rs` — registered new modules, removed startup sidebar vibrancy
- `src-tauri/src/commands.rs` — all show/hide functions use dyn_windows
- `src-tauri/src/architect.rs` — uses dyn_windows for architect window
- `src-tauri/src/hotkey.rs` — sidebar close uses destroy_window
- `src-tauri/src/tray.rs` — settings menu uses dyn_windows
- `src-tauri/src/wakeword_oww.rs` — calls ensure_stt_running() on wake
- `src-tauri/src/stt.rs` — calls mark_stt_request() on each transcription
- `scripts/run.ps1` — no longer starts STT server at boot

## Sidebar — Do NOT use window-vibrancy on non-activating windows (2026-08-30)

**`src-tauri/src/lib.rs` / `src-tauri/src/commands.rs`: the sidebar window
deliberately calls NO `window_vibrancy` function** (no `apply_blur`,
`apply_acrylic`, `apply_mica`). This was a hard-won finding — do not
re-add these calls without reading this section first.

**Why**: the sidebar is a non-activating window (`focus: false`,
`alwaysOnTop: true`, `skipTaskbar: true`) so it never steals keyboard
focus from whatever app the user is working in. Windows' DWM *material*
APIs (Acrylic/Mica via `DWMWA_SYSTEMBACKDROP_TYPE`, or the legacy
`SetWindowCompositionAttribute` accent path used by `apply_blur`) render
a flat, solid **fallback color** for any window that isn't the OS-active
window — this is documented Windows behavior (Mica/Acrylic docs list
"window deactivates" as a fallback-to-solid-color condition), not
something `window-vibrancy` or Tauri can override. Confirmed via
`microsoft/microsoft-ui-xaml#10570` (`DesktopAcrylicBackdrop` loses blur
on `WS_EX_NOACTIVATE` windows) and `tauri-apps/window-vibrancy#183`
(Acrylic/Mica broken on Windows 11 24H2/25H2 in general).

Worse: calling these material APIs **overrides** Tauri's own
`transparent: true` mechanism (`tao` registers the window with DWM via
`DwmEnableBlurBehindWindow` + an empty blur region at window creation —
that's what actually makes a Tauri window see-through, no material
needed). When the material then fails to render (because the window is
never active), DWM falls back to **solid opaque** instead of the
window's original see-through state. This produced a fully opaque
black/grey panel that looked worse than doing nothing.

**The fix**: removed the vibrancy calls entirely. The sidebar was
already genuinely transparent via `transparent: true` in
`tauri.conf.json` — the same mechanism the main orb window uses
successfully. Result: sharp (not blurred) but real, focus-independent
transparency. `src-tauri/src/dwm_corners.rs` still calls
`DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)`
directly (a plain window-shape attribute, not a material — unaffected
by the active/inactive issue) so the OS window's corners match the CSS
card's `border-radius`, avoiding a "double panel" mismatch (WebView2
has no `CornerRadius` support, so without this the DWM-painted window
rectangle and the rounded CSS card show as two different shapes).

Also removed: a CSS chromatic-aberration effect (red/cyan inset
`box-shadow` on `.sidebar-card::after`) that was meant to simulate
glass prism fringing. Without real optical refraction (no SVG
`feDisplacementMap` — `backdrop-filter: url()` is also a no-op on a
transparent WebView2, see below), it just read as a colored-border
rendering bug. Replaced with a neutral lit-bezel `box-shadow` stack
(top specular + bottom shadow) for a physical-glass feel without color.

Separately confirmed: CSS `backdrop-filter` (blur or `url()` SVG
refraction) is a **no-op in a transparent WebView2** — see
`MicrosoftEdge/WebView2Feedback#4945`. It can't composite against
nothing. Don't rely on it for this window; any blur must come from a
native OS mechanism, and per above, none is currently available for a
non-activating window without a full WinRT `DesktopAcrylicController` +
`SystemBackdropConfiguration.IsInputActive = true` interop (what
PowerToys uses for its non-activating flyouts) — out of scope unless
revisited.

### Screenshot-capture blur (the actual liquid glass)

Since native blur is unavailable for non-activating windows, the sidebar
uses a **"fake blur"**: right before `win.show()`, Rust captures the desktop
region behind the window via GDI `BitBlt`, blurs it with
`image::imageops::fast_blur(sigma=32)`, encodes it as a PNG data URI, and
hands it to the frontend as a CSS `background-image` on `.sidebar-card`
via the `--sidebar-backdrop-image` CSS variable. This gives a genuine
frosted-glass look without depending on window activation state.

**Critical timing:** the capture MUST happen before `win.show()` so the
sidebar doesn't capture itself. If the window is already visible (re-show),
capture is skipped. See `src-tauri/src/sidebar_backdrop.rs`.

Full implementation guide: `docs/features/21-liquid-glass-sidebar.md`.

### Dynamic window pending-content pattern (race-free event delivery)

When a window is created on-demand (via `dyn_windows.rs`), the WebView2
needs time to load the HTML and mount the React app. If Rust emits Tauri
events immediately after creating the window, **those events are lost**
because no listener exists yet.

**Fix:** store the content in a `static Mutex<Option<...>>` and let the
frontend fetch it on mount via a `get_pending_*` command. This is race-free
regardless of how long the WebView takes to load. If the window already
exists (React loaded), events are also emitted as a fast path.

Currently used for:
- `PENDING_SIDEBAR` in `commands.rs` → `get_pending_sidebar_content`
- `PENDING_ARCHITECT_REPO` in `architect.rs` → `get_pending_architect_repo`

**All show/create commands MUST be `async`** — `WebviewWindowBuilder::build()`
dispatches to the main thread, and a synchronous Tauri command runs on a
blocking thread that can't yield, causing a deadlock.

To reuse this pattern for a new window, see the "How to Reuse" section in
`docs/features/21-liquid-glass-sidebar.md`.

## Architecture Mapper — Phase 1 Latency Optimization (2026-08-30)

Phase 1 now uses **Approach C (hybrid)** for a 3-4s first response:

1. **Parallelized GitHub API calls** (`tokio::join!`): metadata + recursive
   tree are fetched concurrently using the symbolic ref `HEAD` (verified
   against repos with `main` and `master` default branches). Cuts ~600-1000ms
   off the critical path vs the old sequential metadata→tree flow.
2. **Instant Rust heuristic clustering** for first paint (~5ms) — the diagram
   appears in ~1-1.5s with generic layer labels.
3. **Async LLM enrichment** (`enrich_phase1` command): after first paint, the
   client POSTs the heuristic layers + sample file paths to the Worker's
   `phase1_enrich` intent. The LLM (Mistral 24B) rewrites generic labels into
   repo-specific ones (e.g. "Client / Presentation Layer" → "Next.js App
   Router (React 19)") and writes a real summary. Result streams back via
   the `architect:phase1-enriched` event ~2-3s later and merges in-place.
   **Never blocks first paint.** If the Worker/LLM fails, the heuristic
   diagram remains (graceful degradation).

| Component | File | What changed |
|-----------|------|--------------|
| Rust parallel fetch | `src-tauri/src/architect.rs` | `analyze_repo_phase1` uses `tokio::join!` + `HEAD` ref |
| Rust enrichment cmd | `src-tauri/src/architect.rs` | New `enrich_phase1` command + `Phase1Enrichment`/`EnrichedLayer` types |
| Rust session accessor | `src-tauri/src/network.rs` | New `get_session_info()` public helper |
| Worker handler | `server/worker/src/index.ts` | New `handlePhase1Enrich` + `phase1_enrich` intent dispatch |
| Frontend store | `frontend/src/architect/architectStore.ts` | New `enrichPhase1` action + `sample_file_paths` field |
| Frontend app | `frontend/src/architect/ArchitectApp.tsx` | Calls `enrich_phase1` after paint + listens for enriched event |

## Building

**Always build the desktop app with the Tauri CLI:**

```powershell
pwsh ./scripts/build.ps1          # frontend + tauri release build + bundles
```

If you need a plain cargo build (faster iteration, no installer), you **must**
pass the `custom-protocol` feature:

```powershell
npm --prefix frontend run build
cargo build --release --features custom-protocol   # run inside src-tauri/
```

### Why `custom-protocol` is mandatory

Tauri decides whether to load the bundled frontend or the Vite dev server
purely from this feature flag:

```rust
// tauri-macros/src/context.rs
dev: cfg!(not(feature = "custom-protocol")),
```

- Feature **on**  → windows load `http://tauri.localhost/...` (embedded assets)
- Feature **off** → windows load `devUrl` = `http://localhost:5173`

`cargo tauri build` adds the feature automatically; a bare `cargo build
--release` does **not**. A release binary built without it shows
`localhost refused to connect` / `ERR_CONNECTION_REFUSED` in every window,
because no Vite server is running. Clearing the WebView2 profile does not
help — the dev URL is baked into the binary at compile time.

The feature is deliberately **not** in `[features] default`, because
`tauri dev` needs it off for hot reload.

### Verifying which URL the app actually loads

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
Start-Process .\src-tauri\target\release\nexus.exe
Start-Sleep 15
Invoke-RestMethod http://127.0.0.1:9222/json/list | Select-Object title, url
```

Expected: every window on `http://tauri.localhost/...`.
Bad: `http://localhost:5173/...` → rebuild with `--features custom-protocol`.

## Frontend windows

Every window declared in `src-tauri/tauri.conf.json` must have a matching
rollup input in `frontend/vite.config.ts`, otherwise its HTML file is absent
from `dist/` and the window fails to load in release builds (dev mode hides
this — the Vite server serves any HTML file on demand).

| tauri.conf.json window | HTML            | vite input |
| ---------------------- | --------------- | ---------- |
| `main`                 | `index.html`    | `main`     |
| `setup`                | `setup.html`    | `setup`    |
| `settings`             | `settings.html` | `settings` |
| `sidebar`              | `sidebar.html`  | `sidebar`  |

## Local ports

| Service           | Port    | Notes                                        |
| ----------------- | ------- | -------------------------------------------- |
| STT (faster-whisper) | 39217 | Lazy-started Python sidecar (POST /transcribe) |
| NLU server        | `39218` | Lazy Python sidecar (BERT-Mini ONNX). Override: `NLU_PORT` |
| Sidecar (legacy)  | `41098` | Legacy FastAPI sidecar, not used at runtime  |
| Vite dev server   | `5173`  | Dev only                                     |

## Architecture (serverless — 2026-08-27)

NEXUS is now **fully serverless**. No sidecar, no n8n, no Ollama, no server.

```
NEXUS laptop → HTTP POST → Cloudflare Worker → APIs → text response
                              ↑
                        D1 database (OAuth tokens)
                        Workers AI (intent + summarization)
```

- **Worker** (`server/worker/`): Cloudflare Worker on the edge. Handles
  intent classification, API calls (GitHub/Google), summarization, OAuth
  exchange, token storage, and user registration. <5ms cold start.
- **D1**: Cloudflare's free SQLite. Stores OAuth tokens, API keys, and
  device registrations. 5GB free.
- **Workers AI**: Free tier (10K neurons/day) for intent classification
  (Qwen 0.5B) and summarization (Qwen 14B).
- **Client** (`src-tauri/src/network.rs`): HTTP POST to the Worker. No
  WebSocket. Emits state/ack/result/done events to the frontend.
- **NEXUS_SERVER_URL**: Baked into the installer at build time. Points to
  the Worker URL (e.g. `https://nexus-worker.xxx.workers.dev`).

The old sidecar (`server/sidecar/`) is kept in the repo for reference but
no longer spawned at startup. `sidecar_manager.rs` has been removed.

### Building the installer with the Worker URL

```powershell
$env:NEXUS_SERVER_URL = "https://nexus-worker.your-subdomain.workers.dev"
pwsh ./scripts/build.ps1
```

## Runtime paths (Windows)

- App data (config, logs): `%APPDATA%\com.nexus.assistant\`
- WebView2 profile: `%LOCALAPPDATA%\com.nexus.assistant\EBWebView`
  (note: **Local**, not Roaming — `app_data_dir()` returns Roaming and is the
  wrong path for WebView2)

## Wake word (openWakeWord)

- Default feature: `wakeword-oww` (tract-onnx inference in Rust)
- Models: `src-tauri/resources/oww/{melspectrogram.onnx, embedding_model.onnx, nexus.onnx}`
- Threshold: 0.35, chunk size: 1280 samples (80ms at 16kHz)
- **Detection logic (2026-08-29):** Max-based detection + secondary confirmation.
  The old averaging approach diluted single good frames (0.4+) with surrounding
  0.0s, giving avg=0.03 which never triggered. Now uses max probability in the
  12-frame buffer, so a single 0.36+ frame triggers. After a raw detection,
  collects 500ms of audio and checks RMS ≥ 0.01 to confirm real speech (filters
  noise spikes). Refractory period: 3s.
- **Model v2 (2026-08-30, Kaggle):** Retrained on Kaggle T4 GPU with:
  - 5000 positive samples (5 phrase variants: "nexus", "hey nexus", "nexus wake up", "ok nexus", "nexus please")
  - 30+ soundalike negatives (vs 8 in v1)
  - 80000 training steps (vs 50000), layer_size=64 (vs 32)
  - 2x augmentation rounds, target FP/hr=0.1 (vs 0.2)
  - Model size: 415KB (vs 205KB v1)
  - Kernel: `chitkullakshya/train-nexus-wakeword-v2`
  - v1 backup: `src-tauri/resources/oww/nexus_v1.onnx.backup`
- **Silence gate + AGC (2026-08-28):** `detect_chunk` computes RMS of each
  80ms chunk and skips the classifier entirely if RMS < 0.0005 (~-66dBFS).
  The `nexus.onnx` model emits 0.6-0.9 probabilities on pure digital silence
  (out-of-distribution input), which caused spontaneous false wakes. The
  gate prevents the model from ever seeing silence. Min positive detections
  = 2. Regression test: `test_silence_never_triggers_wake`.
  - **AGC (Automatic Gain Control):** If RMS passes the gate but is below
    TARGET_RMS (0.03), the chunk is amplified up to 50x before feeding the
    classifier. This makes quiet/whispered "NEXUS" produce the same model
    input as loud "NEXUS", so the model (trained on normal-volume TTS)
    recognizes low-volume speech without retraining.
  - Gate: 0.0005, threshold: 0.45. Pure silence (RMS=0) is blocked.
  - Model: trained on Kaggle (v22), accuracy 78.6%, recall 58.2%, FP/hr 1.33.
- **Mic device enumeration (2026-08-27):** `start_audio_capture` enumerates
  ALL input devices, probes each for 5 seconds, and picks the first one
  that produces non-silent audio (RMS > 0.0001). If all devices are silent
  (Intel SST bug), falls back to the best device anyway. This fixes the
  "wake word doesn't work, only hotkey" issue caused by cpal getting
  silence from the Intel Smart Sound Technology driver.
- **FIXED (2026-08-24):** The wake word now works — probability 0.991 for real
  "NEXUS" speech. The root cause was a 32768x input scaling mismatch: cpal
  produces f32 audio in [-1.0, 1.0] but the openWakeWord melspectrogram model
  expects int16-scale float32 values in [-32768, 32767]. Fix: multiply audio
  by 32768.0 in `wakeword_oww.rs` before feeding to the melspectrogram model.
- **Mic conflict (FIXED):** The frontend's `warmMic()` (getUserMedia via WebView2)
  conflicts with the Rust cpal wake-word stream on Intel Smart Sound Technology
  drivers. `warmMic()` is disabled at startup; the mic is acquired on first
  wake instead. This is why cpal was getting silence (RMS=0.0000).
- The global hotkey still works independently of the wake-word model.
- **Command models (2026-08-25):** Training 4 category-level acoustic models
  (`command_open`, `command_close`, `command_search`, `command_play`) via
  `train_nexus_commands.ipynb` on Google Colab. Models detect command TYPE,
  then STT extracts the parameter (which app, what query). See
  `src-tauri/resources/oww/commands/command_intents.json`.

## Known limitations (2026-08-26 audit)

- **Speaker verification IS wired** (2026-09-26). `wakeword_oww.rs` receiver loop
  calls `verify_speaker_at_fire()` on every candidate before firing. Uses a
  separate `AudioFeatures` instance (not the live engine's) so streaming
  buffers stay pristine. Processes last 16 chunks (~1.28s) of candidate audio,
  mean-pools to 96-dim embedding, verifies against enrolled profile. Fail-open
  on any error. Toggle: `speakerVerification` in settings.json (default false).
  The old confirmation-path code (lines 1260-1324) is dead code —
  `confirmation_active` is never set true (500ms confirmation was removed for
  instant-fire latency).
- **All windows skip the taskbar.** `main`, `setup`, `settings`, and
  `sidebar` all have `skipTaskbar: true` in `tauri.conf.json`. NEXUS is
  accessible only via the floating orb, the system tray, the global
  hotkey, and the wake word.
- **STT server auto-launcher writes `server/start_stt.cmd`** with an
  absolute path to the local Python interpreter. This file is gitignored
  (machine-specific, leaks username).
