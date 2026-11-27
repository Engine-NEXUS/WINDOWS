//! Reverse Lab - STATIC reverse-engineering triage for the user's own programs and captured HAR traffic.
//!
//! Hard rules (enforced by tests / grep gate, see `docs/features/91-reverse-lab.md`):
//!   * the target is only ever READ as bytes - never executed, loaded, patched or replayed;
//!   * secrets never leave the device: HAR data is redacted before inference/output/LLM;
//!   * every LLM statement must cite evidence the analysis actually produced (`evidence`, `explain`);
//!   * no decompilation, cracking, keygen, licence bypass or exploit generation.
//!
//! Everything except `target::foreground_exe_path` is a pure function over bytes, so it is fully
//! unit-testable on any OS.

pub mod runner;
pub mod target;

use crate::center::{ConfirmKind, SubCenter, Validity};
use serde_json::Value;

/// Reverse Lab sub-center (registered in `center::sub_center_for`). Read-only analysis: the only gate is
/// "there must be a target"; the routine acknowledgement is spoken before the work starts.
pub struct ReverseCenter;

impl SubCenter for ReverseCenter {
    fn name(&self) -> &'static str {
        "ReverseCenter"
    }

    fn validate(&self, action: &str, slots: &Value) -> Validity {
        let target = slots.get("target").and_then(|v| v.as_str()).unwrap_or("").trim();
        match action {
            "reverse_lab" | "reverse_har" if target.is_empty() => Validity::NeedSlot {
                slot: "target",
                prompt: if action == "reverse_har" { "Which capture should I turn into an API spec, sir?".to_string() } else { "Which program should I analyse, sir?".to_string() },
            },
            "reverse_lab" | "reverse_har" => Validity::Ok,
            _ => Validity::Invalid { reason: "unknown_action", prompt: "I can't do that with Reverse Lab, sir.".to_string() },
        }
    }

    fn confirm_kind(&self, _action: &str, _slots: &Value) -> ConfirmKind {
        ConfirmKind::RepeatBack { ack: nexus_reverse::report::ACK_STATIC.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_requires_a_target_and_acknowledges_statically() {
        let c = ReverseCenter;
        assert_eq!(c.name(), "ReverseCenter");
        assert!(matches!(c.validate("reverse_lab", &serde_json::json!({"target": ""})), Validity::NeedSlot { slot: "target", .. }));
        assert_eq!(c.validate("reverse_lab", &serde_json::json!({"target": "app"})), Validity::Ok);
        assert_eq!(c.validate("reverse_har", &serde_json::json!({"target": "latest"})), Validity::Ok);
        assert!(matches!(c.validate("nonsense", &serde_json::json!({})), Validity::Invalid { .. }));
        match c.confirm_kind("reverse_lab", &serde_json::json!({})) {
            ConfirmKind::RepeatBack { ack } => assert!(ack.contains("Nothing will be run")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn spec_mapping() {
        use super::runner::spec_for;
        use super::target::TargetSpec;
        assert_eq!(spec_for("app", false), TargetSpec::ForegroundApp);
        assert_eq!(spec_for("download", false), TargetSpec::JustDownloaded);
        assert_eq!(spec_for("latest", true), TargetSpec::LatestHar);
        assert_eq!(spec_for("clipboard", true), TargetSpec::Clipboard);
        assert_eq!(spec_for("path:C:\\x\\a.exe", false), TargetSpec::Path("C:\\x\\a.exe".to_string()));
        assert_eq!(spec_for("weird", true), TargetSpec::LatestHar);
        assert_eq!(spec_for("weird", false), TargetSpec::ForegroundApp);
    }
}

