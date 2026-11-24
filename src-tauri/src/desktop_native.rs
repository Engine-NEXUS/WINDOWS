//! Windows accessibility observations. All callers run this blocking adapter off UI/audio threads.
use crate::desktop_control::*;
use crate::shortcuts::map::Action;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use uiautomation::patterns::{
    UIExpandCollapsePattern, UIInvokePattern, UIScrollPattern, UISelectionItemPattern,
    UITextPattern, UITogglePattern, UIValuePattern,
};
use uiautomation::types::{ControlType, Handle, TreeScope};
use uiautomation::{UIAutomation, UIElement};
use windows::Win32::Foundation::{CloseHandle, BOOL, FILETIME, HWND, LPARAM};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
};

pub fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn window_identity(hwnd: isize) -> Option<WindowIdentity> {
    if hwnd == 0 {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(HWND(hwnd), &mut pid) };
    let proc = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut created = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok =
        unsafe { GetProcessTimes(proc, &mut created, &mut exit, &mut kernel, &mut user) }.as_bool();
    let mut path = vec![0u16; 32768];
    let mut len = path.len() as u32;
    let named = unsafe {
        QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut len,
        )
    }
    .as_bool();
    unsafe { CloseHandle(proc) };
    if !ok || !named {
        return None;
    }
    let path = String::from_utf16_lossy(&path[..len as usize]);
    Some(WindowIdentity {
        hwnd,
        pid,
        created: ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
        exe: path.rsplit(['\\', '/']).next()?.to_lowercase(),
    })
}
pub fn foreground() -> Option<WindowIdentity> {
    window_identity(unsafe { GetForegroundWindow() }.0)
}
pub fn focus_executable(exe: &str, cancel: &AtomicBool) -> bool {
    unsafe extern "system" fn collect(hwnd: HWND, arg: LPARAM) -> BOOL {
        let mut title = [0u16; 2];
        if IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).0 == 0
            && GetWindowTextW(hwnd, &mut title) > 0
        {
            if let Some(w) = window_identity(hwnd.0) {
                let mut class = [0u16; 128];
                let n = GetClassNameW(hwnd, &mut class);
                let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
                if w.exe != "explorer.exe"
                    || matches!(class.as_str(), "CabinetWClass" | "ExploreWClass")
                {
                    (*(arg.0 as *mut Vec<WindowIdentity>)).push(w)
                }
            }
        }
        BOOL(1)
    }
    let start = Instant::now();
    while start.elapsed().as_millis() < FOCUS_MS as u128 {
        if cancel.load(Ordering::Relaxed) || crate::ghost::stop_requested() {
            return false;
        }
        let mut windows: Vec<WindowIdentity> = vec![];
        unsafe { EnumWindows(Some(collect), LPARAM(&mut windows as *mut _ as isize)) };
        let matches: Vec<_> = windows
            .into_iter()
            .filter(|w| w.exe.eq_ignore_ascii_case(exe))
            .collect();
        if matches.len() > 1 {
            return false;
        }
        if let Some(w) = matches.first() {
            if window_identity(w.hwnd).as_ref() != Some(w) || cancel.load(Ordering::Relaxed) {
                return false;
            }
            if check_identity(w).is_err() {
                return false;
            }
            if crate::live::commands::window::window_process_elevated(HWND(w.hwnd))
                && !crate::live::commands::window::our_process_elevated()
            {
                return false;
            }
            if !crate::live::commands::window::focus_window(HWND(w.hwnd)) {
                return false;
            }
            // Focus is requested once. Delayed activation is observed, never retried.
            while start.elapsed().as_millis() <= FOCUS_MS as u128 {
                if cancel.load(Ordering::Relaxed) || crate::ghost::stop_requested() {
                    return false;
                }
                if foreground().as_ref() == Some(w) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
            }
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
    }
    false
}
fn related_focus(expected: &WindowIdentity) -> bool {
    if window_identity(expected.hwnd).as_ref() != Some(expected) {
        return false;
    }
    let Some(fg) = foreground() else { return false };
    if fg == *expected {
        return true;
    }
    if fg.pid != expected.pid || fg.created != expected.created {
        return false;
    }
    let mut hwnd = HWND(fg.hwnd);
    for _ in 0..8 {
        hwnd = unsafe { GetWindow(hwnd, GW_OWNER) };
        if hwnd.0 == expected.hwnd {
            return true;
        }
        if hwnd.0 == 0 {
            break;
        }
    }
    false
}
pub fn check_identity(expected: &WindowIdentity) -> Result<(), String> {
    if window_identity(expected.hwnd).as_ref() != Some(expected) {
        return Err("The target application changed.".into());
    }
    let mut title_buf = [0u16; 1024];
    let count = unsafe { GetWindowTextW(HWND(expected.hwnd), &mut title_buf) };
    let title = String::from_utf16_lossy(&title_buf[..count.max(0) as usize]);
    if crate::shortcuts::exec::is_protected_window(&expected.exe, &title) {
        return Err("That window is protected.".into());
    }
    if crate::live::commands::window::window_process_elevated(HWND(expected.hwnd))
        && !crate::live::commands::window::our_process_elevated()
    {
        return Err("That app runs as administrator.".into());
    }
    Ok(())
}
pub fn check_window(expected: &WindowIdentity) -> Result<(), String> {
    check_identity(expected)?;
    if foreground().as_ref() != Some(expected) {
        return Err("The target window changed.".into());
    }
    Ok(())
}
fn elements(a: &UIAutomation, w: &WindowIdentity) -> Result<Vec<UIElement>, String> {
    let root = a
        .element_from_handle(Handle::from(w.hwnd))
        .map_err(|e| e.to_string())?;
    let list = root
        .find_all(
            TreeScope::Descendants,
            &a.create_true_condition().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    if list.len() > 1024 {
        return Err("This window exposes too many controls; narrow the requested panel.".into());
    }
    Ok(list)
}
fn role(el: &UIElement) -> String {
    el.get_control_type()
        .map(|v| format!("{v:?}").to_lowercase())
        .unwrap_or_default()
}
fn container(a: &UIAutomation, el: &UIElement) -> String {
    a.get_control_view_walker()
        .and_then(|w| w.get_parent(el))
        .and_then(|p| p.get_name())
        .unwrap_or_default()
}
fn permitted(name: &str) -> bool {
    !crate::ghost_center::is_risky_target(name) && !crate::ghost_center::is_ambiguous_target(name)
}
pub fn candidates() -> Result<Vec<Candidate>, String> {
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    let start = Instant::now();
    let window = foreground().ok_or("I can't identify the foreground app.")?;
    check_window(&window)?;
    let a = UIAutomation::new().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    for el in elements(&a, &window)? {
        if start.elapsed().as_millis() > VERIFY_MS as u128 {
            return Err("Control lookup timed out; nothing was clicked.".into());
        }
        let r = role(&el);
        if ![
            "button",
            "hyperlink",
            "tabitem",
            "menuitem",
            "listitem",
            "checkbox",
            "radiobutton",
            "splitbutton",
            "combobox",
            "edit",
        ]
        .contains(&r.as_str())
        {
            continue;
        }
        let name = el.get_name().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        out.push(Candidate {
            target: TargetSnapshot {
                window: window.clone(),
                runtime_id: el.get_runtime_id().unwrap_or_default(),
                automation_id: el.get_automation_id().unwrap_or_default(),
                generation,
            },
            label: name.clone(),
            role: r,
            container: container(&a, &el),
            enabled: el.is_enabled().unwrap_or(false),
            visible: !el.is_offscreen().unwrap_or(true),
            password: el.is_password().unwrap_or(true),
            permitted: permitted(&name),
        });
    }
    check_window(&window)?;
    Ok(out)
}
fn find(a: &UIAutomation, target: &TargetSnapshot) -> Result<UIElement, String> {
    let matches: Vec<_> = elements(a, &target.window)?
        .into_iter()
        .filter(|e| {
            e.get_runtime_id().is_ok_and(|id| id == target.runtime_id)
                && (target.automation_id.is_empty()
                    || e.get_automation_id()
                        .is_ok_and(|id| id == target.automation_id))
        })
        .collect();
    if matches.len() != 1 {
        return Err("The selected control changed; please ask again.".into());
    }
    Ok(matches[0].clone())
}
fn fill(state: &mut EffectState, el: &UIElement) {
    if el.is_password().unwrap_or(true) {
        return;
    }
    state.toggle = el
        .get_pattern::<UITogglePattern>()
        .ok()
        .and_then(|p| p.get_toggle_state().ok())
        .map(|v| v as i32);
    state.expanded = el
        .get_pattern::<UIExpandCollapsePattern>()
        .ok()
        .and_then(|p| p.get_state().ok())
        .map(|v| v as i32);
    state.selected = el
        .get_pattern::<UISelectionItemPattern>()
        .ok()
        .and_then(|p| p.is_selected().ok());
    state.value_hash = el
        .get_pattern::<UIValuePattern>()
        .ok()
        .and_then(|p| p.get_value().ok())
        .map(|v| hash(&v));
    state.scroll = el
        .get_pattern::<UIScrollPattern>()
        .ok()
        .and_then(|p| p.get_vertical_scroll_percent().ok())
        .filter(|v| *v >= 0.0);
    if state.scroll.is_some() {
        state.scroll_target = el.get_runtime_id().ok()
    }
}
pub fn snapshot(target: Option<&TargetSnapshot>) -> Option<EffectState> {
    let fg = foreground()?;
    let window = target
        .map(|t| t.window.clone())
        .unwrap_or_else(|| fg.clone());
    if !related_focus(&window) {
        return None;
    }
    let a = UIAutomation::new().ok()?;
    let mut state = EffectState::empty(window.clone());
    if let Ok(root) = a.element_from_handle(Handle::from(window.hwnd)) {
        if let Ok(name) = root.get_name() {
            state.surfaces.push(normalize(&name))
        }
    }
    let mut tabs = Vec::new();
    let mut groups = Vec::new();
    let mut dialogs = Vec::new();
    let mut list = elements(&a, &window).ok()?;
    if fg != window {
        let root = a.element_from_handle(Handle::from(fg.hwnd)).ok()?;
        state.surfaces.push(normalize(&root.get_name().ok()?));
        list.extend(elements(&a, &fg).ok()?)
    }
    let selected = if let Some(t) = target {
        find(&a, t).ok()
    } else {
        a.get_focused_element().ok()
    };
    if let Some(el) = selected.as_ref() {
        fill(&mut state, el);
        if state.scroll.is_none() {
            let walker = a.get_control_view_walker().ok()?;
            let mut parent = el.clone();
            for _ in 0..12 {
                parent = match walker.get_parent(&parent) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                if let Some(percent) = parent
                    .get_pattern::<UIScrollPattern>()
                    .ok()
                    .and_then(|p| p.get_vertical_scroll_percent().ok())
                    .filter(|v| *v >= 0.0)
                {
                    state.scroll = Some(percent);
                    state.scroll_target = parent.get_runtime_id().ok();
                    break;
                }
            }
        }
    }
    for el in list {
        if el.is_offscreen().unwrap_or(true) {
            continue;
        }
        if el
            .get_heading_level()
            .is_ok_and(|v| v != uiautomation::types::HeadingLevel::HeadingLevelNone)
        {
            if let Ok(name) = el.get_name() {
                state.surfaces.push(normalize(&name))
            }
        }
        match el.get_control_type().ok() {
            Some(ControlType::TabItem) => {
                let walker = a.get_control_view_walker().ok()?;
                let parent = walker.get_parent(&el).ok()?;
                let mut ancestor = parent.clone();
                let mut page_tab = false;
                for _ in 0..12 {
                    if ancestor.get_control_type().ok() == Some(ControlType::Document) {
                        page_tab = true;
                        break;
                    }
                    match walker.get_parent(&ancestor) {
                        Ok(p) => ancestor = p,
                        Err(_) => break,
                    }
                }
                if page_tab {
                    continue;
                }
                groups.push(parent.get_runtime_id().ok()?);
                let id = el.get_runtime_id().ok()?;
                if el
                    .get_pattern::<UISelectionItemPattern>()
                    .ok()
                    .and_then(|p| p.is_selected().ok())
                    == Some(true)
                {
                    state.selected_tab = Some(id.clone())
                }
                tabs.push(id)
            }
            Some(ControlType::Document) => {
                if let Ok(name) = el.get_name() {
                    state.surfaces.push(normalize(&name))
                }
                if state.document_hash.is_none() {
                    if let Ok(p) = el.get_pattern::<UIValuePattern>() {
                        if let Ok(value) = p.get_value() {
                            if !value.is_empty() {
                                state.document_hash = Some(hash(&value))
                            }
                        }
                    }
                }
            }
            Some(ControlType::Window) | Some(ControlType::Menu) => {
                if let Ok(id) = el.get_runtime_id() {
                    dialogs.push(id)
                }
                if let Ok(name) = el.get_name() {
                    state.surfaces.push(normalize(&name))
                }
            }
            Some(ControlType::Edit) => {
                if el.get_automation_id().ok().as_deref() == Some("addressEditBox") {
                    state.document_hash = el
                        .get_pattern::<UIValuePattern>()
                        .ok()
                        .and_then(|p| p.get_value().ok())
                        .map(|v| hash(&v))
                }
            }
            _ => {}
        }
    }
    if !tabs.is_empty() && groups.iter().all(|g| Some(g) == groups.first()) {
        state.tab_ids = Some(tabs)
    } else {
        state.selected_tab = None
    }
    state.dialog_ids = Some(dialogs);
    Some(state)
}
pub fn shortcut_snapshot(_action: Action) -> Option<EffectState> {
    let w = foreground()?;
    check_window(&w).ok()?;
    snapshot(None)
}
fn readable_text(el: &UIElement) -> Option<String> {
    if el.is_password().unwrap_or(true) {
        return None;
    }
    let text = el
        .get_pattern::<UIValuePattern>()
        .ok()
        .and_then(|p| p.get_value().ok())
        .or_else(|| {
            el.get_pattern::<UITextPattern>()
                .ok()
                .and_then(|p| p.get_document_range().ok())
                .and_then(|r| r.get_text(65_537).ok())
        })?;
    (text.chars().count() <= 65_536).then_some(text)
}
pub fn insert_text_checked(
    text: &str,
    send: impl FnOnce() -> Result<(), String>,
) -> Result<bool, String> {
    let lookup_start = Instant::now();
    let window = foreground().ok_or("No target window.")?;
    check_window(&window)?;
    let a = UIAutomation::new().map_err(|e| e.to_string())?;
    let el = a.get_focused_element().map_err(|e| e.to_string())?;
    if el.is_password().unwrap_or(true)
        || !el.is_enabled().unwrap_or(false)
        || el.is_offscreen().unwrap_or(true)
    {
        return Err("The focused field is protected or unavailable.".into());
    }
    let id = el.get_runtime_id().map_err(|e| e.to_string())?;
    let before = readable_text(&el);
    let selection = el
        .get_pattern::<UITextPattern>()
        .ok()
        .and_then(|p| p.get_selection().ok())
        .filter(|v| v.len() == 1)
        .and_then(|v| v[0].get_text(65_536).ok());
    check_window(&window)?;
    if crate::ghost::stop_requested() && crate::ghost::session_active() {
        return Err("Stopped before typing.".into());
    }
    if a.get_focused_element()
        .and_then(|e| e.get_runtime_id())
        .map_err(|e| e.to_string())?
        != id
    {
        return Err("The target field changed before typing.".into());
    }
    if lookup_start.elapsed().as_millis() > VERIFY_MS as u128 {
        return Err("Text target observation timed out before typing.".into());
    }
    send()?;
    let start = Instant::now();
    loop {
        if foreground().as_ref() != Some(&window)
            || crate::ghost::session_active() && crate::ghost::stop_requested()
        {
            return Ok(false);
        }
        let current = a.get_focused_element().map_err(|e| e.to_string())?;
        if current.get_runtime_id().map_err(|e| e.to_string())? != id {
            return Ok(false);
        }
        if let (Some(before), Some(after)) = (&before, readable_text(&current)) {
            if start.elapsed().as_millis() <= VERIFY_MS as u128
                && insertion_matches(before, &after, text, selection.as_deref())
                && foreground().as_ref() == Some(&window)
                && !(crate::ghost::session_active() && crate::ghost::stop_requested())
                && a.get_focused_element()
                    .and_then(|e| e.get_runtime_id())
                    .is_ok_and(|current_id| current_id == id)
            {
                return Ok(true);
            }
        } else {
            return Ok(false);
        }
        if start.elapsed().as_millis() >= VERIFY_MS as u128 {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
    }
}
pub struct NativeObserver<'a> {
    pub target: Option<&'a TargetSnapshot>,
    pub cancel: &'a AtomicBool,
    pub start: Instant,
}
impl Observer for NativeObserver<'_> {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms))
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
            || crate::ghost::stop_requested()
            || !crate::ghost::session_active()
    }
    fn current_window(&self) -> Option<WindowIdentity> {
        if let Some(t) = self.target {
            if related_focus(&t.window) {
                return Some(t.window.clone());
            }
        }
        foreground()
    }
    fn observe(&self) -> Option<EffectState> {
        snapshot(self.target)
    }
}
pub fn perform(
    candidate: &Candidate,
    cancel: &AtomicBool,
) -> Result<(crate::ghost_center::Route, Verification), String> {
    let start = Instant::now();
    check_window(&candidate.target.window)?;
    let a = UIAutomation::new().map_err(|e| e.to_string())?;
    let el = find(&a, &candidate.target)?;
    if !candidate.eligible()
        || !el.is_enabled().unwrap_or(false)
        || el.is_offscreen().unwrap_or(true)
        || el.is_password().unwrap_or(true)
        || !el
            .get_name()
            .is_ok_and(|n| normalize(&n) == normalize(&candidate.label))
        || role(&el) != candidate.role
        || container(&a, &el) != candidate.container
        || !permitted(&candidate.label)
    {
        return Err("The target is unavailable or protected.".into());
    }
    let before = snapshot(Some(&candidate.target)).ok_or("I can't observe that control safely.")?;
    let guard = || -> Result<(), String> {
        if !el.is_enabled().unwrap_or(false)
            || el.is_offscreen().unwrap_or(true)
            || el.is_password().unwrap_or(true)
            || !el
                .get_name()
                .is_ok_and(|n| normalize(&n) == normalize(&candidate.label))
            || role(&el) != candidate.role
            || container(&a, &el) != candidate.container
        {
            return Err("The target changed before dispatch.".into());
        }
        check_window(&candidate.target.window)?;
        if start.elapsed().as_millis() > VERIFY_MS as u128
            || cancel.load(Ordering::Relaxed)
            || crate::ghost::stop_requested()
            || !crate::ghost::session_active()
        {
            return Err("Stopped before dispatch.".into());
        }
        Ok(())
    };
    guard()?;
    let (route, effect) = if let Ok(p) = el.get_pattern::<UITogglePattern>() {
        let old = p.get_toggle_state().map_err(|e| e.to_string())? as i32;
        guard()?;
        p.toggle().map_err(|e| e.to_string())?;
        (
            crate::ghost_center::Route::AccessibilityCenter,
            if old == 0 {
                EffectSpec::Toggle(1)
            } else if old == 1 {
                EffectSpec::Toggle(0)
            } else {
                EffectSpec::Unsupported
            },
        )
    } else if let Ok(p) = el.get_pattern::<UISelectionItemPattern>() {
        guard()?;
        p.select().map_err(|e| e.to_string())?;
        (
            crate::ghost_center::Route::AccessibilityCenter,
            EffectSpec::Select,
        )
    } else if let Ok(p) = el.get_pattern::<UIExpandCollapsePattern>() {
        let state = p.get_state().map_err(|e| e.to_string())? as i32;
        if state != 1 {
            guard()?;
            p.expand().map_err(|e| e.to_string())?;
        }
        (
            crate::ghost_center::Route::AccessibilityCenter,
            EffectSpec::Expand(1),
        )
    } else if let Ok(p) = el.get_pattern::<UIInvokePattern>() {
        guard()?;
        p.invoke().map_err(|e| e.to_string())?;
        (
            crate::ghost_center::Route::AccessibilityCenter,
            EffectSpec::DialogOpened(candidate.label.clone()),
        )
    } else {
        let point = el
            .get_clickable_point()
            .map_err(|e| e.to_string())?
            .ok_or("No safe clickable point.")?;
        let checked = || {
            guard()?;
            let hit = a.element_from_point(point).map_err(|e| e.to_string())?;
            if hit.get_runtime_id().map_err(|e| e.to_string())? != candidate.target.runtime_id {
                return Err("Another control covers the target.".into());
            }
            Ok(())
        };
        checked()?;
        crate::live::commands::mouse::click_at_checked(
            point.get_x(),
            point.get_y(),
            || cancel.load(Ordering::Relaxed) || crate::ghost::stop_requested(),
            checked,
        )?;
        (
            crate::ghost_center::Route::CursorCenter,
            EffectSpec::DialogOpened(candidate.label.clone()),
        )
    };
    let env = NativeObserver {
        target: Some(&candidate.target),
        cancel,
        start: Instant::now(),
    };
    Ok((route, verify(&env, &before, &effect)))
}
