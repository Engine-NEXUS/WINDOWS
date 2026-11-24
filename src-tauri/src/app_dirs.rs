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

/// Compile-time identity: installed personal builds never switch profiles through an environment variable.
#[cfg(feature = "contributor")]
pub(crate) const APP_ID: &str = "com.nexus.assistant.contributor";
#[cfg(not(feature = "contributor"))]
pub(crate) const APP_ID: &str = "com.nexus.assistant";

pub(crate) const DEEP_LINK_SCHEME: &str = if cfg!(feature = "contributor") { "nexus-dev" } else { "nexus" };

pub(crate) fn nexus_data_dir() -> Option<std::path::PathBuf> {
    if cfg!(feature = "contributor") {
        return dirs_next::data_dir().map(|base| base.join(APP_ID));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Some(std::path::PathBuf::from(appdata).join(APP_ID));
    }
    let home = std::env::var("HOME").ok()?;
    #[cfg(target_os = "macos")]
    let base = std::path::PathBuf::from(home).join("Library").join("Application Support");
    #[cfg(not(target_os = "macos"))]
    let base = std::path::PathBuf::from(home).join(".config");
    Some(base.join(APP_ID))
}

/// A process-lifetime reservation prevents two contributor launches racing before lazy sidecars start.
/// Existing personal processes are also checked by the CLI; no process is killed or adopted.
pub(crate) fn claim_contributor_session() -> bool {
    static SESSION: std::sync::OnceLock<Option<std::net::TcpListener>> = std::sync::OnceLock::new();
    SESSION.get_or_init(|| {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 39216)).ok()?;
        for port in [39217, 39218, 39219, 39222] {
            if std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
                std::time::Duration::from_millis(200),
            ).is_ok() { return None; }
        }
        Some(listener)
    }).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_always_ends_in_the_nexus_identifier() {
        let dir = nexus_data_dir().expect("a data root or HOME exists in every test env");
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(APP_ID)
        );
    }
}
