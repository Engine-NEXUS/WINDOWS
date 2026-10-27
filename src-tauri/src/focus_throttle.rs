//! Focus Throttle: lowers CPU scheduling priority and efficiency class of other
//! heavy user applications while Hand Mode runs, and restores them exactly.
//!
//! When Hand Mode is engaged, NEXUS raises its own priority to `ABOVE_NORMAL_PRIORITY_CLASS`
//! and lowers non-foreground heavy background processes (> 15% CPU) to
//! `BELOW_NORMAL_PRIORITY_CLASS` + EcoQoS (Efficiency Mode).
//!
//! All modified processes are restored to their exact original priority class
//! and power throttling state when Hand Mode disengages or upon app exit.
//! If NEXUS or the OS crashes during Hand Mode, the durable baseline persisted
//! in `focus_baseline.json` is inspected and restored at the next startup.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// Process sample representing an active process at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcSample {
    pub pid: u32,
    pub parent: u32,
    pub name: String, // exe file name, lowercase
    pub cpu_pct: f32,
    pub foreground: bool,
    pub elevated: bool,
    pub same_user: bool,
}

/// Throttle selection configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub cpu_threshold: f32, // default 15.0
    pub max_targets: usize, // default 6
    pub nexus_pid: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cpu_threshold: 15.0,
            max_targets: 6,
            nexus_pid: std::process::id(),
        }
    }
}

/// Check if an executable name is in the protected/system/media/tool blocklist.
pub fn is_blocked_name(raw_name: &str) -> bool {
    let name = raw_name.to_lowercase();
    let stripped = name.strip_suffix(".exe").unwrap_or(&name);

    // Exact matches: system/shell, security software, media/call apps, nexus tools
    const EXACT_BLOCKLIST: &[&str] = &[
        // system and shell
        "system", "idle", "registry", "smss", "csrss", "wininit", "winlogon",
        "services", "lsass", "svchost", "dwm", "explorer", "audiodg", "ctfmon",
        "fontdrvhost", "searchhost", "sihost", "taskhostw", "runtimebroker",
        // security software
        "msmpeng", "mpdefendernetworkservice", "securityhealthservice",
        // call, meeting, capture and media apps
        "zoom", "teams", "ms-teams", "webex", "skype", "discord",
        "obs64", "obs32", "spotify", "vlc", "wmplayer", "music.ui",
        // tools that drive NEXUS itself
        "claude", "agy",
    ];

    for &blocked in EXACT_BLOCKLIST {
        if stripped == blocked || name == blocked {
            return true;
        }
    }

    // Security software substring matches
    const SECURITY_SUBSTRINGS: &[&str] = &[
        "defender", "avast", "kaspersky", "norton", "mcafee",
        "bitdefender", "malwarebytes", "crowdstrike", "sentinel",
    ];

    for &sub in SECURITY_SUBSTRINGS {
        if name.contains(sub) {
            return true;
        }
    }

    false
}

/// Pure decision function: determines which processes to lower.
pub fn decide(samples: &[ProcSample], cfg: &Config, meeting_active: bool) -> Vec<u32> {
    if meeting_active {
        return Vec::new();
    }

    // Identify foreground process family (foreground process and all its descendants)
    let mut fg_family: HashSet<u32> = samples
        .iter()
        .filter(|s| s.foreground)
        .map(|s| s.pid)
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for s in samples {
            if !fg_family.contains(&s.pid) && fg_family.contains(&s.parent) {
                fg_family.insert(s.pid);
                changed = true;
            }
        }
    }

    // Identify NEXUS family (NEXUS itself and all its descendants, e.g. WebView2)
    let mut nexus_family: HashSet<u32> = HashSet::new();
    if cfg.nexus_pid != 0 {
        nexus_family.insert(cfg.nexus_pid);
        let mut changed_nexus = true;
        while changed_nexus {
            changed_nexus = false;
            for s in samples {
                if !nexus_family.contains(&s.pid) && nexus_family.contains(&s.parent) {
                    nexus_family.insert(s.pid);
                    changed_nexus = true;
                }
            }
        }
    }

    // Filter candidates according to all rules
    let mut candidates: Vec<&ProcSample> = samples
        .iter()
        .filter(|s| {
            !fg_family.contains(&s.pid)
                && !nexus_family.contains(&s.pid)
                && !s.elevated
                && s.same_user
                && s.cpu_pct >= cfg.cpu_threshold
                && !is_blocked_name(&s.name)
        })
        .collect();

    // Pick highest CPU first (tie-break by PID for determinism)
    candidates.sort_by(|a, b| {
        b.cpu_pct
            .partial_cmp(&a.cpu_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.pid.cmp(&b.pid))
    });

    candidates
        .into_iter()
        .take(cfg.max_targets)
        .map(|s| s.pid)
        .collect()
}

