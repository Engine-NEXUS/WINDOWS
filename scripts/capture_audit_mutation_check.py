"""Mutation-check an isolated copy of the real native capture audit module."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError("Mutation anchor is not unique")
    return source.replace(old, new, 1)


def extract_item(source, signature):
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 1
    closing = opening + 1
    while depth:
        depth += (source[closing] == "{") - (source[closing] == "}")
        closing += 1
    return source[start:closing]


def main():
    original = (ROOT / "src-tauri/src/capture_audit.rs").read_text(encoding="utf-8")
    capture = (ROOT / "src-tauri/src/wakeword_oww.rs").read_text(encoding="utf-8")
    original += "\n" + extract_item(capture, "fn advance_capture_session(")
    original += "\n" + extract_item(capture, "fn is_stale_session(")
    original += "\n#[cfg(test)] mod capture_generation_tests {\n"
    for test in ["successive_captures_invalidate_older_work_without_an_abort", "capture_generation_is_unique_under_concurrent_starts_and_aborts"]:
        original += "#[test]\n" + extract_item(capture, "fn " + test + "(") + "\n"
    original += "}\n"
    mutants = {
        "attempt IDs reused": replace_once(original, "self.next.fetch_add(1, Ordering::Relaxed)", "self.next.load(Ordering::Relaxed)"),
        "terminal deduplication removed": replace_once(original, "self.done = true;", "self.done = false;"),
        "abandoned packets disappear": replace_once(original, "fn drop(&mut self) { self.finish(Outcome::Abandoned); }", "fn drop(&mut self) {}"),
        "wrong executable trusted": replace_once(original, 'manifest.get("executable")?.as_str()? != executable_hash', 'false'),
        "new capture reuses previous session": replace_once(original, "counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1", "counter.load(std::sync::atomic::Ordering::SeqCst)"),
        "stale capture accepted": replace_once(original, "taken != current", "false"),
    }
    with tempfile.TemporaryDirectory(prefix="nexus-capture-audit-check-") as directory:
        harness = Path(directory)
        (harness / "src").mkdir()
        (harness / "Cargo.toml").write_text('''[package]
name = "nexus-capture-audit-check"
version = "0.0.0"
edition = "2021"
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"
sha2 = "0.10"
tracing = "0.1"
once_cell = "1"
''', encoding="utf-8")
        shutil.copyfile(ROOT / "src-tauri/Cargo.lock", harness / "Cargo.lock")
        command = ["cargo", "test", "--offline", "--lib", "--manifest-path", str(harness / "Cargo.toml"),
                   "--target-dir", str(ROOT / "src-tauri/target/capture-audit-check"), "-j", "1"]
        for label, code in [("baseline", original), *mutants.items()]:
            (harness / "src/lib.rs").write_text(code, encoding="utf-8")
            result = subprocess.run(command, cwd=ROOT, env=dict(os.environ, CARGO_BUILD_JOBS="1"),
                                    capture_output=True, encoding="utf-8", errors="replace")
            output = result.stdout + result.stderr
            if label == "baseline":
                if result.returncode:
                    raise RuntimeError("Baseline failed:\n" + output[-7000:])
                print("PASS: real capture_audit module baseline", flush=True)
            elif result.returncode == 0:
                raise RuntimeError("SURVIVED: " + label)
            elif "test result: FAILED" not in output:
                raise RuntimeError("Mutation did not reach assertions: " + label + "\n" + output[-7000:])
            else:
                print("KILLED: " + label, flush=True)
    print(f"PASS: {len(mutants)}/{len(mutants)} audit mutations killed", flush=True)


if __name__ == "__main__":
    main()
