//! Packing / obfuscation heuristics (Detect-It-Easy-style leads, NOT proof).
//!
//! A packed or obfuscated program hides its real code and imports, so every other finding about it is
//! unreliable. This module's job is honesty: when it says `Suspected` or `Likely`, the report leads with a
//! warning and no explanation may be presented as settled (enforced in `explain` / `report`).

use crate::dotnet::DotnetFacts;
use crate::pe::PeFacts;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackLevel {
    None,
    Suspected,
    Likely,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reason {
    pub code: &'static str,
    pub text: String,
    pub weight: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackerVerdict {
    pub level: PackLevel,
    /// A known packer/protector name when a section-name signature matched.
    pub name: Option<String>,
    pub score: u32,
    pub reasons: Vec<Reason>,
}

impl PackerVerdict {
    pub fn is_flagged(&self) -> bool {
        self.level != PackLevel::None
    }
}

/// Section-name signatures of well-known packers / protectors (compared case-insensitively).
const KNOWN: &[(&str, &str)] = &[
    ("upx0", "UPX"),
    ("upx1", "UPX"),
    ("upx2", "UPX"),
    (".aspack", "ASPack"),
    (".adata", "ASPack"),
    (".petite", "Petite"),
    (".mpress1", "MPRESS"),
    (".mpress2", "MPRESS"),
    (".themida", "Themida"),
    (".winlice", "Themida/WinLicense"),
    (".vmp0", "VMProtect"),
    (".vmp1", "VMProtect"),
    (".vmp2", "VMProtect"),
    (".enigma1", "Enigma Protector"),
    (".enigma2", "Enigma Protector"),
    (".nsp0", "NsPack"),
    (".nsp1", "NsPack"),
    (".nsp2", "NsPack"),
    (".perplex", "PELock"),
    (".pec1", "PECompact"),
    (".pec2", "PECompact"),
    ("pec1", "PECompact"),
    ("pec2", "PECompact"),
    (".packed", "generic packer"),
    (".rlpack", "RLPack"),
    (".y0da", "yoda's Protector"),
    ("kkrunchy", "kkrunchy"),
    (".boom", "The Boomerang list"),
    ("pebundle", "PEBundle"),
    (".taz", "PESpin"),
];

fn is_loadlib(n: &str) -> bool {
    matches!(n.to_ascii_lowercase().as_str(), "loadlibrarya" | "loadlibraryw" | "loadlibraryexa" | "loadlibraryexw" | "ldrloaddll")
}

fn is_getproc(n: &str) -> bool {
    matches!(n.to_ascii_lowercase().as_str(), "getprocaddress" | "ldrgetprocedureaddress")
}

/// Judge `pe` (and optional .NET facts). Pure; deterministic.
pub fn assess(pe: &PeFacts, dotnet: Option<&DotnetFacts>) -> PackerVerdict {
    let mut reasons: Vec<Reason> = Vec::new();
    let mut name: Option<String> = None;
    let mut add = |code: &'static str, text: String, weight: u32| reasons.push(Reason { code, text, weight });

    for s in &pe.sections {
        let lower = s.name.to_ascii_lowercase();
        if let Some((_, n)) = KNOWN.iter().find(|(sig, _)| lower == *sig) {
            name.get_or_insert_with(|| n.to_string());
            add("packer_section", format!("section '{}' is the signature of {}", s.name, n), 6);
        }
    }

    for s in &pe.sections {
        let lower = s.name.to_ascii_lowercase();
        let ignorable = matches!(lower.as_str(), ".rsrc" | ".reloc" | ".pdata" | ".tls" | ".didat");
        if s.executable && s.entropy >= 7.2 && s.raw_size >= 4096 {
            add("high_entropy_code", format!("executable section '{}' has entropy {:.2} (looks compressed/encrypted)", s.name, s.entropy), 3);
        } else if !ignorable && s.entropy >= 7.5 && s.raw_size >= 16 * 1024 {
            add("high_entropy_data", format!("section '{}' has entropy {:.2} over {} KB", s.name, s.entropy, s.raw_size / 1024), 1);
        }
        if s.executable && s.raw_size == 0 && s.virtual_size >= 0x1000 {
            add("empty_exec_section", format!("executable section '{}' is {} B in memory but empty on disk (unpack target)", s.name, s.virtual_size), 3);
        }
    }

    // entry point placement
    if pe.entry_point != 0 {
        let ep_sec = pe.sections.iter().find(|s| {
            let span = s.virtual_size.max(s.raw_size);
            pe.entry_point >= s.virtual_address && pe.entry_point - s.virtual_address < span
        });
        match ep_sec {
            Some(s) if s.executable && s.writable => add("ep_writable_exec", format!("entry point is in writable+executable section '{}'", s.name), 3),
            None if !pe.has_clr && !pe.is_dll => add("ep_outside_sections", "entry point lies outside every section".to_string(), 3),
            Some(s) if !s.executable && !pe.has_clr => add("ep_not_executable", format!("entry point is in non-executable section '{}'", s.name), 2),
            _ => {}
        }
        if let (Some(s), Some(last)) = (ep_sec, pe.sections.last()) {
            if pe.sections.len() >= 3 && s.name == last.name && !pe.has_clr && pe.sections.iter().filter(|x| x.executable).count() >= 1 && s.executable {
                add("ep_in_last_section", format!("entry point is in the LAST section '{}'", s.name), 1);
            }
        }
    }

    // import table shape
    let n_imports = pe.imports.len();
    let has_ll = pe.imports.iter().any(|i| i.name.as_deref().map_or(false, is_loadlib));
    let has_gp = pe.imports.iter().any(|i| i.name.as_deref().map_or(false, is_getproc));
    if !pe.has_clr && n_imports > 0 && n_imports <= 8 && has_ll && has_gp {
        add("tiny_import_table", format!("only {n_imports} imports, including LoadLibrary + GetProcAddress (APIs resolved at run time)"), 3);
    } else if !pe.has_clr && !pe.is_dll && n_imports == 0 && pe.entry_point != 0 && !pe.has_delay_imports {
        add("no_imports", "no import table at all (everything resolved manually)".to_string(), 2);
    }

    // overlay bigger than the program itself (self-extractors, protectors with appended payload)
    if pe.overlay_size > 2 * 1024 * 1024 && pe.overlay_size * 2 > pe.file_size {
        add("large_overlay", format!("{} MB of data appended after the last section ({}% of the file)", pe.overlay_size / (1024 * 1024), pe.overlay_size * 100 / pe.file_size.max(1)), 1);
    }

    // .NET name obfuscation: lots of identifiers that are 1-2 chars long or non-ASCII
    if let Some(d) = dotnet {
        let n = d.identifiers.len();
        if n >= 50 {
            let weird = d.identifiers.iter().filter(|s| s.chars().count() <= 2 || !s.is_ascii()).count();
            if weird * 100 / n >= 60 {
                add("dotnet_name_obfuscation", format!("{}% of the {} .NET identifiers are 1-2 characters or non-ASCII", weird * 100 / n, n), 3);
            }
        }
    }

    let score: u32 = reasons.iter().map(|r| r.weight).sum();
    let level = if name.is_some() || score >= 5 {
        PackLevel::Likely
    } else if score >= 3 {
        PackLevel::Suspected
    } else {
        PackLevel::None
    };
    PackerVerdict { level, name, score, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe;
    use crate::synth::build_pe;

    const RX: u32 = 0x6000_0020; // code | exec | read
    const RWX: u32 = 0xE000_0020;
    const RW: u32 = 0xC000_0040;

    fn noise(n: usize) -> Vec<u8> {
        // xorshift: high-entropy bytes
        let mut x = 0x1234_5678_9abc_def0u64;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect()
    }

    fn verdict(bytes: &[u8]) -> PackerVerdict {
        assess(&pe::parse(bytes).unwrap(), None)
    }

    #[test]
    fn a_plain_program_is_not_flagged() {
        let code: Vec<u8> = (0..6000).map(|i| ((i * 7) % 61) as u8).collect();
        let b = build_pe(&[(".text", code, RX), (".data", vec![1; 300], RW)], &[("KERNEL32.dll", &["CreateFileW", "ReadFile", "WriteFile", "CloseHandle"])], false);
        let v = verdict(&b);
        assert_eq!(v.level, PackLevel::None, "{:?}", v.reasons);
    }

    #[test]
    fn upx_section_names_are_likely_and_named() {
        let b = build_pe(&[("UPX0", vec![], RWX), ("UPX1", noise(8000), RWX)], &[("KERNEL32.dll", &["LoadLibraryA", "GetProcAddress", "VirtualProtect"])], false);
        let v = verdict(&b);
        assert_eq!(v.level, PackLevel::Likely);
        assert_eq!(v.name.as_deref(), Some("UPX"));
    }

    #[test]
    fn high_entropy_code_with_a_tiny_import_table_is_flagged_even_without_known_names() {
        let b = build_pe(&[(".text", noise(9000), RX)], &[("KERNEL32.dll", &["LoadLibraryW", "GetProcAddress"])], false);
        let v = verdict(&b);
        assert!(v.is_flagged(), "{:?}", v.reasons);
        assert!(v.reasons.iter().any(|r| r.code == "high_entropy_code"));
        assert!(v.reasons.iter().any(|r| r.code == "tiny_import_table"));
    }

    #[test]
    fn writable_executable_entry_section_counts() {
        let b = build_pe(&[(".text", vec![0x90; 600], RWX)], &[("KERNEL32.dll", &["ExitProcess"])], false);
        let v = verdict(&b);
        assert!(v.reasons.iter().any(|r| r.code == "ep_writable_exec"), "{:?}", v.reasons);
    }

    #[test]
    fn compressed_resource_blob_alone_does_not_flag_a_normal_program() {
        let code: Vec<u8> = (0..6000).map(|i| ((i * 5) % 53) as u8).collect();
        let b = build_pe(&[(".text", code, RX), (".rsrc", noise(40_000), 0x4000_0040)], &[("KERNEL32.dll", &["CreateFileW", "ReadFile", "ExitProcess", "GetLastError"])], false);
        assert_eq!(verdict(&b).level, PackLevel::None);
    }

    #[test]
    fn dotnet_name_obfuscation_is_detected() {
        let b = build_pe(&[(".text", vec![0x90; 300], RX)], &[("KERNEL32.dll", &["ExitProcess"])], false);
        let p = pe::parse(&b).unwrap();
        let mut d = DotnetFacts::default();
        d.identifiers = (0..120).map(|i| if i % 4 == 0 { format!("RealName{i}") } else { "a".repeat(1 + i % 2) }).collect();
        assert!(assess(&p, Some(&d)).reasons.iter().any(|r| r.code == "dotnet_name_obfuscation"));
        d.identifiers = (0..120).map(|i| format!("ProperIdentifier{i}")).collect();
        assert!(!assess(&p, Some(&d)).reasons.iter().any(|r| r.code == "dotnet_name_obfuscation"));
    }

    #[test]
    fn known_signature_table_has_no_duplicates_and_is_lowercase() {
        let mut seen = std::collections::HashSet::new();
        for (sig, _) in KNOWN {
            assert_eq!(*sig, sig.to_ascii_lowercase());
            assert!(seen.insert(*sig), "duplicate {sig}");
        }
    }
}
