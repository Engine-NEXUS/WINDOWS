#!/usr/bin/env python3
"""Reverse Lab scorecard: 8 measured rows -> stars -> weighted average (pass = avg >= 4.5, no row < 4,
every hard gate green). Same philosophy as voice_scorecard.py: a row with too little data shows
"not measured" and 0 stars - never a guess - so the feature cannot pass until it has been measured.

    python scripts/reverse_scorecard.py --results reverse_results.json [--baseline before.json]
                                        [--json out.json] [--out report.md]

`reverse_results.json` (produced by scripts/reverse_eval.py, every key optional):
  r1 {files, fields, agree}                  PE parse fidelity vs pefile + LIEF
  r2 {mutants, panics, hangs, oversize}      malformed-input robustness
  r3 {rules, selftest_fail, recall_k, recall_n, benign_fp_k, benign_n}
  r4 {packed_k, packed_n, clean_flag_k, clean_n, unqualified}
  r5 {endpoints, f1, template_acc, field_recall}
  r6 {unsupported_n, dropped_k, uncited_kept}
  r7 {files, p95_s, rss_mb}
  r8 {matrix_k, matrix_n, regressions}
  gates {never_executes, secrets_redacted, no_upload, banner, refusal}   (booleans)
Standard library only.
"""

import argparse
import json
import math
import sys
from pathlib import Path

ROWS = [  # key, label, weight
    ("r1", "PE parse fidelity vs pefile + LIEF", 20),
    ("r2", "Malformed-input robustness", 15),
    ("r3", "Capability detection", 15),
    ("r4", "Packing / obfuscation honesty", 10),
    ("r5", "HAR -> OpenAPI accuracy", 15),
    ("r6", "Evidence-bound explanation", 10),
    ("r7", "Speed and footprint", 5),
    ("r8", "Voice / UX and non-regression", 10),
]
GATES = [
    ("never_executes", "target is never executed or loaded"),
    ("secrets_redacted", "HAR secrets never reach output, logs or LLM prompts"),
    ("no_upload", "binary / HAR never uploaded (capped redacted summary only)"),
    ("banner", "authorization banner on every report"),
    ("refusal", "refusal line for crack / keygen / licence-bypass / exploit requests"),
]
PASS_AVG, PASS_MIN = 4.5, 4


def wilson(k, n, z=1.96):
    if n <= 0:
        return (float("nan"),) * 3
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return p, max(0.0, c - h), min(1.0, c + h)


def _need(cond):
    return cond  # readability helper: a row needs its minimum sample size or it is "not measured"


def score_r1(d):
    files, fields, agree = d.get("files", 0), d.get("fields", 0), d.get("agree", 0)
    if not _need(files >= 200 and fields >= 2000):
        return 0, "not measured (needs >= 200 real PE files and >= 2000 compared fields)"
    a, lo, _ = wilson(agree, fields)
    st = 5 if (a >= 0.999 and lo >= 0.99) else 4 if a >= 0.995 else 3 if a >= 0.98 else 2 if a >= 0.95 else 1
    note = f"{agree}/{fields} fields agree ({a * 100:.2f}%, 95% low {lo * 100:.2f}%) over {files} real PE files"
    noise = d.get("oracle_noise") or {}
    if noise.get("fields"):
        note += (f"; {noise['fields']} oracle-disputed fields excluded (pefile vs LIEF disagree: "
                 f"Reverse Lab matched LIEF on {noise.get('rust_with_lief', 0)}, pefile on {noise.get('rust_with_pefile', 0)})")
    note += "; clean third-party binaries only - not packed/adversarial samples"
    return st, note


def score_r2(d):
    m, pan, hang, over = d.get("mutants", 0), d.get("panics", 0), d.get("hangs", 0), d.get("oversize", 0)
    if m < 2000:
        return 0, "not measured (needs >= 2000 mutants)"
    f = pan + hang + over
    rate = f / m
    st = 5 if f == 0 else 4 if rate <= 0.001 else 3 if rate <= 0.01 else 2 if rate <= 0.05 else 1
    if pan:
        st = min(st, 3)
    return st, f"{f} failures in {m} mutants ({pan} panics, {hang} hangs, {over} over-cap)"


