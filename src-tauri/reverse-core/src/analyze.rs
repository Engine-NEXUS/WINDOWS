//! The analysis pipeline: bytes in, [`Analysis`] out. Pure, read-only, never executes anything.

use crate::dotnet::{self, DotnetFacts};
use crate::electron::{self, AsarFacts};
use crate::evidence::{EvKind, EvidenceSet};
use crate::packer::{self, PackerVerdict};
use crate::pe::{self, PeFacts};
use crate::rules::{self, FactBag, FactKind, RuleHit};
use crate::strings::{self, FoundString, StrClass};
use serde::Serialize;
use std::path::Path;

/// How much of a file is scanned for strings (sections' raw bytes; whole file for non-PE).
pub const STRING_SCAN_CAP: usize = 64 * 1024 * 1024;
pub const OVERLAY_SCAN_CAP: usize = 4 * 1024 * 1024;
pub const MIN_STRING_LEN: usize = 5;
pub const MAX_SHOWN_STRINGS: usize = 200;

// Evidence budgets per kind so one noisy table cannot starve the others.
const EV_IMPORTS: usize = 2500;
const EV_EXPORTS: usize = 500;
const EV_STRINGS: usize = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Pe,
    Asar,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub kind: FileKind,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub pe: Option<PeFacts>,
    pub dotnet: Option<DotnetFacts>,
    /// Strings worth showing (non-plain classes), already redacted.
    pub strings: Vec<FoundString>,
    pub string_total: usize,
    pub packer: Option<PackerVerdict>,
    pub capabilities: Vec<RuleHit>,
    pub asar: Option<AsarFacts>,
    pub evidence: EvidenceSet,
    pub warnings: Vec<String>,
}

/// Characteristics the rules can test (`characteristic: <name>`).
fn characteristics(pe: &PeFacts, packer: &PackerVerdict) -> Vec<&'static str> {
    let mut c = Vec::new();
    if pe.is_dll {
        c.push("is_dll");
    }
    if pe.is_driver {
        c.push("is_driver");
    }
    if pe.has_clr {
        c.push("has_clr");
    }
    if packer.is_flagged() {
        c.push("packed");
    }
    if pe.imports.is_empty() {
        c.push("no_imports");
    }
    if pe.overlay_size > 0 {
        c.push("has_overlay");
    }
    if pe.sections.iter().any(|s| s.executable && s.writable) {
        c.push("writable_exec_section");
    }
    if pe.sections.iter().any(|s| s.executable && s.entropy >= 7.2 && s.raw_size >= 4096) {
        c.push("high_entropy_exec");
    }
    if pe.tls_callbacks > 0 {
        c.push("has_tls_callbacks");
    }
    if pe.requested_execution_level.as_deref() == Some("requireAdministrator") {
        c.push("requires_admin");
    }
    match pe.subsystem {
        Some(2) => c.push("is_gui"),
        Some(3) => c.push("is_console"),
        _ => {}
    }
    c.push(if pe.has_signature { "has_signature" } else { "unsigned" });
    c
}

fn build_bag(pe: &PeFacts, dn: Option<&DotnetFacts>, all: &[FoundString], chars: &[&'static str]) -> FactBag {
    let mut bag = FactBag::default();
    for i in &pe.imports {
        bag.dlls.insert(i.dll.to_ascii_lowercase());
        if let Some(n) = &i.name {
            bag.apis.entry(n.to_ascii_lowercase()).or_insert_with(|| n.clone());
        }
    }
    if let Some(d) = dn {
        // P/Invoke targets and API-looking identifiers live in the .NET #Strings heap
        for id in &d.identifiers {
            bag.apis.entry(id.to_ascii_lowercase()).or_insert_with(|| id.clone());
        }
        bag.strings.extend(d.user_strings.iter().map(|u| crate::redact::redact_text(u)));
    }
    bag.strings.extend(all.iter().map(|s| s.value.clone()));
    bag.sections = pe.sections.iter().map(|s| s.name.to_ascii_lowercase()).collect();
    bag.exports = pe.exports.iter().filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase())).collect();
    bag.chars = chars.iter().map(|c| c.to_string()).collect();
    bag
}

