# 206 — The Agent Island no longer blocks clicks over the top of the screen

Date: 2026-11-22. Branch `fix/agent-island-click-through`. Follows [205](205-overlay-click-blocking-and-analyse-wave.md) and
[feature 196](../features/196-overlay-click-blocking-research-plan-and-implementation.md), which fixed the full-screen stage overlay.

## 1. Report

After the stage fix the owner still could not click Brave's address bar, select a tab or close a tab at times; keyboard shortcuts
and voice commands kept working. A few minutes later it stopped on its own.

## 2. Research

- The checks that separate the causes (all read-only, no mouse movement):
  - the stage overlay: click-through, non-activating, **no `[HITBOX]` line the whole session**, so it was not registering any
    interactive rectangle;
  - no Hand mode or Ghost mode, no stuck modifier key, no mouse button held, the cursor not confined;
  - Windows' own "Click to Do" overlay has a full-screen topmost window, but Windows skips it when deciding what receives a click.
- The NEXUS log showed the **Agent Island** appearing and disappearing (`Hidden -> Collapsed -> Hidden`, about 37 s each time) at the
  moments the user's coding agent (Claude Code in this very session) finished a turn, right when the report was made.
- A read-only probe waited for the island window and asked Windows which window owns the pixels across its rectangle:
  the window was 1140x645 physical px (570x323 logical) at the top centre, style `0x080401B8`, **`LAYERED` not set**, `TRANSPARENT`
  set, and **all 20 of 20 sample points, including the far corners, belonged to the island's WebView2**, not to the app below.
  So the island's "click-through except over the pill" never worked.

## 3. Root cause

`agent_island.rs` switches click-through by setting `WS_EX_TRANSPARENT`. Windows only lets the mouse pass through such a window if it
is **also layered** (`WS_EX_LAYERED`). The island is shown with raw Win32 calls and avoids tao's `set_ignore_cursor_events` (which
hides the pill on hover and drops `NOACTIVATE`), and tao is what adds `LAYERED` inside that call, so the island window was never
layered. While the island was on screen its whole panel-sized window, a rectangle that covers a maximised browser's tab strip
centre, address bar and the top of the page, took every click. An older unit test even assumed the live style contained `LAYERED`
(`0x080C01B8`); the real one did not (`0x080401B8`).

The stage overlay had the same flag but is layered by tao (it passed the earlier probe), which is why only the island was affected.

## 4. Design (owner-approved plan)

- The pill and the expanded panel are solid, as intended: you cannot click through them. Everything outside them is click-through.
- Mechanism: make the island window layered, as the stage is, so the flag works; keep the hover/click decision where it was (a cursor
  poll about every 33 ms that opens the window to the mouse only while the cursor is over the drawn island).
- Shrink the zone around the drawn island that counts as "on the island" from 24 px to 8 px (logical). The same zone drives hover
  (expand) and clicks; a 24 px halo at the top centre covered a browser's tab strip. The one-agent pill is about 48x30 px at the
  smallest size, so 8 px keeps the hover reachable.
- Fallback if a layered window does not draw the page correctly: clip the window to the drawn pill/panel with a window region
  (cost: the page's drop shadow outside the pill would be clipped, or a small invisible halo kept for it).

## 5. Implementation (`src-tauri/src/agent_island.rs`)

- `MARGIN` 24 -> 8, with the reason in the comment.
- `click_through_style(ex, through)`: always sets `LAYERED | TOOLWINDOW | NOACTIVATE`, clears `APPWINDOW` (no Alt+Tab or taskbar
  entry), sets or clears `TRANSPARENT`; every other bit (for example `TOPMOST`) is untouched.
- `set_click_through` (Windows): reads the style, writes it only if it differs, and calls `SetLayeredWindowAttributes(hwnd, 0, 255,
  LWA_ALPHA)` when the window was not layered before (a layered window draws nothing until its alpha is set).
- `Island::tick` now applies it on **every** tick, not only when the state changes, so it also repairs the style if anything else
  changes it; `place_and_show` applies it right after showing the window.

## 6. Tests

- Rust (`agent_island.rs`), run from a scratch crate extracted from the real file because the full `cargo test` build fails on
  `ort-sys` on this machine: **17 pass** (the 15 that existed, two of them rewritten, plus 2 new): the hot zone is small but reachable (6 px counts, 20 px does not);
  the style is layered, non-activating, tool-window and not app-window for four starting styles, including the live one and an empty
  one; idempotent and reversible; and a source scan that the style is applied in `place_and_show` and not only on a state change.
- Mutation checks (each must fail a test): margin back to 24, `LAYERED` dropped, `NOACTIVATE` dropped, `APPWINDOW` kept, the style
  applied only on a state change. All five are caught. One survived the first version of the test (the sample styles already had
  `NOACTIVATE`), so samples with none of the bits were added.
- Live (build `20de5d2c99796fba`, `C:\NEXUS-Dev`): with the island on screen the probe read `ex = 0x080801B8`, **`layered=True`**,
  `transparent=True`, and **0 of 80** sample points (4 rounds x 20 points across the old 1140x645 window) belonged to the island;
  all went to the application underneath. Before the fix it was 20 of 20.

## 7. Not verified

- Whether the layered island draws the pill and the panel correctly and whether hover expands it and the Allow/Deny buttons work in
  this build: the cursor-over-pill behaviour needs a real mouse (the probe never moves the mouse). The owner was asked to check.
- 125 % / 150 % scaling and a second monitor.
- The zone is now 8 px, so the hover on a very small pill is less forgiving than before (a design trade-off the owner chose).

## 8. Notes

- While the island is up it still sits over the top centre of the screen: the pill itself and, when expanded, the whole panel are
  solid by design (an expanded permission request has to be answerable).
- The island only shows while the agent it describes is not being watched; with the Claude app in front it stays hidden, which is
  why it did not appear during the first probe.