/// Record of an individual lowered process and its original state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredProc {
    pub pid: u32,
    pub exe: String,
    pub created: u64, // process creation FILETIME as u64, guards against pid reuse
    pub orig_priority: u32,
    pub orig_throttle_control: u32,
    pub orig_throttle_state: u32,
}

/// Durable baseline tracking state before throttling was applied.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Baseline {
    pub nexus: Option<LoweredProc>,
    pub others: Vec<LoweredProc>,
}

/// Compute the exact list of processes to restore.
///
/// Skips dead processes and processes whose PID was reused (creation time or exe mismatch).
/// Restores in reverse of lowering order (last lowered restored first, NEXUS restored last).
pub fn plan_restore(
    baseline: &Baseline,
    running: &[(u32, String, u64)],
) -> Vec<LoweredProc> {
    let mut out = Vec::new();

    // Restore 'others' in reverse order of lowering
    for item in baseline.others.iter().rev() {
        if running.iter().any(|(r_pid, r_exe, r_created)| {
            *r_pid == item.pid
                && r_exe.eq_ignore_ascii_case(&item.exe)
                && *r_created == item.created
        }) {
            out.push(item.clone());
        }
    }

    // Restore NEXUS afterwards (reverse of raising NEXUS before lowering others)
    if let Some(nexus) = &baseline.nexus {
        if running.iter().any(|(r_pid, r_exe, r_created)| {
            *r_pid == nexus.pid
                && r_exe.eq_ignore_ascii_case(&nexus.exe)
                && *r_created == nexus.created
        }) {
            out.push(nexus.clone());
        }
    }

    out
}

pub const BASELINE_FILE: &str = "focus_baseline.json";

/// Read the baseline from the specified directory.
/// Returns None if the file does not exist.
/// If the file is corrupt, returns an empty Baseline without panicking.
pub fn read_baseline(dir: &Path) -> Option<Baseline> {
    let path = dir.join(BASELINE_FILE);
    if !path.exists() {
        return None;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Some(Baseline::default()),
    };
    Some(serde_json::from_str::<Baseline>(&text).unwrap_or_default())
}

/// Write the baseline atomically via a temporary file and rename.
pub fn write_baseline(dir: &Path, baseline: &Baseline) -> bool {
    let path = dir.join(BASELINE_FILE);
    let tmp_path = dir.join(format!("{}.tmp.{}", BASELINE_FILE, std::process::id()));
    let Ok(json) = serde_json::to_string_pretty(baseline) else {
        return false;
    };
    if std::fs::write(&tmp_path, json).is_err() {
        return false;
    }
    if std::fs::rename(&tmp_path, &path).is_err() {
        let _ = std::fs::remove_file(&tmp_path);
        return false;
    }
    true
}

/// Clear/remove the baseline file from disk.
pub fn clear_baseline(dir: &Path) {
    let path = dir.join(BASELINE_FILE);
    let _ = std::fs::remove_file(path);
}

// ── Windows Glue ─────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
struct AutoHandle(windows::Win32::Foundation::HANDLE);

