//! Test-quiet mode: `NEXUS_QUIET=1` in the environment of the process (off by default, never a setting).
//!
//! For running NEXUS on a machine where nobody should hear it or have the screen touched, such as a server that is
//! being tested: every playback stream is silent (the timing, captions and turn logic run exactly as normal), the
//! master volume is left alone, the screen is never asked to stay awake, and the display is never turned off. It
//! changes nothing about what the assistant decides or says.

use std::sync::OnceLock;

/// True when `NEXUS_QUIET` is `1`, `true` or `yes`. Read once.
pub fn on() -> bool {
    static Q: OnceLock<bool> = OnceLock::new();
    *Q.get_or_init(|| parse(std::env::var("NEXUS_QUIET").ok().as_deref()))
}

fn parse(v: Option<&str>) -> bool {
    matches!(v.map(|s| s.trim().to_lowercase()).as_deref(), Some("1" | "true" | "yes"))
}

/// The playback gain to use: `g` normally, 0 in quiet mode.
pub fn gain(g: f32) -> f32 {
    if on() {
        0.0
    } else {
        g
    }
}

/// Silence a sink at creation in quiet mode (a no-op otherwise).
pub fn mute_sink(sink: &rodio::Sink) {
    if on() {
        sink.set_volume(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_explicit_yes_turns_it_on() {
        for on in ["1", "true", "TRUE", " yes "] {
            assert!(parse(Some(on)), "{on}");
        }
        for off in ["", "0", "false", "no", "quiet", "2"] {
            assert!(!parse(Some(off)), "{off}");
        }
        assert!(!parse(None));
    }
}
