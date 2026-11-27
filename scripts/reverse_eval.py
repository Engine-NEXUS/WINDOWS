#!/usr/bin/env python3
"""Measure Reverse Lab against independent oracles and write `reverse_results.json` for
`reverse_scorecard.py`.

    python scripts/reverse_eval.py pe      [--corpus reverse_corpus] [--bin PATH] [--out reverse_results.json]
    python scripts/reverse_eval.py fuzz    [--mutants 2500] [--seed 1337]
    python scripts/reverse_eval.py all

R1  field-level agreement of `nexus-reverse` with **pefile** (and **LIEF** where it has the same field):
    a field counts only when the two oracles agree with each other (oracle noise is reported, not scored).
    A file the oracle parses but Reverse Lab rejects counts ALL of that file's fields as disagreements.
R2  mutation fuzzing (truncations, bit flips, header lies) of real files: panics / >2 s parses / allocation bombs.
R3  capability rules: self-tests, recall of rule examples through the REAL pipeline (synthetic PEs), and the
    high-severity false-positive rate over the real (benign) corpus.
R4  packing honesty: genuine UPX output (+ name-scrubbed variants) and built-in synthetic fixtures vs. the clean corpus.
R6  evidence-bound explanation: adversarial mock-LLM outputs vs the claim validator (also feeds R4's
    "a packed file never gets an unqualified explanation" check).
R8  voice/UX: the phrase matrix (positives, refusals, near-misses) and non-regression of the existing suites,
    run through `cargo test` in src-tauri (needs the crate to build: on Linux that means the CI/scaffold setup).
R7  speed + footprint over the corpus: p95 parse time for files <= 50 MB and peak RSS growth.
"""

import argparse
import json
import statistics
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORE = ROOT / "src-tauri" / "reverse-core"
DEFAULT_BIN = CORE / "target" / "release" / "nexus-reverse"
VERSION_KEYS = {
    "company_name": "CompanyName", "file_description": "FileDescription", "file_version": "FileVersion",
    "internal_name": "InternalName", "legal_copyright": "LegalCopyright", "original_filename": "OriginalFilename",
    "product_name": "ProductName", "product_version": "ProductVersion",
}


def b2s(v):
    if v is None:
        return None
    return v.decode("utf-8", "replace") if isinstance(v, bytes) else str(v)


# ─── oracles ─────────────────────────────────────────────────────────────

def pefile_facts(path):
    """Normalised facts from pefile, or None when pefile itself cannot parse the file."""
    import pefile
    try:
        pe = pefile.PE(str(path), fast_load=False)
    except Exception:  # noqa: BLE001
        return None
    dd = pe.OPTIONAL_HEADER.DATA_DIRECTORY

    def present(i):
        return len(dd) > i and dd[i].VirtualAddress != 0 and dd[i].Size != 0

    f = {
        "machine": pe.FILE_HEADER.Machine, "timestamp": pe.FILE_HEADER.TimeDateStamp,
        "subsystem": pe.OPTIONAL_HEADER.Subsystem, "entry_point": pe.OPTIONAL_HEADER.AddressOfEntryPoint,
        "image_base": pe.OPTIONAL_HEADER.ImageBase, "is_dll": bool(pe.is_dll()),
        "is_64": pe.OPTIONAL_HEADER.Magic == 0x20B, "dll_characteristics": pe.OPTIONAL_HEADER.DllCharacteristics,
        "n_sections": len(pe.sections),
        "has_clr": present(14), "has_tls": present(9), "has_debug": present(6),
        "has_signature": len(dd) > 4 and dd[4].Size != 0,
    }
    f["sections"] = [
        {"name": b2s(s.Name).rstrip("\x00"), "virtual_size": s.Misc_VirtualSize, "virtual_address": s.VirtualAddress,
         "raw_size": s.SizeOfRawData, "raw_offset": s.PointerToRawData, "characteristics": s.Characteristics}
        for s in pe.sections
    ]
    imps = set()
    for e in getattr(pe, "DIRECTORY_ENTRY_IMPORT", []) or []:
        dll = b2s(e.dll).lower()
        for i in e.imports:
            imps.add((dll, b2s(i.name) if i.name else None, None if i.name else i.ordinal))
    f["imports"] = imps
    exps = set()
    exp = getattr(pe, "DIRECTORY_ENTRY_EXPORT", None)
    if exp is not None:
        for s in exp.symbols:
            exps.add(b2s(s.name) if s.name else f"#{s.ordinal}")
    f["exports"] = exps
    ver = {}
    for fi_list in getattr(pe, "FileInfo", []) or []:
        for fi in fi_list:
            if getattr(fi, "Key", b"") == b"StringFileInfo":
                for st in fi.StringTable:
                    for k, v in st.entries.items():
                        ver.setdefault(b2s(k), b2s(v).strip("\x00 ").strip())
                    break
    f["version"] = {k: ver.get(v) or None for k, v in VERSION_KEYS.items()}
    return f


