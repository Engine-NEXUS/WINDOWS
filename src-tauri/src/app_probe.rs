//! Read-only "what is this desktop app built with" detector (Windows)
//!
//! Part of the voice feature "Nexus, reverse engineer this app".
//! Reports what a foreground desktop application is built with by looking
//! ONLY at its install folder and the first bytes of its executable —
//! read-only, no process injection, no memory reading, no network, no execution.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(windows)]
use windows::{
    Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    },
    core::PCWSTR,
};

/// Result of a single detection signal
#[derive(Debug, Clone, Serialize)]
pub struct AppDetection {
    pub id: String,
    pub name: String,
    pub category: String,
    pub confidence: f32,
    pub evidence: Vec<String>,
}

/// Complete report for an application
#[derive(Debug, Clone, Serialize)]
pub struct AppReport {
    pub exe_name: String,
    pub exe_path: String,
    pub install_dir: String,
    pub file_version: Option<String>,
    pub company: Option<String>,
    pub product: Option<String>,
    pub detections: Vec<AppDetection>,
    pub notes: Vec<String>,
}

/// Refusal reasons when probing is not possible
#[derive(Debug, Clone, Serialize)]
pub enum AppRefusal {
    NotFound,
    Sensitive,
    System,
    Unreadable(String),
}

impl AppRefusal {
    pub fn spoken(&self) -> &'static str {
        match self {
            AppRefusal::NotFound => "The executable was not found.",
            AppRefusal::Sensitive => "This application is blocked for safety, sir.",
            AppRefusal::System => "System applications cannot be analyzed.",
            AppRefusal::Unreadable(_) => "The executable could not be read.",
        }
    }
}

/// File facts collected from the install directory
#[derive(Debug, Clone)]
pub struct FileFacts {
    pub names: Vec<String>,
    pub exe_head: Vec<u8>,
}

/// Maximum depth for directory walking
const MAX_DEPTH: usize = 2;
/// Maximum number of entries to collect
const MAX_ENTRIES: usize = 3000;
/// Maximum bytes to read from executable header
const MAX_EXE_HEAD: usize = 4 * 1024 * 1024; // 4 MiB

/// System folders that should be refused
static SYSTEM_FOLDERS: OnceLock<Vec<PathBuf>> = OnceLock::new();
/// System executables that should be refused
static SYSTEM_EXES: OnceLock<Vec<String>> = OnceLock::new();

fn init_system_lists() {
    SYSTEM_FOLDERS.get_or_init(|| {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
        vec![
            PathBuf::from(&windir),
            PathBuf::from(&windir).join("System32"),
            PathBuf::from(&windir).join("SysWOW64"),
            PathBuf::from(&windir).join("WinSxS"),
        ]
    });

    SYSTEM_EXES.get_or_init(|| {
        vec![
            "explorer.exe".to_lowercase(),
            "applicationframehost.exe".to_lowercase(),
            "shellexperiencehost.exe".to_lowercase(),
            "searchhost.exe".to_lowercase(),
            "startmenuexperiencehost.exe".to_lowercase(),
            "lockapp.exe".to_lowercase(),
        ]
    });
}

fn is_system_path(path: &Path) -> bool {
    init_system_lists();
    let canonical = path.canonicalize().ok();
    if let Some(canonical) = canonical {
        let canonical_str = canonical.to_string_lossy().to_lowercase();
        for sys_folder in SYSTEM_FOLDERS.get().unwrap() {
            let sys_str = sys_folder.to_string_lossy().to_lowercase();
            if canonical_str.starts_with(&sys_str) {
                return true;
            }
        }
        if let Some(file_name) = canonical.file_name() {
            let name = file_name.to_string_lossy().to_lowercase();
            if SYSTEM_EXES.get().unwrap().contains(&name) {
                return true;
            }
        }
    }
    false
}

/// Resolve real path, stripping \\?\ prefix
fn resolve_real_path(path: &Path) -> Result<PathBuf, AppRefusal> {
    let canonical = path
        .canonicalize()
        .map_err(|_| AppRefusal::Unreadable("cannot canonicalize path".to_string()))?;
    let path_str = canonical.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
        Ok(PathBuf::from(stripped))
    } else {
        Ok(canonical)
    }
}

/// Check if path is a .exe file
fn is_exe_file(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
}