#[cfg(target_os = "windows")]
impl Drop for AutoHandle {
    fn drop(&mut self) {
        if self.0 .0 != 0 && self.0 .0 != -1 {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn process_token_elevated(proc: windows::Win32::Foundation::HANDLE) -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::OpenProcessToken;
    unsafe {
        let mut token: HANDLE = HANDLE(0);
        if !OpenProcessToken(proc, TOKEN_QUERY, &mut token).as_bool() {
            return false;
        }
        let mut elev: TOKEN_ELEVATION = std::mem::zeroed();
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
        .as_bool();
        let _ = CloseHandle(token);
        ok && elev.TokenIsElevated != 0
    }
}

#[cfg(target_os = "windows")]
pub fn get_proc_identity(pid: u32) -> Option<(String, u64)> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    if pid == 0 {
        return None;
    }
    let Ok(proc) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return None;
    };
    let _handle = AutoHandle(proc);

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe { GetProcessTimes(proc, &mut created, &mut exited, &mut kernel, &mut user) };
    if !ok.as_bool() {
        return None;
    }
    let created_u64 = ((created.dwHighDateTime as u64) << 32) | (created.dwLowDateTime as u64);

    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let exe = if unsafe {
        QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .as_bool()
        && len > 0
    {
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        Path::new(&full)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default()
    } else {
        String::new()
    };

    Some((exe, created_u64))
}

#[cfg(target_os = "windows")]
pub fn lower(pid: u32) -> Option<LoweredProc> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{
        GetPriorityClass, GetProcessInformation, GetProcessTimes, OpenProcess,
        QueryFullProcessImageNameW, SetPriorityClass, SetProcessInformation,
        BELOW_NORMAL_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, ProcessPowerThrottling, PROCESS_NAME_WIN32,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    };

    if pid == 0 || pid == std::process::id() {
        return None;
    }

    let Ok(proc) = (unsafe {
        OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        )
    }) else {
        return None;
    };
    let _handle = AutoHandle(proc);

    // Skip elevated processes
    if process_token_elevated(proc) {
        return None;
    }

    // Read creation time
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe { GetProcessTimes(proc, &mut created, &mut exited, &mut kernel, &mut user) };
    if !ok.as_bool() {
        return None;
    }
    let created_u64 = ((created.dwHighDateTime as u64) << 32) | (created.dwLowDateTime as u64);

    // Read exe name
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let exe = if unsafe {
        QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .as_bool()
        && len > 0
    {
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        Path::new(&full)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default()
    } else {
        return None;
    };

    // Read original priority class
    let orig_priority = unsafe { GetPriorityClass(proc) };
    if orig_priority == 0 {
        return None;
    }
    // Already at or below the target class: lowering would RAISE it, and restore must never
    // change something we did not need to touch.
    if orig_priority == IDLE_PRIORITY_CLASS.0 || orig_priority == BELOW_NORMAL_PRIORITY_CLASS.0 {
        return None;
    }

    // Read original throttle control/state
    let mut throttling = PROCESS_POWER_THROTTLING_STATE {
        Version: 1,
        ControlMask: 0,
        StateMask: 0,
    };
    let ok = unsafe {
        GetProcessInformation(
            proc,
            ProcessPowerThrottling,
            &mut throttling as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    let (orig_throttle_control, orig_throttle_state) = if ok.as_bool() {
        (throttling.ControlMask, throttling.StateMask)
    } else {
        (0, 0)
    };

    // Lower priority class to BELOW_NORMAL_PRIORITY_CLASS
    let ok_pri = unsafe { SetPriorityClass(proc, BELOW_NORMAL_PRIORITY_CLASS) };
    if !ok_pri.as_bool() {
        return None;
    }

    // Set EcoQoS (execution speed throttling)
    let eco = PROCESS_POWER_THROTTLING_STATE {
        Version: 1,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    };
    unsafe {
        SetProcessInformation(
            proc,
            ProcessPowerThrottling,
            &eco as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
    }

    Some(LoweredProc {
        pid,
        exe,
        created: created_u64,
        orig_priority,
        orig_throttle_control,
        orig_throttle_state,
    })
}

#[cfg(target_os = "windows")]
pub fn restore(lowered: &LoweredProc) -> bool {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetProcessTimes, OpenProcess, SetPriorityClass, SetProcessInformation,
        ProcessPowerThrottling, PROCESS_CREATION_FLAGS, PROCESS_POWER_THROTTLING_STATE,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    };

    if lowered.pid == 0 {
        return false;
    }

    // NEXUS self-restore
    if lowered.pid == std::process::id() {
        if lowered.orig_priority != 0 {
            unsafe {
                SetPriorityClass(
                    GetCurrentProcess(),
                    PROCESS_CREATION_FLAGS(lowered.orig_priority),
                );
            }
        }
        return true;
    }

    let Ok(proc) = (unsafe {
        OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            lowered.pid,
        )
    }) else {
        return false;
    };
    let _handle = AutoHandle(proc);

    // Verify creation time to guard against PID reuse
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe { GetProcessTimes(proc, &mut created, &mut exited, &mut kernel, &mut user) };
    if !ok.as_bool() {
        return false;
    }
    let created_u64 = ((created.dwHighDateTime as u64) << 32) | (created.dwLowDateTime as u64);
    if created_u64 != lowered.created {
        return false; // PID reused by another process
    }

    // Restore priority class
    if lowered.orig_priority != 0 {
        unsafe {
            SetPriorityClass(proc, PROCESS_CREATION_FLAGS(lowered.orig_priority));
        }
    }

    // Restore throttle control/state (control 0 / state 0 returns to system-managed)
    let throttling = PROCESS_POWER_THROTTLING_STATE {
        Version: 1,
        ControlMask: lowered.orig_throttle_control,
        StateMask: lowered.orig_throttle_state,
    };
    unsafe {
        SetProcessInformation(
            proc,
            ProcessPowerThrottling,
            &throttling as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
    }

    true
}

#[cfg(target_os = "windows")]
pub fn raise_nexus() -> Option<LoweredProc> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetPriorityClass, GetProcessTimes, SetPriorityClass,
        ABOVE_NORMAL_PRIORITY_CLASS,
    };

    let pid = std::process::id();
    let proc = unsafe { GetCurrentProcess() };
    let orig_priority = unsafe { GetPriorityClass(proc) };
    if orig_priority == 0 {
        return None;
    }

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe { GetProcessTimes(proc, &mut created, &mut exited, &mut kernel, &mut user) };
    let created_u64 = if ok.as_bool() {
        ((created.dwHighDateTime as u64) << 32) | (created.dwLowDateTime as u64)
    } else {
        0
    };

    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| "nexus.exe".into());

    let ok = unsafe { SetPriorityClass(proc, ABOVE_NORMAL_PRIORITY_CLASS) };
    if !ok.as_bool() {
        return None;
    }

    Some(LoweredProc {
        pid,
        exe,
        created: created_u64,
        orig_priority,
        orig_throttle_control: 0,
        orig_throttle_state: 0,
    })
}