/// Link a matched fact to an evidence ID, creating one on demand where the fact (a plain string, a
/// characteristic) was not recorded yet.
fn evidence_for(ev: &mut EvidenceSet, kind: &FactKind, value: &str, rule: &str) -> Option<String> {
    match kind {
        FactKind::Api => ev
            .find(EvKind::Import, value)
            .map(|e| e.id.clone())
            .or_else(|| ev.find(EvKind::Dotnet, value).map(|e| e.id.clone()))
            .or_else(|| ev.add(EvKind::Dotnet, value.to_string(), format!(".NET identifier / P/Invoke name (matched by rule '{rule}')"))),
        FactKind::Dll => ev
            .items()
            .iter()
            .find(|e| e.kind == EvKind::Import && e.detail.eq_ignore_ascii_case(value))
            .map(|e| e.id.clone()),
        FactKind::Str => ev
            .find(EvKind::Str, value)
            .map(|e| e.id.clone())
            .or_else(|| ev.add(EvKind::Str, value.to_string(), format!("string matched by rule '{rule}'"))),
        FactKind::Section => ev.find(EvKind::Section, value).map(|e| e.id.clone()),
        FactKind::Export => ev.find(EvKind::Export, value).map(|e| e.id.clone()),
        FactKind::Char => ev
            .find(EvKind::Meta, &format!("characteristic: {value}"))
            .map(|e| e.id.clone())
            .or_else(|| ev.add(EvKind::Meta, format!("characteristic: {value}"), "derived from the PE headers")),
    }
}

fn scan_regions<'a>(bytes: &'a [u8], pe: Option<&PeFacts>) -> Vec<&'a [u8]> {
    let Some(pe) = pe else {
        return vec![&bytes[..bytes.len().min(STRING_SCAN_CAP)]];
    };
    let mut budget = STRING_SCAN_CAP;
    let mut out = Vec::new();
    for s in &pe.sections {
        let start = s.raw_offset as usize;
        if start >= bytes.len() || s.raw_size == 0 || budget == 0 {
            continue;
        }
        let end = start.saturating_add(s.raw_size as usize).min(bytes.len()).min(start + budget);
        out.push(&bytes[start..end]);
        budget = budget.saturating_sub(end - start);
    }
    // overlay: installers and self-extractors keep their payload here
    let raw_end = pe.file_size.saturating_sub(pe.overlay_size) as usize;
    if pe.overlay_size > 0 && raw_end < bytes.len() {
        out.push(&bytes[raw_end..bytes.len().min(raw_end + OVERLAY_SCAN_CAP)]);
    }
    out
}

