# 190 · Agent Island: only agents you are not watching, a size you set, a hover that does not jump

Builds on the Agent Island of change 186 (PR #46).

## What changed
- **Watched agents stay off the island.** `agent_watching.rs` decides, per running session, whether the window that
  hosts it is on screen and at least half visible (the foreground window, or a window not covered by the ones above
  it, so a split screen with two agents watches both). Minimised windows, windows on another virtual desktop and
  agents whose window cannot be found are NOT watched and still show. A session must be unwatched for 1.5 s before it
  appears and watched for 0.5 s before it leaves, so alt-tab does not flash the bar. Permission requests are never
  left out. The power rules (keep the screen awake) still look at every running agent. Setting
  `agentIslandHideWatched` (default on) turns it off.
- **Size.** `agentIslandScale` (0.75 to 1.5, default 1.0) scales the pill and the panel. Command Hub: a slider, and
  "Show on my screen", which draws the island for ten seconds with two sample agents at the chosen size.
- **Hover.** The OS window is created once at the (scaled) panel size and never resized. One page element
  (`.ai-island`) morphs between pill and panel (width, height and radius transition, 320 ms ease-out; the pill fades
  out while the panel fades in a beat later). Outside the drawn island the window ignores the mouse
  (`set_ignore_cursor_events`), so nothing is click-blocked; Rust polls the cursor every 33 ms while the island is up
  (it was 150 ms) and turns clicks on only while the cursor is over the island.

## Why the old hover glitched
Rust resized and moved the OS window instantly between pill and panel while the page swapped two different components
and played a 160-180 ms fade, so the window jumped first and the content faded in after.

## Verification
- Rust: `agent_watching` (coverage maths, watched rule, split screen, folder picks the window, debounce),
  `agent_island` (size scaling and clamp, fixed window, drawn rect, click-through rect), `agent_monitor`
  (`island_view`), settings round-trip. Library 3,135 tests pass. Mutations killed: visibility threshold 0.5 -> 0.3,
  and dropping approvals from the island view.
- Frontend: tsc (one known error, `orbRuntime.ts`, fixed by the orb PR), vitest 1,022 pass; colour, contrast, window
  opacity, turn-end, hygiene gates pass. `check-ipc-contract` lists one known item (`action:receipt`, its emitter is
  not committed yet).
- Preview (`dev-agent-island.html`): the morph was sampled frame by frame (760 x 430 down to 92 x 40 in smooth steps,
  radius 24 -> 14 px, pill and panel opacities crossing); size 150 % gives a 138 x 60 pill.

## Not verified
Real windows: a split screen, a covered window, a Windows Terminal tab (one process, tabs told apart only by title),
mixed DPI and several monitors. The click-through toggle on the real window. The hover feel at 33 ms on a real screen.
