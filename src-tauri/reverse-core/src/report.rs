//! Markdown reports (GitHub-flavoured: tables, `[!WARNING]` callouts) for the sidebar and CLI.
//! Every report starts with the authorisation banner; a packed file's limitation comes right after it.

use crate::analyze::{Analysis, FileKind};
use crate::evidence::EvKind;
use crate::explain::{self, Explanation};
use crate::har::HarResult;
use crate::redact;
use crate::rules::Severity;

pub const BANNER: &str = "> [!NOTE]\n> **Static analysis only - nothing was run.** Use Reverse Lab only on software, traffic and systems you own or are explicitly authorised to analyse. Findings are leads for you to verify, not verdicts.\n\n";

/// Spoken when a request asks for cracking / keygens / licence bypass / exploit writing.
pub const REFUSAL: &str = "I can't help with cracking, keygens, licence bypass or exploit writing, sir. I can analyse a program statically and explain what it does.";

/// Short spoken acknowledgement before the work starts.
pub const ACK_STATIC: &str = "Analysing it statically, sir. Nothing will be run.";

fn esc(s: &str) -> String {
    redact::redact_for_llm(s).replace('|', "\\|").replace('\n', " ").replace('`', "'")
}

/// Escape only (no redaction): for values this crate DERIVED (paths, auth labels, operation names), which never
/// contain captured values.
fn plain(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ").replace('`', "'")
}

fn kb(n: u64) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1 << 20) as f64)
    } else {
        format!("{:.0} KB", (n as f64 / 1024.0).max(1.0))
    }
}

fn sev(s: Severity) -> &'static str {
    match s {
        Severity::High => "high",
        Severity::Medium => "medium",
        Severity::Low => "low",
        Severity::Info => "info",
    }
}

/// Spoken one-liner for a finished program analysis.
pub fn spoken_summary(a: &Analysis, e: &Explanation) -> String {
    let packed = a.packer.as_ref().map_or(false, |p| p.is_flagged());
    let high = a.capabilities.iter().filter(|h| h.severity == Severity::High).count();
    let mut s = String::new();
    if packed {
        s.push_str("This file looks packed, so I only see the unpacking stub. ");
    }
    if high > 0 {
        s.push_str(&format!("{high} high-risk capabilit{} found, details are on screen. ", if high == 1 { "y" } else { "ies" }));
    } else {
        s.push_str(&format!("{} Details are on screen. ", e.summary.trim_end_matches('.').to_string() + "."));
    }
    s.push_str("This is static analysis, not a verdict.");
    s
}