/// Analyse `bytes` (never executes them). `file_name` is only used for display.
pub fn analyze_bytes(bytes: &[u8], file_name: &str) -> Analysis {
    let mut warnings = Vec::new();
    let mut ev = EvidenceSet::new();
    let sha256 = pe::sha256_hex(bytes);

    let pe_facts = match pe::parse(bytes) {
        Ok(f) => Some(f),
        Err(e) => {
            warnings.push(format!("Not analysed as a PE file: {e}"));
            None
        }
    };

    if let Some(p) = pe_facts.as_ref() {
        ev.add(EvKind::Meta, format!("{} {}", p.machine_name, if p.is_dll { "DLL" } else { "executable" }), format!("subsystem: {}", p.subsystem_name));
        for s in &p.sections {
            ev.add(
                EvKind::Section,
                s.name.clone(),
                format!("entropy {:.2}, raw {} B, virtual {} B, {}{}", s.entropy, s.raw_size, s.virtual_size, if s.executable { "X" } else { "-" }, if s.writable { "W" } else { "-" }),
            );
        }
        for (i, imp) in p.imports.iter().enumerate() {
            if i >= EV_IMPORTS {
                break;
            }
            let value = imp.name.clone().unwrap_or_else(|| format!("ordinal {}", imp.ordinal.unwrap_or(0)));
            ev.add(EvKind::Import, value, imp.dll.clone());
        }
        for (i, e) in p.exports.iter().enumerate() {
            if i >= EV_EXPORTS {
                break;
            }
            ev.add(EvKind::Export, e.name.clone().unwrap_or_else(|| format!("rva {:#x}", e.rva)), format!("rva {:#x}", e.rva));
        }
        if p.imports_truncated {
            warnings.push(format!("Import table is very large; only the first {} imports were read.", crate::pe::MAX_IMPORTS));
        }
        if p.overlay_size > 0 {
            ev.add(EvKind::Meta, format!("overlay {} bytes", p.overlay_size), "data appended after the last section");
        }
        if let Some(l) = &p.requested_execution_level {
            ev.add(EvKind::Meta, format!("requested execution level {l}"), "from the embedded manifest");
        }
        if let Some(pdb) = &p.pdb_path {
            ev.add(EvKind::Meta, pdb.clone(), "PDB path in the debug directory");
        }
    }

    let dotnet_facts = pe_facts.as_ref().filter(|p| p.has_clr).and_then(|p| {
        // The CLR data directory RVA/size is re-read here with the same bounds-checked helpers.
        let (rva, size) = clr_dir(bytes).unwrap_or((0, 0));
        match dotnet::parse(bytes, &p.sections, rva, size) {
            Ok(d) => Some(d),
            Err(e) => {
                warnings.push(format!(".NET metadata could not be read: {e}"));
                None
            }
        }
    });
    if let Some(d) = &dotnet_facts {
        ev.add(EvKind::Dotnet, format!(".NET runtime {}", d.runtime_version), format!("streams: {}", d.streams.join(", ")));
    }

    // strings
    let mut all = Vec::new();
    for region in scan_regions(bytes, pe_facts.as_ref()) {
        all.extend(strings::extract(region, MIN_STRING_LEN));
        if all.len() >= strings::MAX_RAW_STRINGS {
            break;
        }
    }
    let string_total = all.len();
    let shown = strings::interesting(&all, MAX_SHOWN_STRINGS);
    for s in shown.iter().take(EV_STRINGS) {
        let detail = format!("{:?} string at file offset {:#x}{}", s.class, s.offset, if s.utf16 { " (UTF-16)" } else { "" });
        ev.add(EvKind::Str, s.value.clone(), detail);
    }
    if let Some(d) = &dotnet_facts {
        for u in d.user_strings.iter().take(300) {
            let class = strings::classify(u);
            if class != StrClass::Plain {
                ev.add(EvKind::Str, crate::redact::redact_text(u), format!("{:?} .NET user string", class));
            }
        }
    }

    let (packer_verdict, capabilities) = match pe_facts.as_ref() {
        Some(p) => {
            let verdict = packer::assess(p, dotnet_facts.as_ref());
            for r in &verdict.reasons {
                ev.add(EvKind::Packer, r.code.to_string(), r.text.clone());
            }
            let chars = characteristics(p, &verdict);
            let bag = build_bag(p, dotnet_facts.as_ref(), &all, &chars);
            let hits = match rules::builtin() {
                Ok(set) => set.evaluate(&bag),
                Err(e) => {
                    warnings.push(format!("Capability rules could not be loaded: {e}"));
                    Vec::new()
                }
            };
            let mut hits = hits;
            for h in hits.iter_mut() {
                let mut ids: Vec<String> = Vec::new();
                for f in &h.facts {
                    if let Some(id) = evidence_for(&mut ev, &f.kind, &f.value, &h.name) {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
                ids.truncate(12);
                h.evidence_ids = ids.clone();
                let attack = if h.attack.is_empty() { String::new() } else { format!("{}; ", h.attack.join(", ")) };
                ev.add(EvKind::Rule, h.name.clone(), format!("{attack}severity {:?}; evidence {}", h.severity, ids.join(", ")));
            }
            (Some(verdict), hits)
        }
        None => (None, Vec::new()),
    };

    Analysis {
        kind: if pe_facts.is_some() { FileKind::Pe } else { FileKind::Unknown },
        file_name: file_name.to_string(),
        size: bytes.len() as u64,
        sha256,
        pe: pe_facts,
        dotnet: dotnet_facts,
        strings: shown,
        string_total,
        packer: packer_verdict,
        capabilities,
        asar: None,
        evidence: ev,
        warnings,
    }
}

/// Analyse an `app.asar` archive (read-only listing + package.json).
pub fn analyze_asar_bytes(bytes: &[u8], file_name: &str) -> Analysis {
    let mut ev = EvidenceSet::new();
    let mut warnings = Vec::new();
    let asar = match electron::parse(bytes) {
        Ok(a) => Some(a),
        Err(e) => {
            warnings.push(format!("Not analysed as an asar archive: {e}"));
            None
        }
    };
    if let Some(a) = &asar {
        ev.add(EvKind::Asar, format!("{} files in {} directories", a.file_count, a.dir_count), format!("{} bytes of content", a.total_bytes));
        if let (Some(n), Some(v)) = (&a.package_name, &a.package_version) {
            ev.add(EvKind::Asar, format!("package {n}@{v}"), a.main.clone().map(|m| format!("main: {m}")).unwrap_or_default());
        }
        for d in &a.dependencies {
            ev.add(EvKind::Asar, format!("dependency {d}"), "package.json dependencies");
        }
        for m in &a.native_modules {
            ev.add(EvKind::Asar, format!("native module {m}"), "compiled .node addon (runs native code inside the app)");
        }
    }
    Analysis {
        kind: if asar.is_some() { FileKind::Asar } else { FileKind::Unknown },
        file_name: file_name.to_string(),
        size: bytes.len() as u64,
        sha256: pe::sha256_hex(bytes),
        pe: None,
        dotnet: None,
        strings: Vec::new(),
        string_total: 0,
        packer: None,
        capabilities: Vec::new(),
        asar,
        evidence: ev,
        warnings,
    }
}

/// Analyse a file on disk: PE -> `analyze_bytes` (+ the app's `resources/app.asar` when it has one),
/// `.asar` -> `analyze_asar_bytes`, anything else -> a best-effort string scan. Never executes the file.
pub fn analyze_path(path: &Path) -> Result<Analysis, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    if electron::looks_like_asar(&bytes) && name.to_ascii_lowercase().ends_with(".asar") {
        return Ok(analyze_asar_bytes(&bytes, name));
    }
    let mut a = analyze_bytes(&bytes, name);
    if a.kind == FileKind::Pe {
        if let Some(asar_path) = electron::sibling_asar_path(path) {
            match std::fs::read(&asar_path).map_err(|e| e.to_string()).and_then(|b| electron::parse(&b)) {
                Ok(f) => {
                    a.evidence.add(EvKind::Asar, format!("Electron app: {} files in app.asar", f.file_count), asar_path.display().to_string());
                    if let (Some(n), Some(v)) = (&f.package_name, &f.package_version) {
                        a.evidence.add(EvKind::Asar, format!("package {n}@{v}"), f.main.clone().map(|m| format!("main: {m}")).unwrap_or_default());
                    }
                    for m in &f.native_modules {
                        a.evidence.add(EvKind::Asar, format!("native module {m}"), "compiled .node addon");
                    }
                    a.asar = Some(f);
                }
                Err(e) => a.warnings.push(format!("Found resources/app.asar but could not read it: {e}")),
            }
        }
    }
    Ok(a)
}

/// CLR runtime header (data directory 14) RVA/size read straight from the optional header.
fn clr_dir(bytes: &[u8]) -> Option<(u32, u32)> {
    let e_lfanew = u32::from_le_bytes(bytes.get(0x3c..0x40)?.try_into().ok()?) as usize;
    let opt = e_lfanew.checked_add(24)?;
    let magic = u16::from_le_bytes(bytes.get(opt..opt + 2)?.try_into().ok()?);
    let dd_base = opt + if magic == 0x20b { 112 } else { 96 };
    let at = dd_base + 14 * 8;
    let rva = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
    let size = u32::from_le_bytes(bytes.get(at + 4..at + 8)?.try_into().ok()?);
    Some((rva, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packer::PackLevel;
    use crate::synth::build_pe;

    #[test]
    fn analyses_a_synthetic_pe_end_to_end() {
        let mut data = vec![0x90u8; 200];
        data.extend_from_slice(b"\x00http://c2.example.test/gate.php\x00cmd.exe /c whoami\x00");
        let pe = build_pe(
            &[(".text", data, 0x6000_0020)],
            &[("KERNEL32.dll", &["CreateProcessW", "VirtualAllocEx"]), ("WS2_32.dll", &["connect"])],
            false,
        );
        let a = analyze_bytes(&pe, "sample.exe");
        assert_eq!(a.kind, FileKind::Pe);
        assert_eq!(a.size as usize, pe.len());
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert_eq!(a.pe.as_ref().unwrap().imports.len(), 3);
        assert!(a.evidence.find(EvKind::Import, "VirtualAllocEx").is_some());
        assert!(a.strings.iter().any(|s| s.class == StrClass::Url && s.value.contains("c2.example.test")));
        assert!(a.strings.iter().any(|s| s.class == StrClass::Command));
        let url = a.evidence.items().iter().find(|e| e.kind == EvKind::Str && e.value.starts_with("http")).unwrap();
        assert!(url.detail.contains("offset"));
        assert!(a.dotnet.is_none());
    }

    #[test]
    fn non_pe_input_yields_a_warning_not_a_panic() {
        let a = analyze_bytes(b"just some text http://x.example.test/y", "note.txt");
        assert_eq!(a.kind, FileKind::Unknown);
        assert!(a.pe.is_none());
        assert_eq!(a.warnings.len(), 1);
        assert!(a.strings.iter().any(|s| s.class == StrClass::Url));
    }

    fn rule_names(a: &Analysis) -> Vec<&str> {
        a.capabilities.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn rules_fire_end_to_end_on_a_synthetic_injector_and_cite_real_evidence() {
        let pe = build_pe(
            &[(".text", vec![0x90; 400], 0x6000_0020), (".data", b"\x00SeDebugPrivilege\x00lsass.exe\x00".to_vec(), 0xC000_0040)],
            &[
                ("KERNEL32.dll", &["VirtualAllocEx", "WriteProcessMemory", "CreateRemoteThread", "OpenProcess"]),
                ("ADVAPI32.dll", &["OpenProcessToken", "AdjustTokenPrivileges", "LookupPrivilegeValueW"]),
            ],
            false,
        );
        let a = analyze_bytes(&pe, "inj.exe");
        let names = rule_names(&a);
        assert!(names.contains(&"inject code into another process"), "{names:?}");
        assert!(names.contains(&"enable debug privilege"), "{names:?}");
        let hit = a.capabilities.iter().find(|h| h.name == "inject code into another process").unwrap();
        assert!(!hit.evidence_ids.is_empty());
        for id in &hit.evidence_ids {
            assert!(a.evidence.get(id).is_some(), "dangling evidence id {id}");
        }
        assert!(hit.evidence_ids.iter().any(|id| a.evidence.get(id).unwrap().value == "WriteProcessMemory"));
        assert!(a.evidence.items().iter().any(|e| e.kind == EvKind::Rule && e.value == "inject code into another process"));
        assert_eq!(hit.severity, crate::rules::Severity::High);
    }

    #[test]
    fn a_benign_looking_file_hits_no_high_severity_rule() {
        let pe = build_pe(
            &[(".text", (0..4000).map(|i| ((i * 3) % 71) as u8).collect(), 0x6000_0020)],
            &[("KERNEL32.dll", &["CreateFileW", "ReadFile", "WriteFile", "CloseHandle", "GetLastError", "ExitProcess"])],
            false,
        );
        let a = analyze_bytes(&pe, "ok.exe");
        assert!(a.capabilities.iter().all(|h| h.severity < crate::rules::Severity::High), "{:?}", rule_names(&a));
        assert_eq!(a.packer.as_ref().unwrap().level, PackLevel::None);
    }

    #[test]
    fn packed_files_get_the_packed_characteristic_and_rule() {
        let pe = build_pe(&[("UPX0", vec![], 0xE000_0080), ("UPX1", vec![0xA5; 3000], 0xE000_0020)], &[("KERNEL32.dll", &["LoadLibraryA", "GetProcAddress"])], false);
        let a = analyze_bytes(&pe, "packed.exe");
        assert!(a.packer.as_ref().unwrap().is_flagged());
        assert!(rule_names(&a).contains(&"is packed or protected"), "{:?}", rule_names(&a));
        assert!(a.evidence.items().iter().any(|e| e.kind == EvKind::Packer));
    }

    #[test]
    fn asar_archives_are_analysed_from_bytes_and_paths() {
        let a = crate::electron::testutil::build_asar(&[("package.json", br#"{"name":"x","version":"1.0.0"}"#.as_slice()), ("index.js", b"1".as_slice())]);
        let r = analyze_asar_bytes(&a, "app.asar");
        assert_eq!(r.kind, FileKind::Asar);
        assert_eq!(r.asar.as_ref().unwrap().package_name.as_deref(), Some("x"));
        let dir = std::env::temp_dir().join(format!("nexus_rev_asar_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("resources")).unwrap();
        std::fs::write(dir.join("resources/app.asar"), &a).unwrap();
        let exe = build_pe(&[(".text", vec![0x90; 300], 0x6000_0020)], &[("KERNEL32.dll", &["ExitProcess"])], false);
        std::fs::write(dir.join("app.exe"), &exe).unwrap();
        let via_path = analyze_path(&dir.join("app.exe")).unwrap();
        assert_eq!(via_path.kind, FileKind::Pe);
        assert_eq!(via_path.asar.as_ref().unwrap().package_name.as_deref(), Some("x"));
        assert!(analyze_path(&dir.join("missing.exe")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_input_is_fine() {
        let a = analyze_bytes(b"", "empty");
        assert_eq!(a.kind, FileKind::Unknown);
        assert_eq!(a.size, 0);
    }

    #[test]
    fn secrets_in_strings_never_reach_evidence() {
        let pe = build_pe(&[(".data", b"\x00https://x.test/?token=ABCDEF0123456789SECRET&a=1\x00".to_vec(), 0xC000_0040)], &[], false);
        let a = analyze_bytes(&pe, "s.exe");
        let all: String = a.evidence.items().iter().map(|e| format!("{} {}", e.value, e.detail)).collect::<Vec<_>>().join("\n");
        assert!(!all.contains("ABCDEF0123456789SECRET"), "{all}");
    }
}
