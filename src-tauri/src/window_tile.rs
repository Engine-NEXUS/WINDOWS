//! Machine-side primitives for the "every agent finished" sequence (plan P3): tile the agents' windows into
//! equal columns, then blank the display so the user finds everything arranged on return.
//!
//! Nothing here decides WHEN to do that; the agent tracker calls it. The module is split so that every
//! decision is pure and tested, and only a thin Windows adapter touches Win32:
//!   * `columns` – equal-width layout that fills the work area exactly;
//!   * `compensate` – the invisible DWM border correction (pure arithmetic);
//!   * `find_agent_window` / `window_for_pid` / `resolve_agents` – which window belongs to which agent, and
//!     which agents share one window (a terminal hosting two agents is tiled once and said so);
//!   * `tile_windows` + `TileEnv` – the placement loop with per-window results (`PlaceResult`); an elevated
//!     window is reported and never attempted;
//!   * `may_blank_display` – the refuse-while-the-user-is-here guard; `display_off` – the Win32 call.
//!
//! Not run live in the build that added it: placement on real windows, DPI/multi-monitor behaviour and
//! whether the display wakes again on input (see `docs/changes/147-window-tile-and-display-off.md`).


use crate::session_observer::ProcSnap;

// ─── Geometry ────────────────────────────────────────────────────────

/// A screen rectangle in physical pixels, same meaning as Win32 `RECT` (right/bottom exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub(crate) fn width(&self) -> i32 {
        self.right - self.left
    }
    pub(crate) fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Split `work` into `n` side-by-side columns that fill it exactly: no gaps, no overlap, every column the
/// full height. When the width does not divide evenly the leftover pixels go one each to the leftmost
/// columns, so widths differ by at most one pixel. `n == 0` gives no columns.
pub(crate) fn columns(work: Rect, n: usize) -> Vec<Rect> {
    let w = work.width();
    // Fewer pixels than columns cannot be tiled: no zero-width columns are promised.
    if n == 0 || w < n as i32 || work.height() <= 0 {
        return Vec::new();
    }
    let n = n as i32;
    let base = w / n;
    let extra = w % n;
    let mut out = Vec::with_capacity(n as usize);
    let mut x = work.left;
    for i in 0..n {
        let cw = base + if i < extra { 1 } else { 0 };
        out.push(Rect { left: x, top: work.top, right: x + cw, bottom: work.bottom });
        x += cw;
    }
    out
}

/// A top-level window's own rectangle includes an invisible resize border on Windows 10/11, so asking for
/// the visible column straight away leaves visible gaps and overlaps. Grow the request by the difference
/// between the window rect (`win`) and what is actually drawn (`frame`, DWM extended frame bounds).
pub(crate) fn compensate(target: Rect, win: Rect, frame: Rect) -> Rect {
    Rect {
        left: target.left - (frame.left - win.left),
        top: target.top - (frame.top - win.top),
        right: target.right + (win.right - frame.right),
        bottom: target.bottom + (win.bottom - frame.bottom),
    }
}

// ─── Finding the windows ─────────────────────────────────────────────

/// One top-level window as the tiler sees it (same filter as the shutdown closer, plus its owning pid).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopWin {
    pub hwnd: isize,
    pub pid: u32,
    /// lower-case exe name; empty when unreadable (elevated / protected process)
    pub exe: String,
    pub class: String,
    pub title: String,
}

/// How to recognise an agent's window by name. A window matches when ANY exe word is part of its exe name
/// or ANY title word is part of its title (case-insensitive). Keep the words specific: "claude" as a title
/// word also matches a browser tab called "Claude".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AgentSpec {
    pub exe_words: Vec<String>,
    pub title_words: Vec<String>,
}

const SHELL_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

/// Windows that may be tiled at all: titled, not a shell surface, not NEXUS itself.
pub(crate) fn tileable(w: &TopWin) -> bool {
    !w.title.trim().is_empty() && w.exe != "nexus.exe" && !SHELL_CLASSES.contains(&w.class.as_str())
}

fn contains_any(hay: &str, words: &[String]) -> bool {
    let hay = hay.to_lowercase();
    words.iter().any(|w| !w.trim().is_empty() && hay.contains(&w.trim().to_lowercase()))
}

