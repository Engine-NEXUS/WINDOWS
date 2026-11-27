//! Electron `app.asar` triage. Format (per electron/asar): `u32 = 4`, `u32 H` (size of the header pickle),
//! then a pickle `[u32 payload_size][u32 json_len][json bytes, padded to 4]`; file data starts at `8 + H`,
//! entries carry `size` and a string `offset` relative to that start. Read-only; nothing is extracted to disk.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const MAX_HEADER_JSON: usize = 32 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 200_000;
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Serialize, Default)]
pub struct AsarFacts {
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
    pub native_modules: Vec<String>,
    pub unpacked_count: usize,
    pub has_integrity: bool,
    pub package_name: Option<String>,
    pub package_version: Option<String>,
    pub main: Option<String>,
    pub description: Option<String>,
    pub dependencies: Vec<String>,
    pub top_level: Vec<String>,
    pub preload_scripts: Vec<String>,
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o.checked_add(4)?)?.try_into().ok()?))
}

/// Does `bytes` start like an asar archive (cheap check before the full parse)?
pub fn looks_like_asar(bytes: &[u8]) -> bool {
    u32_at(bytes, 0) == Some(4) && u32_at(bytes, 4).map_or(false, |h| h >= 8 && (h as usize) + 8 <= bytes.len())
}

struct Walk<'a> {
    bytes: &'a [u8],
    data_start: usize,
    facts: AsarFacts,
    entries: usize,
    package_json: Option<(u64, u64)>,
}

impl Walk<'_> {
    fn walk(&mut self, files: &serde_json::Map<String, Value>, path: &str, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for (name, node) in files {
            self.entries += 1;
            if self.entries > MAX_ENTRIES {
                return;
            }
            let full = if path.is_empty() { name.clone() } else { format!("{path}/{name}") };
            if let Some(children) = node.get("files").and_then(Value::as_object) {
                self.facts.dir_count += 1;
                if depth == 0 {
                    self.facts.top_level.push(name.clone());
                }
                self.walk(children, &full, depth + 1);
            } else {
                self.facts.file_count += 1;
                let size = node.get("size").and_then(Value::as_u64).unwrap_or(0);
                self.facts.total_bytes = self.facts.total_bytes.saturating_add(size);
                if depth == 0 {
                    self.facts.top_level.push(name.clone());
                }
                if node.get("unpacked").and_then(Value::as_bool).unwrap_or(false) {
                    self.facts.unpacked_count += 1;
                }
                if node.get("integrity").is_some() {
                    self.facts.has_integrity = true;
                }
                if name.to_ascii_lowercase().ends_with(".node") && self.facts.native_modules.len() < 200 {
                    self.facts.native_modules.push(full.clone());
                }
                let lower = name.to_ascii_lowercase();
                if (lower.contains("preload")) && lower.ends_with(".js") && self.facts.preload_scripts.len() < 50 {
                    self.facts.preload_scripts.push(full.clone());
                }
                if full == "package.json" {
                    let off = node.get("offset").and_then(|o| o.as_str().and_then(|s| s.parse::<u64>().ok()).or_else(|| o.as_u64()));
                    if let Some(off) = off {
                        self.package_json = Some((off, size));
                    }
                }
            }
        }
    }
}

/// Parse an asar archive from `bytes`.
pub fn parse(bytes: &[u8]) -> Result<AsarFacts, String> {
    if !looks_like_asar(bytes) {
        return Err("not an asar archive".into());
    }
    let h = u32_at(bytes, 4).ok_or("truncated")? as usize;
    let json_len = u32_at(bytes, 12).ok_or("truncated header")? as usize;
    if json_len > MAX_HEADER_JSON || 16usize.checked_add(json_len).map_or(true, |e| e > 8 + h || e > bytes.len()) {
        return Err("implausible asar header size".into());
    }
    let json: Value = serde_json::from_slice(&bytes[16..16 + json_len]).map_err(|e| format!("asar header JSON: {e}"))?;
    let files = json.get("files").and_then(Value::as_object).ok_or("asar header has no files table")?;
    let mut w = Walk { bytes, data_start: 8 + h, facts: AsarFacts::default(), entries: 0, package_json: None };
    w.walk(files, "", 0);
    if w.entries > MAX_ENTRIES {
        w.facts.top_level.push("(listing truncated)".to_string());
    }
    if let Some((off, size)) = w.package_json {
        let start = w.data_start.checked_add(off as usize);
        let slice = start.and_then(|s| s.checked_add(size.min(1 << 20) as usize).and_then(|e| w.bytes.get(s..e)));
        if let Some(pj) = slice.and_then(|s| serde_json::from_slice::<Value>(s).ok()) {
            let s = |k: &str| pj.get(k).and_then(Value::as_str).map(str::to_string);
            w.facts.package_name = s("name");
            w.facts.package_version = s("version");
            w.facts.main = s("main");
            w.facts.description = s("description");
            if let Some(d) = pj.get("dependencies").and_then(Value::as_object) {
                w.facts.dependencies = d.iter().take(60).map(|(k, v)| format!("{k}@{}", v.as_str().unwrap_or("?"))).collect();
            }
        }
    }
    w.facts.top_level.sort();
    w.facts.top_level.dedup();
    w.facts.top_level.truncate(40);
    Ok(w.facts)
}