def lief_facts(path):
    """Subset of facts from LIEF (sections / imports / exports / presence flags); {} when unavailable."""
    try:
        import lief
        b = lief.PE.parse(str(path))
    except Exception:  # noqa: BLE001
        return {}
    if b is None:
        return {}
    out = {}
    try:
        out["sections"] = [
            {"name": s.name.rstrip("\x00"), "virtual_size": s.virtual_size, "virtual_address": s.virtual_address,
             "raw_size": s.sizeof_raw_data, "raw_offset": s.pointerto_raw_data, "characteristics": int(s.characteristics)}
            for s in b.sections
        ]
    except Exception:  # noqa: BLE001
        pass
    try:
        imps = set()
        for e in b.imports:
            dll = (e.name or "").lower()
            for i in e.entries:
                if i.is_ordinal:
                    imps.add((dll, None, i.ordinal))
                else:
                    imps.add((dll, i.name, None))
        out["imports"] = imps
    except Exception:  # noqa: BLE001
        pass
    try:
        exp = b.get_export()
        if exp is not None:
            out["exports"] = {e.name if e.name else f"#{e.ordinal}" for e in exp.entries}
    except Exception:  # noqa: BLE001
        pass
    try:
        out["has_signature"] = bool(b.has_signatures)
        out["has_tls"] = bool(b.has_tls)
    except Exception:  # noqa: BLE001
        pass
    return out


# ─── comparison ──────────────────────────────────────────────────────────

SCALARS = ["machine", "timestamp", "subsystem", "entry_point", "image_base", "is_dll", "is_64",
           "dll_characteristics", "n_sections", "has_clr", "has_tls", "has_debug", "has_signature"]
SECTION_FIELDS = ["name", "virtual_size", "virtual_address", "raw_size", "raw_offset", "characteristics"]


def rust_view(rf):
    """Rust PeFacts JSON -> the oracle's normalised schema."""
    imps = set()
    for i in rf["imports"]:
        imps.add((i["dll"].lower(), i["name"], i["ordinal"] if i["name"] is None else None))
    exps = {e["name"] if e["name"] else f"#{e['rva']}" for e in rf["exports"]}
    return {
        "machine": rf["machine"], "timestamp": rf["timestamp"], "subsystem": rf["subsystem"], "entry_point": rf["entry_point"],
        "image_base": rf["image_base"], "is_dll": rf["is_dll"], "is_64": rf["is_64"],
        "dll_characteristics": rf["dll_characteristics"], "n_sections": len(rf["sections"]),
        "has_clr": rf["has_clr"], "has_tls": rf["has_tls"], "has_debug": rf["has_debug"], "has_signature": rf["has_signature"],
        "sections": rf["sections"], "imports": imps, "exports": exps, "version": rf["version"],
    }


def _consensus(a, b):
    """True when the second oracle is absent or agrees (so the field is adjudicable)."""
    return b is None or a == b


NOISE = {"fields": 0, "rust_with_pefile": 0, "rust_with_lief": 0, "categories": {}}


def _note_noise(name, rust_ok_pe, rust_ok_lief):
    NOISE["fields"] += 1
    NOISE["rust_with_pefile"] += bool(rust_ok_pe)
    NOISE["rust_with_lief"] += bool(rust_ok_lief)
    cat = name.split(":")[0].split("[")[0]
    NOISE["categories"][cat] = NOISE["categories"].get(cat, 0) + 1