/// The topmost tileable window matching `spec`; a window matched by its exe beats one matched only by its
/// title (the title of a terminal tab is weaker evidence than the program that owns the window).
pub(crate) fn find_agent_window(spec: &AgentSpec, wins: &[TopWin]) -> Option<isize> {
    let ok: Vec<&TopWin> = wins.iter().filter(|w| tileable(w)).collect();
    ok.iter()
        .find(|w| !w.exe.is_empty() && contains_any(&w.exe, &spec.exe_words))
        .or_else(|| ok.iter().find(|w| contains_any(&w.title, &spec.title_words)))
        .map(|w| w.hwnd)
}

/// Walk from an agent's process (e.g. `node.exe` running claude) up through its parents to the first
/// process that owns a tileable top-level window, and return that window. `prefer_title` picks between
/// several windows of one process (Windows Terminal hosts all its windows in one process). The walk stops
/// at `explorer.exe`: a File Explorer window is never "the agent's window".
pub(crate) fn window_for_pid(pid: u32, procs: &[ProcSnap], wins: &[TopWin], prefer_title: &[String]) -> Option<isize> {
    let mut cur = pid;
    let mut seen: Vec<u32> = Vec::new();
    while cur != 0 && !seen.contains(&cur) && seen.len() < 64 {
        seen.push(cur);
        let owned: Vec<&TopWin> =
            wins.iter().filter(|w| w.pid == cur && tileable(w) && w.exe != "explorer.exe").collect();
        if !owned.is_empty() {
            let pick = owned.iter().find(|w| contains_any(&w.title, prefer_title)).unwrap_or(&owned[0]);
            return Some(pick.hwnd);
        }
        let p = procs.iter().find(|p| p.pid == cur)?;
        if p.name == "explorer.exe" {
            return None;
        }
        cur = p.ppid;
    }
    None
}

/// One tracked agent, as the tracker knows it. Either handle may be missing; the pid is tried first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTarget {
    pub name: String,
    pub pid: Option<u32>,
    pub spec: Option<AgentSpec>,
}

/// One window to tile and the agents that live in it (more than one = a shared window).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tile {
    pub hwnd: isize,
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Resolution {
    /// Distinct windows in the order their first agent was given (that order becomes left-to-right).
    pub tiles: Vec<Tile>,
    /// Agents whose window could not be found.
    pub missing: Vec<String>,
}

impl Tile {
    /// "Claude Code and Codex share one window, so it is tiled once." Empty for an unshared window.
    pub(crate) fn shared_line(&self) -> Option<String> {
        match self.agents.as_slice() {
            [] | [_] => None,
            [a, b] => Some(format!("{a} and {b} share one window, so I tiled it once.")),
            [rest @ .., last] => Some(format!("{} and {last} share one window, so I tiled it once.", rest.join(", "))),
        }
    }
}

/// Resolve every agent to a window and merge agents that resolve to the same window.
pub(crate) fn resolve_agents(agents: &[AgentTarget], procs: &[ProcSnap], wins: &[TopWin]) -> Resolution {
    let mut out = Resolution::default();
    for a in agents {
        let prefer: &[String] = a.spec.as_ref().map(|s| s.title_words.as_slice()).unwrap_or(&[]);
        let hwnd = a
            .pid
            .and_then(|p| window_for_pid(p, procs, wins, prefer))
            .or_else(|| a.spec.as_ref().and_then(|s| find_agent_window(s, wins)));
        match hwnd {
            None => out.missing.push(a.name.clone()),
            Some(h) => match out.tiles.iter_mut().find(|t| t.hwnd == h) {
                Some(t) => t.agents.push(a.name.clone()),
                None => out.tiles.push(Tile { hwnd: h, agents: vec![a.name.clone()] }),
            },
        }
    }
    out
}

// ─── Placing ─────────────────────────────────────────────────────────

/// What happened to one window; the caller can speak each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlaceResult {
    Placed,
    /// The window runs elevated and NEXUS does not: Windows would ignore the move, so it is not tried.
    SkippedElevated,
    /// The window closed (or its handle went stale) before it could be placed.
    SkippedGone,
    /// Anything else, with the reason ("the window would not shrink to the column", ...).
    Failed(String),
    /// This platform has no window adapter.
    Unsupported,
}

/// The machine side of placement; `RealTileEnv` is the Windows one, tests use a fake.
pub(crate) trait TileEnv {
    /// Work area (taskbar excluded) of the monitor to tile on.
    fn work_area(&self) -> Option<Rect>;
    fn exists(&self, hwnd: isize) -> bool;
    fn elevated(&self, hwnd: isize) -> bool;
    fn self_elevated(&self) -> bool;
    /// Restore, move and size one window so that its VISIBLE frame is exactly `rect`.
    fn place(&self, hwnd: isize, rect: Rect) -> PlaceResult;
}

