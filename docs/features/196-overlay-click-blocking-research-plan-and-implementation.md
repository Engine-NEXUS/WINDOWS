# 196 — Overlay click blocking: research, plan and implementation (with the "analyse my screen" wave and the local setup)

Date: 2026-11-22. Repository of the code: `Engine-NEXUS/WINDOWS`, branch `fix/overlay-click-blocking-and-analyse-wave`.
Short change record: [`docs/changes/205-overlay-click-blocking-and-analyse-wave.md`](../changes/205-overlay-click-blocking-and-analyse-wave.md).
Wave design record written before the build: [`195-screen-analysis-wave-animation-plan.md`](195-screen-analysis-wave-animation-plan.md).

This file is the long form: what was asked, how the system works, every measurement and hypothesis, the plan, exactly what was
built and why, how each piece was tested, what could not be tested, and what is still open. It is written so that someone who
was not in the session can repeat the investigation or change the code later.

---

## 1. What was asked, in order

1. Pull the newest GitHub changes, read the docs to understand the project and its working state, and set NEXUS up locally so
   the owner can build and test it.
2. Placement rule for the installs: laptop-integration software on the system drive (C:), project packages on the project
   drive (A:), and NEXUS itself on the system drive for testing (it is an assistant that lives on the computer).
3. Add the owner's Groq key and Gemini key to NEXUS, and make sure a user has somewhere to enter keys in the starting setup.
4. Check why the wake word did not wake NEXUS and whether a permission was missing.
5. Design a screen-analysis animation: when the user says "analyse my screen", waves leave the orb once and cover the screen.
   Plan it as a UI/UX engineer, run it on localhost, open it in Brave for review before applying it to NEXUS.