def compare_file(pe_o, lief_o, rust):
    """-> (fields_scored, fields_agree, oracle_disagreements, details). `rust` None = parse failure."""
    scored = agree = noise = 0
    detail = []

    def score(name, oracle_val, rust_val, other=None):
        nonlocal scored, agree, noise
        if not _consensus(oracle_val, other):
            noise += 1
            _note_noise(name, rust is not None and rust_val == oracle_val, rust is not None and rust_val == other)
            return
        scored += 1
        if rust is not None and oracle_val == rust_val:
            agree += 1
        else:
            detail.append(name)

    r = rust or {}
    for k in SCALARS:
        score(k, pe_o[k], r.get(k), lief_o.get(k))
    for i, s in enumerate(pe_o["sections"]):
        other = lief_o["sections"][i] if i < len(lief_o.get("sections", [])) else None
        rs = r.get("sections", [])
        rsec = rs[i] if i < len(rs) else {}
        for k in SECTION_FIELDS:
            score(f"section[{i}].{k}", s[k], rsec.get(k), None if other is None else other[k])
    # set-valued fields: each element is one field; elements in only one set disagree
    for key in ("imports", "exports"):
        po = pe_o[key]
        ro = r.get(key, set()) if rust is not None else set()
        lo = lief_o.get(key)
        universe = po | ro | (lo or set())
        for el in universe:
            in_p, in_r = el in po, el in ro
            if lo is not None and (el in lo) != in_p:
                noise += 1
                _note_noise(f"{key}:{el}", in_r == in_p, in_r == (el in lo))
                continue
            scored += 1
            if in_p == in_r:
                agree += 1
            else:
                detail.append(f"{key}:{el}")
    for k, v in pe_o["version"].items():
        if v is None and (r.get("version", {}) or {}).get(k) is None:
            continue
        score(f"version.{k}", v, (r.get("version") or {}).get(k))
    return scored, agree, noise, detail


# ─── runners ─────────────────────────────────────────────────────────────

def run_pe(corpus, binary):
    pe_dir = Path(corpus) / "pe"
    proc = subprocess.run([str(binary), "batch", str(pe_dir)], capture_output=True, text=True)
    rows = [json.loads(l) for l in proc.stdout.splitlines() if l.strip()]
    # R7 is measured on the WHOLE pipeline (strings, evidence, ...) for files <= 50 MB
    full = subprocess.run([str(binary), "batch", str(pe_dir), "--full", "--max-mb", "50"], capture_output=True, text=True)
    frows = [json.loads(l) for l in full.stdout.splitlines() if l.strip()]
    rss_kb = None
    for line in full.stderr.splitlines():
        if line.startswith("peak_rss_delta_kb="):
            rss_kb = int(line.split("=")[1])
    files = fields = agree = noise = 0
    problems = {}
    rust_fail_oracle_ok = 0
    times = []
    times = [r["ms"] / 1000.0 for r in frows if r["ok"]]
    for row in rows:
        p = Path(row["path"])
        pe_o = pefile_facts(p)
        if pe_o is None:
            continue  # the oracle cannot adjudicate a file it cannot parse
        rust = rust_view(row["facts"]) if row.get("ok") else None
        if rust is None:
            rust_fail_oracle_ok += 1
        s, a, n, d = compare_file(pe_o, lief_facts(p), rust)
        files += 1
        fields += s
        agree += a
        noise += n
        if d:
            problems[p.name] = d
    t95 = sorted(times)[int(0.95 * (len(times) - 1))] if times else None
    cats = {}
    for names in problems.values():
        for n in names:
            cats[n.split(":")[0].split("[")[0]] = cats.get(n.split(":")[0].split("[")[0], 0) + 1
    return {
        "r1": {"files": files, "fields": fields, "agree": agree, "oracle_disagreements": noise, "problem_categories": cats, "oracle_noise": dict(NOISE),
               "rust_rejected_but_pefile_parsed": rust_fail_oracle_ok, "worst": {k: v[:6] for k, v in list(problems.items())[:15]}},
        "r7": {"files": len(times), "p95_s": t95, "rss_mb": None if rss_kb is None else rss_kb / 1024.0,
               "median_s": statistics.median(times) if times else None},
    }