/// Tile `hwnds` left to right in equal columns. A window that is gone or elevated is reported and takes no
/// column, so the rest still fill the whole width. Duplicate handles are placed once.
pub(crate) fn tile_windows(env: &dyn TileEnv, hwnds: &[isize]) -> Vec<(isize, PlaceResult)> {
    let mut order: Vec<isize> = Vec::new();
    for h in hwnds {
        if !order.contains(h) {
            order.push(*h);
        }
    }
    let self_elev = env.self_elevated();
    let mut results: Vec<(isize, Option<PlaceResult>)> = Vec::new();
    let mut live: Vec<isize> = Vec::new();
    for h in &order {
        if !env.exists(*h) {
            results.push((*h, Some(PlaceResult::SkippedGone)));
        } else if crate::live::commands::window::elevated_block_reason(env.elevated(*h), self_elev).is_some() {
            results.push((*h, Some(PlaceResult::SkippedElevated)));
        } else {
            live.push(*h);
            results.push((*h, None));
        }
    }
    let cols = env.work_area().map(|w| columns(w, live.len()));
    for (h, slot) in results.iter_mut() {
        if slot.is_some() {
            continue;
        }
        let idx = live.iter().position(|l| l == h).unwrap_or(0);
        *slot = Some(match cols.as_ref().and_then(|c| c.get(idx)) {
            Some(rect) => env.place(*h, *rect),
            None => PlaceResult::Failed("no usable work area".to_string()),
        });
    }
    results.into_iter().map(|(h, r)| (h, r.unwrap_or(PlaceResult::Unsupported))).collect()
}

// ─── Display off ─────────────────────────────────────────────────────

/// The user touched keyboard or mouse more recently than this: the screen stays on.
pub(crate) const MIN_IDLE_SECS: u64 = 60;

/// Why the display may not be blanked right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Why {
    /// The user used the keyboard or mouse less than `MIN_IDLE_SECS` ago.
    UserActive { idle_secs: u64 },
    /// A meeting is on (a dark screen on a shared call is not a harmless thing to do).
    Meeting,
    /// The idle time could not be read; not knowing counts as "the user may be here".
    IdleUnknown,
}

/// Pure guard. `user_idle_secs` comes from `memcore::resume::user_idle_secs()` (None = unreadable),
/// `meeting_active` from the meeting detector. Both conditions are checked every time; the meeting wins
/// the explanation because it holds however long the user has been away.
pub(crate) fn may_blank_display(user_idle_secs: Option<u64>, meeting_active: bool) -> Result<(), Why> {
    if meeting_active {
        return Err(Why::Meeting);
    }
    match user_idle_secs {
        None => Err(Why::IdleUnknown),
        Some(s) if s < MIN_IDLE_SECS => Err(Why::UserActive { idle_secs: s }),
        Some(_) => Ok(()),
    }
}

// ─── Windows adapter ─────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub(crate) struct RealTileEnv;

/// Every tileable top-level window now, topmost first, with its owning pid. Reuses the shutdown closer's
/// enumeration (`power_close::RealEnv::list`), which keeps its state on the stack and is thread-safe.
#[cfg(target_os = "windows")]
pub(crate) fn list_top_windows() -> Vec<TopWin> {
    use crate::power_close::{CloseEnv, RealEnv};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    RealEnv
        .list()
        .into_iter()
        .map(|w| {
            let mut pid = 0u32;
            // SAFETY: a stale handle just leaves pid at 0.
            unsafe {
                GetWindowThreadProcessId(HWND(w.hwnd), &mut pid);
            }
            TopWin { hwnd: w.hwnd, pid, exe: w.exe, class: w.class, title: w.title }
        })
        .filter(tileable)
        .collect()
}