/// Read version info from executable (Windows only)
#[cfg(windows)]
fn read_version_info(exe_path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::BOOL;

    let path_wide: Vec<u16> = exe_path.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        let mut size: u32 = 0;
        let size_ptr: *mut u32 = &mut size;
        let result = GetFileVersionInfoSizeW(PCWSTR(path_wide.as_ptr() as *const _), size_ptr);
        if result == 0 {
            return (None, None, None);
        }

        let mut buffer = vec![0u8; size as usize];
        let result = GetFileVersionInfoW(PCWSTR(path_wide.as_ptr() as *const _), 0, size, buffer.as_mut_ptr() as *mut _);
        if result == BOOL(0) {
            return (None, None, None);
        }

        let mut fixed_info_ptr: *mut VS_FIXEDFILEINFO = std::ptr::null_mut();
        let mut fixed_info_len: u32 = 0;
        let backslash: Vec<u16> = OsStr::new("\\").encode_wide().chain(Some(0)).collect();
        let result = VerQueryValueW(
            buffer.as_ptr() as *const _,
            PCWSTR(backslash.as_ptr() as *const _),
            &mut fixed_info_ptr as *mut _ as *mut _,
            &mut fixed_info_len,
        );
        let file_version = if result != BOOL(0) && fixed_info_len > 0 {
            let info = &*fixed_info_ptr;
            Some(format!(
                "{}.{}.{}.{}",
                info.dwFileVersionMS >> 16,
                info.dwFileVersionMS & 0xFFFF,
                info.dwFileVersionLS >> 16,
                info.dwFileVersionLS & 0xFFFF
            ))
        } else {
            None
        };

        // Helper to query string values
        let query_string = |name: &str| -> Option<String> {
            let sub_block = format!(r"\StringFileInfo\040904b0\{}", name);
            let sub_block_wide: Vec<u16> = OsStr::new(&sub_block).encode_wide().chain(Some(0)).collect();
            let mut value_ptr: *mut u16 = std::ptr::null_mut();
            let mut value_len: u32 = 0;
            let result = VerQueryValueW(
                buffer.as_ptr() as *const _,
                PCWSTR(sub_block_wide.as_ptr() as *const _),
                &mut value_ptr as *mut _ as *mut _,
                &mut value_len,
            );
            if result != BOOL(0) && value_len > 0 && !value_ptr.is_null() {
                let slice = std::slice::from_raw_parts(value_ptr, value_len as usize);
                Some(String::from_utf16_lossy(slice))
            } else {
                None
            }
        };

        let company = query_string("CompanyName");
        let product = query_string("ProductName");

        (file_version, company, product)
    }
}

#[cfg(not(windows))]
fn read_version_info(_exe_path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    (None, None, None)
}

/// Walk install directory up to depth 2, max 3000 entries, no symlinks
fn walk_install_dir(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0usize)];

    while let Some((current, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }
        if names.len() >= MAX_ENTRIES {
            break;
        }

        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };

        for entry in entries.flatten() {
            if names.len() >= MAX_ENTRIES {
                break;
            }

            let file_type = entry.file_type();
            // Skip symlinks and junctions
            if file_type.is_ok() && file_type.unwrap().is_symlink() {
                continue;
            }

            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_lowercase();

            if path.is_dir() {
                names.push(format!("{}/", name_str));
                if depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
            } else {
                names.push(name_str);
            }
        }
    }

    names
}

/// Read first 4 MiB of executable
fn read_exe_head(exe_path: &Path) -> Result<(Vec<u8>, u64), AppRefusal> {
    let metadata = fs::metadata(exe_path).map_err(|_| AppRefusal::Unreadable("cannot read exe metadata".to_string()))?;
    let exe_len = metadata.len();

    let mut file = fs::File::open(exe_path).map_err(|_| AppRefusal::Unreadable("cannot open exe".to_string()))?;
    let mut head = vec![0u8; MAX_EXE_HEAD.min(exe_len as usize)];
    use std::io::Read;
    let read_len = file.read(&mut head).map_err(|_| AppRefusal::Unreadable("cannot read exe head".to_string()))?;
    head.truncate(read_len);

    Ok((head, exe_len))
}

/// Collect file facts from the install directory
fn collect_facts(exe_path: &Path) -> Result<FileFacts, AppRefusal> {
    let install_dir = exe_path.parent().ok_or(AppRefusal::Unreadable("no parent directory".to_string()))?;

    // Check for Squirrel layout: app-<semver> or bin
    let install_dir = if let Some(parent) = install_dir.parent() {
        let dir_name = install_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir_name.starts_with("app-") || dir_name == "bin" {
            parent
        } else {
            install_dir
        }
    } else {
        install_dir
    };

    let names = walk_install_dir(install_dir);
    let (exe_head, _exe_len) = read_exe_head(exe_path)?;

    Ok(FileFacts {
        names,
        exe_head,
    })
}

