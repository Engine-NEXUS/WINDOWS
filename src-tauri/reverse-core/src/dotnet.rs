//! .NET (CLR) metadata triage: runtime version, stream list, #Strings identifiers and #US string
//! literals. Static and shallow on purpose - no IL decompilation (that needs `ilspycmd`, a documented
//! optional follow-up). All reads are bounds-checked.

use crate::pe::{rva_to_offset, SectionFacts};
use serde::Serialize;

pub const MAX_STREAMS: usize = 32;
pub const MAX_IDENTIFIERS: usize = 60_000;
pub const MAX_USER_STRINGS: usize = 2_000;

#[derive(Debug, Clone, Serialize, Default)]
pub struct DotnetFacts {
    pub runtime_version: String,
    pub streams: Vec<String>,
    pub il_only: bool,
    pub requires_32bit: bool,
    pub strong_name_signed: bool,
    pub has_native_entry: bool,
    pub user_strings: Vec<String>,
    #[serde(skip)]
    pub identifiers: Vec<String>,
    pub identifier_count: usize,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o.checked_add(4)?)?.try_into().ok()?))
}

/// Split a #Strings heap into its NUL-terminated UTF-8 entries.
pub fn parse_strings_heap(heap: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for part in heap.split(|&b| b == 0) {
        if part.is_empty() || part.len() > 256 {
            continue;
        }
        if out.len() >= MAX_IDENTIFIERS {
            break;
        }
        out.push(String::from_utf8_lossy(part).into_owned());
    }
    out
}

/// Parse a #US (user string) heap: entries are `compressed-length, UTF-16LE data, 1 flag byte`.
pub fn parse_user_strings(heap: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 1usize; // the heap starts with a single 0 byte
    while pos < heap.len() && out.len() < MAX_USER_STRINGS {
        let b0 = heap[pos];
        let (len, hdr) = if b0 & 0x80 == 0 {
            (b0 as usize, 1)
        } else if b0 & 0xC0 == 0x80 {
            match heap.get(pos + 1) {
                Some(&b1) => ((((b0 & 0x3f) as usize) << 8) | b1 as usize, 2),
                None => break,
            }
        } else if b0 & 0xE0 == 0xC0 {
            let (Some(&b1), Some(&b2), Some(&b3)) = (heap.get(pos + 1), heap.get(pos + 2), heap.get(pos + 3)) else { break };
            ((((b0 & 0x1f) as usize) << 24) | ((b1 as usize) << 16) | ((b2 as usize) << 8) | b3 as usize, 4)
        } else {
            break;
        };
        pos += hdr;
        if len == 0 {
            continue;
        }
        let Some(end) = pos.checked_add(len) else { break };
        let Some(data) = heap.get(pos..end) else { break };
        let chars = &data[..data.len().saturating_sub(1)];
        let u: Vec<u16> = chars.chunks_exact(2).take(1024).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let s = String::from_utf16_lossy(&u);
        if s.chars().count() >= 3 {
            out.push(s);
        }
        pos = end;
    }
    out
}