pub fn program_report(a: &Analysis, how: &str, e: &Explanation) -> String {
    let mut s = String::from(BANNER);
    s.push_str(&format!("# Reverse Lab - {}\n\n", esc(&a.file_name)));
    for l in &e.limitations {
        s.push_str(&format!("> [!WARNING]\n> {l}\n\n"));
    }
    s.push_str(&format!("{}\n\n", e.summary));
    // facts
    s.push_str("| Fact | Value |\n|---|---|\n");
    let kind = match a.kind {
        FileKind::Pe => "Windows PE",
        FileKind::Asar => "Electron app.asar",
        FileKind::Unknown => "not recognised",
    };
    s.push_str(&format!("| Source | {how} |\n| Type | {kind} |\n| Size | {} |\n| SHA-256 | `{}` |\n", kb(a.size), a.sha256));
    if let Some(p) = &a.pe {
        s.push_str(&format!("| Architecture | {} {} |\n", p.machine_name, if p.is_dll { "DLL" } else { "executable" }));
        s.push_str(&format!("| Subsystem | {} |\n", p.subsystem_name));
        if let Some(c) = &p.version.company_name {
            s.push_str(&format!("| Company | {} |\n", esc(c)));
        }
        if let Some(c) = &p.version.product_name {
            s.push_str(&format!("| Product | {} |\n", esc(c)));
        }
        if let Some(c) = &p.version.file_version {
            s.push_str(&format!("| File version | {} |\n", esc(c)));
        }
        s.push_str(&format!(
            "| Hardening | ASLR {}, DEP {}, {} |\n",
            if p.aslr { "yes" } else { "no" },
            if p.dep { "yes" } else { "no" },
            if p.has_signature { "has an embedded signature (not verified here)" } else { "unsigned" }
        ));
        if p.has_clr {
            if let Some(d) = &a.dotnet {
                s.push_str(&format!("| .NET | runtime {} |\n", esc(&d.runtime_version)));
            }
        }
        if p.overlay_size > 0 {
            s.push_str(&format!("| Overlay | {} appended after the last section |\n", kb(p.overlay_size)));
        }
        if p.lenient_parse {
            s.push_str("| Parse | tolerant mode (the file's headers are unusual) |\n");
        }
    }
    if let Some(f) = &a.asar {
        s.push_str(&format!("| Electron package | {} {} ({} files) |\n", esc(f.package_name.as_deref().unwrap_or("?")), esc(f.package_version.as_deref().unwrap_or("")), f.file_count));
        if !f.native_modules.is_empty() {
            s.push_str(&format!("| Native modules | {} |\n", f.native_modules.iter().take(8).map(|m| esc(m)).collect::<Vec<_>>().join(", ")));
        }
    }
    s.push('\n');

    // capabilities
    let caps: Vec<_> = a.capabilities.iter().filter(|h| h.severity >= Severity::Low).collect();
    if !caps.is_empty() {
        s.push_str("## Capabilities found (rule-based)\n\n| Capability | Severity | ATT&CK | Evidence |\n|---|---|---|---|\n");
        for h in caps.iter().take(40) {
            let id = a.evidence.items().iter().find(|e| e.kind == EvKind::Rule && e.value == h.name).map(|e| e.id.as_str()).unwrap_or("");
            s.push_str(&format!("| {} | {} | {} | {} |\n", esc(&h.name), sev(h.severity), esc(&h.attack.join(", ")), id));
        }
        s.push('\n');
    }

    // explanation
    s.push_str("## What the evidence supports\n\n");
    if e.claims.is_empty() {
        s.push_str("_No statements could be tied to specific evidence._\n\n");
    } else {
        for c in &e.claims {
            s.push_str(&format!("- {} _({})_\n", esc(&c.text), c.evidence.join(", ")));
        }
        s.push('\n');
    }
    s.push_str("## Not determined\n\n");
    for n in &e.not_determined {
        s.push_str(&format!("- {}\n", esc(n)));
    }
    if !e.dropped.is_empty() {
        s.push_str(&format!("\n_{} statement(s) from the language model were discarded because the evidence did not support them._\n", e.dropped.len()));
    }
    s.push('\n');

    // imports + strings
    if let Some(p) = &a.pe {
        if !p.imports.is_empty() {
            let mut by: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
            for i in &p.imports {
                by.entry(i.dll.to_ascii_lowercase()).or_default().push(i.name.clone().unwrap_or_else(|| format!("#{}", i.ordinal.unwrap_or(0))));
            }
            s.push_str("## Imports\n\n| DLL | Functions |\n|---|---|\n");
            for (dll, f) in by.iter().take(25) {
                let shown: Vec<String> = f.iter().take(10).map(|x| esc(x)).collect();
                s.push_str(&format!("| {} | {}{} |\n", esc(dll), shown.join(", "), if f.len() > 10 { format!(" ... (+{})", f.len() - 10) } else { String::new() }));
            }
            s.push('\n');
        }
        if !p.sections.is_empty() {
            s.push_str("## Sections\n\n| Name | Raw size | Entropy | Flags |\n|---|---|---|---|\n");
            for sec in p.sections.iter().take(24) {
                s.push_str(&format!("| {} | {} | {:.2} | {}{} |\n", esc(&sec.name), kb(sec.raw_size as u64), sec.entropy, if sec.executable { "X" } else { "-" }, if sec.writable { "W" } else { "-" }));
            }
            s.push('\n');
        }
    }
    let strs: Vec<_> = a.evidence.items().iter().filter(|e| e.kind == EvKind::Str).take(15).collect();
    if !strs.is_empty() {
        s.push_str("## Interesting strings\n\n");
        for e in strs {
            let v: String = esc(&e.value).chars().take(120).collect();
            s.push_str(&format!("- `{v}` _({})_\n", e.id));
        }
        s.push('\n');
    }
    if !a.warnings.is_empty() {
        s.push_str("## Notes\n\n");
        for w in a.warnings.iter().take(8) {
            s.push_str(&format!("- {}\n", esc(w)));
        }
        s.push('\n');
    }
    s.push_str(&format!("_Evidence IDs (E-...) refer to the {} facts extracted from the file._\n", a.evidence.len()));
    s
}

