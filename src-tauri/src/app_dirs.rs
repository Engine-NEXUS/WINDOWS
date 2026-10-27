//! NEXUS's own data directory for callers that have no `AppHandle`.
//!
//! Where a Tauri `AppHandle` exists, always prefer the resolver
//! (`app.path().app_data_dir()`). This module is the shared env-derived
//! fallback so the codebase stops carrying four divergent copies of it:
//!
//! | platform  | root |
//! |---|---|
//! | Windows   | `%APPDATA%\com.nexus.assistant` |
//! | macOS     | `~/Library/Application Support/com.nexus.assistant` |
//! | Linux/else| `~/.config/com.nexus.assistant` |
//!
//! `dirs_next::data_dir()` is deliberately not used: it returns
//! `~/.local/share` on Linux, while the pre-existing envless fallbacks
//! persisted user data under `~/.config` and NEXUS must keep reading them.

pub(crate) fn nexus_data_dir() -> Option<std::path::PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Some(std::path::PathBuf::from(appdata).join("com.nexus.assistant"));
    }
    let home = std::env::var("HOME").ok()?;
    #[cfg(target_os = "macos")]
    let base = std::path::PathBuf::from(home).join("Library").join("Application Support");
    #[cfg(not(target_os = "macos"))]
    let base = std::path::PathBuf::from(home).join(".config");
    Some(base.join("com.nexus.assistant"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_always_ends_in_the_nexus_identifier() {
        let dir = nexus_data_dir().expect("a data root or HOME exists in every test env");
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some("com.nexus.assistant")
        );
    }
}
