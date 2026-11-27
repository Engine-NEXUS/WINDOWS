//! PE (Portable Executable) facts via `goblin` - read-only, bounds-capped.
//!
//! This never loads or runs the file; it only reads header/table bytes. Everything the rest of the
//! pipeline needs (imports, exports, sections + entropy, version info, manifest level, CLR/TLS/
//! signature/debug presence, overlay) is collected here into [`PeFacts`].

use goblin::pe::PE;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const MAX_IMPORTS: usize = 20_000;
pub const MAX_EXPORTS: usize = 20_000;
pub const MAX_SECTIONS: usize = 96; // the PE format's own limit

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SectionFacts {
    /// The raw 8-byte header name (what the Windows loader and pefile/LIEF report), e.g. `/4` for a
    /// COFF-string-table reference.
    pub name: String,
    /// The resolved long name for such references (`.debug_info`), when the string table has one.
    pub long_name: Option<String>,
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub raw_size: u32,
    pub raw_offset: u32,
    pub characteristics: u32,
    /// Shannon entropy (bits/byte, 0-8) of the raw bytes actually present in the file.
    pub entropy: f64,
    pub executable: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImportFacts {
    pub dll: String,
    /// `None` for by-ordinal imports.
    pub name: Option<String>,
    pub ordinal: Option<u16>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExportFacts {
    pub name: Option<String>,
    pub rva: u32,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct VersionFacts {
    pub company_name: Option<String>,
    pub file_description: Option<String>,
    pub file_version: Option<String>,
    pub internal_name: Option<String>,
    pub legal_copyright: Option<String>,
    pub original_filename: Option<String>,
    pub product_name: Option<String>,
    pub product_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeFacts {
    /// True when the strict parser rejected the file and the tolerant one was used (typical of packed
    /// files whose tables point into empty on-disk sections). Facts are then best-effort.
    pub lenient_parse: bool,
    pub is_64: bool,
    pub is_dll: bool,
    pub is_driver: bool,
    pub machine: u16,
    pub machine_name: String,
    pub timestamp: u32,
    pub subsystem: Option<u16>,
    pub subsystem_name: String,
    pub entry_point: u32,
    pub image_base: u64,
    pub dll_characteristics: u16,
    pub aslr: bool,
    pub dep: bool,
    pub sections: Vec<SectionFacts>,
    pub imports: Vec<ImportFacts>,
    pub imports_truncated: bool,
    pub exports: Vec<ExportFacts>,
    pub exports_truncated: bool,
    pub has_delay_imports: bool,
    pub has_clr: bool,
    pub has_tls: bool,
    pub tls_callbacks: usize,
    pub has_signature: bool,
    pub has_debug: bool,
    pub pdb_path: Option<String>,
    pub has_rich_header: bool,
    pub version: VersionFacts,
    /// `asInvoker` | `highestAvailable` | `requireAdministrator` when a manifest says so.
    pub requested_execution_level: Option<String>,
    pub ui_access: bool,
    pub overlay_size: u64,
    pub file_size: u64,
    pub sha256: String,
}

pub fn machine_name(m: u16) -> &'static str {
    match m {
        0x014c => "x86 (i386)",
        0x8664 => "x64 (AMD64)",
        0x01c0 => "ARM",
        0x01c4 => "ARM Thumb-2",
        0xaa64 => "ARM64",
        0x0200 => "Itanium (IA-64)",
        0x0ebc => "EFI byte code",
        0x5032 | 0x5064 => "RISC-V",
        0 => "unknown",
        _ => "other",
    }
}

pub fn subsystem_name(s: Option<u16>) -> &'static str {
    match s {
        Some(1) => "Native (driver / boot-time)",
        Some(2) => "Windows GUI",
        Some(3) => "Windows console",
        Some(5) => "OS/2 console",
        Some(7) => "POSIX console",
        Some(9) => "Windows CE GUI",
        Some(10) => "EFI application",
        Some(11) => "EFI boot service driver",
        Some(12) => "EFI runtime driver",
        Some(13) => "EFI ROM",
        Some(14) => "Xbox",
        Some(16) => "Windows boot application",
        Some(_) => "other",
        None => "none",
    }
}

/// Shannon entropy in bits per byte (0.0 for empty input).
pub fn entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum()
}

fn section_name(raw: &[u8; 8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(8);
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

/// Cheap structural sanity before the full parser runs: rejects files that cannot be a PE at all
/// and the header lies that would make a parser allocate or loop pointlessly.
pub fn quick_validate(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
        return Err("not a PE file (no MZ header)".into());
    }
    let e_lfanew = u32::from_le_bytes([bytes[0x3c], bytes[0x3d], bytes[0x3e], bytes[0x3f]]) as usize;
    if e_lfanew < 0x40 || e_lfanew.checked_add(24).map_or(true, |end| end > bytes.len()) {
        return Err("invalid PE header offset".into());
    }
    if &bytes[e_lfanew..e_lfanew + 4] != b"PE\0\0" {
        return Err("missing PE signature".into());
    }
    let nsec = u16::from_le_bytes([bytes[e_lfanew + 6], bytes[e_lfanew + 7]]) as usize;
    if nsec == 0 || nsec > MAX_SECTIONS {
        return Err(format!("implausible section count ({nsec})"));
    }
    Ok(())
}

fn nonempty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string()).filter(|v| !v.is_empty())
}