#[cfg(target_os = "windows")]
pub fn snapshot_running_for_baseline(baseline: &Baseline) -> Vec<(u32, String, u64)> {
    let mut pids = Vec::new();
    if let Some(nexus) = &baseline.nexus {
        pids.push(nexus.pid);
    }
    for item in &baseline.others {
        pids.push(item.pid);
    }
    pids.sort_unstable();
    pids.dedup();

    let mut running = Vec::new();
    for pid in pids {
        if let Some((exe, created)) = get_proc_identity(pid) {
            running.push((pid, exe, created));
        }
    }
    running
}

#[cfg(target_os = "windows")]
pub fn sample_processes() -> Vec<ProcSample> {
    use sysinfo::{ProcessesToUpdate, System};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    std::thread::sleep(std::time::Duration::from_millis(1000));
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let fg_pid = unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 != 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid as *mut u32);
            pid
        } else {
            0
        }
    };

    let my_pid = sysinfo::Pid::from_u32(std::process::id());
    let my_uid = sys.process(my_pid).and_then(|p| p.user_id()).cloned();

    let mut samples = Vec::new();
    for (pid, proc) in sys.processes() {
        let pid_u32 = pid.as_u32();
        let parent = proc.parent().map(|p| p.as_u32()).unwrap_or(0);
        let name = proc.name().to_string_lossy().to_lowercase();
        let cpu_pct = proc.cpu_usage();
        let foreground = pid_u32 != 0 && pid_u32 == fg_pid;

        let same_user = match (&my_uid, proc.user_id()) {
            (Some(my), Some(their)) => my == their,
            _ => false,
        };

        let elevated = if pid_u32 == 0 {
            true
        } else {
            match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid_u32) } {
                Ok(h) => {
                    let is_elevated = process_token_elevated(h);
                    unsafe { CloseHandle(h) };
                    is_elevated
                }
                Err(_) => true,
            }
        };

        samples.push(ProcSample {
            pid: pid_u32,
            parent,
            name,
            cpu_pct,
            foreground,
            elevated,
            same_user,
        });
    }

    samples
}