#[cfg(target_os = "windows")]
fn to_rect(r: windows::Win32::Foundation::RECT) -> Rect {
    Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

#[cfg(target_os = "windows")]
impl TileEnv for RealTileEnv {
    /// Primary monitor's work area (taskbar excluded). Per-monitor tiling is a v2 option.
    fn work_area(&self) -> Option<Rect> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY};
        // SAFETY: `info` is a correctly sized MONITORINFO; a null window handle selects the primary monitor.
        unsafe {
            let mon = MonitorFromWindow(HWND(0), MONITOR_DEFAULTTOPRIMARY);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(mon, &mut info).as_bool() {
                Some(to_rect(info.rcWork))
            } else {
                None
            }
        }
    }

    fn exists(&self, hwnd: isize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::IsWindow;
        // SAFETY: IsWindow accepts any value.
        unsafe { IsWindow(HWND(hwnd)).as_bool() }
    }

    fn elevated(&self, hwnd: isize) -> bool {
        use windows::Win32::Foundation::HWND;
        crate::live::commands::window::window_process_elevated(HWND(hwnd))
    }

    fn self_elevated(&self) -> bool {
        crate::live::commands::window::our_process_elevated()
    }

    fn place(&self, hwnd: isize, rect: Rect) -> PlaceResult {
        use std::ffi::c_void;
        use windows::Win32::Foundation::{HWND, RECT};
        use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowRect, IsIconic, IsWindow, IsZoomed, SetWindowPos, ShowWindow, SW_RESTORE, SWP_NOACTIVATE,
            SWP_NOZORDER,
        };
        let h = HWND(hwnd);
        // SAFETY: every call takes a window handle that may be stale (they then fail, which is handled) and
        // RECTs that live on this stack frame.
        unsafe {
            if !IsWindow(h).as_bool() {
                return PlaceResult::SkippedGone;
            }
            // A minimised or maximised window ignores SetWindowPos until it is back to its normal state.
            if IsIconic(h).as_bool() || IsZoomed(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            let mut wr = RECT::default();
            if !GetWindowRect(h, &mut wr).as_bool() {
                return PlaceResult::SkippedGone;
            }
            let mut fb = RECT::default();
            let dwm_ok = DwmGetWindowAttribute(
                h,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut fb as *mut RECT as *mut c_void,
                std::mem::size_of::<RECT>() as u32,
            )
            .is_ok();
            // No DWM answer (classic theme, odd window): no border to compensate.
            let ask = if dwm_ok { compensate(rect, to_rect(wr), to_rect(fb)) } else { rect };
            if !SetWindowPos(h, HWND(0), ask.left, ask.top, ask.width(), ask.height(), SWP_NOACTIVATE | SWP_NOZORDER)
                .as_bool()
            {
                return PlaceResult::Failed("Windows refused to move the window".to_string());
            }
            // A window with a minimum size wider than the column silently keeps its own size: say so.
            let mut after = RECT::default();
            let seen = if dwm_ok {
                DwmGetWindowAttribute(
                    h,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut after as *mut RECT as *mut c_void,
                    std::mem::size_of::<RECT>() as u32,
                )
                .is_ok()
            } else {
                GetWindowRect(h, &mut after).as_bool()
            };
            if seen && !close_enough(rect, to_rect(after)) {
                return PlaceResult::Failed("the window would not fit the column (minimum size?)".to_string());
            }
            PlaceResult::Placed
        }
    }
}

/// Same rectangle within a few pixels (rounding at fractional DPI scales).
fn close_enough(want: Rect, got: Rect) -> bool {
    const TOL: i32 = 3;
    (want.left - got.left).abs() <= TOL
        && (want.top - got.top).abs() <= TOL
        && (want.right - got.right).abs() <= TOL
        && (want.bottom - got.bottom).abs() <= TOL
}

