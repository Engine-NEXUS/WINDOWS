// Compiles only the actual pure lifecycle/power-worker helpers, not NEXUS or Tauri.
#[path = "../../src-tauri/src/agent_lifecycle.rs"]
pub mod agent_lifecycle;
#[path = "../../src-tauri/src/power_thread.rs"]
pub mod power_thread;
#[path = "../../src-tauri/src/pii_filter.rs"]
pub mod pii_filter;
#[path = "../../src-tauri/src/session_verdict.rs"]
pub mod session_verdict;
#[path = "../../src-tauri/src/agent_tracker.rs"]
pub mod agent_tracker;
#[path = "../../src-tauri/src/session_observer.rs"]
pub mod session_observer;
pub mod watch_runner { include!(concat!(env!("OUT_DIR"),"/watch_policy.rs")); }
pub mod hook_parser { include!(concat!(env!("OUT_DIR"),"/hook_parser.rs")); }
// Boundary enums only; the actual countdown/observer/verdict/tracker policy is compiled above.
pub mod power_cmd {
    #[derive(Debug,Clone,Copy,PartialEq,Eq)]
    pub enum PowerKind{Shutdown,Restart,Sleep,Hibernate,Lock,SignOut}
    impl PowerKind{pub fn as_str(self)->&'static str{match self{Self::Shutdown=>"shutdown",Self::Restart=>"restart",Self::Sleep=>"sleep",Self::Hibernate=>"hibernate",Self::Lock=>"lock",Self::SignOut=>"signout"}}}
}
pub mod power_plan {
    #[derive(Debug,Clone,Copy,PartialEq,Eq)]
    pub enum WatchPowerMode{Off,Dry,On}
}
#[cfg(test)]
mod runtime_shim;
#[cfg(test)]
mod tests;
