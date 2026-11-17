#[path = "../../../src-tauri/src/shortcuts/keys.rs"]
pub mod keys;
#[path = "../../../src-tauri/src/shortcuts/map.rs"]
pub mod map;
#[path = "../../../src-tauri/src/shortcuts/router.rs"]
pub mod router;
// Native type-check boundary only; no real foreground, credentials or input are exercised.
pub fn enabled() -> bool {
    false
}
pub mod exec {
    pub fn is_protected_window(_: &str, _: &str) -> bool {
        true
    }
    pub fn do_action_cancelled(
        _: super::map::Action,
        _: Option<u32>,
        _: &std::sync::atomic::AtomicBool,
    ) -> super::router::Outcome {
        super::router::Outcome::Failed("boundary".into())
    }
}