/// Read the CLR header and metadata root at `clr_rva`.
pub fn parse(bytes: &[u8], sections: &[SectionFacts], clr_rva: u32, clr_size: u32) -> Result<DotnetFacts, String> {
    if clr_size < 72 {
        return Err("CLR header too small".into());
    }
    let off = rva_to_offset(sections, clr_rva).ok_or("CLR header RVA is outside every section")?;
    let hdr = bytes.get(off..off.checked_add(72).ok_or("overflow")?).ok_or("CLR header truncated")?;
    let meta_rva = u32_at(hdr, 8).ok_or("bad CLR header")?;
    let meta_size = u32_at(hdr, 12).ok_or("bad CLR header")? as usize;
    let flags = u32_at(hdr, 16).ok_or("bad CLR header")?;
    let moff = rva_to_offset(sections, meta_rva).ok_or("metadata RVA is outside every section")?;
    let mend = moff.checked_add(meta_size.min(bytes.len())).ok_or("overflow")?.min(bytes.len());
    let meta = bytes.get(moff..mend).ok_or("metadata truncated")?;
    if u32_at(meta, 0) != Some(0x424A_5342) {
        return Err("metadata root signature (BSJB) not found".into());
    }
    let vlen = u32_at(meta, 12).ok_or("bad metadata root")? as usize;
    if vlen > 255 {
        return Err("implausible runtime version length".into());
    }
    let ver = meta.get(16..16 + vlen).ok_or("metadata root truncated")?;
    let runtime_version = String::from_utf8_lossy(ver).trim_end_matches('\0').to_string();
    let mut p = 16 + vlen + 2; // skip flags
    let nstreams = u16_at(meta, p).ok_or("bad stream count")? as usize;
    p += 2;
    if nstreams > MAX_STREAMS {
        return Err("implausible stream count".into());
    }
    let mut facts = DotnetFacts {
        runtime_version,
        il_only: flags & 0x1 != 0,
        requires_32bit: flags & 0x2 != 0,
        strong_name_signed: flags & 0x8 != 0,
        has_native_entry: flags & 0x10 != 0,
        ..Default::default()
    };
    for _ in 0..nstreams {
        let so = u32_at(meta, p).ok_or("stream header truncated")? as usize;
        let ss = u32_at(meta, p + 4).ok_or("stream header truncated")? as usize;
        let name_start = p + 8;
        let name_len = meta.get(name_start..).and_then(|r| r.iter().position(|&b| b == 0)).ok_or("stream name unterminated")?;
        if name_len > 32 {
            return Err("implausible stream name".into());
        }
        let name = String::from_utf8_lossy(&meta[name_start..name_start + name_len]).into_owned();
        p = name_start + ((name_len + 1 + 3) & !3);
        let data = so.checked_add(ss).and_then(|e| meta.get(so..e));
        match (name.as_str(), data) {
            ("#Strings", Some(d)) => {
                facts.identifiers = parse_strings_heap(d);
                facts.identifier_count = facts.identifiers.len();
            }
            ("#US", Some(d)) => facts.user_strings = parse_user_strings(d),
            _ => {}
        }
        facts.streams.push(name);
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_heap_splits_on_nul() {
        let heap = b"\0System.Net\0WebClient\0DownloadString\0\0";
        assert_eq!(parse_strings_heap(heap), ["System.Net", "WebClient", "DownloadString"]);
    }

    #[test]
    fn user_strings_decode_utf16_with_compressed_lengths() {
        // "http://x" as UTF-16 + flag byte; len = 8*2+1 = 17
        let mut heap = vec![0u8];
        let s = "http://x";
        heap.push((s.len() * 2 + 1) as u8);
        for c in s.chars() {
            heap.push(c as u8);
            heap.push(0);
        }
        heap.push(0); // flag byte
        // a deliberately truncated 2-byte-length entry (claims 259 bytes, none present):
        // the parser must stop at it, not panic or read past the heap.
        heap.push(0x81);
        heap.push(0x03);
        let got = parse_user_strings(&heap);
        assert_eq!(got, ["http://x"]);
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        let sections = vec![SectionFacts {
            name: ".text".into(),
            long_name: None,
            virtual_size: 0x1000,
            virtual_address: 0x1000,
            raw_size: 0x200,
            raw_offset: 0x200,
            characteristics: 0x6000_0020,
            entropy: 0.0,
            executable: true,
            writable: false,
        }];
        let bytes = vec![0xAAu8; 0x400];
        assert!(parse(&bytes, &sections, 0x1000, 72).is_err());
        assert!(parse(&bytes, &sections, 0x9999_0000, 72).is_err());
        assert!(parse(&bytes, &sections, 0x1000, 10).is_err());
    }

    fn le32(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    #[test]
    fn parses_a_hand_built_metadata_root() {
        // CLR header at file 0x200 (rva 0x1000), metadata at file 0x300 (rva 0x1100)
        let mut bytes = vec![0u8; 0x800];
        bytes[0x200..0x204].copy_from_slice(&le32(72));
        bytes[0x208..0x20c].copy_from_slice(&le32(0x1100)); // metadata rva
        bytes[0x20c..0x210].copy_from_slice(&le32(0x300)); // metadata size
        bytes[0x210..0x214].copy_from_slice(&le32(0x1)); // flags: IL only
        let m = 0x300;
        bytes[m..m + 4].copy_from_slice(&le32(0x424A_5342));
        bytes[m + 12..m + 16].copy_from_slice(&le32(12));
        bytes[m + 16..m + 28].copy_from_slice(b"v4.0.30319\0\0");
        let mut p = m + 28 + 2;
        bytes[p..p + 2].copy_from_slice(&2u16.to_le_bytes()); // 2 streams
        p += 2;
        // stream data lives at metadata offsets 0x100 / 0x140
        let strings = b"\0System.Net\0WebClient\0";
        bytes[m + 0x100..m + 0x100 + strings.len()].copy_from_slice(strings);
        let mut us = vec![0u8, 11];
        for c in "evil.test".chars() {
            us.push(c as u8);
            us.push(0);
        }
        // len byte must be 2*9+1 = 19
        us[1] = 19;
        us.push(0);
        bytes[m + 0x140..m + 0x140 + us.len()].copy_from_slice(&us);
        // stream headers
        for (name, off, size) in [("#Strings", 0x100u32, strings.len() as u32), ("#US", 0x140u32, us.len() as u32)] {
            bytes[p..p + 4].copy_from_slice(&le32(off));
            bytes[p + 4..p + 8].copy_from_slice(&le32(size));
            let nb = name.as_bytes();
            bytes[p + 8..p + 8 + nb.len()].copy_from_slice(nb);
            p += 8 + ((nb.len() + 1 + 3) & !3);
        }
        let sections = vec![SectionFacts {
            name: ".text".into(),
            long_name: None,
            virtual_size: 0x600,
            virtual_address: 0x1000,
            raw_size: 0x600,
            raw_offset: 0x200,
            characteristics: 0x6000_0020,
            entropy: 0.0,
            executable: true,
            writable: false,
        }];
        let f = parse(&bytes, &sections, 0x1000, 72).unwrap();
        assert_eq!(f.runtime_version, "v4.0.30319");
        assert!(f.il_only && !f.requires_32bit);
        assert_eq!(f.streams, ["#Strings", "#US"]);
        assert_eq!(f.identifiers, ["System.Net", "WebClient"]);
        assert_eq!(f.user_strings, ["evil.test"]);
    }
}
