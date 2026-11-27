//! `nexus-reverse` - command-line front end for the Reverse Lab core and its measurement harness.
//!
//!   nexus-reverse analyse <file>            full analysis as JSON (nothing is executed)
//!   nexus-reverse batch <dir> [--full] [--max-mb N]
//!                                           one JSON line per file: facts + ms (R1); --full = whole pipeline, files <= N MB (R7)
//!   nexus-reverse fuzz <dir> [--mutants N] [--seed S] [--inject panic|slow|alloc]   (--inject = harness self-test)
//!                                           mutate real files, count panics / slow parses / big allocations (R2)
//!   nexus-reverse rules --selftest          run every rule's own examples (R3 gate)
//!   nexus-reverse recall                    rule examples -> synthetic PEs -> the REAL pipeline; recall + negatives (R3)
//!   nexus-reverse benign <dir> [--max-mb N] known-benign files: high-severity rule hits + packer false flags (R3, R4)
//!   nexus-reverse packed <dir>              packed fixtures (real UPX output etc. + built-in synthetic ones): flag rate (R4)

use nexus_reverse::analyze::{analyze_bytes, Analysis};
use nexus_reverse::pe;
use nexus_reverse::rules::{self, Severity};
use nexus_reverse::synth::build_pe;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

// ─── allocation accounting (for the "no allocation bombs" check) ───────────
struct Counting;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            let now = CURRENT.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l);
        CURRENT.fetch_sub(l.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = System.realloc(p, l, new);
        if !q.is_null() {
            if new >= l.size() {
                let now = CURRENT.fetch_add(new - l.size(), Ordering::Relaxed) + (new - l.size());
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                CURRENT.fetch_sub(l.size() - new, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static A: Counting = Counting;

// ─── tiny deterministic PRNG (xorshift64*) ─────────────────────────────────
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

fn peak_rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines().find(|l| l.starts_with("VmHWM:"))?.split_whitespace().nth(1)?.parse().ok()
}

fn cmd_analyse(path: &str) -> i32 {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return 2;
        }
    };
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
    let a = analyze_bytes(&bytes, name);
    println!("{}", serde_json::to_string_pretty(&a).unwrap());
    0
}

fn cmd_batch(dir: &str, full: bool, max_mb: u64) -> i32 {
    let mut files = Vec::new();
    walk(Path::new(dir), &mut files);
    let rss0 = peak_rss_kb().unwrap_or(0);
    for p in files {
        if full && p.metadata().map_or(true, |m| m.len() > max_mb << 20) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let t = Instant::now();
        if full {
            // end-to-end pipeline timing (strings + evidence + everything), no facts echoed
            let r = catch_unwind(AssertUnwindSafe(|| analyze_bytes(&bytes, "batch")));
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            println!("{}", serde_json::json!({"path": p.display().to_string(), "size": bytes.len(), "ok": r.is_ok(), "ms": ms}));
            continue;
        }
        let r = catch_unwind(AssertUnwindSafe(|| pe::parse(&bytes)));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let line = match r {
            Ok(Ok(f)) => serde_json::json!({"path": p.display().to_string(), "size": bytes.len(), "ok": true, "ms": ms, "facts": f}),
            Ok(Err(e)) => serde_json::json!({"path": p.display().to_string(), "size": bytes.len(), "ok": false, "ms": ms, "error": e}),
            Err(_) => serde_json::json!({"path": p.display().to_string(), "size": bytes.len(), "ok": false, "ms": ms, "error": "PANIC"}),
        };
        println!("{line}");
    }
    if let Some(r) = peak_rss_kb() {
        eprintln!("peak_rss_delta_kb={}", r.saturating_sub(rss0));
    }
    0
}

/// Locations of header fields inside a (valid) PE, for targeted "header lie" mutations.
fn header_fields(b: &[u8]) -> Vec<(usize, usize)> {
    let mut f = Vec::new();
    if b.len() < 0x40 {
        return f;
    }
    f.push((0x3c, 4));
    let pe_off = u32::from_le_bytes([b[0x3c], b[0x3d], b[0x3e], b[0x3f]]) as usize;
    if pe_off + 24 > b.len() {
        return f;
    }
    f.push((pe_off + 6, 2)); // NumberOfSections
    f.push((pe_off + 20, 2)); // SizeOfOptionalHeader
    let opt = pe_off + 24;
    let magic = u16::from_le_bytes([b[opt], b[opt + 1]]);
    let (dd, win) = if magic == 0x20b { (opt + 112, opt + 24) } else { (opt + 96, opt + 28) };
    f.push((win + 4, 4)); // SectionAlignment
    f.push((win + 8, 4)); // FileAlignment
    f.push((opt + 56, 4)); // SizeOfImage (PE32+; harmless elsewhere)
    for i in 0..16 {
        f.push((dd + i * 8, 4));
        f.push((dd + i * 8 + 4, 4));
    }
    let opt_size = u16::from_le_bytes([b[pe_off + 20], b[pe_off + 21]]) as usize;
    let nsec = u16::from_le_bytes([b[pe_off + 6], b[pe_off + 7]]) as usize;
    let st = opt + opt_size;
    for s in 0..nsec.min(96) {
        let o = st + s * 40;
        for rel in [8usize, 12, 16, 20] {
            f.push((o + rel, 4));
        }
    }
    f.retain(|&(o, n)| o + n <= b.len());
    f
}

fn mutate(src: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut b = src.to_vec();
    match rng.below(6) {
        0 => b.truncate(rng.below(b.len().max(1))),
        1 => {
            for _ in 0..1 + rng.below(64) {
                let i = rng.below(b.len().max(1));
                if let Some(x) = b.get_mut(i) {
                    *x ^= 1 << rng.below(8);
                }
            }
        }
        2 => {
            let lim = b.len().min(1024).max(1);
            for _ in 0..1 + rng.below(16) {
                let i = rng.below(lim);
                if let Some(x) = b.get_mut(i) {
                    *x ^= 1 << rng.below(8);
                }
            }
        }
        3 => {
            let fields = header_fields(&b);
            for _ in 0..1 + rng.below(4) {
                if fields.is_empty() {
                    break;
                }
                let (o, n) = fields[rng.below(fields.len())];
                let v: u32 = match rng.below(5) {
                    0 => 0,
                    1 => 0xFFFF_FFFF,
                    2 => 0x7FFF_FFFF,
                    3 => 0x8000_0000,
                    _ => rng.next() as u32,
                };
                b[o..o + n].copy_from_slice(&v.to_le_bytes()[..n]);
            }
        }
        4 => {
            let start = rng.below(b.len().max(1));
            let end = (start + 4096).min(b.len());
            b[start..end].iter_mut().for_each(|x| *x = 0);
        }
        _ => {
            if b.len() > 64 {
                let len = 1 + rng.below(b.len().min(4096));
                let from = rng.below(b.len() - len.min(b.len() - 1));
                let to = rng.below(b.len() - len.min(b.len() - 1));
                let chunk: Vec<u8> = b[from..from + len.min(b.len() - from)].to_vec();
                for (k, c) in chunk.iter().enumerate() {
                    if let Some(x) = b.get_mut(to + k) {
                        *x = *c;
                    }
                }
            }
        }
    }
    b
}

fn cmd_fuzz(dir: &str, mutants: usize, seed: u64, inject: Option<&str>) -> i32 {
    let mut files = Vec::new();
    walk(Path::new(dir), &mut files);
    // sources: real PE files up to 8 MiB (keeps a 2000-mutant run fast)
    let mut sources: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for p in files {
        if let Ok(b) = std::fs::read(&p) {
            if b.len() <= 8 << 20 && pe::parse(&b).is_ok() {
                sources.push((p, b));
            }
        }
    }
    if sources.is_empty() {
        eprintln!("no parseable PE files in {dir}");
        println!("{}", serde_json::json!({"mutants": 0, "panics": 0, "hangs": 0, "oversize": 0, "sources": 0}));
        return 2;
    }
    static PANICS: AtomicUsize = AtomicUsize::new(0);
    std::panic::set_hook(Box::new(|_| {
        PANICS.fetch_add(1, Ordering::Relaxed);
    }));
    let mut rng = Rng(seed.max(1) ^ 0x9E37_79B9_7F4A_7C15);
    let (mut hangs, mut oversize, mut ok_parses) = (0usize, 0usize, 0usize);
    let mut worst_ms = 0f64;
    let mut worst_alloc = 0usize;
    let mut offenders: BTreeMap<String, usize> = BTreeMap::new();
    for i in 0..mutants {
        let (path, src) = &sources[rng.below(sources.len())];
        let m = mutate(src, &mut rng);
        let base = CURRENT.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let panics_before = PANICS.load(Ordering::Relaxed);
        let t = Instant::now();
        let r = catch_unwind(AssertUnwindSafe(|| {
            // Harness self-test: prove the counters can actually see a panic / slow parse / allocation bomb.
            if i == 0 {
                match inject {
                    Some("panic") => panic!("injected panic"),
                    Some("slow") => std::thread::sleep(std::time::Duration::from_millis(2100)),
                    Some("alloc") => {
                        let v = vec![1u8; 700 << 20];
                        std::hint::black_box(&v);
                    }
                    _ => {}
                }
            }
            analyze_bytes(&m, "mutant")
        }));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let alloc = PEAK.load(Ordering::Relaxed).saturating_sub(base).saturating_sub(m.len());
        worst_ms = worst_ms.max(ms);
        worst_alloc = worst_alloc.max(alloc);
        let cap = (256usize << 20).max(m.len().saturating_mul(16));
        if ms > 2000.0 {
            hangs += 1;
            *offenders.entry(format!("slow:{}", path.display())).or_default() += 1;
        }
        if alloc > cap {
            oversize += 1;
            *offenders.entry(format!("alloc:{}", path.display())).or_default() += 1;
        }
        if PANICS.load(Ordering::Relaxed) > panics_before || r.is_err() {
            *offenders.entry(format!("panic:{}#{}", path.display(), i)).or_default() += 1;
        }
        if r.as_ref().map_or(false, |a| a.pe.is_some()) {
            ok_parses += 1;
        }
    }
    let panics = PANICS.load(Ordering::Relaxed);
    let report = serde_json::json!({
        "mutants": mutants, "sources": sources.len(), "panics": panics, "hangs": hangs, "oversize": oversize,
        "still_parsed": ok_parses, "worst_ms": worst_ms, "worst_alloc_bytes": worst_alloc,
        "seed": seed, "offenders": offenders.iter().take(20).collect::<Vec<_>>(),
    });
    println!("{report}");
    0
}

// ─── R3 / R4 evaluation ────────────────────────────────────────────────────

fn cmd_rules_selftest() -> i32 {
    match rules::builtin() {
        Ok(set) => {
            let rep = rules::selftest(set);
            println!("{}", serde_json::to_string(&rep).unwrap());
            if rep.failures.is_empty() { 0 } else { 1 }
        }
        Err(e) => {
            println!("{}", serde_json::json!({"rules": 0, "failures": [e]}));
            1
        }
    }
}

/// Build a PE carrying exactly the facts of a rule example, or None when this fixture builder cannot
/// express them (export tables, .NET header, packed/derived characteristics).
fn pe_for_example(ex: &rules::Example) -> Option<Vec<u8>> {
    if !ex.exports.is_empty() {
        return None;
    }
    let mut is_dll = false;
    let mut subsystem: Option<u16> = None;
    for c in &ex.chars {
        match c.as_str() {
            "is_dll" => is_dll = true,
            "is_gui" => subsystem = Some(2),
            "is_console" => subsystem = Some(3),
            "unsigned" | "no_imports" => {}
            _ => return None,
        }
    }
    let mut data: Vec<u8> = Vec::new();
    for s in &ex.strings {
        data.extend_from_slice(s.as_bytes());
        data.push(0);
    }
    while data.len() < 64 {
        data.push(0);
    }
    let mut sections: Vec<(String, Vec<u8>, u32)> = vec![(".text".into(), vec![0x90; 256], 0x6000_0020), (".data".into(), data, 0xC000_0040)];
    for sec in &ex.sections {
        sections.push((sec.clone(), vec![0u8; 64], 0x4000_0040));
    }
    let secs: Vec<(&str, Vec<u8>, u32)> = sections.iter().map(|(n, d, c)| (n.as_str(), d.clone(), *c)).collect();
    let first_dll = ex.dlls.first().cloned().unwrap_or_else(|| "KERNEL32.dll".to_string());
    let api_refs: Vec<&str> = ex.apis.iter().map(String::as_str).collect();
    let mut imports: Vec<(String, Vec<String>)> = Vec::new();
    if !api_refs.is_empty() || !ex.dlls.is_empty() {
        let mut first: Vec<String> = ex.apis.clone();
        if first.is_empty() {
            first.push("PlaceholderFn".into());
        }
        imports.push((first_dll, first));
        for d in ex.dlls.iter().skip(1) {
            imports.push((d.clone(), vec!["PlaceholderFn".into()]));
        }
    }
    let imp_refs: Vec<(&str, Vec<&str>)> = imports.iter().map(|(d, f)| (d.as_str(), f.iter().map(String::as_str).collect())).collect();
    let imp_slices: Vec<(&str, &[&str])> = imp_refs.iter().map(|(d, f)| (*d, f.as_slice())).collect();
    let mut pe = build_pe(&secs, &imp_slices, is_dll);
    if let Some(sub) = subsystem {
        // Subsystem sits at optional-header offset 68 (PE32+); the builder puts the PE header at 0x80.
        let off = 0x80 + 24 + 68;
        pe[off..off + 2].copy_from_slice(&sub.to_le_bytes());
    }
    Some(pe)
}

fn cmd_recall() -> i32 {
    let set = match rules::builtin() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("rules failed to load: {e}");
            return 2;
        }
    };
    let (mut pos_n, mut pos_k, mut neg_n, mut neg_ok, mut skipped) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut misses: Vec<String> = Vec::new();
    for r in &set.rules {
        for (i, ex) in r.def.examples.iter().enumerate() {
            let Some(bytes) = pe_for_example(ex) else {
                skipped += 1;
                continue;
            };
            let a: Analysis = analyze_bytes(&bytes, "example.exe");
            let fired = a.capabilities.iter().any(|h| h.name == r.def.name);
            if ex.expect {
                pos_n += 1;
                if fired {
                    pos_k += 1;
                } else {
                    misses.push(format!("{} #{i}: expected a hit through the real pipeline", r.def.name));
                }
            } else {
                neg_n += 1;
                if !fired {
                    neg_ok += 1;
                } else {
                    misses.push(format!("{} #{i}: negative example fired through the real pipeline", r.def.name));
                }
            }
        }
    }
    let rep = rules::selftest(set);
    println!(
        "{}",
        serde_json::json!({
            "rules": rep.rules, "selftest_fail": rep.failures.len(), "recall_n": pos_n, "recall_k": pos_k,
            "negative_n": neg_n, "negative_ok": neg_ok, "skipped_inexpressible": skipped, "misses": misses,
        })
    );
    0
}