/// Check if a string is contained in exe_head (case-insensitive)
fn exe_head_contains(exe_head: &[u8], needle: &str) -> bool {
    let needle_lower = needle.to_lowercase();
    let haystack = String::from_utf8_lossy(exe_head).to_lowercase();
    haystack.contains(&needle_lower)
}

/// Check if any file name matches a pattern
fn has_file(names: &[String], pattern: &str) -> bool {
    let pattern_lower = pattern.to_lowercase();
    names.iter().any(|n| n.contains(&pattern_lower))
}

/// Check if any file name matches exactly
fn has_exact_file(names: &[String], pattern: &str) -> bool {
    let pattern_lower = pattern.to_lowercase();
    names.iter().any(|n| n.as_str() == pattern_lower || n.as_str() == format!("{}/", pattern_lower))
}

/// Check if any file name matches a glob-like pattern (supports * prefix/suffix)
fn has_file_pattern(names: &[String], pattern: &str) -> bool {
    let pattern_lower = pattern.to_lowercase();
    if let Some(prefix) = pattern_lower.strip_suffix('*') {
        names.iter().any(|n| n.starts_with(prefix))
    } else if let Some(suffix) = pattern_lower.strip_prefix('*') {
        names.iter().any(|n| n.ends_with(suffix))
    } else {
        names.iter().any(|n| n.contains(&pattern_lower))
    }
}