fn manifest_level(data: &[u8]) -> (Option<String>, bool) {
    // Manifests are almost always UTF-8; accept UTF-16LE too.
    let text = if data.len() >= 2 && data[0] == 0xff && data[1] == 0xfe {
        let u: Vec<u16> = data[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(data).into_owned()
    };
    let lower = text.to_ascii_lowercase();
    let level = ["requireadministrator", "highestavailable", "asinvoker"].iter().find_map(|l| {
        let key = format!("level=\"{l}\"");
        let key2 = format!("level='{l}'");
        (lower.contains(&key) || lower.contains(&key2)).then(|| match *l {
            "requireadministrator" => "requireAdministrator",
            "highestavailable" => "highestAvailable",
            _ => "asInvoker",
        })
    });
    let ui = lower.contains("uiaccess=\"true\"") || lower.contains("uiaccess='true'");
    (level.map(str::to_string), ui)
}

/// Parse `bytes` as a PE. Pure function; never executes anything.
pub fn parse(bytes: &[u8]) -> Result<PeFacts, String> {
    quick_validate(bytes)?;
    let (pe, lenient_parse) = match PE::parse(bytes) {
        Ok(pe) => (pe, false),
        Err(strict_err) => {
            // Packed files often carry tables (e.g. the exception directory) that point into sections with
            // no file data; Windows loads them and pefile/LIEF read them, so retry tolerantly.
            let opts = goblin::pe::options::ParseOptions::default().with_parse_mode(goblin::options::ParseMode::Permissive);
            match PE::parse_with_opts(bytes, &opts) {
                Ok(pe) => (pe, true),
                Err(_) => return Err(format!("PE parse error: {strict_err}")),
            }
        }
    };
    let oh = pe.header.optional_header.as_ref();
    let machine = pe.header.coff_header.machine;
    let subsystem = oh.map(|o| o.windows_fields.subsystem);
    let dllc = oh.map(|o| o.windows_fields.dll_characteristics).unwrap_or(0);
    let dd = oh.map(|o| &o.data_directories);
    let present = |f: fn(&goblin::pe::data_directories::DataDirectories) -> Option<&goblin::pe::data_directories::DataDirectory>| -> bool {
        dd.and_then(|d| f(d)).map_or(false, |d| d.virtual_address != 0 && d.size != 0)
    };
    let has_signature = dd
        .and_then(|d| d.get_certificate_table())
        .map_or(false, |d| d.size != 0); // certificate table "address" is a FILE offset, so only size is checked
    let file_len = bytes.len() as u64;

    let mut sections = Vec::new();
    let mut raw_end: u64 = 0;
    for s in pe.sections.iter().take(MAX_SECTIONS) {
        let start = s.pointer_to_raw_data as usize;
        let size = s.size_of_raw_data as usize;
        let slice: &[u8] = match start.checked_add(size) {
            Some(end) if start < bytes.len() => &bytes[start..end.min(bytes.len())],
            _ => &[],
        };
        if !slice.is_empty() {
            raw_end = raw_end.max(start as u64 + slice.len() as u64);
        }
        sections.push(SectionFacts {
            name: section_name(&s.name),
            long_name: s.real_name.clone().filter(|n| !n.is_empty()),
            virtual_size: s.virtual_size,
            virtual_address: s.virtual_address,
            raw_size: s.size_of_raw_data,
            raw_offset: s.pointer_to_raw_data,
            characteristics: s.characteristics,
            entropy: entropy(slice),
            executable: s.characteristics & 0x2000_0000 != 0,
            writable: s.characteristics & 0x8000_0000 != 0,
        });
    }

    let mut imports = Vec::new();
    let mut imports_truncated = false;
    for i in pe.imports.iter() {
        if imports.len() >= MAX_IMPORTS {
            imports_truncated = true;
            break;
        }
        let by_ordinal = i.name.starts_with("ORDINAL ");
        imports.push(ImportFacts {
            dll: i.dll.to_string(),
            name: if by_ordinal { None } else { Some(i.name.to_string()) },
            ordinal: if by_ordinal { i.name["ORDINAL ".len()..].trim().parse().ok().or(Some(i.ordinal)) } else { None },
        });
    }

    let mut exports = Vec::new();
    let mut exports_truncated = false;
    for e in pe.exports.iter() {
        if exports.len() >= MAX_EXPORTS {
            exports_truncated = true;
            break;
        }
        exports.push(ExportFacts { name: e.name.map(str::to_string), rva: e.rva as u32 });
    }

    let (mut version, mut level, mut ui_access) = (VersionFacts::default(), None, false);
    if let Some(r) = pe.resource_data.as_ref() {
        if let Some(v) = r.version_info.as_ref() {
            let s = &v.string_info;
            version = VersionFacts {
                company_name: nonempty(s.company_name()),
                file_description: nonempty(s.file_description()),
                file_version: nonempty(s.file_version()),
                internal_name: nonempty(s.internal_name()),
                legal_copyright: nonempty(s.legal_copyright()),
                original_filename: nonempty(s.original_filename()),
                product_name: nonempty(s.product_name()),
                product_version: nonempty(s.product_version()),
            };
        }
        if let Some(m) = r.manifest_data.as_ref() {
            let (l, u) = manifest_level(m.data);
            level = l;
            ui_access = u;
        }
    }

    let pdb_path = pe
        .debug_data
        .as_ref()
        .and_then(|d| d.codeview_pdb70_debug_info.as_ref())
        .map(|c| String::from_utf8_lossy(c.filename).trim_end_matches('\0').to_string())
        .filter(|s| !s.is_empty());

    let subsystem_native = subsystem == Some(1);
    let imports_ntoskrnl = pe.libraries.iter().any(|l| l.eq_ignore_ascii_case("ntoskrnl.exe") || l.eq_ignore_ascii_case("hal.dll"));

    Ok(PeFacts {
        lenient_parse,
        is_64: pe.is_64,
        is_dll: pe.is_lib,
        is_driver: subsystem_native && imports_ntoskrnl,
        machine,
        machine_name: machine_name(machine).to_string(),
        timestamp: pe.header.coff_header.time_date_stamp,
        subsystem,
        subsystem_name: subsystem_name(subsystem).to_string(),
        entry_point: pe.entry,
        image_base: pe.image_base,
        dll_characteristics: dllc,
        aslr: dllc & 0x0040 != 0,
        dep: dllc & 0x0100 != 0,
        sections,
        imports,
        imports_truncated,
        exports,
        exports_truncated,
        has_delay_imports: present(|d| d.get_delay_import_descriptor()),
        has_clr: present(|d| d.get_clr_runtime_header()),
        has_tls: present(|d| d.get_tls_table()),
        tls_callbacks: pe.tls_data.as_ref().map_or(0, |t| t.callbacks.len()),
        has_signature,
        has_debug: present(|d| d.get_debug_table()),
        pdb_path,
        has_rich_header: pe.header.rich_header.is_some(),
        version,
        requested_execution_level: level,
        ui_access,
        overlay_size: file_len.saturating_sub(raw_end),
        file_size: file_len,
        sha256: sha256_hex(bytes),
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Map an RVA to a file offset using the section table (None when it falls outside every section).
pub fn rva_to_offset(sections: &[SectionFacts], rva: u32) -> Option<usize> {
    for s in sections {
        let span = s.virtual_size.max(s.raw_size);
        if rva >= s.virtual_address && rva - s.virtual_address < span {
            let off = (rva - s.virtual_address) as u64 + s.raw_offset as u64;
            return usize::try_from(off).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::synth::build_pe;
    use super::*;

    #[test]
    fn entropy_bounds() {
        assert_eq!(entropy(&[]), 0.0);
        assert_eq!(entropy(&[7u8; 1000]), 0.0);
        let all: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        assert!((entropy(&all) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_non_pe_and_header_lies_without_panicking() {
        assert!(parse(b"").is_err());
        assert!(parse(b"MZ").is_err());
        assert!(parse(&vec![0u8; 4096]).is_err());
        let mut pe = build_pe(&[(".text", vec![0x90; 64], 0x6000_0020)], &[], false);
        assert!(parse(&pe).is_ok());
        pe[0x3c..0x40].copy_from_slice(&0xffff_fff0u32.to_le_bytes()); // e_lfanew far outside
        assert!(parse(&pe).is_err());
    }

    #[test]
    fn parses_sections_imports_and_flags_of_a_synthetic_pe() {
        let code: Vec<u8> = (0..300).map(|i| (i * 7) as u8).collect();
        let pe = build_pe(
            &[(".text", code, 0x6000_0020), (".data", vec![1; 64], 0xC000_0040)],
            &[("KERNEL32.dll", &["CreateProcessW", "VirtualAlloc"]), ("WS2_32.dll", &["connect"])],
            false,
        );
        let f = parse(&pe).unwrap();
        assert!(f.is_64 && !f.is_dll && !f.has_clr && !f.has_signature);
        assert_eq!(f.machine, 0x8664);
        assert_eq!(f.subsystem, Some(3));
        assert_eq!(f.subsystem_name, "Windows console");
        assert!(f.aslr && f.dep);
        assert_eq!(f.sections.len(), 3);
        assert_eq!(f.sections[0].name, ".text");
        assert!(f.sections[0].executable && !f.sections[0].writable);
        assert!(f.sections[1].writable);
        let names: Vec<_> = f.imports.iter().filter_map(|i| i.name.clone()).collect();
        assert_eq!(names, ["CreateProcessW", "VirtualAlloc", "connect"]);
        assert_eq!(f.imports[2].dll, "WS2_32.dll");
        assert_eq!(f.sha256.len(), 64);
        assert_eq!(f.file_size as usize, pe.len());
        assert_eq!(f.overlay_size, 0);
    }

    #[test]
    fn overlay_is_trailing_bytes_after_the_last_section() {
        let mut pe = build_pe(&[(".text", vec![0x90; 64], 0x6000_0020)], &[], true);
        pe.extend_from_slice(&[0xAB; 1000]);
        let f = parse(&pe).unwrap();
        assert!(f.is_dll);
        assert_eq!(f.overlay_size, 1000);
    }

    #[test]
    fn manifest_level_detection() {
        let m = br#"<requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>"#;
        assert_eq!(manifest_level(m), (Some("requireAdministrator".into()), false));
        let m2 = br#"<requestedExecutionLevel level='asInvoker' uiAccess='true'/>"#;
        assert_eq!(manifest_level(m2), (Some("asInvoker".into()), true));
        assert_eq!(manifest_level(b"<assembly/>"), (None, false));
    }

    #[test]
    fn rva_mapping() {
        let pe = build_pe(&[(".text", vec![0x90; 64], 0x6000_0020)], &[], false);
        let f = parse(&pe).unwrap();
        let s = &f.sections[0];
        assert_eq!(rva_to_offset(&f.sections, s.virtual_address + 5), Some(s.raw_offset as usize + 5));
        assert_eq!(rva_to_offset(&f.sections, 0x7fff_0000), None);
    }

    #[test]
    fn packed_style_exception_directory_in_an_empty_section_still_parses_leniently() {
        // UPX-style layout: first section is empty on disk, and the exception directory points into it.
        let mut b = crate::synth::build_pe(&[("UPX0", vec![], 0xE000_0080), ("UPX1", vec![7u8; 4000], 0xE000_0020)], &[("KERNEL32.dll", &["LoadLibraryA", "GetProcAddress"])], false);
        let dd3 = 0x80 + 24 + 112 + 3 * 8; // exception directory entry
        b[dd3..dd3 + 4].copy_from_slice(&0x1000u32.to_le_bytes());
        b[dd3 + 4..dd3 + 8].copy_from_slice(&12u32.to_le_bytes());
        assert!(goblin::pe::PE::parse(&b).is_err(), "fixture must be rejected by the strict parser, otherwise this test proves nothing");
        let f = parse(&b).expect("lenient retry should accept it");
        assert!(f.lenient_parse);
        assert_eq!(f.sections.len(), 3);
        // ... and a normal file is NOT marked lenient
        let ok = crate::synth::build_pe(&[(".text", vec![0x90; 300], 0x6000_0020)], &[("KERNEL32.dll", &["ExitProcess"])], false);
        assert!(!parse(&ok).unwrap().lenient_parse);
    }
}