def run_fuzz(corpus, binary, mutants, seed):
    proc = subprocess.run([str(binary), "fuzz", str(Path(corpus) / "pe"), "--mutants", str(mutants), "--seed", str(seed)],
                          capture_output=True, text=True)
    last = [l for l in proc.stdout.splitlines() if l.startswith("{")][-1]
    d = json.loads(last)
    return {"r2": {"mutants": d["mutants"], "panics": d["panics"], "hangs": d["hangs"], "oversize": d["oversize"],
                   "sources": d.get("sources"), "worst_ms": d.get("worst_ms"), "offenders": d.get("offenders")}}


def _json_line(proc):
    lines = [l for l in proc.stdout.splitlines() if l.startswith("{")]
    if not lines:
        raise RuntimeError(f"no JSON from {proc.args}: {proc.stderr[:300]}")
    return json.loads(lines[-1])


def run_rules(corpus, binary):
    st = _json_line(subprocess.run([str(binary), "rules", "--selftest"], capture_output=True, text=True))
    rc = _json_line(subprocess.run([str(binary), "recall"], capture_output=True, text=True))
    bn = _json_line(subprocess.run([str(binary), "benign", str(Path(corpus) / "pe")], capture_output=True, text=True))
    return {"r3": {
        "rules": st.get("rules", 0), "selftest_fail": len(st.get("failures", [])),
        "recall_k": rc["recall_k"], "recall_n": rc["recall_n"], "negative_ok": rc["negative_ok"], "negative_n": rc["negative_n"],
        "skipped_inexpressible": rc["skipped_inexpressible"], "recall_misses": rc["misses"],
        "benign_fp_k": bn["benign_fp_k"], "benign_n": bn["benign_n"], "benign_hits": bn["high_examples"], "per_rule_fp": bn["per_rule"],
    }, "_clean": {"clean_n": bn["clean_n"], "clean_flag_k": bn["clean_flag_k"], "flag_examples": bn["flag_examples"]}}


def run_packed(corpus, binary, clean):
    pk = _json_line(subprocess.run([str(binary), "packed", str(Path(corpus) / "packed")], capture_output=True, text=True))
    cat = pk["by_category"]
    synth = cat.get("synth", [0, 0])
    real_n = pk["packed_n"] - synth[0]
    real_k = pk["packed_k"] - synth[1]
    return {"r4": {
        "packed_n": real_n, "packed_k": real_k, "by_category": cat, "missed": pk["missed"],
        "synthetic_n": synth[0], "synthetic_k": synth[1],
        "clean_n": clean["clean_n"], "clean_flag_k": clean["clean_flag_k"], "clean_flag_examples": clean["flag_examples"],
        # filled in by the explanation-gate measurement (Phase 4); None = not measured, never "0 problems"
        "unqualified": None,
    }}


def run_explain(corpus, binary):
    cmd = [str(binary), "explain-eval", str(Path(corpus) / "pe")]
    if (Path(corpus) / "packed").exists():
        cmd += ["--packed", str(Path(corpus) / "packed")]
    d = _json_line(subprocess.run(cmd, capture_output=True, text=True))
    r6 = {k: d[k] for k in ("outputs", "files", "unsupported_n", "dropped_k", "supported_n", "supported_kept", "uncited_kept", "families")}
    r6["unsupported_kept"] = d["unsupported_kept"]
    r6["wrongly_dropped_examples"] = d["wrongly_dropped"][:8]
    return {"r6": r6, "_unqualified": {"unqualified": d["unqualified"], "packed_checked": d["packed_checked"]}}


def _cargo_test(args, env):
    p = subprocess.run(["cargo", "test", "--lib"] + args + ["--", "--test-threads=1", "--nocapture"], cwd=str(ROOT / "src-tauri"),
                       capture_output=True, text=True, env=env)
    out = p.stdout + p.stderr
    passed = failed = 0
    for line in out.splitlines():
        if line.startswith("test result:"):
            parts = line.split()
            passed += int(parts[3])
            failed += int(parts[5])
    return out, passed, failed


