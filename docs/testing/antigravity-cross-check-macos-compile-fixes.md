# Antigravity cross-check brief — macOS compile fixes (commit 76ec071)

Purpose: an independent review of the 8-error macOS compile fix, by a different tool than the one that wrote it.
Status: **NOT RUN.** `agy` is not installed in the cloud session that wrote the fix, so no Antigravity review has happened.
Run it on the owner's PC in a **separate git worktree** (never in the main working tree: it carries uncommitted owner work).

## Run

```powershell
git fetch origin claude/bold-noether-fizcog
git worktree add ..\nexus-xcheck origin/claude/bold-noether-fizcog
cd ..\nexus-xcheck
agy --print "$(Get-Content docs/testing/antigravity-cross-check-macos-compile-fixes.md -Raw)" --mode default
```

Use `--mode default` (review only, no auto-accepted edits). Do not pass `accept-edits`: this is a **read-only review**.
If `agy` hangs with `--sandbox` on Windows (a reported issue), run without it.

## Prompt (give this to the reviewer)

You are reviewing commit `76ec071` against its parent. It changes only these files under `src-tauri/src/`:
`shortcuts/exec.rs`, `screen_capture_mac.rs`, `live/mod.rs`, `vision.rs`, `browser_url.rs`, `hand/mod.rs`.
Do NOT edit anything. Report findings only. For each finding give: file:line, severity (blocker / should-fix / note),
what is wrong, and the evidence you checked. If you cannot verify something, say so instead of guessing.

Check, in this order:

1. **Windows is unchanged.** For every hunk, state whether the code compiled for `target_os = "windows"` is byte-for-byte
   the same behaviour as before. The only Windows-visible edits should be `#[cfg(target_os = "windows")]` wrappers that keep
   the same statements. Flag any hunk where Windows behaviour could differ (control flow, return type, unused-variable or
   unused-import warnings that would fail a `-D warnings` build).
2. **`live/mod.rs` `live_ghost_click`.** On non-Windows the function must return an error result and not touch `app`
   incorrectly; on Windows the `match` must be the function's tail value exactly as before.
3. **`vision.rs` `resolve_named_target`.** A `#[cfg]` on an `if let` statement: confirm it is valid and that on non-Windows
   `name` is still used later, so no unused-variable warning.
4. **`shortcuts/exec.rs`.** Confirm against the `enigo` 0.5.0 source (cargo registry) that `Keyboard::raw(&mut self,
   keycode: u16, direction: Direction)` exists, takes a macOS `CGKeyCode`, and that the macOS backend updates modifier flags
   for modifier key codes (look for `add_event_flag` in `src/macos/macos_impl.rs`). Confirm `Key` is no longer imported
   where it is unused.
5. **`screen_capture_mac.rs`.** `dimension_to_i32`: any input where it returns a wrong value or panics (NaN, infinity,
   `-0.0`, values near `i32::MAX`, `.5` rounding)? `use image::GenericImageView;`: could it conflict with another import?
6. **`hand/mod.rs` and `browser_url.rs`.** The new macOS `guard_now` branch must refuse (fail closed) when both the foreground
   title and process are empty. Is there any other macOS path that sends clicks without passing through `guard_now`?
   Are there other callers of `foreground_process_name` / `foreground_window_title` on macOS that would now behave differently?
7. **What else would fail on macOS** after these 8 errors are gone? rustc stops before borrow-checking when name-resolution
   errors exist, so list any Windows-only symbol still referenced from shared (non-cfg) code. Search for calls into modules or
   functions that are `#[cfg(target_os = "windows")]` and that are reachable on macOS.

Output a table of findings, then a one-line verdict: SAFE TO MERGE / CHANGES NEEDED.

## What the cloud session already verified (so the reviewer can concentrate on the rest)

* `screen_capture_mac.rs` functions compile and their unit tests pass in a scratch crate on Linux against `image 0.25.10`.
* The real macOS result comes from the `macOS port check` GitHub Actions job; Windows and Linux jobs run on the same push.