fn cmd_benign(dir: &str, max_mb: u64) -> i32 {
    let mut files = Vec::new();
    walk(Path::new(dir), &mut files);
    let (mut n, mut high_files, mut packer_flags) = (0usize, 0usize, 0usize);
    let mut per_rule: BTreeMap<String, usize> = BTreeMap::new();
    let mut high_examples: Vec<String> = Vec::new();
    let mut flagged_examples: Vec<String> = Vec::new();
    for p in files {
        if p.metadata().map_or(true, |m| m.len() > max_mb << 20) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        if pe::parse(&bytes).is_err() {
            continue; // not a PE the parser accepts: nothing to score
        }
        let a = analyze_bytes(&bytes, "benign");
        n += 1;
        let highs: Vec<&str> = a.capabilities.iter().filter(|h| h.severity >= Severity::High).map(|h| h.name.as_str()).collect();
        if !highs.is_empty() {
            high_files += 1;
            for h in &highs {
                *per_rule.entry((*h).to_string()).or_default() += 1;
            }
            if high_examples.len() < 15 {
                high_examples.push(format!("{}: {}", p.file_name().and_then(|s| s.to_str()).unwrap_or("?"), highs.join("; ")));
            }
        }
        if a.packer.as_ref().map_or(false, |v| v.is_flagged()) {
            packer_flags += 1;
            if flagged_examples.len() < 15 {
                let why = a.packer.as_ref().map(|v| v.reasons.iter().map(|r| r.code).collect::<Vec<_>>().join(",")).unwrap_or_default();
                flagged_examples.push(format!("{}: {why}", p.file_name().and_then(|s| s.to_str()).unwrap_or("?")));
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "benign_n": n, "benign_fp_k": high_files, "per_rule": per_rule, "high_examples": high_examples,
            "clean_n": n, "clean_flag_k": packer_flags, "flag_examples": flagged_examples,
        })
    );
    0
}