def score_r3(d):
    rn, bn = d.get("recall_n", 0), d.get("benign_n", 0)
    if rn < 50 or bn < 100 or d.get("rules", 0) < 1:
        return 0, "not measured (needs >= 50 recall cases, >= 100 benign files, >= 1 rule)"
    rec = d.get("recall_k", 0) / rn
    fp = d.get("benign_fp_k", 0) / bn
    st = 5 if (rec >= 0.95 and fp <= 0.01) else 4 if (rec >= 0.90 and fp <= 0.03) else 3 if rec >= 0.80 else 2 if rec >= 0.6 else 1
    note = f"recall {rec * 100:.1f}% ({d.get('recall_k', 0)}/{rn}), benign FP {fp * 100:.2f}% ({d.get('benign_fp_k', 0)}/{bn}), {d['rules']} rules; caveat: recall cases are the rule authors' own examples rebuilt as PEs, not real malware, and no independent oracle (capa) was run"
    if d.get("selftest_fail", 0):
        st = min(st, 3)
        note += f", {d['selftest_fail']} rule self-test(s) FAIL"
    return st, note


def score_r4(d):
    pn, cn = d.get("packed_n", 0), d.get("clean_n", 0)
    if pn < 20 or cn < 100:
        return 0, "not measured (needs >= 20 packed fixtures, >= 100 clean files)"
    fl, ff = d.get("packed_k", 0) / pn, d.get("clean_flag_k", 0) / cn
    st = 5 if (fl >= 0.95 and ff <= 0.02) else 4 if (fl >= 0.90 and ff <= 0.05) else 3 if fl >= 0.80 else 2 if fl >= 0.6 else 1
    note = f"flagged {fl * 100:.1f}% of packed, false flags {ff * 100:.1f}% of clean"
    uq = d.get("unqualified")
    if uq is None:
        st = min(st, 4)  # the "flagged files never get an unqualified explanation" half is not measured yet
        note += "; explanation gate not measured yet (capped at 4)"
    elif uq:
        st = min(st, 2)
        note += f"; {uq} packed file(s) got an UNQUALIFIED explanation"
    return st, note


def score_r5(d):
    n = d.get("endpoints", 0)
    if n < 40:
        return 0, "not measured (needs >= 40 ground-truth endpoints)"
    f1, ta, fr = d.get("f1", 0), d.get("template_acc", 0), d.get("field_recall", 0)
    worst = min(f1, ta)
    st = 5 if (worst >= 0.95 and fr >= 0.90) else 4 if (worst >= 0.90 and fr >= 0.80) else 3 if worst >= 0.80 else 2 if worst >= 0.60 else 1
    return st, f"endpoint+method F1 {f1:.3f}, path-template accuracy {ta:.3f}, schema-field recall {fr:.3f} over {n} ground-truth endpoints x {d.get('captures', 1)} capture(s), {d.get('requests', '?')} requests; mock APIs, not production traffic"


def score_r6(d):
    n = d.get("unsupported_n", 0)
    if n < 200:
        return 0, "not measured (needs >= 200 adversarial LLM outputs)"
    dropped, kept_bad = d.get("dropped_k", 0), d.get("uncited_kept", 0)
    if kept_bad or dropped < n:
        return 1, f"{n - dropped} unsupported claim(s) NOT dropped, {kept_bad} uncited claim(s) kept"
    return 5, f"{dropped}/{n} unsupported claims dropped, 0 uncited kept"


