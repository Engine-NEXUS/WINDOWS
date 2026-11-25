#[path = "../../../src-tauri/src/live/safety.rs"]
pub mod safety;
pub mod commands {
    pub mod keyboard {
        pub fn type_text(_: &str) -> Result<(), String> {
            Err("boundary".into())
        }
        pub fn press_key(_: &str) -> Result<(), String> {
            Err("boundary".into())
        }
        pub fn press_hotkey(_: &[&str]) -> Result<(), String> {
            Err("boundary".into())
        }
    }
    pub mod window {
        #[cfg(windows)]
        pub fn window_process_elevated(_: windows::Win32::Foundation::HWND) -> bool {
            true
        }
        pub fn our_process_elevated() -> bool {
            false
        }
        pub fn elevated_block_reason(_: bool, _: bool) -> Option<String> {
            None
        }
        #[cfg(windows)]
        pub fn focus_window(_: windows::Win32::Foundation::HWND) -> bool {
            false
        }
    }
    pub mod mouse {
        pub fn current_pos() -> Option<(i32, i32)> {
            None
        }
        pub fn click_at_checked(
            _: i32,
            _: i32,
            _: impl Fn() -> bool,
            _: impl Fn() -> Result<(), String>,
        ) -> Result<(), String> {
            Err("test boundary".into())
        }
    }
}