/// Built-in synthetic packed fixtures (named by technique so the Python side can report categories).
fn synthetic_packed() -> Vec<(String, Vec<u8>)> {
    // pseudo-random bytes = incompressible, like real compressed/encrypted payloads
    fn noise(len: usize, mut seed: u64) -> Vec<u8> {
        (0..len)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 24) as u8
            })
            .collect()
    }
    let tiny: &[(&str, &[&str])] = &[("KERNEL32.dll", &["LoadLibraryA", "GetProcAddress", "VirtualAlloc", "ExitProcess"])];
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |name: &str, bytes: Vec<u8>| v.push((format!("synth_{name}.exe"), bytes));
    add("upx_names", build_pe(&[("UPX0", vec![], 0xE000_0080), ("UPX1", noise(20_000, 1), 0xE000_0020), (".rsrc", vec![0; 512], 0x4000_0040)], tiny, false));
    add("aspack_names", build_pe(&[(".aspack", noise(12_000, 2), 0xE000_0020), (".adata", vec![0; 512], 0xC000_0040)], tiny, false));
    add("themida_names", build_pe(&[(".themida", noise(30_000, 3), 0xE000_0020), (".text", vec![0x90; 256], 0x6000_0020)], tiny, false));
    add("vmp_names", build_pe(&[(".vmp0", noise(25_000, 4), 0xE000_0020), (".vmp1", noise(9_000, 5), 0xC000_0040)], tiny, false));
    add("mpress_names", build_pe(&[(".MPRESS1", noise(14_000, 6), 0xE000_0020), (".MPRESS2", noise(6_000, 7), 0xC000_0040)], tiny, false));
    add("renamed_high_entropy_text", build_pe(&[(".text", noise(40_000, 8), 0x6000_0020), (".data", vec![0; 512], 0xC000_0040)], tiny, false));
    add("renamed_rwx_high_entropy", build_pe(&[(".text", noise(24_000, 9), 0xE000_0060)], tiny, false));
    add("renamed_empty_exec_stub", build_pe(&[(".text", vec![], 0xE000_0080), (".data", noise(18_000, 10), 0xC000_0040)], tiny, false));
    add("odd_names_high_entropy", build_pe(&[(".xz9", noise(22_000, 11), 0xE000_0020), (".q1", noise(5_000, 12), 0xC000_0040)], tiny, false));
    add("encrypted_blob_data_section", build_pe(&[(".text", vec![0x90; 300], 0x6000_0020), (".rdata", noise(70_000, 13), 0x4000_0040)], tiny, false));
    add("stub_with_loadlib_getproc_only", build_pe(&[(".text", vec![0x90; 300], 0x6000_0020), (".data", noise(30_000, 14), 0xC000_0040)], &[("KERNEL32.dll", &["LoadLibraryA", "GetProcAddress"])], false));
    add("no_imports_high_entropy", build_pe(&[(".text", noise(30_000, 15), 0xE000_0020)], &[], false));
    v
}