def score_r7(d):
    if d.get("files", 0) < 30:
        return 0, "not measured (needs >= 30 timed files)"
    t, mb = d.get("p95_s"), d.get("rss_mb")
    if t is None or mb is None:
        return 0, "not measured"
    st = 5 if (t <= 2 and mb <= 150) else 4 if (t <= 4 and mb <= 250) else 3 if (t <= 8 and mb <= 400) else 2
    return st, f"p95 {t:.2f} s, RSS delta {mb:.0f} MB over {d['files']} files"


def score_r8(d):
    n = d.get("matrix_n", 0)
    if n < 40:
        return 0, "not measured (needs >= 40 phrases incl. near-misses)"
    a = d.get("matrix_k", 0) / n
    st = 5 if a >= 0.95 else 4 if a >= 0.90 else 3 if a >= 0.80 else 2 if a >= 0.6 else 1
    note = f"phrase matrix {a * 100:.1f}% ({d.get('matrix_k', 0)}/{n})"
    if d.get("regressions", 0):
        st = min(st, 2)
        note += f"; {d['regressions']} existing-test REGRESSION(S)"
    return st, note


SCORERS = {"r1": score_r1, "r2": score_r2, "r3": score_r3, "r4": score_r4, "r5": score_r5, "r6": score_r6,
           "r7": score_r7, "r8": score_r8}


def build(results):
    rows = []
    for key, label, weight in ROWS:
        st, note = SCORERS[key](results.get(key, {}))
        rows.append({"key": key, "label": label, "weight": weight, "stars": st, "note": note})
    total_w = sum(r["weight"] for r in rows)
    avg = sum(r["stars"] * r["weight"] for r in rows) / total_w
    gates = results.get("gates", {})
    gate_rows = [{"key": k, "label": lbl, "ok": bool(gates.get(k)), "measured": k in gates} for k, lbl in GATES]
    measured = [r for r in rows if r["stars"] > 0]
    lowest = min((r["stars"] for r in rows), default=0)
    passed = avg >= PASS_AVG and lowest >= PASS_MIN and all(g["ok"] for g in gate_rows)
    return {"rows": rows, "gates": gate_rows, "average": round(avg, 3), "lowest": lowest,
            "unmeasured": len(rows) - len(measured), "passed": passed}


def bar(n):
    return "★" * n + "☆" * (5 - n)


def render(card, baseline=None, title="NEXUS Reverse Lab scorecard"):
    base = {r["key"]: r for r in (baseline or {}).get("rows", [])}
    verdict = "PASS" if card["passed"] else "NOT YET (needs avg >= 4.5, no row < 4, all gates green)"
    lines = [f"# {title}", "",
             f"**Weighted average: {card['average']:.2f} / 5** - {verdict}", ""]
    if baseline:
        lines += [f"Before: {baseline.get('average', 0):.2f} / 5", ""]
    lines += ["| Row | Weight | Stars | Evidence |", "|---|---|---|---|"]
    for r in card["rows"]:
        was = ""
        if r["key"] in base and base[r["key"]]["stars"] != r["stars"]:
            was = f" (was {base[r['key']]['stars']})"
        lines.append(f"| {r['label']} | {r['weight']} | {bar(r['stars'])}{was} | {r['note']} |")
    lines += ["", "| Hard gate | Status |", "|---|---|"]
    for g in card["gates"]:
        lines.append(f"| {g['label']} | {'OK' if g['ok'] else ('FAIL' if g['measured'] else 'not verified')} |")
    lines.append("")
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--results", help="reverse_results.json (omit = baseline: nothing measured)")
    ap.add_argument("--baseline")
    ap.add_argument("--json")
    ap.add_argument("--out")
    args = ap.parse_args(argv)
    results = json.loads(Path(args.results).read_text(encoding="utf-8")) if args.results else {}
    base = json.loads(Path(args.baseline).read_text(encoding="utf-8")) if args.baseline else None
    card = build(results)
    text = render(card, base)
    print(text)
    if args.out:
        Path(args.out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out).write_text(text, encoding="utf-8")
    if args.json:
        Path(args.json).write_text(json.dumps(card, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