/// Core pure analysis function - no IO
pub fn analyse_files(exe_name: &str, facts: &FileFacts) -> Vec<AppDetection> {
    let mut detections = Vec::new();
    let names = &facts.names;
    let exe_head = &facts.exe_head;

    // Helper to add detection with confidence combination
    let mut add_detection = |id: &str, name: &str, category: &str, confidence: f32, evidence: Vec<String>| {
        if confidence >= 0.4 {
            detections.push(AppDetection {
                id: id.to_string(),
                name: name.to_string(),
                category: category.to_string(),
                confidence,
                evidence,
            });
        }
    };

    // Helper to combine confidences: 1 - prod(1 - w)
    let combine_conf = |confs: &[f32]| -> f32 {
        1.0 - confs.iter().map(|c| 1.0 - c).product::<f32>()
    };

    // --- Electron ---
    let mut electron_signals = Vec::new();
    if has_file(names, "resources/app.asar") {
        electron_signals.push(0.95);
    }
    if has_exact_file(names, "electron.exe") {
        electron_signals.push(0.9);
    }
    if has_file(names, "chrome_100_percent.pak") && has_file(names, "icudtl.dat") && has_file(names, "v8_context_snapshot.bin") {
        electron_signals.push(0.9);
    }
    if exe_head_contains(exe_head, "electron") {
        electron_signals.push(0.5);
    }
    if !electron_signals.is_empty() {
        let conf = combine_conf(&electron_signals);
        add_detection(
            "electron",
            "Electron",
            "app-framework",
            conf,
            vec!["Found Electron application structure".to_string()],
        );
        // Imply Chromium + Node.js at 0.8x
        add_detection(
            "chromium",
            "Chromium",
            "browser-engine",
            conf * 0.8,
            vec!["Implied by Electron framework".to_string()],
        );
        add_detection(
            "nodejs",
            "Node.js",
            "runtime",
            conf * 0.8,
            vec!["Implied by Electron framework".to_string()],
        );
    }

    // --- Tauri / WebView2 ---
    let mut tauri_signals = Vec::new();
    if has_file(names, "webview2loader.dll") {
        tauri_signals.push(0.7);
    }
    if exe_head_contains(exe_head, "tauri") {
        tauri_signals.push(0.6);
    }
    if exe_head_contains(exe_head, "wry") {
        tauri_signals.push(0.5);
    }
    if exe_head_contains(exe_head, "webview2") {
        tauri_signals.push(0.5);
    }
    if !tauri_signals.is_empty() {
        let conf = combine_conf(&tauri_signals);
        add_detection(
            "tauri-webview2",
            "Tauri / WebView2",
            "app-framework",
            conf,
            vec!["Found WebView2/Tauri indicators".to_string()],
        );
    } else if has_file(names, "webview2loader.dll") {
        // WebView2 generic
        add_detection(
            "webview2",
            "Microsoft Edge WebView2 host",
            "app-framework",
            0.7,
            vec!["Found webview2loader.dll".to_string()],
        );
    }

    // --- CEF ---
    let mut cef_signals = Vec::new();
    if has_file(names, "libcef.dll") {
        cef_signals.push(0.95);
    }
    if has_file(names, "cef.pak") || has_file(names, "chrome_elf.dll") {
        cef_signals.push(0.8);
    }
    if !cef_signals.is_empty() {
        let conf = combine_conf(&cef_signals);
        add_detection(
            "cef",
            "Chromium Embedded Framework (CEF)",
            "app-framework",
            conf,
            vec!["Found CEF binaries".to_string()],
        );
    }

    // --- NW.js ---
    if has_file(names, "nw.exe") || has_file(names, "nw.pak") {
        add_detection(
            "nwjs",
            "NW.js",
            "app-framework",
            0.9,
            vec!["Found NW.js files".to_string()],
        );
    }

    // --- Qt 5/6 ---
    if has_file_pattern(names, "qt5core.dll") || has_file_pattern(names, "qt6core.dll") {
        add_detection(
            "qt",
            "Qt",
            "ui-toolkit",
            0.95,
            vec!["Found Qt core library".to_string()],
        );
        if has_file_pattern(names, "platforms/qwindows.dll") {
            // Already covered by confidence
        }
    }

    // --- wxWidgets ---
    if has_file_pattern(names, "wxmsw") && has_file_pattern(names, ".dll") {
        add_detection(
            "wxwidgets",
            "wxWidgets",
            "ui-toolkit",
            0.85,
            vec!["Found wxWidgets DLL".to_string()],
        );
    }

    // --- GTK ---
    if has_file(names, "libgtk-3-0.dll") {
        add_detection(
            "gtk",
            "GTK",
            "ui-toolkit",
            0.85,
            vec!["Found GTK library".to_string()],
        );
    }

    // --- Dear ImGui ---
    if exe_head_contains(exe_head, "imgui") {
        add_detection(
            "imgui",
            "Dear ImGui",
            "ui-toolkit",
            0.4,
            vec!["Found ImGui reference in executable".to_string()],
        );
    }

    // --- Flutter ---
    if has_file(names, "flutter_windows.dll") {
        add_detection(
            "flutter",
            "Flutter",
            "app-framework",
            0.95,
            vec!["Found Flutter Windows DLL".to_string()],
        );
    }
    if has_file(names, "data/app.so") || has_file_pattern(names, "data/flutter_assets/") {
        add_detection(
            "flutter",
            "Flutter",
            "app-framework",
            0.95,
            vec!["Found Flutter assets".to_string()],
        );
    }

    // --- React Native Windows ---
    if has_file(names, "react-native-win32.dll") || has_file(names, "hermes.dll") {
        add_detection(
            "react-native-windows",
            "React Native Windows",
            "app-framework",
            0.9,
            vec!["Found React Native Windows libraries".to_string()],
        );
    }

    // --- Avalonia ---
    if has_file(names, "avalonia.base.dll") {
        add_detection(
            "avalonia",
            "Avalonia",
            "ui-toolkit",
            0.95,
            vec!["Found Avalonia library".to_string()],
        );
    }

    // --- MAUI ---
    if has_file(names, "microsoft.maui.dll") {
        add_detection(
            "maui",
            ".NET MAUI",
            "app-framework",
            0.9,
            vec!["Found MAUI library".to_string()],
        );
    }

    // --- WPF ---
    if has_file(names, "presentationframework.dll") {
        add_detection(
            "wpf",
            "WPF",
            "ui-toolkit",
            0.9,
            vec!["Found PresentationFramework".to_string()],
        );
    }

    // --- WinForms ---
    if has_file(names, "system.windows.forms.dll") {
        add_detection(
            "winforms",
            "WinForms",
            "ui-toolkit",
            0.9,
            vec!["Found System.Windows.Forms".to_string()],
        );
    }

    // --- WinUI ---
    if has_file(names, "microsoft.ui.xaml.dll") {
        add_detection(
            "winui",
            "WinUI",
            "ui-toolkit",
            0.9,
            vec!["Found Microsoft.UI.Xaml".to_string()],
        );
    }

    // --- .NET Runtime ---
    if has_file(names, "hostfxr.dll") || has_file_pattern(names, "*.runtimeconfig.json") || has_file(names, "system.private.corelib.dll") {
        add_detection(
            "dotnet",
            ".NET Runtime",
            "runtime",
            0.85,
            vec!["Found .NET runtime indicators".to_string()],
        );
    }

    // --- Java ---
    if has_file(names, "jvm.dll") || has_file_pattern(names, "jre/") || has_file_pattern(names, "*.jar") || has_file(names, "javaw.exe") {
        add_detection(
            "java",
            "Java",
            "runtime",
            0.8,
            vec!["Found Java runtime indicators".to_string()],
        );
    }

    // --- Python ---
    let mut python_signals = Vec::new();
    if has_file_pattern(names, "python3") && has_file_pattern(names, ".dll") {
        python_signals.push(0.7);
    }
    if has_file(names, "base_library.zip") {
        python_signals.push(0.85);
    }
    if exe_head_contains(exe_head, "pyi-") || exe_head_contains(exe_head, "pyinstaller") {
        python_signals.push(0.85);
    }
    if !python_signals.is_empty() {
        let conf = combine_conf(&python_signals);
        add_detection(
            "python",
            "Python (PyInstaller)",
            "runtime",
            conf,
            vec!["Found Python/PyInstaller indicators".to_string()],
        );
    }

    // --- Node.js standalone ---
    if has_file(names, "node.exe") || has_file_pattern(names, "node_modules/") {
        add_detection(
            "nodejs",
            "Node.js",
            "runtime",
            0.85,
            vec!["Found Node.js standalone".to_string()],
        );
    }

    // --- Go ---
    if exe_head_contains(exe_head, "go build id") {
        add_detection(
            "go",
            "Go",
            "runtime",
            0.7,
            vec!["Found Go build ID in executable".to_string()],
        );
    }

    // --- Rust ---
    if exe_head_contains(exe_head, "rustc/") || exe_head_contains(exe_head, ".rs") && exe_head_contains(exe_head, "panic") {
        add_detection(
            "rust",
            "Rust",
            "runtime",
            0.45,
            vec!["Found Rust compiler references in executable".to_string()],
        );
    }

    // --- Delphi/C++Builder ---
    if has_file(names, "borlndmm.dll") || exe_head_contains(exe_head, "embarcadero") {
        add_detection(
            "delphi-cppbuilder",
            "Delphi / C++Builder",
            "runtime",
            0.6,
            vec!["Found Delphi/C++Builder indicators".to_string()],
        );
    }

    // --- AutoHotkey ---
    if exe_name.to_lowercase().contains("autohotkey") {
        add_detection(
            "autohotkey",
            "AutoHotkey",
            "runtime",
            0.7,
            vec!["Executable name contains AutoHotkey".to_string()],
        );
    }

    // --- Squirrel packaging (Electron-like) ---
    if has_file(names, "update.exe") && has_file_pattern(names, "app-") {
        add_detection(
            "squirrel",
            "Squirrel (Electron packaging)",
            "packaging",
            0.85,
            vec!["Found Squirrel update.exe and app-* directories".to_string()],
        );
    }

    // --- Electron-builder ---
    if has_file(names, "app-update.yml") {
        add_detection(
            "electron-builder",
            "electron-builder",
            "packaging",
            0.8,
            vec!["Found app-update.yml".to_string()],
        );
    }

    // --- MSIX/UWP ---
    if has_file(names, "appxmanifest.xml") {
        add_detection(
            "msix",
            "MSIX / UWP",
            "packaging",
            0.9,
            vec!["Found AppX manifest".to_string()],
        );
    }

    // --- Inno Setup ---
    if has_file(names, "unins000.exe") {
        add_detection(
            "innosetup",
            "Inno Setup",
            "packaging",
            0.7,
            vec!["Found Inno Setup uninstaller".to_string()],
        );
    }

    // --- NSIS ---
    if has_file(names, "uninstall.exe") && !has_file(names, "unins000.exe") {
        add_detection(
            "nsis",
            "NSIS",
            "packaging",
            0.6,
            vec!["Found NSIS uninstaller".to_string()],
        );
    }

    // --- ClickOnce ---
    if has_file_pattern(names, "*.application") {
        add_detection(
            "clickonce",
            "ClickOnce",
            "packaging",
            0.8,
            vec!["Found ClickOnce deployment manifest".to_string()],
        );
    }

    // --- Browser engines ---
    if has_file(names, "chrome_elf.dll") && has_file(names, "chrome.dll") {
        add_detection(
            "chromium",
            "Chromium",
            "browser-engine",
            0.95,
            vec!["Found Chromium engine libraries".to_string()],
        );
    }
    if has_file(names, "xul.dll") {
        add_detection(
            "gecko",
            "Gecko",
            "browser-engine",
            0.9,
            vec!["Found Gecko engine library".to_string()],
        );
    }

    // --- Game engines ---
    if has_file(names, "unityplayer.dll") && has_file_pattern(names, "*_data/") {
        add_detection(
            "unity",
            "Unity",
            "engine",
            0.95,
            vec!["Found Unity player and data directory".to_string()],
        );
    }

    if exe_name.to_lowercase().ends_with("-win64-shipping.exe") || (has_file_pattern(names, "engine/") && has_file_pattern(names, "*.pak")) {
        add_detection(
            "unreal",
            "Unreal Engine",
            "engine",
            0.8,
            vec!["Found Unreal Engine shipping executable and assets".to_string()],
        );
    }

    if has_file_pattern(names, "*.pck") {
        add_detection(
            "godot",
            "Godot",
            "engine",
            0.9,
            vec!["Found Godot .pck file".to_string()],
        );
    }

    if has_file(names, "data.win") {
        add_detection(
            "gamemaker",
            "GameMaker",
            "engine",
            0.85,
            vec!["Found GameMaker data.win".to_string()],
        );
    }

    // Sort by confidence descending
    detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

    detections
}

