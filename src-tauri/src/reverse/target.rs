//! Resolving WHAT to analyse from a spoken request.
//!
//! "this app" -> the foreground window's executable (full path), "the file I just downloaded" ->
//! newest program file in Downloads, "the latest HAR" -> newest `.har` in Downloads/Desktop,
//! or an explicit / clipboard path. Pure helpers are separated from the Windows-only call so they
//! can be tested anywhere.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Program file extensions Reverse Lab will triage (PE family + Electron bundles).
pub const PROGRAM_EXTS: &[&str] = &["exe", "dll", "sys", "ocx", "scr", "cpl", "efi", "msi", "asar"];
pub const HAR_EXTS: &[&str] = &["har"];

/// How the user referred to the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetSpec {
    /// "this app / this program / what I'm looking at"
    ForegroundApp,
    /// "the file I just downloaded"
    JustDownloaded,
    /// "the latest HAR / the capture I just saved"
    LatestHar,
    /// explicit path (typed in Settings, IPC, or read from the clipboard)
    Path(String),
    /// path text currently on the clipboard
    Clipboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    NothingFound(&'static str),
    NotAFile(String),
    TooLarge { bytes: u64, cap: u64 },
    SelfTarget,
    Unsupported(&'static str),
}

impl TargetError {
    /// One sentence NEXUS can speak.
    pub fn spoken(&self) -> String {
        match self {
            TargetError::NothingFound(what) => format!("I couldn't find {what}, sir."),
            TargetError::NotAFile(p) => format!("That path isn't a readable file: {p}."),
            TargetError::TooLarge { bytes, cap } => format!(
                "That file is {} megabytes; my limit is {}. You can raise it in settings.",
                bytes / (1024 * 1024),
                cap / (1024 * 1024)
            ),
            TargetError::SelfTarget => "That's me, sir. Point me at another program.".to_string(),
            TargetError::Unsupported(why) => format!("I can't do that here: {why}."),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub path: PathBuf,
    /// How it was found - shown in the report ("foreground app", "newest download", ...).
    pub how: &'static str,
}

/// Strip the decoration a pasted path usually has: quotes, `file:///`, surrounding spaces,
/// a trailing newline. Returns `None` for text that cannot be a single path.
pub fn clean_path_text(text: &str) -> Option<PathBuf> {
    let t = text.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`').trim();
    if t.is_empty() || t.contains('\n') || t.contains('\r') || t.len() > 1024 {
        return None;
    }
    let t = t.strip_prefix("file:///").or_else(|| t.strip_prefix("file://")).unwrap_or(t);
    let t = t.replace('/', std::path::MAIN_SEPARATOR_STR);
    if t.is_empty() {
        None
    } else {
        Some(PathBuf::from(t))
    }
}

fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Newest file (by modified time) directly inside `dirs` whose extension is in `exts`, no older
/// than `max_age` before `now`. Partial downloads (`.crdownload`, `.part`, `.tmp`) never match
/// because their extension is not in `exts`.
pub fn newest_matching(dirs: &[PathBuf], exts: &[&str], max_age: Duration, now: SystemTime) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !exts.contains(&ext_of(&p).as_str()) {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            if !md.is_file() {
                continue;
            }
            let Ok(mt) = md.modified() else { continue };
            // files from the future (clock skew) count as "now"
            let age = now.duration_since(mt).unwrap_or(Duration::ZERO);
            if age > max_age {
                continue;
            }
            if best.as_ref().map_or(true, |(t, _)| mt > *t) {
                best = Some((mt, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// `%USERPROFILE%\Downloads` and `Desktop` (or `$HOME/...` off Windows).
pub fn user_dirs() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    match home {
        Some(h) => (vec![h.join("Downloads")], vec![h.join("Downloads"), h.join("Desktop")]),
        None => (vec![], vec![]),
    }
}

/// Validate an explicit path: must be a regular file within the size cap.
pub fn validate_file(path: &Path, max_bytes: u64) -> Result<(), TargetError> {
    let md = std::fs::metadata(path).map_err(|_| TargetError::NotAFile(path.display().to_string()))?;
    if !md.is_file() {
        return Err(TargetError::NotAFile(path.display().to_string()));
    }
    if md.len() > max_bytes {
        return Err(TargetError::TooLarge { bytes: md.len(), cap: max_bytes });
    }
    Ok(())
}

/// True when `candidate` is this very executable (NEXUS must not "reverse engineer" itself by
/// accident when its own non-activating window had focus).
pub fn is_self_exe(candidate: &Path) -> bool {
    let Ok(me) = std::env::current_exe() else { return false };
    match (me.file_name(), candidate.file_name()) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

/// Full path of the foreground window's executable. Windows only; `None` elsewhere or on failure
/// (elevated/protected processes refuse `PROCESS_QUERY_LIMITED_INFORMATION` less often than the
/// older VM_READ right the other call sites use).
#[cfg(target_os = "windows")]
pub fn foreground_exe_path() -> Option<PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid as *mut u32);
        if pid == 0 {
            return None;
        }
        let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(proc, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).as_bool();
        let _ = CloseHandle(proc);
        if !ok || len == 0 {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn foreground_exe_path() -> Option<PathBuf> {
    None
}

/// Resolve a spoken target to a concrete, size-checked file.
pub fn resolve(spec: &TargetSpec, max_bytes: u64) -> Result<ResolvedTarget, TargetError> {
    let now = SystemTime::now();
    let (dl, har_dirs) = user_dirs();
    let (path, how): (PathBuf, &'static str) = match spec {
        TargetSpec::ForegroundApp => {
            let p = foreground_exe_path().ok_or(TargetError::NothingFound("the app in front"))?;
            if is_self_exe(&p) {
                return Err(TargetError::SelfTarget);
            }
            (p, "foreground app")
        }
        TargetSpec::JustDownloaded => (
            newest_matching(&dl, PROGRAM_EXTS, Duration::from_secs(30 * 60), now)
                .ok_or(TargetError::NothingFound("a program you downloaded in the last half hour"))?,
            "newest download",
        ),
        TargetSpec::LatestHar => (
            newest_matching(&har_dirs, HAR_EXTS, Duration::from_secs(24 * 3600), now)
                .ok_or(TargetError::NothingFound("a recent HAR capture"))?,
            "newest HAR",
        ),
        TargetSpec::Path(s) => (clean_path_text(s).ok_or(TargetError::NotAFile(s.clone()))?, "given path"),
        TargetSpec::Clipboard => {
            let text = arboard::Clipboard::new()
                .and_then(|mut c| c.get_text())
                .map_err(|_| TargetError::NothingFound("anything on the clipboard"))?;
            (clean_path_text(&text).ok_or(TargetError::NothingFound("a file path on the clipboard"))?, "clipboard path")
        }
    };
    validate_file(&path, max_bytes)?;
    Ok(ResolvedTarget { path, how })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::UNIX_EPOCH;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nexus_reverse_target_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path, age_secs: u64, now: SystemTime) {
        fs::write(p, b"MZ").unwrap();
        let f = fs::File::options().write(true).open(p).unwrap();
        f.set_modified(now - Duration::from_secs(age_secs)).unwrap();
    }

    #[test]
    fn test_clean_path_text() {
        let sep = std::path::MAIN_SEPARATOR_STR;
        assert_eq!(clean_path_text("  \"C:/Tools/a b.exe\"  ").unwrap(), PathBuf::from(format!("C:{sep}Tools{sep}a b.exe")));
        assert_eq!(clean_path_text("file:///C:/x/y.har").unwrap(), PathBuf::from(format!("C:{sep}x{sep}y.har")));
        assert!(clean_path_text("").is_none());
        assert!(clean_path_text("line1\nline2").is_none());
        assert!(clean_path_text(&"a".repeat(2000)).is_none());
    }

    #[test]
    fn test_newest_matching_picks_newest_recent_program_and_ignores_partials() {
        let d = tmpdir("newest");
        let now = SystemTime::now();
        touch(&d.join("old.exe"), 3 * 3600, now); // too old
        touch(&d.join("a.exe"), 600, now);
        touch(&d.join("b.dll"), 60, now); // newest eligible
        touch(&d.join("c.crdownload"), 5, now); // partial: never matches
        touch(&d.join("notes.txt"), 1, now);
        let got = newest_matching(&[d.clone()], PROGRAM_EXTS, Duration::from_secs(1800), now).unwrap();
        assert_eq!(got.file_name().unwrap(), "b.dll");
        assert!(newest_matching(&[d.clone()], HAR_EXTS, Duration::from_secs(1800), now).is_none());
        assert!(newest_matching(&[d.join("missing")], PROGRAM_EXTS, Duration::from_secs(1800), now).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn test_future_mtime_counts_as_now_and_extension_match_is_case_insensitive() {
        let d = tmpdir("future");
        let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        let p = d.join("CAPTURE.HAR");
        fs::write(&p, b"{}").unwrap();
        fs::File::options().write(true).open(&p).unwrap().set_modified(now + Duration::from_secs(500)).unwrap();
        assert!(newest_matching(&[d.clone()], HAR_EXTS, Duration::from_secs(60), now).is_some());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn test_validate_file_limits() {
        let d = tmpdir("validate");
        let f = d.join("x.exe");
        fs::write(&f, vec![0u8; 100]).unwrap();
        assert!(validate_file(&f, 1000).is_ok());
        assert_eq!(validate_file(&f, 10), Err(TargetError::TooLarge { bytes: 100, cap: 10 }));
        assert!(matches!(validate_file(&d, 1000), Err(TargetError::NotAFile(_))));
        assert!(matches!(validate_file(&d.join("nope.exe"), 1000), Err(TargetError::NotAFile(_))));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn test_resolve_explicit_path_and_errors_are_speakable() {
        let d = tmpdir("resolve");
        let f = d.join("prog.exe");
        fs::write(&f, b"MZ").unwrap();
        let r = resolve(&TargetSpec::Path(f.display().to_string()), 1 << 20).unwrap();
        assert_eq!(r.how, "given path");
        assert!(resolve(&TargetSpec::Path("".into()), 1 << 20).is_err());
        for e in [
            TargetError::NothingFound("a recent HAR capture"),
            TargetError::TooLarge { bytes: 600 << 20, cap: 512 << 20 },
            TargetError::SelfTarget,
        ] {
            assert!(!e.spoken().is_empty());
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn test_foreground_exe_path_is_none_off_windows_and_self_exe_detection() {
        #[cfg(not(target_os = "windows"))]
        assert!(foreground_exe_path().is_none());
        let me = std::env::current_exe().unwrap();
        assert!(is_self_exe(&me));
        assert!(!is_self_exe(Path::new("notepad.exe")));
    }
}