fn cmd_packed(dir: &str) -> i32 {
    let mut files = Vec::new();
    walk(Path::new(dir), &mut files);
    let mut items: Vec<(String, Vec<u8>)> = Vec::new();
    for p in files {
        if let Ok(b) = std::fs::read(&p) {
            items.push((p.file_name().and_then(|s| s.to_str()).unwrap_or("?").to_string(), b));
        }
    }
    items.extend(synthetic_packed());
    let (mut n, mut k) = (0usize, 0usize);
    let mut by_cat: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut missed: Vec<String> = Vec::new();
    for (name, bytes) in &items {
        let a = analyze_bytes(bytes, name);
        // A fixture Reverse Lab cannot parse counts as NOT flagged (never silently dropped from the denominator).
        let flagged = a.pe.is_some() && a.packer.as_ref().map_or(false, |v| v.is_flagged());
        let cat = name.split('_').next().unwrap_or("other").to_string();
        let e = by_cat.entry(cat).or_default();
        e.0 += 1;
        n += 1;
        if flagged {
            e.1 += 1;
            k += 1;
        } else {
            missed.push(name.clone());
        }
    }
    println!(
        "{}",
        serde_json::json!({"packed_n": n, "packed_k": k, "by_category": by_cat, "missed": missed.iter().take(30).collect::<Vec<_>>()})
    );
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let code = match args.first().map(String::as_str) {
        Some("analyse") | Some("analyze") if args.len() >= 2 => cmd_analyse(&args[1]),
        Some("batch") if args.len() >= 2 => cmd_batch(
            &args[1],
            args.iter().any(|a| a == "--full"),
            opt("--max-mb").and_then(|v| v.parse().ok()).unwrap_or(50),
        ),
        Some("fuzz") if args.len() >= 2 => cmd_fuzz(
            &args[1],
            opt("--mutants").and_then(|v| v.parse().ok()).unwrap_or(2500),
            opt("--seed").and_then(|v| v.parse().ok()).unwrap_or(1337),
            opt("--inject").as_deref(),
        ),
        Some("rules") if args.iter().any(|a| a == "--selftest") => cmd_rules_selftest(),
        Some("recall") => cmd_recall(),
        Some("benign") if args.len() >= 2 => cmd_benign(&args[1], opt("--max-mb").and_then(|v| v.parse().ok()).unwrap_or(50)),
        Some("packed") if args.len() >= 2 => cmd_packed(&args[1]),
        _ => {
            eprintln!("usage: nexus-reverse analyse <file> | batch <dir> | fuzz <dir> [--mutants N] [--seed S] | rules --selftest | recall | benign <dir> | packed <dir>");
            2
        }
    };
    std::process::exit(code);
}
