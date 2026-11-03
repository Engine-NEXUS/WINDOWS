// A release build is a GUI program: no console window (a console here showed up at every logon and closing it killed
// NEXUS). Debug builds keep the console. `lib.rs` has the same attribute but it only counts in the binary crate.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nexus_lib::run()
}
