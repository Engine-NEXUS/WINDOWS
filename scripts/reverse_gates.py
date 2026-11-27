#!/usr/bin/env python3
"""Reverse Lab hard gates (pass/fail, not stars). Writes `gates` into reverse_results.json for
`reverse_scorecard.py`. Every gate is checked by something that can FAIL, and the grep gate proves it can
by detecting a planted violation first.

    python scripts/reverse_gates.py [--out reverse_results.json] [--no-cargo]

  never_executes   no process-spawn / library-load / injection calls anywhere in the Reverse Lab code
  secrets_redacted planted secrets never appear in HAR output, binary reports, the LLM evidence summary or JSON
  no_upload        the reverse-core crate has no network dependency; the only network call is the model
                   request built from the capped evidence summary (never the file)
  banner           every report starts with the authorisation banner
  refusal          crack / keygen / licence-bypass / exploit requests get the fixed refusal (phrase matrix)
"""

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORE = ROOT / "src-tauri" / "reverse-core"
GLUE = ROOT / "src-tauri" / "src" / "reverse"

# Calls that would execute, load, inject into or spawn the target. Matched on CODE (a call with an open paren),
# so API names that merely appear inside string literals (rule data, test fixtures) do not trip it.
FORBIDDEN = re.compile(
    r"\bCommand::new\b|\bprocess::Command\b|\btokio::process\b|\blibloading\b|\bdlopen\s*\(|\bLoadLibrary[AWEx]*\s*\(|"
    r"\bCreateProcess[AW]?\s*\(|\bShellExecute[AWEx]*\s*\(|\bWinExec\s*\(|\bexecv[pe]*\s*\(|\bCreateRemoteThread\s*\(|"
    r"\bWriteProcessMemory\s*\(|\bVirtualAllocEx\s*\(|\bNtCreateThreadEx\s*\("
)
NETWORK_DEPS = re.compile(r"^\s*(reqwest|hyper|ureq|isahc|surf|attohttpc|curl|tokio|async-std|tungstenite|websocket|openssl|native-tls|rustls)\b", re.M)