/// Turn the display off (it comes back on the next key press or mouse move). The message is sent with a
/// timeout and `SMTO_ABORTIFHUNG` to the desktop window: never a blocking `SendMessage(HWND_BROADCAST)`,
/// which can hang the caller on one unresponsive window. Callers must check `may_blank_display` first.
#[cfg(target_os = "windows")]
pub(crate) fn display_off() -> Result<(), String> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_NORMAL, WM_SYSCOMMAND,
    };
    /// SC_MONITORPOWER
    const SC_MONITORPOWER: usize = 0xF170;
    /// lParam 2 = power the monitor off (1 = low power, -1 = on)
    const MONITOR_OFF: isize = 2;
    // SAFETY: plain message send to the desktop window with a bounded wait.
    let ok = unsafe {
        let mut result = 0usize;
        SendMessageTimeoutW(
            GetDesktopWindow(),
            WM_SYSCOMMAND,
            WPARAM(SC_MONITORPOWER),
            LPARAM(MONITOR_OFF),
            SMTO_ABORTIFHUNG | SMTO_NORMAL,
            1000,
            &mut result,
        )
    };
    if ok.0 != 0 {
        Ok(())
    } else {
        Err("the desktop did not answer the display-off request".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) struct RealTileEnv;

#[cfg(not(target_os = "windows"))]
pub(crate) fn list_top_windows() -> Vec<TopWin> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
impl TileEnv for RealTileEnv {
    fn work_area(&self) -> Option<Rect> {
        None
    }
    fn exists(&self, _hwnd: isize) -> bool {
        true
    }
    fn elevated(&self, _hwnd: isize) -> bool {
        false
    }
    fn self_elevated(&self) -> bool {
        false
    }
    fn place(&self, _hwnd: isize, _rect: Rect) -> PlaceResult {
        PlaceResult::Unsupported
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn display_off() -> Result<(), String> {
    Err("unsupported on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn r(l: i32, t: i32, rr: i32, b: i32) -> Rect {
        Rect { left: l, top: t, right: rr, bottom: b }
    }

    // ── layout ──

    /// Every property a tiling must have, for one work area and column count.
    fn check_tiling(work: Rect, n: usize) {
        let cols = columns(work, n);
        assert_eq!(cols.len(), n);
        let mut x = work.left;
        for c in &cols {
            assert_eq!(c.left, x, "gap or overlap before {c:?} in {work:?} n={n}");
            assert!(c.width() > 0);
            assert_eq!((c.top, c.bottom), (work.top, work.bottom));
            x = c.right;
        }
        assert_eq!(x, work.right, "columns must end exactly at the right edge");
        let (min, max) = cols.iter().fold((i32::MAX, i32::MIN), |(lo, hi), c| (lo.min(c.width()), hi.max(c.width())));
        assert!(max - min <= 1, "widths differ by more than one pixel: {cols:?}");
        // the leftover pixels go to the leftmost columns
        let widths: Vec<i32> = cols.iter().map(Rect::width).collect();
        assert!(widths.windows(2).all(|p| p[0] >= p[1]), "wider columns must come first: {widths:?}");
    }

    #[test]
    fn columns_fill_the_area_for_one_to_six() {
        for n in 1..=6 {
            check_tiling(r(0, 0, 1920, 1040), n);
            check_tiling(r(0, 0, 1921, 1040), n); // odd
            check_tiling(r(0, 0, 1366, 728), n);
            check_tiling(r(0, 0, 2559, 1400), n);
        }
    }

    #[test]
    fn columns_with_negative_origin_monitor() {
        // a monitor to the left of and above the primary
        for n in 1..=6 {
            check_tiling(r(-1920, -200, 0, 840), n);
            check_tiling(r(-1367, -1, 0, 727), n);
        }
        let c = columns(r(-1920, -200, 0, 840), 2);
        assert_eq!(c[0], r(-1920, -200, -960, 840));
        assert_eq!(c[1], r(-960, -200, 0, 840));
    }

    #[test]
    fn columns_edge_cases() {
        let w = r(10, 20, 1930, 1060);
        assert!(columns(w, 0).is_empty());
        assert_eq!(columns(w, 1), vec![w]);
        assert!(columns(r(0, 0, 0, 100), 3).is_empty());
        // narrower than the column count: nothing is promised rather than zero-width columns
        assert!(columns(r(0, 0, 3, 100), 4).is_empty());
        assert_eq!(columns(r(0, 0, 4, 100), 4).len(), 4);
    }

    #[test]
    fn remainder_pixels_go_one_each_to_the_leftmost_columns() {
        // 1000 / 3 = 333 remainder 1 -> 334, 333, 333
        let w: Vec<i32> = columns(r(0, 0, 1000, 10), 3).iter().map(Rect::width).collect();
        assert_eq!(w, vec![334, 333, 333]);
        // 1000 / 6 = 166 remainder 4 -> four 167s then two 166s
        let w: Vec<i32> = columns(r(0, 0, 1000, 10), 6).iter().map(Rect::width).collect();
        assert_eq!(w, vec![167, 167, 167, 167, 166, 166]);
        assert_eq!(w.iter().sum::<i32>(), 1000);
    }

    #[test]
    fn compensate_grows_the_request_by_the_invisible_border() {
        // typical Win10/11: 7 px invisible border left/right/bottom, none on top
        let win = r(100, 100, 900, 700);
        let frame = r(107, 100, 893, 693);
        let ask = compensate(r(0, 0, 960, 1040), win, frame);
        assert_eq!(ask, r(-7, 0, 967, 1047));
        // after the window is placed at `ask`, its visible frame is the column again
        assert_eq!(ask.left + 7, 0);
        assert_eq!(ask.right - 7, 960);
        // no border: unchanged
        assert_eq!(compensate(r(0, 0, 10, 10), win, win), r(0, 0, 10, 10));
    }

    // ── display-off guard ──

    #[test]
    fn guard_truth_table() {
        assert_eq!(may_blank_display(Some(61), false), Ok(()));
        assert_eq!(may_blank_display(Some(60), false), Ok(())); // exactly 60 s away is enough
        assert_eq!(may_blank_display(Some(59), false), Err(Why::UserActive { idle_secs: 59 }));
        assert_eq!(may_blank_display(Some(0), false), Err(Why::UserActive { idle_secs: 0 }));
        assert_eq!(may_blank_display(Some(3600), true), Err(Why::Meeting)); // away for an hour, still a meeting
        assert_eq!(may_blank_display(Some(5), true), Err(Why::Meeting));
        assert_eq!(may_blank_display(None, false), Err(Why::IdleUnknown));
        assert_eq!(may_blank_display(None, true), Err(Why::Meeting));
    }

    // ── resolver ──

    fn ps(pid: u32, ppid: u32, name: &str) -> ProcSnap {
        ProcSnap { pid, ppid, name: name.into(), cmd: name.into(), cpu_pct: 0.0, io_total: 0 }
    }
    fn tw(hwnd: isize, pid: u32, exe: &str, class: &str, title: &str) -> TopWin {
        TopWin { hwnd, pid, exe: exe.into(), class: class.into(), title: title.into() }
    }
    fn spec(exe: &[&str], title: &[&str]) -> AgentSpec {
        AgentSpec {
            exe_words: exe.iter().map(|s| s.to_string()).collect(),
            title_words: title.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn world() -> (Vec<ProcSnap>, Vec<TopWin>) {
        let procs = vec![
            ps(1, 0, "explorer.exe"),
            ps(100, 1, "windowsterminal.exe"),
            ps(101, 100, "pwsh.exe"),
            ps(102, 101, "node.exe"), // claude
            ps(201, 100, "pwsh.exe"),
            ps(202, 201, "node.exe"), // codex, same terminal
            ps(300, 1, "antigravity.exe"),
            ps(400, 1, "code.exe"),
            ps(401, 400, "node.exe"), // an agent inside an IDE window
            ps(900, 1, "orphan.exe"), // no window anywhere above it
        ];
        let wins = vec![
            tw(500, 100, "windowsterminal.exe", "CASCADIA_HOSTING_WINDOW_CLASS", "claude - pwsh"),
            tw(600, 300, "antigravity.exe", "Chrome_WidgetWin_1", "NEXUS - Antigravity"),
            tw(700, 400, "code.exe", "Chrome_WidgetWin_1", "main.rs - Visual Studio Code"),
            tw(800, 1, "explorer.exe", "CabinetWClass", "Downloads"),
            tw(801, 1, "explorer.exe", "Shell_TrayWnd", "Taskbar"),
        ];
        (procs, wins)
    }

    #[test]
    fn pid_walk_reaches_the_terminal_window() {
        let (procs, wins) = world();
        assert_eq!(window_for_pid(102, &procs, &wins, &[]), Some(500));
        assert_eq!(window_for_pid(401, &procs, &wins, &[]), Some(700));
        assert_eq!(window_for_pid(300, &procs, &wins, &[]), Some(600)); // the process owns its own window
    }

    #[test]
    fn pid_walk_never_lands_on_explorer_or_loops() {
        let (procs, wins) = world();
        // orphan.exe -> explorer.exe: the Downloads folder window must NOT be taken
        assert_eq!(window_for_pid(900, &procs, &wins, &[]), None);
        // unknown pid
        assert_eq!(window_for_pid(5555, &procs, &wins, &[]), None);
        // a parent cycle terminates
        let cyc = vec![ps(1, 2, "a.exe"), ps(2, 1, "b.exe")];
        assert_eq!(window_for_pid(1, &cyc, &wins, &[]), None);
    }

    #[test]
    fn pid_walk_prefers_the_window_named_like_the_agent() {
        let (procs, mut wins) = world();
        wins.insert(0, tw(499, 100, "windowsterminal.exe", "CASCADIA_HOSTING_WINDOW_CLASS", "ssh server"));
        assert_eq!(window_for_pid(102, &procs, &wins, &[]), Some(499)); // topmost without a hint
        assert_eq!(window_for_pid(102, &procs, &wins, &["claude".to_string()]), Some(500));
    }

    #[test]
    fn name_match_prefers_exe_over_title_and_skips_shell_windows() {
        let (_, wins) = world();
        assert_eq!(find_agent_window(&spec(&["antigravity"], &[]), &wins), Some(600));
        assert_eq!(find_agent_window(&spec(&[], &["visual studio code"]), &wins), Some(700));
        // exe hit on a later window beats a title hit on an earlier one
        let mixed = vec![tw(1, 10, "brave.exe", "Chrome_WidgetWin_1", "Claude"), tw(2, 11, "claude.exe", "X", "whatever")];
        assert_eq!(find_agent_window(&spec(&["claude"], &["claude"]), &mixed), Some(2));
        // the taskbar and NEXUS itself are never candidates
        assert_eq!(find_agent_window(&spec(&["explorer"], &["taskbar"]), &wins), Some(800));
        let ours = vec![tw(3, 12, "nexus.exe", "X", "NEXUS")];
        assert_eq!(find_agent_window(&spec(&["nexus"], &["nexus"]), &ours), None);
        // no words = no match
        assert_eq!(find_agent_window(&AgentSpec::default(), &wins), None);
    }

    fn target(name: &str, pid: Option<u32>, s: Option<AgentSpec>) -> AgentTarget {
        AgentTarget { name: name.into(), pid, spec: s }
    }

    #[test]
    fn shared_window_is_deduped_and_reported() {
        let (procs, wins) = world();
        let agents = vec![
            target("Claude Code", Some(102), None),
            target("Antigravity", None, Some(spec(&["antigravity"], &[]))),
            target("Codex", Some(202), None), // same terminal as Claude Code
        ];
        let res = resolve_agents(&agents, &procs, &wins);
        assert_eq!(res.missing, Vec::<String>::new());
        assert_eq!(
            res.tiles,
            vec![
                Tile { hwnd: 500, agents: vec!["Claude Code".into(), "Codex".into()] },
                Tile { hwnd: 600, agents: vec!["Antigravity".into()] },
            ]
        );
        assert_eq!(res.tiles[0].shared_line().as_deref(), Some("Claude Code and Codex share one window, so I tiled it once."));
        assert_eq!(res.tiles[1].shared_line(), None);
    }

    #[test]
    fn missing_window_is_reported_not_tiled() {
        let (procs, wins) = world();
        let agents = vec![
            target("Aider", Some(900), Some(spec(&[], &["aider"]))), // pid walk fails, title finds nothing
            target("Ghost", None, None),
            target("Cursor", None, Some(spec(&["cursor"], &[]))),
        ];
        let res = resolve_agents(&agents, &procs, &wins);
        assert!(res.tiles.is_empty());
        assert_eq!(res.missing, vec!["Aider", "Ghost", "Cursor"]);
    }

    #[test]
    fn pid_that_fails_falls_back_to_the_name() {
        let (procs, wins) = world();
        let res = resolve_agents(&[target("Antigravity", Some(900), Some(spec(&["antigravity"], &[])))], &procs, &wins);
        assert_eq!(res.tiles, vec![Tile { hwnd: 600, agents: vec!["Antigravity".into()] }]);
    }

    #[test]
    fn three_way_share_line() {
        let t = Tile { hwnd: 1, agents: vec!["A".into(), "B".into(), "C".into()] };
        assert_eq!(t.shared_line().as_deref(), Some("A, B and C share one window, so I tiled it once."));
    }

    // ── tile_windows with a fake machine ──

    struct Fake {
        work: Option<Rect>,
        gone: Vec<isize>,
        elevated: Vec<isize>,
        self_elevated: bool,
        refuse: Vec<isize>,
        placed: RefCell<Vec<(isize, Rect)>>,
    }
    impl Fake {
        fn new() -> Self {
            Fake { work: Some(r(0, 0, 1000, 600)), gone: vec![], elevated: vec![], self_elevated: false, refuse: vec![], placed: RefCell::new(vec![]) }
        }
    }
    impl TileEnv for Fake {
        fn work_area(&self) -> Option<Rect> {
            self.work
        }
        fn exists(&self, h: isize) -> bool {
            !self.gone.contains(&h)
        }
        fn elevated(&self, h: isize) -> bool {
            self.elevated.contains(&h)
        }
        fn self_elevated(&self) -> bool {
            self.self_elevated
        }
        fn place(&self, h: isize, rect: Rect) -> PlaceResult {
            if self.refuse.contains(&h) {
                return PlaceResult::Failed("nope".into());
            }
            self.placed.borrow_mut().push((h, rect));
            PlaceResult::Placed
        }
    }

    #[test]
    fn tiles_in_order_across_the_whole_width() {
        let env = Fake::new();
        let out = tile_windows(&env, &[5, 6, 7]);
        assert!(out.iter().all(|(_, r)| *r == PlaceResult::Placed));
        let placed = env.placed.borrow();
        assert_eq!(placed[0], (5, r(0, 0, 334, 600)));
        assert_eq!(placed[1], (6, r(334, 0, 667, 600)));
        assert_eq!(placed[2], (7, r(667, 0, 1000, 600)));
    }

    #[test]
    fn elevated_and_gone_windows_are_reported_and_take_no_column() {
        let mut env = Fake::new();
        env.elevated = vec![6];
        env.gone = vec![8];
        let out = tile_windows(&env, &[5, 6, 7, 8]);
        assert_eq!(
            out,
            vec![
                (5, PlaceResult::Placed),
                (6, PlaceResult::SkippedElevated),
                (7, PlaceResult::Placed),
                (8, PlaceResult::SkippedGone),
            ]
        );
        let placed = env.placed.borrow();
        // two live windows share the full width; the elevated one was never touched
        assert_eq!(*placed, vec![(5, r(0, 0, 500, 600)), (7, r(500, 0, 1000, 600))]);
    }

    #[test]
    fn elevated_window_is_placed_when_nexus_is_elevated_too() {
        let mut env = Fake::new();
        env.elevated = vec![5];
        env.self_elevated = true;
        assert_eq!(tile_windows(&env, &[5]), vec![(5, PlaceResult::Placed)]);
    }

    #[test]
    fn duplicates_failures_and_missing_work_area() {
        let mut env = Fake::new();
        env.refuse = vec![6];
        let out = tile_windows(&env, &[5, 5, 6]);
        assert_eq!(out, vec![(5, PlaceResult::Placed), (6, PlaceResult::Failed("nope".into()))]);
        assert_eq!(env.placed.borrow().len(), 1);

        let mut none = Fake::new();
        none.work = None;
        let out = tile_windows(&none, &[5]);
        assert!(matches!(out[0].1, PlaceResult::Failed(_)));
        assert!(tile_windows(&Fake::new(), &[]).is_empty());
    }

    #[test]
    fn display_off_has_the_expected_shape_and_is_never_called_here() {
        // taken as a fn pointer only: no test may blank the machine running it
        let f: fn() -> Result<(), String> = display_off;
        let _ = f;
    }

    #[test]
    fn close_enough_tolerates_rounding_only() {
        assert!(close_enough(r(0, 0, 100, 100), r(1, -2, 102, 99)));
        assert!(!close_enough(r(0, 0, 100, 100), r(0, 0, 140, 100)));
    }

    // ── dev: tile two Notepad windows (run by hand; blanks nothing) ──

    /// Opens two Notepad windows, tiles them side by side on the primary monitor, prints what happened and
    /// closes the two it opened. Never calls `display_off`.
    ///   cargo test --lib --features custom-protocol,admin-brain -- window_tile::tests::dev_tile_two_notepads --ignored --nocapture
    #[test]
    #[ignore = "opens real Notepad windows; run by hand"]
    #[cfg(target_os = "windows")]
    fn dev_tile_two_notepads() {
        let known: Vec<isize> = list_top_windows().iter().filter(|w| w.exe == "notepad.exe").map(|w| w.hwnd).collect();
        for _ in 0..2 {
            std::process::Command::new("notepad.exe").spawn().expect("start notepad");
            std::thread::sleep(std::time::Duration::from_millis(700));
        }
        let mut mine: Vec<isize> = Vec::new();
        for _ in 0..20 {
            mine = list_top_windows()
                .iter()
                .filter(|w| w.exe == "notepad.exe" && !known.contains(&w.hwnd))
                .map(|w| w.hwnd)
                .collect();
            if mine.len() >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        println!("new notepad windows: {mine:?}");
        if mine.len() < 2 {
            println!("fewer than two new Notepad windows (new-tab behaviour?); nothing to tile");
        } else {
            let results = tile_windows(&RealTileEnv, &mine[..2]);
            for (h, res) in &results {
                println!("{h}: {res:?}");
            }
            std::thread::sleep(std::time::Duration::from_secs(6));
        }
        {
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
            for h in &mine {
                // SAFETY: posting WM_CLOSE to a window we opened.
                unsafe {
                    let _ = PostMessageW(HWND(*h), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}