pub fn har_report(r: &HarResult, how: &str) -> String {
    let mut s = String::from(BANNER);
    s.push_str("# Reverse Lab - API from captured traffic\n\n");
    s.push_str(&format!("Source: {how}. {} requests analysed ({} static assets and {} other entries skipped).\n\n", r.entries - r.skipped_static - r.skipped_other, r.skipped_static, r.skipped_other));
    s.push_str(&format!("**{} endpoint(s)** inferred. Credentials, cookies and identifiers were dropped before inference ({} sensitive values seen); the OpenAPI document contains **no example values**.\n\n", r.endpoints.len(), r.redactions));
    if !r.endpoints.is_empty() {
        s.push_str("| Method | Path | Requests | Auth | Notes |\n|---|---|---|---|---|\n");
        for e in r.endpoints.iter().take(60) {
            let mut notes = Vec::new();
            if !e.pagination.is_empty() {
                notes.push(format!("pagination: {}", e.pagination.join(", ")));
            }
            if e.graphql {
                notes.push("GraphQL".to_string());
            }
            s.push_str(&format!("| {} | `{}` | {} | {} | {} |\n", e.method, plain(&e.path), e.requests, plain(&e.auth.join(", ")), plain(&notes.join("; "))));
        }
        if r.endpoints.len() > 60 {
            s.push_str(&format!("\n_... and {} more_\n", r.endpoints.len() - 60));
        }
        s.push('\n');
    }
    if !r.graphql_ops.is_empty() {
        s.push_str("## GraphQL operations\n\n| Kind | Name | Calls | Top-level fields |\n|---|---|---|---|\n");
        for o in r.graphql_ops.iter().take(40) {
            s.push_str(&format!("| {} | {} | {} | {} |\n", o.kind, plain(&o.name), o.count, plain(&o.fields.join(", "))));
        }
        s.push('\n');
    }
    if !r.websockets.is_empty() {
        s.push_str("## WebSockets (listed, not modelled)\n\n");
        for w in r.websockets.iter().take(10) {
            s.push_str(&format!("- `{}`\n", plain(w)));
        }
        s.push('\n');
    }
    s.push_str("## How far to trust this\n\n- Path parameters that were only seen with one or two different values stay literal - capture more varied traffic to generalise them.\n- Schemas reflect only what appeared in this capture; fields that never appeared are missing.\n- Nothing was replayed or sent; this is derived purely from the file.\n\n");
    for w in &r.warnings {
        s.push_str(&format!("> [!WARNING]\n> {}\n\n", plain(w)));
    }
    s
}

