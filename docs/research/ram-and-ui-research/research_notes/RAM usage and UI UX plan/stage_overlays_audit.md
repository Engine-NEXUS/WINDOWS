# NEXUS always-on visual layer: stage overlay, orb, captions, spinner, task chip, tour, hand overlay, notify expand and native toast

Method and limits. Code read in full for the files named below; rectangles computed by hand from the code; contrast ratios computed with the WCAG relative-luminance formula in PowerShell. The owner's machine was inspected read-only (settings.json, cdp_unified.log, screen metrics). The Vite dev server was started and stopped (port 5173 confirmed free afterwards). Only `caption-showcase.html` and `dev-annotations.html` were rendered (the latter in light colour scheme); the real stage components cannot render outside Tauri, so no screenshot of the production stage exists. NEXUS was not started, stopped or rebuilt. Tags: [C] confirmed from code, data or computation; [I] inferred, needs a live check.

Owner's actual config (read from `%APPDATA%\com.nexus.assistant\settings.json`, log `cdp_unified.log`): display 1920x1080 at 100 %, taskbar auto-hidden (work area equals bounds); stage mounted at 1920x1080 DPR 1; `orbPosition` top, `orbSize` 200, `orbCapsuleStyle` pebble, `themeMode` dark, loading 0.95/0.05/80. [C]

---

## 1. Where does each element sit, and can two collide (125/150 % DPI, multi-monitor)?

### Takeaway
At the owner's 1920x1080/100 % setup nothing collides, but only because three accidents line up: the stage window is hard-coded to 1920x1080 logical, the taskbar auto-hides, and the pebble is short. On any other scale or resolution the stage does not match the monitor (no code resizes it), and the hand chip collides with the caption slot on a visible taskbar.