6. Wire the approved design ("Rings", the owner's settings) into NEXUS.
7. The main subject of this file: **NEXUS blocked clicking and copying in Brave.** The owner: the full-screen WebView covers the
   whole screen. Find every cause, plan, implement in sections, test, test again, rebuild, and push to GitHub once it is right.
   "I don't want this to repeat."

Result at the end of the session: the owner tested the rebuilt app in Brave and reported **all clear**.

---

## 2. How the NEXUS screen works (research)

### 2.1 The windows NEXUS puts on the screen

All windows are created on demand by `dyn_windows.rs` (`WindowConfig`). Every one is frameless, transparent and always on top.

| Label | Size (logical) | Purpose | Click behaviour |
|---|---|---|---|
| `stage` | 1920x1080 configured, then fitted to the primary monitor on every show (2880x1800 physical on the owner's laptop, scale 2) | The always-on-screen overlay: orb, captions, loading spinner, tour callouts, annotations, hand overlay, the new wave | Full screen. Click-through except over registered rectangles (section 2.2) |
| `agent-island` | 760x430 configured, sized by `agent_island.rs` (570x323 measured live) | Pill of running coding agents' logos at the top centre | Own cursor loop; flips only `WS_EX_TRANSPARENT`; non-activating (`agent_island.rs`, with tests) |
| `notify-toast` | 520x520 configured, 320 wide, 352 high at most when shown | Alerts | Shown without focus, hard cap 60 s, then destroyed |
| `sidebar`, `architect-map`, `maps-panel`, `place-card` | by view (420 px wide at the right edge for the response sidebar) | Panels | Real windows; they are meant to take clicks while open |
| `settings`, `setup` | about 400-640 wide, centred | Hub / wizard | Real windows |
| `calibrate-toolbar` | 540x92 | Animation calibration HUD | Interactive during calibration |
| `hand` | 8x8, opaque, invisible | Hand-mode camera page | Click-through |
| early orb (`early_orb.rs`) | native layered windows | Boot greeting picture | `WS_EX_TRANSPARENT | NOACTIVATE | TOOLWINDOW`, destroyed after the clip |

Only the `stage` is both full-screen and shared by many features, so it is where a blocking bug can hide.

### 2.2 How the stage lets clicks through

- Windows has no per-pixel hit testing for a transparent WebView2 window, so NEXUS uses the documented workaround: the whole
  window is **click-through** (`WS_EX_TRANSPARENT`), and a background thread (`stage-hitbox`, `stage.rs`) polls the cursor and
  turns click-through **off** only while the cursor is inside a registered rectangle ("hitbox"), so the page can receive
  the click there.
- Rectangles are in **physical pixels**, kept per named source in one table, so independent features cannot clobber each other.
  Sources before this work: `orb` (Rust, `window_manager.rs`), `loading` and `orb` during calibration (`calibration.rs`),
  `tour`, `spatial` and `annotation` (frontend, via the `stage_set_hitboxes` command), and `live-glass`
  (`register_glass_hitboxes`, no frontend caller found).
- The loop sleeps 250 ms when there are no rectangles, 100 ms when the cursor is far from every rectangle, 30 ms in a Ghost
  session, and 16 ms when the cursor is inside or within 120 px of one.
- The stage is also capture-excluded (`SetWindowDisplayAffinity`) only for the moment of a screenshot, so NEXUS never
  photographs itself.

### 2.3 Why a bug here is serious

Whenever the table holds a rectangle the cursor is inside, the **entire full-screen window** becomes interactive, not just the
rectangle's pixels, because tao/WebView2 toggle the whole window. So one stale rectangle in the wrong place blocks the user's own
application underneath it, and it keeps blocking until the table is cleared, which before this work could mean until NEXUS
restarted.

---

## 3. Evidence gathered

### 3.1 Live measurement of the running app (idle)

A small Win32 probe (PowerShell `Add-Type`, read-only; it did not move the mouse) listed the NEXUS windows and their extended
style bits, and asked Windows which window owns the pixel under several screen points.

Before the fixes, `NEXUS Stage`:

- 1440x900 in the probe's DPI-virtualised coordinates (the real window is 2880x1800 at scale 2), topmost, visible.
- `ex = 0x000C0138` = `LAYERED | APPWINDOW | WINDOWEDGE | TRANSPARENT | ACCEPTFILES | TOPMOST`.
- `TRANSPARENT` set: click-through at idle. The probe points landed on Brave, not on NEXUS. So the overlay is **not**
  permanently blocking; the block happens only when a rectangle is active.
- `NOACTIVATE` **not** set and `APPWINDOW` set: the overlay can take keyboard focus when clicked, and it appears in Alt+Tab.
- The "NEXUS Agents" window (agent island) was visible, 570x323 logical, topmost, `NOACTIVATE`, click-through at idle.

After the fixes (release build `8f1576f5a63f13af`): `ex = 0x080801B8` = `NOACTIVATE | LAYERED | WINDOWEDGE | TOOLWINDOW |
TRANSPARENT | ACCEPTFILES | TOPMOST`, `APPWINDOW` cleared.

### 3.2 What the code showed

- `window_manager::show_orb_interactive` (called on **every wake**, `wake_orb_src`) registered `orb_rect()` as an interactive
  hitbox. `orb_rect` is a square of the configured orb size: 200 logical px, which is **400x400 physical** on the owner's 200 %
  screen, at the top centre. The visible capsule is only 301x105.
- The only code that cleared it was the frontend effect in `OrbFrame.tsx`: when the assistant `state` changed to `idle`, it
  called `set_orb_interactive(false)`. The effect depends on `[state]`, so a wake that never changes the store state, or a turn
  that ends in a way that skips the reset, leaves the rectangle registered. The project's own history lists this class
  ("stuck orb", turn-end gate) many times.
- The orb has **no click handler**. Its `pointerEvents` are used only by the calibration drag/wheel code. So the interactive
  rectangle had no purpose outside calibration.
- The owner's log from the wake tests shows several wakes that ended with `stt-capture: aborted (had_speech=false)`, exactly the
  turns that do not run the normal reset path.
- On the owner's laptop the square covers Brave's tab strip and address bar, which matches "I can't click or copy in Brave".

### 3.3 Other observations from the same investigation

- The stage was a normal activatable window. A click on any of its rectangles activated NEXUS, so the next Ctrl+C or Ctrl+V
  went to the overlay, not to the user's application. This explains "copy" failing even for a short block.
- tao's `set_ignore_cursor_events` rebuilds the **whole** extended style from tao's own flags. The Agent Island code documents
  that it drops `TOOLWINDOW` and `NOACTIVATE` (`agent_island.rs`, "Do NOT use `set_ignore_cursor_events` here"). The stage loop
  used it on every change, so even if NOACTIVATE had been set once it would not have survived.
- The hitbox table had no lifetime and no size check. A rectangle of the wrong scale (the owner's screen is 200 %) or a stale one
  would block until restart.
- The loop thread had no panic protection. A panic there would leave the stage in whatever state it was in, possibly interactive.

---

## 4. Hypotheses, ranked, and what became of each

| # | Hypothesis | Verdict | Action |
|---|---|---|---|
| 1 | The orb rectangle stays registered after a wake (stale hitbox) | **Most likely cause.** Confirmed in the code path; the exact moment of the owner's block was not captured because no log line existed | Orb is a hitbox only during calibration; Rust refuses it otherwise; frontend asks only while calibrating |
| 2 | The overlay steals keyboard focus, so copy goes to NEXUS | **Confirmed by measurement** (`NOACTIVATE` was not set) | Stage made non-activating; Alt+Tab entry removed |
| 3 | A rectangle with the wrong scale or an oversized one | Not observed; plausible at 200 % scaling | Size cap (35 % of the screen), clamp to screen, refuse empty/off-screen |
| 4 | Any source that is never cleared blocks until restart | Real design gap | Expiry (60 s, 180 s for tour/spatial/annotation); calibration pinned |
| 5 | The hitbox loop panics and leaves the window interactive | Possible, never seen | `catch_unwind`, force click-through, restart state |
| 6 | Toggle lag: up to 100 ms before click-through reacts when the cursor is far | Real but minor (a quick click right after moving can land in the old state) | Not changed; the loop already drops to 16 ms near a rectangle |
| 7 | The Agent Island window (570x323 at the top) blocks the top centre | Checked: it flips only `WS_EX_TRANSPARENT`, only while the cursor is on the drawn pill, and has tests | Not changed |
| 8 | The stage is briefly interactive when it is recreated after a blackout | Possible (seen once in the log) | The style is now re-applied right after show, and re-checked on every loop pass |
| 9 | The new wave canvas blocks clicks | Cannot: the OS hit test is decided in Rust; the canvas is `pointer-events: none` and hidden when idle | None needed |
| 10 | `NOACTIVATE` breaks keyboard handling inside the stage (the Esc listeners in `main.tsx` and `TourOverlay.tsx`) | Real side effect | Accepted and documented: the callout's close button and the global Esc still work |

Honest limit: the owner's block was not reproduced by the tool (it needs the cursor to be inside a rectangle). The conclusion
rests on the code reading (#1, #2), the measurement of the window style (#2), and the owner's all-clear after the fixes. The new
`[HITBOX]` log lines exist so that the next block names its own cause.

---

## 5. The plan

Principle: do not fix one rectangle, fix the **class** "something registered an interactive area and nothing removes it, and
clicking it steals focus".

| Section | Goal |
|---|---|
| A | Remove the cause: the orb is not interactive outside calibration |
| B | Make every stale area impossible to keep: expiry, size cap, clamping |
| C | Stop the overlay taking focus and stop tao undoing it |
| D | Make the loop safe against its own crash |
| E | Make the next report self-explaining: logging |
| F | Tests for each of the above, mutation checks, repo gates |
| G | Rebuild, deploy to the system drive, live check, owner retest |
| H | Commit on a branch, push, write the records |

Constraints followed: no `#[allow(dead_code)]`, no new unmatched IPC events, the opacity/colour/turn-end/hygiene gates stay
green, the app is edited only with the owner's runtime untouched while a build is in progress (the build tool refuses a source
change during a build).

---

## 6. The implementation

### 6.1 Section A: the orb is only a hitbox during calibration

- `window_manager.rs`
  - `show_orb_interactive` now only emits `stage:orb_rect`, `stage:orb_position` and `stage:orb_visible`. It no longer calls
    `set_orb_hitbox_interactive`. The doc comment records why.
  - New pure `orb_hitbox_allowed(calibrating: bool) -> bool`. `set_orb_interactive(interactive)` registers the orb rect only if
    `interactive && orb_hitbox_allowed(calibration::is_active())`; otherwise it clears the `orb` source. The calibration
    registration uses `set_hitbox_source_pinned`.
- `OrbFrame.tsx`: the old effect (`state === "idle"` decides) is replaced by
  `const calibrating = useAssistant((s) => !!s.calibrationTarget);` and an effect keyed on `[calibrating]` that sends
  `set_orb_interactive({ interactive: calibrating })`.
- `calibration.rs`: its two registrations (`orb`, `loading`) use `set_hitbox_source_pinned`, so a long calibration session does not
  lose its drag targets.

### 6.2 Section B: the hitbox book (`stage.rs`, `HitBook`)

The old global was a `HashMap<String, Vec<StageRect>>`. It is now a `HitBook`:

- `entries: source -> { rects, at, pinned }`, and `screen: Option<(w, h)>`, set from `fit_to_primary_monitor` (physical size of
  the primary monitor, where the stage lives at (0, 0)).
- `set(source, rects, pinned, now)`: each rect is sanitised, and an empty result removes the source. Sanitising: refuse width or
  height <= 0; clamp to `[0, screen]`; refuse if it ends up empty; refuse if it covers more than `MAX_RECT_SCREEN_SHARE`
  (0.35) of the screen unless `pinned`. Returns how many rects were refused, and `set_hitbox_source_with` logs
  `[HITBOX] '<source>': refused N rect(s) ...`.
- `live(now)`: drops every unpinned source older than its lifetime and returns the live rects plus the names and ages that
  expired. Lifetimes (`hit_ttl`): `tour`, `spatial`, `annotation` = 180 s (a callout or pins can stay up while the user reads);
  everything else = 60 s.
- `signature()`: one sorted line per source (`name[WxH@x,y ...]`), used to log the rectangle set only when it changes.
- Public API: `set_hitbox_source` (unpinned) and `set_hitbox_source_pinned` (calibration). Callers did not change shape.

### 6.3 Section C: non-activating, no Alt+Tab, click-through by direct style bits

- `stage_ex_style(ex, click_through)` (pure): `ex | TOOLWINDOW | NOACTIVATE`, minus `APPWINDOW`, plus or minus `TRANSPARENT`.
  Every other bit, including `LAYERED` and `TOPMOST`, is left exactly as it was.
- `apply_click_through(win, through)`: on Windows reads the extended style with `GetWindowLongPtrW`, computes
  `stage_ex_style`, writes only if different (`SetWindowLongPtrW`). Because it reads first, **every call also repairs** any other
  code that changed the bits. Non-Windows falls back to tao.
- `ensure_stage` and `stage_show_sync` still make the initial tao `set_ignore_cursor_events(true)` call, then call
  `apply_click_through(&win, true)` straight after, because tao just rebuilt the style from its own flags.
- The loop no longer calls tao's `set_ignore_cursor_events` at all (a test enforces it, section 7).

### 6.4 Section D: the loop cannot strand the screen

- `spawn_hitbox_loop` runs `hit_tick(&app, &mut state)` inside `catch_unwind(AssertUnwindSafe(..))`. On a panic it logs
  `[HITBOX] the hitbox loop panicked; forcing the stage click-through and carrying on`, calls `apply_click_through(win, true)`,
  resets its state and sleeps 500 ms.
- `hit_tick` returns the sleep time: 500 ms when hidden or disabled, 250 ms with no rectangles (and the style is re-asserted
  every pass), 16 / 30 / 100 ms as before.

### 6.5 Section E: logging

- `[HITBOX] interactive rects now: ...` when the set of sources or rectangles changes (and "none" when it empties).
- `[HITBOX] '<source>' expired after Ns without a refresh (a stale hitbox would have blocked that part of the screen)`.
- `[HITBOX] '<source>': refused N rect(s) ...` for the size/empty/off-screen rules.
- `[HITBOX] the stage has been interactive for 20s (cursor at ..); rects: ...` once per stretch, so a stuck rectangle names itself.
- In the owner's all-clear run the log contained **no** `[HITBOX]` line: no wake registered any rectangle, which is the intended
  behaviour.

### 6.6 What was deliberately not changed

- The poll intervals (toggle lag, #6) and the Agent Island (#7).
- The tour/annotation/spatial rectangles themselves: they are real interactive controls and now simply expire if abandoned.
- `register_glass_hitboxes` / `live_glass` (no frontend caller): left alone.

---

## 7. Testing

### 7.1 Frontend

- `orbHitbox.test.ts` (new, source-scan like the Agent Island tests): `set_orb_interactive` appears once in `OrbFrame.tsx`, is
  sent as `interactive: calibrating`, the `calibrating` selector exists, and the old "not idle means interactive" pattern is gone.
- Whole suite after all changes: `tsc` clean, **vitest 1241/1241** across 118 files (the earlier baseline in this session was 1230;
  the 11 new tests are the 9 wave tests and the 2 orb-hitbox tests).

### 7.2 Rust

- 13 tests were written in `stage.rs` (`hitbook_tests`): expiry, refresh restarts the clock, long window for the tour source,
  pinned sources, oversize refusal, empty/off-screen refusal and clamping, a refused update clears the old rectangles, sources
  do not clobber each other, behaviour without a known screen, the style bits (`NOACTIVATE`, `TOOLWINDOW`, no `APPWINDOW`,
  `TRANSPARENT` on demand, `LAYERED`/`TOPMOST` untouched, idempotent), source scans (the loop never uses tao's call; waking the
  orb registers nothing), and `orb_hitbox_allowed`.
- **The full `cargo test` build could not be used.** In the test profile on this machine `ort-sys` fails with "could not find
  native static library `onnxruntime`", identically with and without the project's environment variables (the normal release
  build is unaffected). So the logic was tested by extracting `StageRect`, `HitBook`, the TTL constants and `stage_ex_style`,
  with the tests, from the real `stage.rs` into a scratch crate and running that: **12 tests pass** (the source-scan tests read
  the real files). The `orb_hitbox_allowed` test needs the real `window_manager` module and therefore did not run.
- Mutation checks on the extracted code (each must make a test fail): expiry removed, size cap removed, `NOACTIVATE` dropped,
  pinned sources allowed to expire. **All four were killed.**

### 7.3 Repository gates (all green after the change)

IPC contract (272 commands, 213 invoked, no new one-sided events), window-opacity registry, hard-coded colours (none new),
turn-end gate, repo hygiene, feature registry.

### 7.4 Live

- Release build `8f1576f5a63f13af`, deployed to `C:\NEXUS-Dev`, hash equal to the build output.
- Stage window style read back (section 3.1). No panic, no error, no `[HITBOX]` line in the startup or test log.
- The owner exercised Brave after wakes with and without a command: **all clear**.

### 7.5 Not tested

Other applications than Brave; a second monitor (the stage covers the primary monitor only); a real stuck rectangle (none could be
produced after the fix, so the expiry path is covered by unit tests only); 125 % / 150 % scaling.

---

## 8. The "analyse my screen" wave

### 8.1 Design (UI/UX plan, reviewed by the owner in Brave)

A prototype page, `frontend/dev-analyse-wave.html`, served by `npm run dev` only (like the other `dev-*` pages), let the owner
compare three styles on light, dark and busy backdrops, with the orb docked top or bottom, duration, ring count, strength, a
slow-motion speed and a reduced-motion switch. Styles: **Rings** (chosen), **Scan** (a dot grid revealed behind the front),
**Rings + edge** (rings, then a breathing screen-edge glow while the model thinks).

The owner's final settings (read from their screenshot): Rings, one ring, 1400 ms, strength 80, speed **0.25x** (I first
mistook the speed for review slow-motion; the owner then said it was not in slow speed, so the shipped duration is
1400 / 0.25 = **5600 ms**).

Rules: black, white and grey only; the ring has a white core and a dark rim so it reads on light and dark screens; additive
overlay only (the stage is transparent over the real desktop, so it cannot distort it); click-through; one fullscreen canvas
only while it plays; Esc / "stop" / end of the tour clears it; reduced motion = one soft wash instead of a travelling ring.

### 8.2 Timing finding that changed the wiring

The first wiring fired the event right after the screenshot. The log showed: command heard 10:57:45, acknowledgement 10:57:47,
**wave 10:57:54**. The 7 s in between is context gathering (OCR, page metadata, foreground checks) before the capture. So the
ring would have appeared 9 s after the user spoke. It now fires at the acknowledgement (the same moment as the spinner,
`run_screen_tour` step 1), only when speech is allowed (not in a meeting, where the overlay would be visible to the call). The
screenshot is unaffected because the stage is capture-excluded for the capture itself.

### 8.3 Code

- `analyseWaveModel.ts`: pure timing and geometry. `DEFAULT_WAVE = { rings: 1, durationMs: 5600, leadMs: 80, staggerMs: 110,
  amp: 0.8 }`; `waveFrame(elapsed, origin, size, reduced, params)` returns the rings (radius with easeOutCubic to the farthest
  corner + 4 %, thickness 20 -> 7 px, alpha `(1-u)^1.35 * amp * (1 - 0.2 i)`), the reduced-motion wash (`sin(u*pi) * 0.16`), and
  `done`.
- `AnalyseWave.tsx`: one canvas, listens for `screen:analyse_wave` (start), `orb:cancel` and `screen:tour_end` (stop) and
  `stage:orb_rect` (origin = centre of the orb rect, physical px / devicePixelRatio; top centre until the rect is known). The
  animation loop runs only while playing, then clears and hides the canvas. `analyse-wave.css`: layout only, `z-index: 9990`
  (below the capsule and the tour layers), hidden unless `--on`.
- `main.tsx` mounts `<AnalyseWave />` before `<TourOverlay />`; `orchestrator.rs` emits `screen:analyse_wave` in
  `run_screen_tour`.
- Tests: `analyseWaveModel.test.ts`, 9 tests (shipped settings; rings launch in order; the front grows, thins and fades and later
  rings are fainter; it reaches the far corner; done exactly at the end; bottom dock; reduced motion; easing).

### 8.4 Live result

Log of the owner's test: "Analyse my screen." heard in 483 ms, routed to the screen analysis, `screen:analyse_wave` emitted,
Gemini (`gemini-3.5-flash-lite`) answered, three items in the tour script. The owner saw the ring (late at first, which led to
section 8.2) and later confirmed everything was clear.

---

## 9. The other fixes in this branch

- **Cloud voice default.** A fresh profile saved the Kokoro voice id `af_sky` as the cloud voice (`ttsVoice` default in
  `commands.rs`, read back by the setup wizard as the selected voice). Edge rejects it, so every reply fell back to the local
  voice. Fix: the wizard accepts only a cloud voice id (ending in `Neural`) and reads `edgeTtsVoice` first; the built-in default
  is `en-US-AvaNeural`.
- **Keys in the starting setup.** The "Free AI Keys (Optional)" card already existed but sat below the volume slider in step 2.
  It is now the first block of step 2, and the step is called "Keys & Preferences".
- **Local speech server not found.** `lazy_stt::stt_script_path` climbed four parent folders with `?`, which returned `None` for
  the whole function when the app lives in a shallow folder such as `C:\NEXUS-Dev`, before the production path
  (`<exe dir>/resources/server/stt_server.py`) was tried. Each candidate is now built independently.

---

## 10. Local setup and deployment record (for repeating it)

### 10.1 Layout (owner's rule)

- System drive C: (software that integrates with Windows): Node 24, Python 3.12, Rust via rustup, WebView2, Git (already
  present); Visual Studio Build Tools 2022 with the C++ workload and Windows SDK, and LLVM 23.1.6 (installed during the session).
- Project drive A:: `.venv`, `frontend/node_modules`, and the Rust build output `src-tauri/target-contributor`. The build tool
  fixes the output under the repository and refuses a symlinked output folder.
- NEXUS itself for testing: copied to `C:\NEXUS-Dev` (`nexus.exe`, `nexus-build.json`, `resources/`). Compiling stays on A: so
  the 10-20 GB of intermediate files stay off C:.
- Runtime data (settings, memory, logs, credential store) is under `%APPDATA%\com.nexus.assistant.contributor` and the Windows
  credential store, on C:, separate from the owner's personal NEXUS data.

### 10.2 Commands used

```text
node nexus.mjs doctor
node nexus.mjs build                       # also creates .venv and installs frontend packages on first run
copy src-tauri\target-contributor\release\nexus.exe, nexus-build.json and resources\ to C:\NEXUS-Dev
set NEXUS_MANAGED_PYTHON=A:\NEXUS\NEXUS\.venv\Scripts\python.exe, NEXUS_PROFILE=contributor, RUST_LOG=info,nexus=debug
C:\NEXUS-Dev\nexus.exe          (log to C:\NEXUS-Dev\logs)
```

Build times seen: 33 min first (cold), then 18, 10, 9, 9 min for incremental builds.

### 10.3 Pitfalls met

- **Another script was installing the same tools.** `C:\Users\Lakshya\Backup-Tools\install-dev-tools.ps1` was running `winget`
  installs (including Build Tools without the C++ workload) at the same time; the first Build Tools attempts exited at once
  (installer error 87, then 5007) until it had finished. Do not run two installers together.
- **`vs_installer.exe modify ... --wait`** is rejected (error 87); use `setup.exe modify ... --quiet --norestart` elevated, without `--wait`.
- **A running Vite dev server holds `esbuild.exe`**, so `npm ci` (the first step of `build`) fails with EPERM. Stop the preview
  server before building.
- **The build tool refuses to record a build if source changed during it** ("source changed during build; rebuild after edits
  finish"). Finish edits, then build once.
- **`cargo test` in the test profile fails on `ort-sys`** (section 7.2).
- **Settings file and keys.** The app moves API keys from `settings.json` into the Windows credential store and leaves the
  field empty. After one settings rewrite by hand (with the key field empty) a later start showed "Groq missing" and the log
  showed a credential-store delete. The key was re-added and then survived a plain restart. The exact trigger was not proven;
  when editing `settings.json` by hand, stop the app first and expect to re-enter a key if it disappears. (Key values are
  deliberately not recorded in any file.)
- **Wake word and the verification step.** The wake model scored the owner's "Nexus" between 0.71 and 0.99 against a 0.68
  threshold (permissions were fine). The second check, a short speech transcription, failed when the network dropped and the
  offline fallback could not be found (section 9), so the wake was suppressed. With Groq reachable it passed ("Nexus.").
- **Agent hooks.** At startup NEXUS connects the coding agents it finds. On this machine it added a `notify` line to
  `C:\Users\Lakshya\.codex\config.toml` (backup next to it) and an OpenCode plugin file. This is by design and the owner chose to
  keep it.
- **Pull with local edits.** A fast-forward pull refuses when local edits touch files the pull changes; stash the tracked edits,
  pull, pop. Two untracked docs that now exist on GitHub were moved (not deleted) to
  `A:\NEXUS\local-backup-before-pull-20261122`.

---

## 11. Open items

1. Run the whole Rust suite once the `ort-sys` test-profile failure is understood (likely a static-library search path for the
   test profile). Until then the new stage tests run only through the extraction harness.
2. Test the overlay with other applications, 125 % / 150 % scaling and a second monitor.
3. Decide whether the stage should also get keyboard Esc back for callouts, now that it is non-activating.
4. The offline Moonshine speech server does not become ready within 15 s on a cold start, so the offline fallback of the wake
   check is slow (the Groq path is fine).
5. The wave plays only on the vision tour path, not on the read/scan (OCR) path. Decide if it should.
6. If a block is ever reported again, ask for the log lines containing `[HITBOX]` first: they list the active rectangles, expiries,
   refusals and any 20-second interactive stretch.