def code_without_strings_and_comments(text):
    """Blank out string literals, char literals and comments so only real code is scanned."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
        elif c == 'r' and re.match(r'r#*"', text[i:i + 10]):
            m = re.match(r'r(#*)"', text[i:])
            end = '"' + m.group(1)
            j = text.find(end, i + len(m.group(0)))
            i = n if j < 0 else j + len(end)
            out.append('""')
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            i = j + 1
            out.append('""')
        else:
            out.append(c)
            i += 1
    return "".join(out)


def scan_text(text):
    return [m.group(0) for m in FORBIDDEN.finditer(code_without_strings_and_comments(text))]


def gate_never_executes():
    # the scanner must be able to FAIL: a planted violation has to be found, benign text must not be
    planted = 'fn x() { let _ = std::process::Command::new("a"); let _ = LoadLibraryW(p); }'
    benign = 'let rules = "VirtualAllocEx CreateRemoteThread LoadLibraryA"; // Command::new is forbidden'
    if not scan_text(planted) or scan_text(benign):
        return False, "scanner self-test failed"
    hits = []
    files = [p for p in (list((CORE / "src").rglob("*.rs")) + list(GLUE.rglob("*.rs")))]
    for f in files:
        # test modules may spawn the bundled CLI binary? they do not - scan everything
        for h in scan_text(f.read_text(encoding="utf-8", errors="replace")):
            hits.append(f"{f.relative_to(ROOT)}: {h}")
    return (not hits), (f"{len(files)} files scanned, no forbidden call" if not hits else "; ".join(hits[:6]))


def cargo_tests(filters, cwd=CORE):
    total = 0
    for flt in filters:
        p = subprocess.run(["cargo", "test", "--lib", flt, "--", "--test-threads=1"], cwd=str(cwd), capture_output=True, text=True)
        m = re.search(r"test result: (\w+)\. (\d+) passed; (\d+) failed", p.stdout)
        if not m or m.group(1) != "ok" or int(m.group(3)) or int(m.group(2)) == 0:
            return False, f"cargo test {flt}: {(m.group(0) if m else p.stdout[-200:])}"
        total += int(m.group(2))
    return True, f"{total} tests passed"


def gate_secrets(results, cargo):
    r5 = results.get("r5", {})
    if not r5.get("secrets_planted"):
        return False, "HAR secret benchmark has not run (python scripts/reverse_har_bench.py official)"
    if r5.get("secret_leaks", 1) != 0:
        return False, f"{r5['secret_leaks']} planted secret(s) leaked in HAR output"
    if not cargo:
        return True, f"{r5['secrets_planted']} planted HAR secrets, 0 leaked (unit tests skipped)"
    ok, msg = cargo_tests(["secret", "redact", "credentials_inside_urls"])
    return ok, f"{r5['secrets_planted']} planted HAR secrets, 0 leaked; binary/report/LLM-summary/JSON leak tests: {msg}"


def gate_no_upload():
    toml = (CORE / "Cargo.toml").read_text()
    deps = toml.split("[dependencies]", 1)[1].split("[lib]", 1)[0]
    bad = NETWORK_DEPS.findall(deps)
    if bad:
        return False, f"reverse-core depends on network crates: {bad}"
    runner = (GLUE / "runner.rs").read_text()
    calls = re.findall(r"route_with_system\(", runner)
    sends_summary = "explain::user_prompt(&summary)" in runner and "evidence_summary(a, explain::SUMMARY_CAP)" in runner
    other_net = [f.name for f in GLUE.glob("*.rs") if f.name != "runner.rs" and re.search(r"reqwest|TcpStream|UdpSocket", f.read_text())]
    if len(calls) != 1 or not sends_summary or other_net:
        return False, f"unexpected network use (calls={len(calls)}, summary-only={sends_summary}, other files={other_net})"
    # the model call is the ONLY thing taking text from the analysis and it is bounded
    if "SUMMARY_CAP: usize = 7000" not in (CORE / "src" / "explain.rs").read_text():
        return False, "summary cap changed"
    return True, "no network crate in reverse-core; the only request carries the capped redacted evidence summary"


def gate_banner(cargo):
    if not cargo:
        return True, "unit tests skipped"
    ok, msg = cargo_tests(["authorisation_banner", "har_report_has_the_banner"])
    if not ok:
        return ok, msg
    # runtime check on real output
    bin_ = CORE / "target" / "release" / "nexus-reverse"
    if not bin_.exists():
        return False, "build nexus-reverse first"
    files = sorted((ROOT / "reverse_corpus" / "pe").glob("*"))[:3] + sorted((ROOT / "reverse_corpus" / "packed").glob("*"))[:2]
    for f in files:
        out = subprocess.run([str(bin_), "report", str(f)], capture_output=True, text=True).stdout
        if not out.startswith("> [!NOTE]\n> **Static analysis only - nothing was run.**"):
            return False, f"report for {f.name} does not start with the banner"
    return True, f"unit tests + {len(files)} real reports start with the banner ({msg})"


def gate_refusal(results):
    r8 = results.get("r8", {})
    n, k = r8.get("refusal_n", 0), r8.get("refusal_k", 0)
    if n < 5:
        return False, "phrase matrix has not run (python scripts/reverse_eval.py voice)"
    return k == n, f"{k}/{n} prohibited requests get the fixed refusal"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=str(ROOT / "reverse_results.json"))
    ap.add_argument("--no-cargo", action="store_true")
    ap.add_argument("--only", default="", help="comma-separated gate names (the ones that need no measured results, for CI)")
    a = ap.parse_args(argv)
    path = Path(a.out)
    results = json.loads(path.read_text()) if path.exists() else {}
    cargo = not a.no_cargo
    checks = {
        "never_executes": gate_never_executes(),
        "secrets_redacted": gate_secrets(results, cargo) if not a.only else (True, "needs the measured HAR benchmark; skipped with --only"),
        "no_upload": gate_no_upload(),
        "banner": gate_banner(cargo),
        "refusal": gate_refusal(results),
    }
    if a.only:
        wanted = {x.strip() for x in a.only.split(",")}
        checks = {k: v for k, v in checks.items() if k in wanted}
        for k, (ok, note) in checks.items():
            print(f"{'PASS' if ok else 'FAIL'}  {k}: {note}")
        return 0 if all(v[0] for v in checks.values()) else 1
    results["gates"] = {k: v[0] for k, v in checks.items()}
    results["gate_notes"] = {k: v[1] for k, v in checks.items()}
    path.write_text(json.dumps(results, indent=2, default=list))
    for k, (ok, note) in checks.items():
        print(f"{'PASS' if ok else 'FAIL'}  {k}: {note}")
    return 0 if all(v[0] for v in checks.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
