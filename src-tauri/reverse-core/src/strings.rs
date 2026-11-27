//! Printable-string extraction (ASCII + UTF-16LE) with classification. Values are passed through
//! `redact::redact_text` so an API key embedded in a binary never shows up in a report.

use crate::redact::redact_text;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::OnceLock;

pub const MAX_RAW_STRINGS: usize = 40_000;
pub const MAX_STRING_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrClass {
    Url,
    Command,
    Ip,
    Registry,
    Pdb,
    Email,
    UserAgent,
    Domain,
    Path,
    Blob,
    Plain,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FoundString {
    pub value: String,
    pub offset: usize,
    pub utf16: bool,
    pub class: StrClass,
}

struct Rx {
    url: Regex,
    ip: Regex,
    registry: Regex,
    command: Regex,
    path: Regex,
    email: Regex,
    domain: Regex,
}

fn rx() -> &'static Rx {
    static R: OnceLock<Rx> = OnceLock::new();
    R.get_or_init(|| Rx {
        url: Regex::new(r"(?i)\b(?:https?|ftp|wss?)://[^\s'\x22<>]{3,}").unwrap(),
        ip: Regex::new(r"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?::\d{1,5})?$").unwrap(),
        registry: Regex::new(r"(?i)^(?:HKEY_[A-Z_]+|HKLM|HKCU|HKCR|HKU)\\|^SOFTWARE\\|^SYSTEM\\CurrentControlSet|\\CurrentVersion\\Run").unwrap(),
        command: Regex::new(
            r"(?i)(?:\bcmd(?:\.exe)?\s+/[ck]\b|\bpowershell(?:\.exe)?\s+-|\brundll32(?:\.exe)?\s|\bregsvr32(?:\.exe)?\s|\bmshta(?:\.exe)?\s|\bwscript(?:\.exe)?\s|\bcscript(?:\.exe)?\s|\bschtasks(?:\.exe)?\s+/|\bbitsadmin\s|\bcertutil(?:\.exe)?\s+-|\bvssadmin\s|\bwmic\s|\bnet\s+user\s|\bnetsh\s)",
        )
        .unwrap(),
        path: Regex::new(r"(?i)^(?:[a-z]:\\[^\\\s]|\\\\[^\\\s]+\\|%[a-z_]+%\\)").unwrap(),
        email: Regex::new(r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$").unwrap(),
        domain: Regex::new(r"(?i)^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+(?:com|net|org|io|ru|cn|xyz|top|info|biz|co|me|tk|ml|ga|cf|gq|cc|onion|dev|app|cloud|site|online)$").unwrap(),
    })
}

fn is_public_ipv4(s: &str) -> bool {
    let Some(c) = rx().ip.captures(s) else { return false };
    let oct: Vec<u32> = (1..=4).filter_map(|i| c.get(i)?.as_str().parse().ok()).collect();
    if oct.len() != 4 || oct.iter().any(|&o| o > 255) {
        return false;
    }
    // version-number lookalikes ("1.0.0.0", "6.1.7601.0" would have failed the <=255 test already)
    !(oct == [0, 0, 0, 0] || oct[0] == 0 || oct.iter().all(|&o| o <= 9))
}

pub fn classify(s: &str) -> StrClass {
    let r = rx();
    if r.url.is_match(s) {
        StrClass::Url
    } else if r.command.is_match(s) {
        StrClass::Command
    } else if is_public_ipv4(s) {
        StrClass::Ip
    } else if r.registry.is_match(s) {
        StrClass::Registry
    } else if s.to_ascii_lowercase().ends_with(".pdb") {
        StrClass::Pdb
    } else if r.email.is_match(s) {
        StrClass::Email
    } else if s.contains("Mozilla/") {
        StrClass::UserAgent
    } else if r.domain.is_match(s) {
        StrClass::Domain
    } else if r.path.is_match(s) {
        StrClass::Path
    } else if s.len() >= 40
        && s.bytes().filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')).count() * 100 / s.len() >= 97
        && !s.contains(' ')
    {
        StrClass::Blob
    } else {
        StrClass::Plain
    }
}

fn push(out: &mut Vec<FoundString>, seen: &mut HashSet<String>, raw: &str, offset: usize, utf16: bool) {
    if out.len() >= MAX_RAW_STRINGS {
        return;
    }
    let value: String = raw.chars().take(MAX_STRING_CHARS).collect();
    if !seen.insert(value.clone()) {
        return;
    }
    let class = classify(&value);
    out.push(FoundString { value: redact_text(&value), offset, utf16, class });
}

/// Extract ASCII and UTF-16LE strings of at least `min_len` characters from `bytes`.
/// De-duplicates by value; stops after [`MAX_RAW_STRINGS`].
pub fn extract(bytes: &[u8], min_len: usize) -> Vec<FoundString> {
    let min_len = min_len.max(3);
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // ASCII
    let (mut start, mut len) = (0usize, 0usize);
    for (i, &b) in bytes.iter().enumerate() {
        if (0x20..=0x7e).contains(&b) || b == 0x09 {
            if len == 0 {
                start = i;
            }
            len += 1;
        } else {
            if len >= min_len {
                push(&mut out, &mut seen, &String::from_utf8_lossy(&bytes[start..start + len]), start, false);
            }
            len = 0;
        }
    }
    if len >= min_len {
        push(&mut out, &mut seen, &String::from_utf8_lossy(&bytes[start..start + len]), start, false);
    }

    // UTF-16LE (printable ASCII range only): char byte then 0x00
    for parity in 0..2usize {
        let (mut start, mut n) = (0usize, 0usize);
        let mut i = parity;
        while i + 1 < bytes.len() {
            let (c, z) = (bytes[i], bytes[i + 1]);
            if z == 0 && ((0x20..=0x7e).contains(&c) || c == 0x09) {
                if n == 0 {
                    start = i;
                }
                n += 1;
            } else {
                if n >= min_len {
                    let s: String = bytes[start..start + n * 2].chunks_exact(2).map(|p| p[0] as char).collect();
                    push(&mut out, &mut seen, &s, start, true);
                }
                n = 0;
            }
            i += 2;
        }
        if n >= min_len {
            let s: String = bytes[start..start + n * 2].chunks_exact(2).map(|p| p[0] as char).collect();
            push(&mut out, &mut seen, &s, start, true);
        }
    }
    out
}

/// The strings worth showing/citing: every non-`Plain` class, ordered by how telling the class is,
/// capped at `max`.
pub fn interesting(all: &[FoundString], max: usize) -> Vec<FoundString> {
    let mut v: Vec<FoundString> = all.iter().filter(|s| s.class != StrClass::Plain).cloned().collect();
    v.sort_by(|a, b| a.class.cmp(&b.class).then(a.offset.cmp(&b.offset)));
    v.truncate(max);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        let cases = [
            ("http://evil.example.test/a.php?id=1", StrClass::Url),
            ("cmd.exe /c whoami", StrClass::Command),
            ("powershell -nop -w hidden -enc AAAA", StrClass::Command),
            ("203.0.113.77", StrClass::Ip),
            ("203.0.113.77:8080", StrClass::Ip),
            ("1.0.0.0", StrClass::Plain),
            ("6.1.7601.17514", StrClass::Plain),
            ("HKEY_CURRENT_USER\\Software\\Foo", StrClass::Registry),
            ("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run", StrClass::Registry),
            ("C:\\work\\build\\app.pdb", StrClass::Pdb),
            ("admin@example.org", StrClass::Email),
            ("Mozilla/5.0 (Windows NT 10.0)", StrClass::UserAgent),
            ("update.example.xyz", StrClass::Domain),
            ("C:\\Windows\\System32\\drivers\\etc\\hosts", StrClass::Path),
            ("%APPDATA%\\Roaming\\x.dat", StrClass::Path),
            ("hello world this is plain text", StrClass::Plain),
        ];
        for (s, want) in cases {
            assert_eq!(classify(s), want, "{s}");
        }
        let blob = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2d3h5eg==";
        assert_eq!(classify(blob), StrClass::Blob);
    }

    #[test]
    fn extracts_ascii_and_utf16_and_dedupes() {
        let mut data = Vec::new();
        data.extend_from_slice(b"\x00\x01http://a.example.test/x\x00\x02");
        data.extend_from_slice(b"http://a.example.test/x\x00"); // duplicate
        data.extend_from_slice(&[0xff, 0xfe]); // separator so the NUL of the previous string is not read as UTF-16
        for c in "cmd.exe /c dir".chars() {
            data.push(c as u8);
            data.push(0);
        }
        data.extend_from_slice(&[0xff, 0xfe, 0x00]);
        let s = extract(&data, 5);
        let urls: Vec<_> = s.iter().filter(|x| x.class == StrClass::Url).collect();
        assert_eq!(urls.len(), 1, "{s:?}");
        assert!(!urls[0].utf16);
        let cmd = s.iter().find(|x| x.class == StrClass::Command).expect("utf16 command");
        assert!(cmd.utf16);
        assert_eq!(cmd.value, "cmd.exe /c dir");
    }

    #[test]
    fn secrets_inside_strings_are_redacted() {
        let data = b"\x00password=hunter2hunter2&x=1\x00 https://x.test/?api_key=SUPERSECRETKEY123 \x00";
        let s = extract(data, 5);
        let all: String = s.iter().map(|x| x.value.clone()).collect::<Vec<_>>().join("|");
        assert!(!all.contains("hunter2hunter2") && !all.contains("SUPERSECRETKEY123"), "{all}");
    }

    #[test]
    fn interesting_orders_by_class_and_caps() {
        let all = vec![
            FoundString { value: "plain".into(), offset: 1, utf16: false, class: StrClass::Plain },
            FoundString { value: "c:\\x\\y".into(), offset: 2, utf16: false, class: StrClass::Path },
            FoundString { value: "http://x.test/".into(), offset: 9, utf16: false, class: StrClass::Url },
        ];
        let i = interesting(&all, 10);
        assert_eq!(i.iter().map(|s| s.class).collect::<Vec<_>>(), [StrClass::Url, StrClass::Path]);
        assert_eq!(interesting(&all, 1).len(), 1);
    }

    #[test]
    fn caps_raw_strings() {
        let mut data = Vec::new();
        for i in 0..(MAX_RAW_STRINGS + 500) {
            data.extend_from_slice(format!("string-number-{i}\x00").as_bytes());
        }
        assert_eq!(extract(&data, 5).len(), MAX_RAW_STRINGS);
    }
}