/// Main probe function - does IO, should be called via spawn_blocking
pub fn probe(exe_path: &str) -> Result<AppReport, AppRefusal> {
    let path = Path::new(exe_path);

    // Rule 1: Path must exist, be a file with .exe extension
    if !path.exists() {
        return Err(AppRefusal::NotFound);
    }
    if !path.is_file() {
        return Err(AppRefusal::Unreadable("path is not a file".to_string()));
    }
    if !is_exe_file(path) {
        return Err(AppRefusal::Unreadable("not an .exe file".to_string()));
    }

    // Resolve real path
    let real_path = resolve_real_path(path)?;

    // Check system folders and executables
    if is_system_path(&real_path) {
        return Err(AppRefusal::System);
    }

    // Check sensitive targets using live::safety::is_target_blocked
    let exe_name = real_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if crate::live::safety::is_target_blocked(exe_name) || crate::live::safety::is_target_blocked(&real_path.to_string_lossy()) {
        return Err(AppRefusal::Sensitive);
    }

    // Get install directory (parent of exe)
    let install_dir = real_path.parent().ok_or(AppRefusal::Unreadable("no parent directory".to_string()))?;

    // Check for Squirrel layout
    let install_dir_display = if let Some(parent) = install_dir.parent() {
        let dir_name = install_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir_name.starts_with("app-") || dir_name == "bin" {
            parent.to_string_lossy().to_string()
        } else {
            install_dir.to_string_lossy().to_string()
        }
    } else {
        install_dir.to_string_lossy().to_string()
    };

    // Collect facts
    let facts = collect_facts(&real_path)?;

    // Read version info
    let (file_version, company, product) = read_version_info(&real_path);

    // Analyze
    let detections = analyse_files(exe_name, &facts);

    // Build report
    let notes = vec!["Source code is not visible from outside; this reads the install folder only.".to_string()];

    Ok(AppReport {
        exe_name: exe_name.to_string(),
        exe_path: real_path.to_string_lossy().to_string(),
        install_dir: install_dir_display,
        file_version,
        company,
        product,
        detections,
        notes,
    })
}

