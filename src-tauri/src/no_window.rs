//! Start console programs (python, node, gh, taskkill, reg...) without a console window.
//!
//! The release `nexus.exe` is a GUI program with no console of its own (`main.rs`). A child console program started from
//! a process with no console gets a NEW console window unless it is created with CREATE_NO_WINDOW, so every such spawn
//! goes through `command()`. On other platforms it is plain `Command::new`.

use std::ffi::OsStr;
use std::process::Command;

/// `Command::new(program)` that never shows a console window.
pub fn command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut c = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_a_command_for_the_given_program() {
        let c = super::command("python");
        assert_eq!(c.get_program(), "python");
    }
}