/// Convenience: validate (or fall back) and render in one call.
pub fn explain_and_report(a: &Analysis, how: &str, raw_llm: Option<&str>) -> (Explanation, String) {
    let e = match raw_llm {
        Some(raw) => explain::validate(raw, a),
        None => explain::deterministic(a),
    };
    let md = program_report(a, how, &e);
    (e, md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::analyze_bytes;
    use crate::synth::build_pe;

    fn sample() -> Analysis {
        let pe = build_pe(
            &[(".text", vec![0x90; 300], 0x6000_0020), (".data", b"\x00http://example.test/a|b\x00".to_vec(), 0xC000_0040)],
            &[("KERNEL32.dll", &["VirtualAllocEx", "WriteProcessMemory", "CreateRemoteThread"])],
            false,
        );
        analyze_bytes(&pe, "inj|ector.exe")
    }

    #[test]
    fn every_program_report_starts_with_the_authorisation_banner() {
        let a = sample();
        let (_, md) = explain_and_report(&a, "test", None);
        assert!(md.starts_with(BANNER), "{md}");
        assert!(md.contains("Static analysis only") && md.contains("authorised"));
        assert!(md.contains("inject code into another process"));
        assert!(md.contains("E-RULE-"));
    }

    #[test]
    fn table_cells_are_escaped_and_secrets_scrubbed() {
        let a = sample();
        let (_, md) = explain_and_report(&a, "test", None);
        assert!(md.contains("inj\\|ector.exe"), "pipe in a file name must not break the table");
    }

    #[test]
    fn packed_report_carries_the_limitation_right_after_the_banner() {
        let pe = build_pe(&[("UPX0", vec![], 0xE000_0080), ("UPX1", vec![0xA5; 6000], 0xE000_0020)], &[("KERNEL32.dll", &["LoadLibraryA", "GetProcAddress"])], false);
        let a = analyze_bytes(&pe, "p.exe");
        let (_, md) = explain_and_report(&a, "test", Some("garbage"));
        let after = &md[BANNER.len()..];
        assert!(after.contains("packed or protected"));
        assert!(md.find("packed or protected").unwrap() < md.find("## Capabilities").unwrap_or(usize::MAX));
    }

    #[test]
    fn har_report_has_the_banner_and_no_example_values() {
        let har = serde_json::json!({"log": {"entries": [{"request": {"method": "GET", "url": "https://api.test/users/42?token=abcdef1234567890abcdef1234567890", "headers": [{"name": "Authorization", "value": "Bearer abcdef1234567890"}]}, "response": {"status": 200, "headers": [], "content": {"mimeType": "application/json", "text": "{\"id\":42,\"email\":\"a@b.c\"}"}}}]}});
        let r = crate::har::analyze_har(&serde_json::to_vec(&har).unwrap()).unwrap();
        let md = har_report(&r, "unit test");
        assert!(md.starts_with(BANNER));
        assert!(md.contains("/users/{userId}"));
        assert!(!md.contains("abcdef1234567890") && !md.contains("a@b.c"));
    }

    #[test]
    fn spoken_summary_mentions_packing_and_never_a_verdict() {
        let pe = build_pe(&[("UPX0", vec![], 0xE000_0080), ("UPX1", vec![0xA5; 6000], 0xE000_0020)], &[("KERNEL32.dll", &["LoadLibraryA", "GetProcAddress"])], false);
        let a = analyze_bytes(&pe, "p.exe");
        let e = explain::deterministic(&a);
        let s = spoken_summary(&a, &e);
        assert!(s.contains("packed") && s.contains("not a verdict"));
        assert!(!s.to_lowercase().contains("malware"));
    }

    #[test]
    fn refusal_and_ack_lines_are_fixed_and_short() {
        assert!(REFUSAL.len() < 200 && ACK_STATIC.len() < 80);
        assert!(REFUSAL.contains("cracking") && REFUSAL.contains("statically"));
    }

    #[test]
    fn secrets_embedded_in_a_binary_reach_neither_the_report_nor_the_llm_summary_nor_the_json() {
        let jwt = format!("eyJ{}.eyJ{}.{}", "hbGciOiJIUzI1NiJ9abcd", "zdWIiOiJ1c2VyMSIsInJvbGUiOiJhZG1pbiJ9", "c2lnbmF0dXJlc2lnbmF0dXJl");
        let aws = format!("AKIA{}", "ABCDEFGHIJKLMNOP");
        let blob = format!("\0password=hunter2hunter2\0{aws}\0Authorization: Bearer {jwt}\0https://user:pw123456@api.example.test/v1\0");
        let pe = build_pe(&[(".text", vec![0x90; 200], 0x6000_0020), (".data", blob.into_bytes(), 0xC000_0040)], &[("KERNEL32.dll", &["CreateFileW"])], false);
        let a = analyze_bytes(&pe, "secrets.exe");
        let (_, md) = explain_and_report(&a, "test", None);
        let summary = explain::evidence_summary(&a, explain::SUMMARY_CAP);
        let json = serde_json::to_string(&a).unwrap();
        for (what, text) in [("report", &md), ("llm summary", &summary), ("json", &json)] {
            for needle in ["hunter2hunter2", aws.as_str(), jwt.as_str(), "pw123456"] {
                assert!(!text.contains(needle), "{what} leaked {needle}");
            }
        }
        assert!(summary.len() <= explain::SUMMARY_CAP * 2, "the model never receives more than the cap");
    }
}