// ── Non-Windows Fallbacks ────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
pub fn lower(_pid: u32) -> Option<LoweredProc> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn restore(_lowered: &LoweredProc) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn raise_nexus() -> Option<LoweredProc> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn sample_processes() -> Vec<ProcSample> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
pub fn snapshot_running_for_baseline(_baseline: &Baseline) -> Vec<(u32, String, u64)> {
    Vec::new()
}

// ── Global State and Engine Loop ─────────────────────────────────────────────

static ACTIVE_BASELINE: Mutex<Option<Baseline>> = Mutex::new(None);
static BASELINE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn read_setting_bool<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    key: &str,
    default: bool,
) -> bool {
    let Ok(dir) = app.path().app_data_dir() else {
        return default;
    };
    let Ok(text) = std::fs::read_to_string(dir.join("settings.json")) else {
        return default;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return default;
    };
    json.get(key)
        .or_else(|| json.get("hand_focus_lower_apps"))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn is_meeting_active<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    app.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
        .map(|m| m.is_meeting_active())
        .unwrap_or(false)
}

/// Startup crash recovery: if a baseline file exists from a previous crash/kill,
/// restore all still-running processes recorded in it and remove the file.
pub fn recover_on_start(dir: &Path) {
    *BASELINE_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.to_path_buf());
    let path = dir.join(BASELINE_FILE);
    if !path.exists() {
        return;
    }
    if let Some(baseline) = read_baseline(dir) {
        let running = snapshot_running_for_baseline(&baseline);
        let to_restore = plan_restore(&baseline, &running);
        let count = to_restore
            .iter()
            .filter(|p| Some(p.pid) != baseline.nexus.as_ref().map(|n| n.pid))
            .count();
        for item in to_restore {
            restore(&item);
        }
        if count > 0 {
            println!("[FOCUS] recovered {} apps from previous run", count);
        }
    }
    clear_baseline(dir);
}

/// Synchronous restore called at app exit (RunEvent::Exit).
pub fn restore_now() {
    let dir = BASELINE_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .or_else(crate::app_dirs::nexus_data_dir);
    let Some(dir) = dir else {
        return;
    };

    let baseline = ACTIVE_BASELINE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .or_else(|| read_baseline(&dir));

    let Some(baseline) = baseline else {
        clear_baseline(&dir);
        return;
    };

    let running = snapshot_running_for_baseline(&baseline);
    let to_restore = plan_restore(&baseline, &running);
    let count = to_restore
        .iter()
        .filter(|p| Some(p.pid) != baseline.nexus.as_ref().map(|n| n.pid))
        .count();

    for item in to_restore {
        restore(&item);
    }

    clear_baseline(&dir);
    if count > 0 {
        println!("[FOCUS] restored {} apps", count);
    }
}

