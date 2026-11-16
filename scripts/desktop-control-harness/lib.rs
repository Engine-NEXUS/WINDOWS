// Actual pure production routing modules; no application/OS input backend.
#[path = "../../src-tauri/src/desktop_control.rs"]
pub mod desktop_control;
#[cfg(windows)]
#[path = "../../src-tauri/src/desktop_native.rs"]
pub mod desktop_native;
pub mod shortcuts;
pub mod browser_url {
    pub fn foreground_window_title() -> Option<String> {
        None
    }
    pub fn foreground_process_name() -> Option<String> {
        None
    }
    pub fn foreground_process_path() -> Option<std::path::PathBuf> {
        None
    }
}
pub mod ghost {
    pub struct GhostDrillGuard;
    impl GhostDrillGuard {
        pub fn new() -> Self {
            Self
        }
    }
    pub fn drop_followups() {}

    pub fn stop_requested() -> bool {
        false
    }
    pub fn session_active() -> bool {
        false
    }
}
pub mod choice_boundary;
pub mod orchestration_boundary;
pub mod app_registry {
    pub enum LaunchMethod {
        Exe { path: String },
        Aumid { aumid: String },
        Other,
    }
    pub struct AppEntry {
        pub launch: LaunchMethod,
    }
    pub fn lookup_strict(_: &str) -> Option<AppEntry> {
        None
    }
    pub fn window_aumid(_: isize) -> Option<String> {
        None
    }
}
#[path = "../../src-tauri/src/ghost_center.rs"]
pub mod ghost_center;
#[path = "../../src-tauri/src/ghost_learning.rs"]
pub mod ghost_learning;
pub mod desktop_choices {
    include!(concat!(env!("OUT_DIR"), "/choices.rs"));
}
pub mod network {
    pub fn uuid_v4() -> String {
        "test-token".into()
    }
}
pub mod app_dirs {
    pub const APP_ID: &str = "test.desktop-control";
}
pub mod commands {
    pub fn get_pending_sidebar_view() -> Result<Option<String>, String> {
        Ok(None)
    }
    pub fn read_speaker_verification<R: crate::choice_boundary::Runtime>(
        _: &crate::choice_boundary::AppHandle<R>,
    ) -> bool {
        true
    }
    pub async fn show_sidebar_view<R: crate::choice_boundary::Runtime>(
        _: crate::choice_boundary::AppHandle<R>,
        _: String,
        _: Option<String>,
    ) -> Result<(), String> {
        Ok(())
    }
}
#[path = "../../src-tauri/src/dictation.rs"]
pub mod dictation;
#[path = "../../src-tauri/src/dictation_text.rs"]
pub mod dictation_text;
pub mod live;
#[cfg(test)]
mod regressions {
    use crate::shortcuts::{
        keys::Chord,
        map::{Action, SEED},
        router::*,
    };
    use std::cell::Cell;
    struct EnvTest {
        time: Cell<u64>,
        sends: Cell<usize>,
        vanished: bool,
    }
    impl Env for EnvTest {
        fn foreground(&self) -> Option<Foreground> {
            if self.sends.get() > 0 && self.vanished {
                return None;
            }
            Some(Foreground {
                exe: if self.sends.get() > 0 {
                    "other.exe"
                } else {
                    "msedge.exe"
                }
                .into(),
                title: "changed".into(),
                hwnd: if self.sends.get() > 0 { 9 } else { 7 },
                elevated: false,
            })
        }
        fn send(&self, _: &Chord) -> Result<(), String> {
            self.sends.set(self.sends.get() + 1);
            Ok(())
        }
        fn sleep_ms(&self, ms: u64) {
            self.time.set(self.time.get() + ms)
        }
        fn now_ms(&self) -> u64 {
            self.time.get()
        }
        fn stop_requested(&self) -> bool {
            false
        }
        fn self_elevated(&self) -> bool {
            false
        }
        fn is_protected(&self, _: &str, _: &str) -> bool {
            false
        }
    }
    #[test]
    fn unrelated_window_after_dispatch_is_not_success() {
        let e = EnvTest {
            time: Cell::new(0),
            sends: Cell::new(0),
            vanished: false,
        };
        assert!(!matches!(
            run(&e, &SEED, Action::NewTab, None),
            Outcome::Sent {
                verified: Verified::Yes,
                ..
            }
        ));
        assert_eq!(e.sends.get(), 1);
    }
    #[test]
    fn disappearing_foreground_is_not_success() {
        let e = EnvTest {
            time: Cell::new(0),
            sends: Cell::new(0),
            vanished: true,
        };
        assert!(!matches!(
            run(&e, &SEED, Action::NewTab, None),
            Outcome::Sent {
                verified: Verified::Yes,
                ..
            }
        ));
        assert_eq!(e.sends.get(), 1);
    }
}