def run_voice():
    """R8 + gate 5: phrase matrix and existing-suite regressions, measured by cargo test."""
    import os
    import re
    env = dict(os.environ)
    env.setdefault("ORT_PREFER_DYNAMIC_LINK", "1")
    out, _, mf = _cargo_test(["reverse_"], env)
    m = re.search(r"REVERSE_MATRIX (\d+) (\d+)", out)
    r = re.search(r"REVERSE_REFUSAL (\d+) (\d+)", out)
    sp = re.search(r"REVERSE_SECOND_PASS pos (\d+)/(\d+) neg_ok (\d+)/(\d+)", out)
    if not m:
        return {"r8": {"error": "matrix did not run", "tail": out[-500:]}}
    suites = {}
    regress = 0
    for name in ("intent_parser", "center", "orchestrator"):
        _, p, f = _cargo_test([name], env)
        suites[name] = [p, f]
        regress += f
    e2e = subprocess.run(["cargo", "test", "--test", "e2e_voice_fixture"], cwd=str(ROOT / "src-tauri"), capture_output=True, text=True, env=env)
    e2e_ok = e2e.returncode == 0
    if not e2e_ok:
        regress += 1
    r8 = {"matrix_k": int(m.group(1)), "matrix_n": int(m.group(2)), "regressions": regress, "reverse_test_failures": mf, "suites": suites,
          "e2e_fixture_ok": e2e_ok,
          "refusal_k": int(r.group(1)) if r else 0, "refusal_n": int(r.group(2)) if r else 0}
    # Historical, measured during development BEFORE the parser was widened (recorded so the matrix score is not
    # read as generalisation): fresh natural phrasings the parser had never seen.
    r8["first_pass_unseen"] = {"set1_positives": [10, 18], "set2_positives": [11, 14], "set2_false_captures": 1,
                               "note": "caught/total on phrasings written after the first matrix passed; parser widened afterwards"}
    if sp:
        r8["second_pass"] = {"positives": [int(sp.group(1)), int(sp.group(2))], "near_misses_ok": [int(sp.group(3)), int(sp.group(4))]}
    return {"r8": r8}


def merge(out_path, new):
    p = Path(out_path)
    cur = json.loads(p.read_text()) if p.exists() else {}
    cur.update(new)
    p.write_text(json.dumps(cur, indent=2, default=list))
    return cur


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=["pe", "fuzz", "rules", "packed", "explain", "voice", "all"])
    ap.add_argument("--corpus", default=str(ROOT / "reverse_corpus"))
    ap.add_argument("--bin", default=str(DEFAULT_BIN))
    ap.add_argument("--out", default=str(ROOT / "reverse_results.json"))
    ap.add_argument("--mutants", type=int, default=2500)
    ap.add_argument("--seed", type=int, default=1337)
    args = ap.parse_args(argv)
    if not Path(args.bin).exists():
        print(f"build first: cargo build --release --manifest-path {CORE}/Cargo.toml")
        return 2
    res = {}
    if args.cmd in ("pe", "all"):
        res.update(run_pe(args.corpus, args.bin))
    if args.cmd in ("fuzz", "all"):
        res.update(run_fuzz(args.corpus, args.bin, args.mutants, args.seed))
    if args.cmd in ("rules", "packed", "all"):
        r = run_rules(args.corpus, args.bin)
        clean = r.pop("_clean")
        res.update(r)
        if (Path(args.corpus) / "packed").exists():
            prev = {}
            if Path(args.out).exists():
                prev = json.loads(Path(args.out).read_text()).get("r4", {})
            r4 = run_packed(args.corpus, args.bin, clean)
            if prev.get("unqualified") is not None:
                r4["r4"]["unqualified"] = prev["unqualified"]
            res.update(r4)
    if args.cmd in ("explain", "all"):
        ex = run_explain(args.corpus, args.bin)
        uq = ex.pop("_unqualified")
        res.update(ex)
        prev = json.loads(Path(args.out).read_text()) if Path(args.out).exists() else {}
        r4 = res.get("r4") or prev.get("r4")
        if r4 is not None:
            r4 = dict(r4)
            r4["unqualified"] = uq["unqualified"]
            r4["unqualified_checked"] = uq["packed_checked"]
            res["r4"] = r4
    if args.cmd in ("voice", "all"):
        res.update(run_voice())
    merge(args.out, res)
    print(json.dumps(res, indent=2, default=list))
    return 0


if __name__ == "__main__":
    sys.exit(main())