/// Spawn the focus throttle ticker thread.
pub fn spawn<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    *BASELINE_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.clone());
    recover_on_start(&dir);

    let app_handle = app.clone();
    std::thread::Builder::new()
        .name("focus-throttle".into())
        .spawn(move || {
            let mut engaged = false;
            let mut baseline = Baseline::default();
            let mut last_resample = std::time::Instant::now();

            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));

                let paused = crate::focus_mode::paused();
                let flag = read_setting_bool(&app_handle, "handFocusLowerApps", true);

                if paused && flag && !engaged {
                    engaged = true;
                    baseline = Baseline::default();

                    // Durable file written BEFORE any changes
                    write_baseline(&dir, &baseline);

                    // Raise NEXUS priority
                    if let Some(nexus_lowered) = raise_nexus() {
                        baseline.nexus = Some(nexus_lowered);
                        write_baseline(&dir, &baseline);
                    }

                    // Sample and lower
                    let samples = sample_processes();
                    let meeting = is_meeting_active(&app_handle);
                    let cfg = Config {
                        cpu_threshold: 15.0,
                        max_targets: 6,
                        nexus_pid: std::process::id(),
                    };
                    let pids = decide(&samples, &cfg, meeting);

                    let mut lowered_names = Vec::new();
                    for pid in pids {
                        if let Some(lowered) = lower(pid) {
                            lowered_names.push(lowered.exe.clone());
                            baseline.others.push(lowered);
                            write_baseline(&dir, &baseline);
                        }
                    }

                    *ACTIVE_BASELINE.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(baseline.clone());
                    last_resample = std::time::Instant::now();

                    println!(
                        "[FOCUS] lowered {} apps: {}",
                        lowered_names.len(),
                        lowered_names.join(", ")
                    );
                } else if paused && flag && engaged {
                    // Periodic re-sampling every 30s to lower heavy newcomers
                    if last_resample.elapsed() >= std::time::Duration::from_secs(30) {
                        last_resample = std::time::Instant::now();
                        let cfg = Config {
                            cpu_threshold: 15.0,
                            max_targets: 6,
                            nexus_pid: std::process::id(),
                        };

                        if baseline.others.len() < cfg.max_targets {
                            let samples = sample_processes();
                            let meeting = is_meeting_active(&app_handle);
                            let pids = decide(&samples, &cfg, meeting);

                            let mut newcomer_names = Vec::new();
                            for pid in pids {
                                if baseline.others.len() >= cfg.max_targets {
                                    break;
                                }
                                if baseline.others.iter().any(|o| o.pid == pid) {
                                    continue;
                                }
                                if let Some(lowered) = lower(pid) {
                                    newcomer_names.push(lowered.exe.clone());
                                    baseline.others.push(lowered);
                                    write_baseline(&dir, &baseline);
                                }
                            }

                            if !newcomer_names.is_empty() {
                                *ACTIVE_BASELINE.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(baseline.clone());
                                println!(
                                    "[FOCUS] lowered {} apps: {}",
                                    newcomer_names.len(),
                                    newcomer_names.join(", ")
                                );
                            }
                        }
                    }
                } else if (!paused || !flag) && engaged {
                    engaged = false;
                    let running = snapshot_running_for_baseline(&baseline);
                    let to_restore = plan_restore(&baseline, &running);
                    let count = to_restore
                        .iter()
                        .filter(|p| Some(p.pid) != baseline.nexus.as_ref().map(|n| n.pid))
                        .count();

                    for item in to_restore {
                        restore(&item);
                    }

                    clear_baseline(&dir);
                    *ACTIVE_BASELINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    baseline = Baseline::default();

                    println!("[FOCUS] restored {} apps", count);
                }
            }
        })
        .ok();
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        pid: u32,
        parent: u32,
        name: &str,
        cpu_pct: f32,
        foreground: bool,
        elevated: bool,
        same_user: bool,
    ) -> ProcSample {
        ProcSample {
            pid,
            parent,
            name: name.into(),
            cpu_pct,
            foreground,
            elevated,
            same_user,
        }
    }

    #[test]
    fn meeting_active_lowers_nothing() {
        let samples = vec![sample(100, 1, "heavy_app.exe", 50.0, false, false, true)];
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 6,
            nexus_pid: 9999,
        };
        assert!(decide(&samples, &cfg, true).is_empty());
    }

    #[test]
    fn foreground_process_and_children_never_lowered() {
        let samples = vec![
            sample(10, 1, "parent_app.exe", 40.0, true, false, true), // foreground
            sample(11, 10, "child_worker.exe", 60.0, false, false, true), // child of foreground
            sample(12, 11, "grandchild.exe", 70.0, false, false, true), // grandchild of foreground
            sample(20, 1, "heavy_background.exe", 35.0, false, false, true), // eligible
        ];
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 6,
            nexus_pid: 9999,
        };
        let targets = decide(&samples, &cfg, false);
        assert_eq!(targets, vec![20]);
    }

    #[test]
    fn nexus_and_all_descendants_never_lowered() {
        let nexus_pid = 500;
        let samples = vec![
            sample(nexus_pid, 1, "nexus.exe", 25.0, false, false, true),
            sample(501, nexus_pid, "msedgewebview2.exe", 30.0, false, false, true),
            sample(502, 501, "webview_subproc.exe", 40.0, false, false, true),
            sample(600, 1, "target.exe", 20.0, false, false, true),
        ];
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 6,
            nexus_pid,
        };
        let targets = decide(&samples, &cfg, false);
        assert_eq!(targets, vec![600]);
    }

    #[test]
    fn elevated_and_other_user_never_lowered() {
        let samples = vec![
            sample(10, 1, "admin_app.exe", 80.0, false, true, true), // elevated
            sample(20, 1, "other_user.exe", 75.0, false, false, false), // other user
            sample(30, 1, "my_normal_app.exe", 45.0, false, false, true), // normal
        ];
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 6,
            nexus_pid: 9999,
        };
        let targets = decide(&samples, &cfg, false);
        assert_eq!(targets, vec![30]);
    }

    #[test]
    fn low_cpu_never_lowered() {
        let samples = vec![
            sample(10, 1, "idle_app.exe", 14.9, false, false, true), // below 15.0 threshold
            sample(20, 1, "active_app.exe", 15.0, false, false, true), // exactly threshold
        ];
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 6,
            nexus_pid: 9999,
        };
        let targets = decide(&samples, &cfg, false);
        assert_eq!(targets, vec![20]);
    }

    #[test]
    fn system_and_shell_processes_blocked() {
        let blocked_names = [
            "system", "idle", "registry", "smss.exe", "csrss.exe", "wininit.exe",
            "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe", "dwm.exe",
            "explorer.exe", "audiodg.exe", "ctfmon.exe", "fontdrvhost.exe",
            "searchhost.exe", "sihost.exe", "taskhostw.exe", "runtimebroker.exe",
        ];
        let mut samples = Vec::new();
        for (i, &name) in blocked_names.iter().enumerate() {
            samples.push(sample((i + 100) as u32, 1, name, 80.0, false, false, true));
        }
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 100,
            nexus_pid: 9999,
        };
        assert!(decide(&samples, &cfg, false).is_empty());
    }

    #[test]
    fn security_software_exact_and_substring_blocked() {
        let blocked_names = [
            "msmpeng.exe",
            "mpdefendernetworkservice.exe",
            "securityhealthservice.exe",
            "windowsdefender.exe",
            "avast_ui.exe",
            "kaspersky_av.exe",
            "norton_security.exe",
            "mcafee_scanner.exe",
            "bitdefender_agent.exe",
            "malwarebytes_tray.exe",
            "crowdstrike_sensor.exe",
            "sentinelone.exe",
        ];
        let mut samples = Vec::new();
        for (i, &name) in blocked_names.iter().enumerate() {
            samples.push(sample((i + 200) as u32, 1, name, 85.0, false, false, true));
        }
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 100,
            nexus_pid: 9999,
        };
        assert!(decide(&samples, &cfg, false).is_empty());
    }

    #[test]
    fn call_meeting_capture_media_and_tools_blocked() {
        let blocked_names = [
            "zoom.exe",
            "teams.exe",
            "ms-teams.exe",
            "webex.exe",
            "skype.exe",
            "discord.exe",
            "obs64.exe",
            "obs32.exe",
            "spotify.exe",
            "vlc.exe",
            "wmplayer.exe",
            "music.ui.exe",
            "claude.exe",
            "agy.exe",
        ];
        let mut samples = Vec::new();
        for (i, &name) in blocked_names.iter().enumerate() {
            samples.push(sample((i + 300) as u32, 1, name, 70.0, false, false, true));
        }
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 100,
            nexus_pid: 9999,
        };
        assert!(decide(&samples, &cfg, false).is_empty());
    }

    #[test]
    fn highest_cpu_first_and_max_targets_respected() {
        let samples = vec![
            sample(1, 0, "app1.exe", 20.0, false, false, true),
            sample(2, 0, "app2.exe", 50.0, false, false, true),
            sample(3, 0, "app3.exe", 90.0, false, false, true),
            sample(4, 0, "app4.exe", 30.0, false, false, true),
            sample(5, 0, "app5.exe", 70.0, false, false, true),
        ];
        let cfg = Config {
            cpu_threshold: 15.0,
            max_targets: 3,
            nexus_pid: 9999,
        };
        let targets = decide(&samples, &cfg, false);
        // Sorted: app3 (90), app5 (70), app2 (50) -> max_targets = 3
        assert_eq!(targets, vec![3, 5, 2]);
    }

    #[test]
    fn plan_restore_preserves_reverse_order_and_handles_pid_reuse_and_dead_procs() {
        let p1 = LoweredProc {
            pid: 101,
            exe: "app1.exe".into(),
            created: 1000,
            orig_priority: 32,
            orig_throttle_control: 0,
            orig_throttle_state: 0,
        };
        let p2 = LoweredProc {
            pid: 102,
            exe: "app2.exe".into(),
            created: 2000,
            orig_priority: 32,
            orig_throttle_control: 1,
            orig_throttle_state: 1,
        };
        let p3 = LoweredProc {
            pid: 103,
            exe: "app3.exe".into(),
            created: 3000,
            orig_priority: 32,
            orig_throttle_control: 0,
            orig_throttle_state: 0,
        };
        let nexus = LoweredProc {
            pid: 9999,
            exe: "nexus.exe".into(),
            created: 500,
            orig_priority: 32,
            orig_throttle_control: 0,
            orig_throttle_state: 0,
        };

        let baseline = Baseline {
            nexus: Some(nexus.clone()),
            others: vec![p1.clone(), p2.clone(), p3.clone()],
        };

        // Case 1: Exact match on all -> reverse order: p3, p2, p1, nexus
        let running_all = vec![
            (101, "app1.exe".into(), 1000),
            (102, "app2.exe".into(), 2000),
            (103, "app3.exe".into(), 3000),
            (9999, "nexus.exe".into(), 500),
        ];
        let restored = plan_restore(&baseline, &running_all);
        assert_eq!(restored, vec![p3.clone(), p2.clone(), p1.clone(), nexus.clone()]);

        // Case 2: Dead process (p2 not in running), PID reuse on p3 (different creation time),
        // PID reuse on p1 (different exe name)
        let running_reused_and_dead = vec![
            (101, "reused_by_other.exe".into(), 1000), // different exe
            (103, "app3.exe".into(), 9999),             // different created time
            (9999, "nexus.exe".into(), 500),           // nexus alive
        ];
        let restored2 = plan_restore(&baseline, &running_reused_and_dead);
        // Only nexus matches
        assert_eq!(restored2, vec![nexus]);
    }

    #[test]
    fn baseline_round_trip_and_corrupt_file_handling() {
        let temp_dir = std::env::temp_dir().join(format!("focus_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let baseline = Baseline {
            nexus: Some(LoweredProc {
                pid: 1234,
                exe: "nexus.exe".into(),
                created: 4567,
                orig_priority: 32,
                orig_throttle_control: 0,
                orig_throttle_state: 0,
            }),
            others: vec![LoweredProc {
                pid: 5678,
                exe: "render.exe".into(),
                created: 8910,
                orig_priority: 16384,
                orig_throttle_control: 1,
                orig_throttle_state: 1,
            }],
        };

        // Round-trip
        assert!(write_baseline(&temp_dir, &baseline));
        let loaded = read_baseline(&temp_dir).expect("should read baseline");
        assert_eq!(loaded, baseline);

        // Clear
        clear_baseline(&temp_dir);
        assert!(read_baseline(&temp_dir).is_none());

        // Corrupt file handling: must return empty Baseline and never panic
        std::fs::write(temp_dir.join(BASELINE_FILE), "CORRUPT JSON { [").unwrap();
        let loaded_corrupt = read_baseline(&temp_dir).expect("should return default on corrupt");
        assert_eq!(loaded_corrupt, Baseline::default());
        assert!(loaded_corrupt.others.is_empty());
        assert!(loaded_corrupt.nexus.is_none());

        clear_baseline(&temp_dir);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