#[cfg(test)]
mod request_regressions {
    use super::shortcuts::{
        keys::Chord,
        map::{Action, SEED},
        router::*,
    };
    use std::{
        cell::Cell,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct Slow<'a> {
        time: Cell<u64>,
        sends: Cell<usize>,
        cancel: &'a AtomicBool,
        cancel_during_lookup: bool,
    }
    impl Env for Slow<'_> {
        fn foreground(&self) -> Option<Foreground> {
            Some(Foreground {
                exe: "msedge.exe".into(),
                title: "fixture".into(),
                hwnd: 1,
                elevated: false,
            })
        }
        fn send(&self, _: &Chord) -> Result<(), String> {
            self.sends.set(self.sends.get() + 1);
            Ok(())
        }
        fn sleep_ms(&self, n: u64) {
            self.time.set(self.time.get() + n)
        }
        fn now_ms(&self) -> u64 {
            self.time.get()
        }
        fn stop_requested(&self) -> bool {
            false
        }
        fn self_elevated(&self) -> bool {
            false
        }
        fn is_protected(&self, _: &str, _: &str) -> bool {
            false
        }
        fn effect_snapshot(&self, _: Action) -> Option<crate::desktop_control::EffectState> {
            if self.cancel_during_lookup {
                self.cancel.store(true, Ordering::Relaxed)
            } else {
                self.time.set(self.time.get().max(2501))
            };
            None
        }
    }
    #[test]
    fn cancellation_during_lookup_sends_nothing() {
        let c = AtomicBool::new(false);
        let e = Slow {
            time: Cell::new(0),
            sends: Cell::new(0),
            cancel: &c,
            cancel_during_lookup: true,
        };
        assert!(matches!(
            run_cancelled(&e, &SEED, Action::NewTab, None, &c),
            Outcome::Aborted(Abort::Stopped)
        ));
        assert_eq!(e.sends.get(), 0);
    }
    #[test]
    fn expired_lookup_sends_nothing() {
        let c = AtomicBool::new(false);
        let e = Slow {
            time: Cell::new(0),
            sends: Cell::new(0),
            cancel: &c,
            cancel_during_lookup: false,
        };
        assert!(matches!(
            run_cancelled(&e, &SEED, Action::NewTab, None, &c),
            Outcome::Failed(_)
        ));
        assert_eq!(e.sends.get(), 0);
    }
    #[test]
    fn control_nine_means_last_browser_tab() {
        use crate::desktop_control::*;
        let mut b = EffectState::empty(WindowIdentity {
            hwnd: 1,
            pid: 1,
            created: 1,
            exe: "msedge.exe".into(),
        });
        b.tab_ids = Some((0..12).map(|i| vec![i]).collect());
        assert_eq!(
            expected_effect(Action::GotoTab, Some(9), &b),
            EffectSpec::TabSelected(Some(11))
        );
    }
}

#[cfg(test)]
mod process_identity_regression {
    use crate::{
        desktop_control::WindowIdentity,
        shortcuts::{
            keys::Chord,
            map::{Action, SEED},
            router::*,
        },
    };
    use std::cell::Cell;
    struct Restart {
        identities: Cell<usize>,
        sends: Cell<usize>,
    }
    impl Env for Restart {
        fn identity_required(&self) -> bool {
            true
        }
        fn window_identity(&self) -> Option<WindowIdentity> {
            let n = self.identities.get();
            self.identities.set(n + 1);
            Some(WindowIdentity {
                hwnd: 1,
                pid: 2,
                created: if n == 0 { 3 } else { 4 },
                exe: "msedge.exe".into(),
            })
        }
        fn foreground(&self) -> Option<Foreground> {
            Some(Foreground {
                hwnd: 1,
                exe: "msedge.exe".into(),
                title: "fixture".into(),
                elevated: false,
            })
        }
        fn send(&self, _: &Chord) -> Result<(), String> {
            self.sends.set(self.sends.get() + 1);
            Ok(())
        }
        fn sleep_ms(&self, _: u64) {}
        fn now_ms(&self) -> u64 {
            0
        }
        fn stop_requested(&self) -> bool {
            self.sends.get() > 0
        }
        fn self_elevated(&self) -> bool {
            false
        }
        fn is_protected(&self, _: &str, _: &str) -> bool {
            false
        }
    }
    #[test]
    fn reused_handle_in_new_process_lifetime_sends_nothing() {
        let e = Restart {
            identities: Cell::new(0),
            sends: Cell::new(0),
        };
        assert!(matches!(
            run(&e, &SEED, Action::NewTab, None),
            Outcome::Aborted(Abort::ForegroundChanged { .. })
        ));
        assert_eq!(e.sends.get(), 0);
    }
}