### Cited Findings
Geometry sources
- Orb rect: `window_manager.rs:77-106` read_orb_settings defaults (0.5, 0.0 top / 1.0 bottom, 200 px, clamp 180-400); `overlay_xy` `window_manager.rs:155-171` centre-anchors and clamps; the rect is sent in physical px and divided by DPR in `OrbFrame.tsx:696-753`. Classic capsule = 200x200 CSS px square, flush to top edge. [C]
- Pebble (owner's style) ignores the Rust rect: hard-coded 301x105, centred on `window.innerWidth`, top 0 (`OrbFrame.tsx:704-722`, `PebbleScoopFrame` W/H `OrbFrame.tsx:90-91`); orb inside is a fixed 100x100 cradle (`styles.css:132-141`). [C]
- Default capsule style disagrees: Rust `default_orb_capsule_style()` = "classic" (`commands.rs:2148`), frontend store falls back to "pebble" (`store/assistant.ts:269`). [C]
- Loading spinner: default (0.95, 0.05, 80) (`window_manager.rs:133-150`) so box = x(0.95W-40), y(0.05H-40) clamped; the Lottie inside is a fixed 60x60 (`ghost.css:507-510`), so `loadingSize` 40-160 resizes only the wrapper, not the animation. [C]
- Background task chip stack: `top:112px; right:24px; max-width 320; z-index 30` fixed in CSS (`background-task.css:2-12`), up to 3 chips plus "+N more" (`backgroundTaskModel.ts:11-15`). Not tied to the spinner's rect. [C]
- Captions: top-dock slot is `x = 0.515*innerWidth` (centred) and `y = 0.895*innerHeight` (`ResponseCaption.tsx:66-84`, `LiveCaption.tsx:79-95`), i.e. the bottom of the screen, not "below the capsule" as the component doc comment says. Bottom-dock fallback is below the orb rect bottom, which for a bottom-docked orb is off the bottom edge (`rect.y+rect.h` equals screen height, +20). [C]
- Hand chip: horizontally centred (0.5W), vertically `H - chipH - (taskbar + 12)` when orb is top (`handOverlayModel.ts:105-116`). [C]
- Notify expand: from-rect = bottom-right of work area (toast), to-rect = CodeRabbit sidebar rect, 595 wide, right-docked, y=20, h=min(H-40,1040) (`work_area.rs:47-66`, `commands.rs:1006-1011`, `notify_hub.rs:27-28`). z-index 40. [C]
- Z-order inside the stage: expand 40, hand 40, task chip 30, orb 9998, caption 10000, spatial 99997-99999, tour svg 99990, tour callout 99991. The notify expand card passes under the orb, captions and tour. [C]
- Stage window: created `inner_size(1920,1080)` logical, no position, `center:false` (`dyn_windows.rs:79-86`, `:155-165`). A grep for set_size / set_position / fullscreen / maximize touching the stage found none; `stage.rs` has no monitor sizing; `monitor_info()` returns the primary monitor's PHYSICAL size and the rect math assumes the stage origin equals the monitor origin and the stage is as large as the monitor. [C for absence in the grep]
- Multi-monitor: hand overlay says it "covers the primary monitor only" (`handOverlayModel.ts:72`); notify expand uses the primary monitor (`notify_hub.rs:183`). [C]

Computed rectangles (CSS px, stage assumed to match the screen)

| Screen | Pebble | Classic capsule | Spinner box (Lottie) | Task chips (3+more) | Caption slot (1 line / 2 lines) | Hand chip, taskbar 48 |
|---|---|---|---|---|---|---|
| 1920x1080 @100 % | x 809-1111, y 0-105 | x 860-1060, y 0-200 | x 1784-1864, y 14-94 (1794-1854, 24-84) | x ~1576-1896, y 112-250 | y 967-997 / 967-1019, centre x 989 | y 990-1018 (overlaps caption 7 px) |
| 1536x864 (125 %) | same 301x105 | same 200x200 | x 1419-1499, y 3-83 | y 112-250 | y 773-803 / 773-825 (taskbar top 816) | y 776-804 (overlaps caption almost fully) |
| 1280x720 (150 %) | same | same | x 1176-1256, y 0-80 (clamped) | y 112-250 | y 644-674 / 644-696 (taskbar top 672) | y 632-660 (overlaps 16 px) |

- Spinner-to-chip gap is 18 px at 1080p and 32 px at 720p; spinner right edge (1864) and chip right edge (1896) do not align (32 px). With `loadingSize` 160 the spinner occupies y 0-160, x 1744-1904, which overlaps the first chip (y 112-142). The user can also calibrate the spinner anywhere; the chips never follow. [C]
- At 125 %/150 % the caption slot is 7-24 px into the taskbar for two-line captions (taskbar 48 DIP, `taskbarBottomPx` logic `handOverlayModel.ts:119-122`). [C computed]
- Owner's taskbar auto-hides, so `taskbarBottomPx` returns 0 and the hand chip sits at y 1040-1068, clear of the caption (967-997). [C]

### Inferences
- [I, high priority] On a 125 % panel, the stage is 1920x1080 logical = 2400x1350 physical on a 1536x864-logical screen. Captions at `0.895*1080 = 967` are below the visible screen (864) and centred 221 px right of the orb; the hand chip uses `innerHeight` too. Even at 100 % on a 2560x1440 or 1366x768 screen the percentages are relative to a window that is not the screen. The stage window origin is also never set. Verification: read the `[ANIM] Stage overlay mounted: WxH (DPR)` line on a scaled machine, or run NEXUS on a 125 % display.
- [I] Pebble mode has a hit-box mismatch: Rust registers the 200x200 orb rect as the interactive hitbox while the state is not idle (`window_manager.rs:239-244, :303-311`), but the visible pebble is 301x105. Result: an invisible click-eating zone x 860-1060 / y 105-200 under the pebble, and the pebble wings (x 809-860, 1060-1111) are not clickable. Rust has no knowledge of capsule style (grep for capsule/pebble in `src-tauri/src` finds only the setting plumbing).
- [I] The CodeRabbit sidebar (x 1315-1910, y 20-1060 at 1080p) covers the same top-right region as the spinner and chips. Both are topmost windows, so either the spinner/chips are hidden behind the sidebar or they float over its content, depending on last z-order change.
- [I] Caption at the bottom while the orb is at the top gives about 900 px of eye travel, and the code comment, the hand-chip rule ("edge opposite the orb") and the caption all disagree about the intended relationship. Subtitles at the bottom are a legitimate convention; the defect is the unstated, inconsistent contract.

### Gaps
- No live measurement of stage size on a scaled display; no screenshot of the production stage (needs Tauri).
- Real taskbar height on 125/150 % (assumed 48 DIP).

---

## 2. Orb, capsule and caption visuals in light and dark

### Takeaway
Dark theme is coherent (white particles on black, black pill with white text). Light theme is half-migrated: the classic capsule turns `#fafafa` with black-ink particles, but the pebble stays black (black ink on black = invisible orb), the caption pill stays black, and the live caption has no background at all so its white text fails on light wallpapers.

### Cited Findings
- Particles are pure white in every state: shader `tint = vec3(1.0)` (`voice-orb.js:316-319`), fragment colour white (`:352`). In light mode `ink > 0.5` outputs black with alpha from luminance (`:353-357`); Avatar passes `theme={themeMode}` (`Avatar.tsx:466`). [C]
- Classic capsule: `#000 !important`, 10 px radius on the free edge, border `rgba(255,255,255,.16)`, padding 14 (`styles.css:38-66`). Light override: `#fafafa`, border `rgba(60,60,67,.22)`, shadow 0.28 (`styles.css:290-305`). [C]
- Pebble: SVG fill hard-coded `#000000` (`OrbFrame.tsx:253`); no light-theme rule exists for `.pebble-scoop-*` or the SVG; `:root[data-theme="light"]` rules only target `.orb-capsule`. With ink particles in Light, the pebble draws black particles on a black pouch. [C code; not rendered]
- `orbColor` setting (owner value `#f2b859`) and the Hub "Orb Color Theme" have no visible effect: the shader forces white and ignores `userTint` in the final colour (`voice-orb.js:316-319, 352`). The 2026-10-06 "pure white" directive overrode the 2026-10-05 colour feature without removing the control. [C]
- The classic capsule hover adds an amber glow `rgba(242,184,89,.25)` (`styles.css:70`) that breaks the white-only palette; `.avatar-wrap--waiting` and calibration glow are blue (`:179, :193`). [C]
- Response caption: black pill `#000`, 1px border `rgba(255,255,255,.16)`, 10 px radius, white 16.5 px / weight 550 text, 2 px vertical padding, min-height 30, max-width 620 (`ghost.css:408-456`); no light-theme variant. [C]
- Live caption (user's own words): class `response-caption live-caption`, so white text (`--ink-primary:#fff`) on a transparent background with a 14 px dark text-shadow, opacity .82, font 16.5 (`ghost.css:322-338, :483-486`). [C]
- Contrast, computed: white on `#808080` wallpaper 3.95:1; white on `#e8e8e8` 1.23:1 (live caption, no pill; shadow is the only help); light capsule `#fafafa` vs white wallpaper 1.04:1, its border about 1.55:1; dark task chip `#151515` vs black wallpaper 1.15:1 and its border `#2b2b30` vs the chip only 1.30:1; chip time text `#9aa0aa` on `#151515` 6.94:1; chip spinner track `#3a3a42` vs `#151515` 1.62:1 (decorative track, the `#8ab4ff` arc is the signal); tour pin white on `#5a9eff` 2.70:1 and on `#259ed6` 3.03:1 (11.5 px text, fails 4.5:1); hand chip live white on `#a01e1e` 7.78:1; tour accent `#2f7dff` on `#10141e` 4.82:1; guide bubble 17.06:1. [C computed]
- Typography is inconsistent across the layer: captions use SF Pro/-apple-system stack at 16.5 px; chips, expand card and hand chip use Segoe UI 12-14 px; tour uses system-ui 13.5-15 px; pin 11.5 px. Radii: 10 (capsule, caption, chip), 12 (expand card, bubble), 14 (hand chip), 16 (tour), 13 (pin), 999 (badges). [C]
- Ghost ring blue `rgba(90,160,255,.9)` 2 px, hand ring amber dashed, spatial box light-grey, tour accent blue: four different accent systems for "NEXUS is pointing at/acting here". [C]

### Inferences
- [I] In Light theme with pebble (the owner's style), the orb is likely invisible; with classic it works. This is the single most visible light-mode defect and is cheap to verify by switching Hub theme to Light.
- [I] On light wallpapers the live caption is close to unreadable; on dark wallpapers it is fine.
- [I] In Light theme the stage mixes light (orb capsule, tour card) and dark (caption, chips, hand chip, expand card) surfaces. Solid dark HUD chips are defensible on any wallpaper (18:1 against white), but then the capsule should not flip.

### Gaps
- No rendered light-theme stage image; pebble-in-light is code-only evidence.
- Real wallpaper behaviour of the 14 px text-shadow not measured.

---

## 3. Motion, reduced motion, interaction (hit boxes, Escape, focus, ARIA)

### Takeaway
Reduced-motion and forced-colors support exists only in the newest files (tour, notify expand, task chip, voice-orb shader). The older stage CSS (ghost.css, hand-overlay.css, styles.css) and the pebble spring animation ignore it. Escape handlers live in a window that is click-through and unfocused, so they probably never fire.

### Cited Findings
- Reduced motion handled: `tour.css:584-615`, `notify-expand.css:20`, `NotifyExpand.tsx:6,45,51`, `background-task.css:40`, `voice-orb.js:459`. Not handled: `ghost.css` (ghost-pulse ring, pointer-bounce, spatial-box breathe/pulse, response-caption-word-in, caption-line blur/translate transitions, caption capsule transitions), `hand-overlay.css` (ripple), `styles.css` (orb slide 0.72 s translate, `ghost-wave` bars, preview-pulse), `PebbleScoopFrame` rAF spring (`OrbFrame.tsx:180-197`, mode 1 overshoots and oscillates by design, `computeStretch` `:32-37`), `LoadingIndicator` Lottie loop (`LoadingIndicator.tsx:54-60`, loops forever). [C]
- Forced-colors / prefers-contrast handled only in `tour.css:617-648`. Captions, capsule, chips, expand card, hand chip: none. [C]
- Flash rate: spatial-box-pulse 0.9 s (about 1.1 Hz), ghost-pulse 1.6 s, hand ripple one-shot 0.5 s; all under 3 Hz. [C]
- Pointer-events: orb slider, captions, chips, expand, hand overlay are `pointer-events:none`; interactivity comes from Rust-side hitboxes (`stage_set_hitboxes`, sources orb / tour / spatial / loading). [C]
- Spatial pin hitbox is a fixed 96x26 px starting 6 px left and 4 px above the pin (`main.tsx:117-122`) while the pin is a flexible-width button (`ghost.css:166-186`, label up to 18 chars). Estimated pin width 120-160 px, so about 40 % of a long pin is not clickable; the pin's bottom 4 px also falls outside. [C code, estimate for width]
- Tour close button is 22x22 (`tour.css:430-436`) below the 24x24 minimum, but WCAG 2.5.8 passes by the spacing exception because no other target lies within a 24 px circle ([W3C 2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)). Pin height 26 passes. [C]
- Escape: three independent keydown handlers: spatial layer clears pins (`main.tsx:139-149`), StageApp hides the guide bubble and invokes `orchestrator_cancel` (`main.tsx:351-362`), TourOverlay dismisses and cancels (`TourOverlay.tsx:123-131`). One keypress therefore dismisses pins AND cancels the whole turn. The only global Escape registrations in Rust are Ghost (`ghost.rs:545-570`) and Hand (`hand/mod.rs:665-683`); the stage is created `focus:false`, click-through, WS_EX_TOOLWINDOW (`dyn_windows.rs:75-86`). The tour close tooltip says "Dismiss tour (Esc)" (`TourOverlay.tsx:302`). [C]
- ARIA: only `BackgroundTaskChip` has `role="status" aria-live="polite"` (`BackgroundTaskChip.tsx:46`). Captions, loading spinner, tour callout, spatial pins (button with title only) and hand chip have no role or live region; NotifyExpand is `aria-hidden`. Tour SVG is `aria-hidden`; the close button has `aria-label`. [C]
- Hit target for loading indicator `data-interactive` is set even when not calibrating, with `onPointerDown`/`onWheel` guarded by `calibrating`; the Rust "loading" hitbox registration was not found in the stage component (only calibration reports). [C partial]
- Auto-play motion longer than 5 s that the user cannot stop: Lottie spinner (only while a turn runs), task chip spinner (while a task runs), spatial box breathing (20 s TTL, `main.tsx:84`), orb idle motion (shader pauses under reduced-motion). WCAG 2.2.2 requires pause/stop/hide for non-essential motion longer than 5 s ([W3C 2.2.2](https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html)); progress/loading indicators are usually argued to be essential, but the decorative breathing is not. [C]
- Apple's guidance on Reduce Motion: replace scaling, spinning and multi-axis motion with dissolves or highlights rather than only deleting animation ([Apple Reduced Motion criteria](https://developer.apple.com/help/app-store-connect/manage-app-accessibility/reduced-motion-evaluation-criteria/), [WWDC23 session](https://developer.apple.com/videos/play/wwdc2023/10078/)).

### Inferences
- [I] Because the stage never has keyboard focus, the three Escape handlers and the tooltip are likely dead; only Ghost and Hand sessions have a working Esc. The doc line "Esc closes tour" (AGENTS.md 2026-10-06 entry) was never live-verified. Test: start a tour, press Esc without clicking.
- [I] A user with Windows "Show animations" off still gets the orb sliding in 720 ms, the pebble spring overshoot, ghost-wave bars and the ring pulse.
- [I] The shared Escape-plus-cancel behaviour makes dismissing a pin cancel a running turn.

### Gaps
- Whether Windows gives the click-through stage keyboard focus after a hit-box click; not tested.
- Whether UI Automation / Narrator exposes the topmost stage at all.

---

## 4. Notification flow: toast, click, expand, sidebar

### Takeaway
The path is well-guarded (opaque activation id, escaped text, urgent only for failures, recoveries never break through DND), but the toast is a bare two-line template with no icon, no tag/group and no expiry, and the expand overlay is theme-unaware and uses React inline styles that the project itself claims the stage CSP drops.

### Cited Findings
- Toast XML: `toast_xml` emits `activationType="foreground"`, `launch="nexus://notify/open?event=<id>"`, one ToastGeneric binding with exactly two `<text>` elements (title max 120, body max 240) and optionally `scenario="urgent"` (`notify_native.rs:48-64`). No image, no `appLogoOverride`, no attribution, no audio element, no duration, no buttons. [C]
- AUMID registration writes only `DisplayName` and `ShowInSettings` under HKCU `AppUserModelId\com.nexus.assistant` (`notify_native.rs:74-80`), no `IconUri` or `IconBackgroundColor`. [C]
- No `Tag`, `Group` or `ExpirationTime` is set before `Show` (`notify_native.rs:83-110`). Microsoft documents that an app can set Expiration (default and maximum 3 days) and use tag/group to replace or remove notifications ([Manage app notifications](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/manage-app-notifications)). The ledger event lives 7 days (`notify_hub.rs:25`), so the Action Center entry can vanish while the sidebar record is still valid, and a stale "failed" toast stays after a later recovery. [C]
- DND: failures set `scenario=urgent` (when `notifyUrgent` on, default true), recoveries do not (`notify_hub.rs:144-169`, tests `notify_native.rs:141-147`); Windows prompts the owner once to allow important notifications, NEXUS cannot grant it itself (`notify_native.rs:48-56`). Microsoft's own pages are inconsistent: the App SDK content page lists Urgent and `IsUrgentScenarioSupported`, while the App SDK schema page lists only Default/Reminder/Alarm/IncomingCall ([content](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content), [schema](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-schema)). The doc comment says "NOT verified on a device". [C]
- Wording: title from `notify_brain::render_for`, tail punctuation trimmed (`notify_hub.rs:82`), e.g. "PR failed in zync"; preview `owner/repo · run 123`; spoken variants add ", sir.". Status words are failed / recovered / info. Consistent across toast and expand card. [C]
- Expand card: dark solid `#151515`, `#f1f1f1` text, 3 px left border `#8ab4ff` info / `#ff7a4d` failed / `#5fd38a` recovered (`notify-expand.css`), contrast of accents on the card 8.74 / 7.08 / 9.71:1. Duration 280 ms, hold timeout 1500 ms, leave 180 ms (`notifyExpandModel.ts:22-23`, `notify_hub.rs:30`). Card shows only the title, not the status word or repo, so failed vs recovered is distinguished by the title text plus a 3 px colour edge. [C]
- `NotifyExpand` applies position, size and transition through a React `style` prop (`NotifyExpand.tsx:52-58`), the pattern that `main.tsx:37-44` and `OrbFrame.tsx:679-694` document as silently dropped by the stage CSP. React normally writes styles through CSSOM, which CSP does not block, so the project's note may be wrong or the expand may be broken; the two statements cannot both be right. [C code, outcome unknown]
- Windows toasts follow the OS theme, the expand card and the rest of the stage follow `themeMode`; the card has no light variant. [C]
- Reduced motion: expand duration drops to 0 and the `all` transition is cancelled (`notifyExpandModel.ts:41-44`, `notify-expand.css:20`). Good. [C]

### Inferences
- [I] On first failure the owner sees a generic-icon "NEXUS" toast; with a sub-3-second default duration (no `duration="long"`), it is easy to miss when not in front of the screen, while recovery quietly lands in the Action Center.
- [I] If the expand card is un-positioned (CSP theory), it would flash at the top-left rather than grow from the toast.

### Gaps
- Not run on a device (toast, click activation, DND). The Microsoft Fluent notification-design page was not fetched successfully (404 on the adaptive-interactive-toasts URL); recommendations below rely on the schema/manage pages above.

---

## 5. Deviations from current best practice and a clean version of each element

### Takeaway
The biggest gaps are not styling: the stage-to-monitor contract, a hit-box that ignores the pebble, light-theme inconsistency, and reduced-motion/forced-colors coverage. Each element needs one clear geometry owner and one token set.

### Cited Findings (defect list, ordered by severity)
P0
1. Stage is not sized or positioned to the monitor (`dyn_windows.rs:79-86`; no sizing code). Clean version: on show and on `WM_DISPLAYCHANGE`/scale change set the stage to the primary monitor's physical rect, and send `window.innerWidth/Height` back so Rust can assert match; log a warning when they differ.
2. Pebble style: orb invisible in Light (`OrbFrame.tsx:253` vs ink in `voice-orb.js:353`). Clean: pebble fill from a token (`--orb-surface`), same light/dark pair as the capsule.
3. Orb hit-box ignores pebble (`window_manager.rs:239-244`). Clean: the frontend reports its real capsule rect (`stage_set_hitboxes` source "orb") instead of Rust guessing.
4. Hand chip and caption share the bottom centre (`handOverlayModel.ts:113-115` vs `ResponseCaption.tsx:72-75`): collide on a visible taskbar. Clean: one layout manager with named slots (orb, status chip, caption, spinner, task stack) and an anchor system (below).
P1
5. Live caption has no surface; white text on light wallpapers (`ghost.css:322-338`, `:483`). Clean: reuse the caption pill; distinguish user speech by a lighter border and italic or a "You" tag, not by dropping the background.
6. Caption pill has no light variant; capsule does (`styles.css:290`). Decide one rule (HUD is always dark, or both follow theme) and apply to orb, caption, chips, hand chip, expand card.
7. Loading size setting does nothing to the Lottie (`ghost.css:507-510`); chip stack top is a magic 112 px (`background-task.css:4`). Clean: Lottie 100 % of the wrapper; stack anchored from the spinner's actual rect (see slots).
8. Escape handlers on an unfocused click-through window; triple handling cancels a turn from a pin dismissal (`main.tsx:139-149, 351-362`, `TourOverlay.tsx:123-131`). Clean: Rust owns Esc for the duration of any dismissible overlay (as Ghost/Hand already do) and routes one event; pin dismissal must not cancel.
9. Reduced-motion gaps (list in section 3). Clean: a single `@media (prefers-reduced-motion: reduce)` block in a shared stage stylesheet and a `useReducedMotion()` hook for rAF/Lottie (pause Lottie, replace pebble spring with a 150 ms fade).
10. Toast: add `IconUri`/appLogo, `Tag`+`Group` per entity key, `ExpirationTime` matching the ledger TTL (or the 3-day max), `duration="long"` for failures; remove the stale failed toast on recovery.
11. Spatial pin hitbox 96x26 narrower than the pin (`main.tsx:117-122`). Clean: measure the element (`getBoundingClientRect`) and register that.
P2
12. `orbColor` is a dead control (`voice-orb.js:316-319`); remove from Hub or implement.
13. Amber hover glow on capsule breaks the white palette (`styles.css:70`).
14. Tour pin text white on `#5a9eff` is 2.70:1 and on `#259ed6` 3.03:1 at 11.5 px: darken the gradient (for example `#1d6fb8`, about 5:1) or use dark text.
15. Task chip and expand-card edge is 1.15-1.30:1 on dark surfaces; add a 1 px light border (`rgba(255,255,255,.18)`) like the capsule.
16. Typography and radius drift (section 2). Clean: tokens `--hud-radius:10px`, `--hud-font`, sizes 12/13/16.5.
17. Default capsule style disagreement classic/pebble (`commands.rs:2148` vs `store/assistant.ts:269`).
18. NotifyExpand `style` prop vs documented CSP rule: verify, then either use CSSOM everywhere or delete the folk rule.
19. No live regions on captions/spinner; add `role="status"` on one shared visually-hidden region mirroring the current caption for screen readers.

Proposed slot map (default 1920x1080, all CSS px, one layout owner)
- Top-centre: orb (pebble 301x105 or capsule 200x200), reserved band y 0-105/200.
- Top-right: spinner box 80x80 at x 1784, y 14; task stack directly below (`top = spinnerBottom + 8`, right-aligned to the spinner's right edge, hidden behind the sidebar rule below).
- Bottom-centre: caption pill, slot y = `workAreaBottom - 24 - captionHeight`; hand chip sits above the caption (stack upward) instead of the same row.
- Right sidebar region (x > W - 605): spinner/chips move left by sidebar width when the sidebar is open, or are drawn in the sidebar header.
- Expand card: z-index above everything except tour, since it is a modal transition.

Best-practice comparisons (URLs)
- WCAG 2.2 target size 24x24 or spacing exception ([2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)); non-text contrast 3:1 for UI components and graphics needed to understand content ([1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html)); pause/stop/hide for motion longer than 5 s ([2.2.2](https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html)). NEXUS deviations: pin text and pin gradient, chip/capsule edges (decorative edges are arguable), decorative looping motion without pause.
- Windows app notifications: tag/group for replace and removal, expiration default and maximum 3 days ([manage notifications](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/manage-app-notifications)); content/scenario options including urgent and its support check ([content](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)). Deviation: none of tag/group/expiry/icon used.
- Apple: Reduce Motion should substitute dissolves for scaling/spinning rather than remove feedback ([criteria](https://developer.apple.com/help/app-store-connect/manage-app-accessibility/reduced-motion-evaluation-criteria/)). Deviation: pebble spring and orb slide ignore it. The HIG Motion page itself was not retrieved; only the criteria page and WWDC23 were.
- Material 3: loading indicator for 200 ms-5 s waits, progress indicator beyond 5 s, assist chips shown after primary content with a verb label ([progress](https://m3.material.io/components/progress-indicators/guidelines), [chips](https://m3.material.io/components/chips/guidelines)). NEXUS: the turn spinner and the task chip both act as loading indicators; a task running minutes shows elapsed time but no determinate state and no stop action (the chip is never interactive); the chip is labelled by noun, not verb. Page snippet was marked old by the search tool; check the live site.

### Inferences
- [I] Treating the stage as a layout system (named slots with anchors and collision rules) removes most of the P0/P1 collisions in one change; each element stays individually simple.
- [I] The "Windows 11 Fluent 2 overlay" guidance could not be fetched (404); the Windows recommendations here come from the notification schema and management pages, not a Fluent design page.

### Gaps
- No Fluent 2 page for ambient overlays retrieved; no Apple HIG "Motion"/"Indicators" page text retrieved.
- Light-theme screenshots of the real stage and any DPI-scaled run are missing; items tagged [I] above should be verified before work is scheduled.
- Harness pages rendered: only `caption-showcase.html` (dark, design exploration, not production CSS) and `dev-annotations.html` (light; ring, underline, bracket, circle styles legible on a light card). `dev-stack`, `pebble-showcase`, `dev-coderabbit` were not rendered.