/// Where an Electron app keeps its code relative to its executable: `<exe dir>/resources/app.asar`.
pub fn sibling_asar_path(exe: &Path) -> Option<PathBuf> {
    let p = exe.parent()?.join("resources").join("app.asar");
    p.is_file().then_some(p)
}

#[cfg(test)]
pub(crate) mod testutil {
    use serde_json::json;

    /// Build a small asar: files `(path, content)`; `integrity` marks files with an integrity block.
    pub fn build_asar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut root = serde_json::Map::new();
        for (path, content) in files {
            let parts: Vec<&str> = path.split('/').collect();
            let mut cur = &mut root;
            for dir in &parts[..parts.len() - 1] {
                cur = cur
                    .entry(dir.to_string())
                    .or_insert_with(|| json!({"files": {}}))
                    .get_mut("files")
                    .unwrap()
                    .as_object_mut()
                    .unwrap();
            }
            cur.insert(
                parts[parts.len() - 1].to_string(),
                json!({"size": content.len(), "offset": data.len().to_string(), "integrity": {"algorithm": "SHA256", "hash": "00"}}),
            );
            data.extend_from_slice(content);
        }
        let header = serde_json::to_vec(&json!({ "files": root })).unwrap();
        let padded = (header.len() + 3) & !3;
        let mut out = Vec::new();
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&((padded + 8) as u32).to_le_bytes()); // H = payload_size field + json_len field + padded json
        out.extend_from_slice(&((padded + 4) as u32).to_le_bytes()); // pickle payload size
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(&header);
        out.resize(8 + padded + 8, 0);
        out.extend_from_slice(&data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::build_asar;
    use super::*;

    #[test]
    fn parses_structure_package_json_and_native_modules() {
        let pkg = br#"{"name":"demo-app","version":"1.2.3","main":"main.js","description":"A demo","dependencies":{"express":"^4.18.0","ws":"8.0.0"}}"#;
        let a = build_asar(&[
            ("package.json", pkg),
            ("main.js", b"console.log(1)"),
            ("preload.js", b"x"),
            ("node_modules/native/build/addon.node", b"\x7fELF"),
            ("node_modules/native/index.js", b"y"),
            ("src/ui/index.html", b"<html>"),
        ]);
        assert!(looks_like_asar(&a));
        let f = parse(&a).unwrap();
        assert_eq!(f.file_count, 6);
        assert!(f.dir_count >= 4);
        assert_eq!(f.package_name.as_deref(), Some("demo-app"));
        assert_eq!(f.package_version.as_deref(), Some("1.2.3"));
        assert_eq!(f.main.as_deref(), Some("main.js"));
        assert_eq!(f.dependencies, ["express@^4.18.0", "ws@8.0.0"]);
        assert_eq!(f.native_modules, ["node_modules/native/build/addon.node"]);
        assert_eq!(f.preload_scripts, ["preload.js"]);
        assert!(f.has_integrity);
        assert!(f.top_level.contains(&"node_modules".to_string()) && f.top_level.contains(&"package.json".to_string()));
    }

    #[test]
    fn rejects_non_asar_and_hostile_headers_without_panicking() {
        assert!(parse(b"").is_err());
        assert!(parse(b"MZ not an asar at all").is_err());
        let mut a = build_asar(&[("a.js", b"x")]);
        a[12..16].copy_from_slice(&u32::MAX.to_le_bytes()); // json_len lie
        assert!(parse(&a).is_err());
        let mut b = build_asar(&[("a.js", b"x")]);
        b[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // header size lie
        assert!(parse(&b).is_err());
        let mut c = build_asar(&[("a.js", b"x")]);
        let n = c.len();
        c.truncate(n.min(20));
        assert!(parse(&c).is_err());
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let deep = |n: usize| {
            let mut path = String::new();
            for _ in 0..n {
                path.push_str("d/");
            }
            path.push_str("f.txt");
            build_asar(&[(path.as_str(), b"x")])
        };
        // moderately deep trees parse; hostile depth is rejected by the JSON recursion limit (an Err,
        // never a stack overflow or panic)
        assert!(parse(&deep(20)).is_ok());
        let _ = parse(&deep(500));
    }
}