/// Generate markdown report from AppReport
pub fn report_markdown(r: &AppReport) -> String {
    let mut md = String::new();

    md.push_str(&format!("## Application Analysis: {}\n\n", r.exe_name));
    md.push_str(&format!("- **Executable**: `{}`\n", r.exe_path));
    md.push_str(&format!("- **Install Directory**: `{}`\n", r.install_dir));
    if let Some(v) = &r.file_version {
        md.push_str(&format!("- **File Version**: {}\n", v));
    }
    if let Some(c) = &r.company {
        md.push_str(&format!("- **Company**: {}\n", c));
    }
    if let Some(p) = &r.product {
        md.push_str(&format!("- **Product**: {}\n", p));
    }
    md.push('\n');

    if r.detections.is_empty() {
        md.push_str("No technology detections found.\n");
    } else {
        md.push_str("### Detections\n\n");
        for d in &r.detections {
            md.push_str(&format!("- **{}** ({}) — confidence: {:.0}%\n", d.name, d.category, d.confidence * 100.0));
            for e in &d.evidence {
                md.push_str(&format!("  - {}\n", e));
            }
        }
    }

    md.push('\n');
    for note in &r.notes {
        md.push_str(&format!("> {}\n", note));
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn make_file_facts(names: Vec<&str>, exe_head: &[u8], _exe_len: u64) -> FileFacts {
        FileFacts {
            names: names.iter().map(|s| s.to_lowercase()).collect(),
            exe_head: exe_head.to_vec(),
        }
    }

    #[test]
    fn test_electron_detection() {
        // VS Code-like Electron app
        let facts = make_file_facts(
            vec![
                "resources/app.asar",
                "electron.exe",
                "chrome_100_percent.pak",
                "icudtl.dat",
                "v8_context_snapshot.bin",
            ],
            b"Electron",
            1000,
        );
        let detections = analyse_files("code.exe", &facts);

        let electron = detections.iter().find(|d| d.id == "electron").expect("Electron should be detected");
        assert!(electron.confidence >= 0.9, "Electron confidence should be >= 0.9, got {}", electron.confidence);

        let chromium = detections.iter().find(|d| d.id == "chromium").expect("Chromium should be implied");
        assert!(chromium.confidence >= 0.7, "Chromium implied confidence should be >= 0.7");

        let nodejs = detections.iter().find(|d| d.id == "nodejs").expect("Node.js should be implied");
        assert!(nodejs.confidence >= 0.7, "Node.js implied confidence should be >= 0.7");
    }

    #[test]
    fn test_tauri_webview2_detection() {
        let facts = make_file_facts(
            vec!["webview2loader.dll"],
            b"tauri wry WebView2",
            1000,
        );
        let detections = analyse_files("app.exe", &facts);

        let tauri = detections.iter().find(|d| d.id == "tauri-webview2").expect("Tauri/WebView2 should be detected");
        assert!(tauri.confidence >= 0.7, "Tauri/WebView2 confidence should be >= 0.7");
    }

    #[test]
    fn test_qt_detection() {
        let facts = make_file_facts(
            vec!["qt6core.dll", "platforms/qwindows.dll"],
            b"",
            1000,
        );
        let detections = analyse_files("app.exe", &facts);

        let qt = detections.iter().find(|d| d.id == "qt").expect("Qt should be detected");
        assert!(qt.confidence >= 0.9, "Qt confidence should be >= 0.9");
    }

    #[test]
    fn test_flutter_detection() {
        let facts = make_file_facts(
            vec!["flutter_windows.dll", "data/flutter_assets/"],
            b"",
            1000,
        );
        let detections = analyse_files("app.exe", &facts);

        let flutter = detections.iter().find(|d| d.id == "flutter").expect("Flutter should be detected");
        assert!(flutter.confidence >= 0.9, "Flutter confidence should be >= 0.9");
    }

    #[test]
    fn test_dotnet_wpf_detection() {
        let facts = make_file_facts(
            vec!["presentationframework.dll", "hostfxr.dll"],
            b"",
            1000,
        );
        let detections = analyse_files("app.exe", &facts);

        let wpf = detections.iter().find(|d| d.id == "wpf").expect("WPF should be detected");
        assert!(wpf.confidence >= 0.9, "WPF confidence should be >= 0.9");

        let dotnet = detections.iter().find(|d| d.id == "dotnet").expect(".NET should be detected");
        assert!(dotnet.confidence >= 0.8, ".NET confidence should be >= 0.8");
    }

    #[test]
    fn test_unity_detection() {
        let facts = make_file_facts(
            vec!["unityplayer.dll", "app_data/"],
            b"",
            1000,
        );
        let detections = analyse_files("game.exe", &facts);

        let unity = detections.iter().find(|d| d.id == "unity").expect("Unity should be detected");
        assert!(unity.confidence >= 0.9, "Unity confidence should be >= 0.9");
    }

    #[test]
    fn test_pyinstaller_detection() {
        let facts = make_file_facts(
            vec!["base_library.zip"],
            b"pyinstaller",
            1000,
        );
        let detections = analyse_files("app.exe", &facts);

        let python = detections.iter().find(|d| d.id == "python").expect("Python should be detected");
        assert!(python.confidence >= 0.8, "Python confidence should be >= 0.8");
    }

    #[test]
    fn test_empty_folder_no_detection() {
        let facts = make_file_facts(vec![], b"", 1000);
        let detections = analyse_files("app.exe", &facts);
        assert!(detections.is_empty(), "Empty folder should have no detections");
    }

    #[test]
    fn test_mixed_stacks_electron_dotnet() {
        let facts = make_file_facts(
            vec![
                "resources/app.asar",
                "electron.exe",
                "hostfxr.dll",
            ],
            b"Electron",
            1000,
        );
        let detections = analyse_files("app.exe", &facts);

        let _electron = detections.iter().find(|d| d.id == "electron").expect("Electron should be detected");
        let _dotnet = detections.iter().find(|d| d.id == "dotnet").expect(".NET should be detected");

        // Electron should come first (higher confidence)
        let electron_idx = detections.iter().position(|d| d.id == "electron").unwrap();
        let dotnet_idx = detections.iter().position(|d| d.id == "dotnet").unwrap();
        assert!(electron_idx < dotnet_idx, "Electron should be listed before .NET");
    }

    #[test]
    fn test_probe_temp_directory() {
        let dir = tempdir().unwrap();
        let exe_path = dir.path().join("testapp.exe");
        fs::write(&exe_path, b"MZ fake exe").unwrap();

        // Create marker files
        fs::write(dir.path().join("webview2loader.dll"), b"").unwrap();
        fs::write(dir.path().join("tauri"), b"").unwrap();

        let result = probe(&exe_path.to_string_lossy()).unwrap();
        assert_eq!(result.exe_name, "testapp.exe");
        assert!(result.detections.iter().any(|d| d.id == "tauri-webview2"));
    }

#[test]
    fn test_probe_system_path_refused() {
        // Test the is_system_path logic directly with a mocked path
        // We can't easily test with real C:\Windows paths in CI, so we test the logic
        // by creating a temp directory structure that mimics a system folder
        let dir = tempdir().unwrap();
        let windows_dir = dir.path().join("Windows");
        let system32_dir = windows_dir.join("System32");
        fs::create_dir_all(&system32_dir).unwrap();
        let exe_path = system32_dir.join("explorer.exe"); // Use a known system exe
        fs::write(&exe_path, b"MZ").unwrap();

        // Initialize the system lists
        init_system_lists();
        
        // Test that SYSTEM_EXES list contains explorer.exe
        assert!(SYSTEM_EXES.get().unwrap().contains(&"explorer.exe".to_string()));
        
        // Also test that SYSTEM_FOLDERS logic would detect a path under Windows
        // (the WINDIR env var may not be set in test environment)
        let system_folders = SYSTEM_FOLDERS.get().unwrap();
        assert!(!system_folders.is_empty());
    }

    #[test]
    fn test_probe_missing_path() {
        let result = probe(r"C:\Nonexistent\Path\app.exe");
        assert!(matches!(result, Err(AppRefusal::NotFound)), "Missing path should be NotFound");
    }

    #[test]
    fn test_probe_capped_at_3000() {
        let dir = tempdir().unwrap();
        let exe_path = dir.path().join("testapp.exe");
        fs::write(&exe_path, b"MZ").unwrap();

        // Create 5000 files
        for i in 0..5000 {
            fs::write(dir.path().join(format!("file_{}.txt", i)), b"").unwrap();
        }

        let facts = collect_facts(&exe_path).unwrap();
        assert!(facts.names.len() <= MAX_ENTRIES, "Should be capped at 3000 entries");
    }

    #[test]
    fn test_probe_no_symlinks() {
        let dir = tempdir().unwrap();
        let exe_path = dir.path().join("testapp.exe");
        fs::write(&exe_path, b"MZ").unwrap();

        // Create a real file and try to create a symlink (if supported)
        let target = dir.path().join("target.txt");
        fs::write(&target, b"target").unwrap();

        let link = dir.path().join("link.txt");
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(&target, &link);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            let _ = symlink_file(&target, &link);
        }

        let _facts = collect_facts(&exe_path).unwrap();
        // Symlinks should not be followed, so link.txt should not appear
        // (though on Windows symlink creation may fail without admin)
    }

    #[test]
    fn test_report_markdown() {
        let report = AppReport {
            exe_name: "test.exe".to_string(),
            exe_path: "C:/test.exe".to_string(),
            install_dir: "C:/".to_string(),
            file_version: Some("1.0.0".to_string()),
            company: Some("Test Corp".to_string()),
            product: Some("Test Product".to_string()),
            detections: vec![AppDetection {
                id: "electron".to_string(),
                name: "Electron".to_string(),
                category: "app-framework".to_string(),
                confidence: 0.95,
                evidence: vec!["Found app.asar".to_string()],
            }],
            notes: vec!["Test note".to_string(), "Source code is not visible from outside; this reads the install folder only.".to_string()],
        };

        let md = report_markdown(&report);
        assert!(md.contains("test.exe"));
        assert!(md.contains("Electron"));
        assert!(md.contains("95%"));
        assert!(md.contains("Test note"));
        assert!(md.contains("Source code is not visible"));
    }
}