//! Standalone pure helper tests; does not compile or launch the NEXUS application.
extern crate self as dirs_next;
pub fn data_dir() -> Option<std::path::PathBuf> { Some(std::path::PathBuf::from("/isolated-data")) }
#[path = "../src-tauri/src/app_dirs.rs"]
mod app_dirs;

#[test]
fn build_identity_and_credential_namespace_are_distinct() {
    let expected = if cfg!(feature = "contributor") { "com.nexus.assistant.contributor" } else { "com.nexus.assistant" };
    assert_eq!(app_dirs::APP_ID, expected);
    assert_eq!(app_dirs::DEEP_LINK_SCHEME, if cfg!(feature = "contributor") { "nexus-dev" } else { "nexus" });
}

#[cfg(feature = "contributor")]
#[test]
fn contributor_fallback_uses_the_native_data_root_and_never_the_personal_directory() {
    let path = app_dirs::nexus_data_dir().unwrap();
    assert_eq!(path, std::path::PathBuf::from("/isolated-data/com.nexus.assistant.contributor"));
}
