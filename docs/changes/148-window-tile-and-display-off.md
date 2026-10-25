# 148 - Window tiling and display-off primitives (agent tracker, plan P3)

Plan: `C:\Users\Chitkul Lakshya\.claude\plans\gleaming-exploring-oasis.md`, section P3. The multi-agent
tracker will call these when every tracked agent has finished: tile their windows into equal columns, then
turn the display off. This change is only the machine side; nothing calls it yet.

## What was added

* `src-tauri/src/window_tile.rs` (new, `pub(crate) mod window_tile;` in `lib.rs`).
* `Win32_Graphics_Dwm` feature on the `windows` crate (Cargo.toml, one line).
* No other file touched. The module carries `#![cfg_attr(not(test), expect(dead_code, ...))]` until the
  tracker uses it; a normal `cargo check` reports no warning from it.

## What the tracker can call

| Call | What it does |
|---|---|
| `list_top_windows() -> Vec<TopWin>` | Tileable top-level windows, topmost first, with pid/exe/class/title. Reuses `power_close::RealEnv::list` (stack-local state, thread-safe) and adds the pid. |
| `resolve_agents(&[AgentTarget], &[ProcSnap], &[TopWin]) -> Resolution` | Per agent: pid walk first (`AgentTarget.pid`), then name match (`AgentSpec`). Returns `tiles` (distinct windows in first-agent order, each with the agents living in it) and `missing` (agent names with no window). `Tile::shared_line()` gives the sentence for a shared window ("Claude Code and Codex share one window, so I tiled it once."). |
| `window_for_pid(pid, procs, wins, prefer_title)` / `find_agent_window(spec, wins)` | The two resolvers on their own. The pid walk goes node.exe -> shell -> terminal and takes the first ancestor owning a visible window; it never lands on `explorer.exe`, stops on parent cycles, and uses `prefer_title` to pick between several windows of one process (Windows Terminal). Name match: exe-word hit beats title-word hit; the taskbar/desktop and `nexus.exe` are never candidates. |
| `tile_windows(&RealTileEnv, &[hwnd]) -> Vec<(hwnd, PlaceResult)>` | Left-to-right equal columns on the primary monitor's work area (taskbar excluded). Gone and elevated windows are reported and take no column, so the rest still fill the width. Duplicate handles are placed once. |
| `PlaceResult` | `Placed`, `SkippedElevated`, `SkippedGone`, `Failed(reason)`, `Unsupported` (non-Windows). |
| `may_blank_display(user_idle_secs: Option<u64>, meeting_active: bool) -> Result<(), Why>` | Refuses when a meeting is active (`Why::Meeting`), when the idle time is unreadable (`Why::IdleUnknown`) or when the user used keyboard/mouse in the last 60 s (`Why::UserActive{idle_secs}`); exactly 60 s is allowed. Feed it `memcore::resume::user_idle_secs()` and the meeting detector. |
| `display_off() -> Result<(), String>` | `SendMessageTimeoutW(GetDesktopWindow(), WM_SYSCOMMAND, SC_MONITORPOWER, 2)` with `SMTO_ABORTIFHUNG`, 1 s timeout; never `HWND_BROADCAST`. Non-Windows returns an "unsupported" error. The caller must check the guard first. |

`columns(work, n)` and `compensate(target, window_rect, frame_rect)` are the pure layout pieces (n=0 or a work
area narrower than n pixels gives no columns; the remainder pixels go one each to the leftmost columns).

## Decisions worth knowing

* The spec's `may_blank_display(idle, meeting, now)` has no `now`: the idle seconds already encode "now", and an
  unused parameter would only invite misuse.
* Placement restores a minimised or maximised window (`SW_RESTORE`), widens the request by the invisible DWM
  border (`DWMWA_EXTENDED_FRAME_BOUNDS` vs `GetWindowRect`), calls `SetWindowPos(NOACTIVATE|NOZORDER)` and then
  checks the visible frame landed within 3 px; a window with a minimum size wider than its column reports
  `Failed("... minimum size?")` instead of pretending.
* Elevated windows are decided by `live::commands::window::elevated_block_reason(window_process_elevated,
  our_process_elevated)`; an elevated target is not attempted unless NEXUS is elevated too.
* v1 tiles on the primary monitor. Multi-monitor is a follow-up (the trait already has one `work_area`).
* Several agents in ONE Windows Terminal process with several windows: only `prefer_title` separates them.
  Pass the agent's title word in its `AgentSpec` when that is possible.
* Restoring a minimised window with `SW_RESTORE` may also activate it; the move itself does not.

## Verification

* `cargo test --lib --features custom-protocol,admin-brain -- window_tile`: 20 tests pass, 1 ignored (the dev
  test below). Layout (n = 1..6, odd widths, negative-origin monitors, remainder distribution), border
  compensation, the guard truth table, the resolver against fake process/window tables (shared window,
  missing window, elevated, explorer, parent cycle) and `tile_windows` against a fake machine.
* Mutation checks (each must fail a test): guard `<` -> `<=`, 60 s -> 30 s, meeting flag ignored, remainder to
  the rightmost columns, remainder dropped. Results are listed at the end of this file.
* `cargo check` (non-test): no warning from `window_tile`.

## NOT run live

* Placement on real windows: the Win32 adapter compiled but no real window was moved by this change's tests.
  Try it by hand (opens two Notepad windows, tiles them, closes them, never blanks the display):

  ```
  cargo test --lib --features custom-protocol,admin-brain -- window_tile::tests::dev_tile_two_notepads --ignored --nocapture
  ```

  Windows 11 Notepad may open the second one as a tab; the test then says so and tiles nothing.
* `display_off()` is not exercised anywhere (no test calls it, deliberately). Whether the display wakes on
  the first key/mouse input, and whether anything of ours (synthetic input from hand mode, a keep-awake lease)
  wakes it early, has not been checked.
* DPI scaling at 125/150 %, mixed-DPI monitors, windows with a minimum size, UWP/store windows
  (`ApplicationFrameHost`) and the exclusive-fullscreen case were not tried.

## Mutation results (all killed, final code restored and re-run green)

| Mutation | Failing test |
|---|---|
| guard `s < 60` -> `s <= 60` | `guard_truth_table` |
| `MIN_IDLE_SECS` 60 -> 30 | `guard_truth_table` |
| meeting flag ignored | `guard_truth_table` |
| remainder pixels to the rightmost columns | 4 layout tests |
| remainder pixels dropped | 4 layout tests |
