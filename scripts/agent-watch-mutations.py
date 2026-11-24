"""Offline safety mutations in a temporary mirror; never edits application sources."""
from pathlib import Path
import json
import hashlib
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SOURCES = ['agent_lifecycle.rs', 'power_thread.rs', 'pii_filter.rs',
           'session_verdict.rs', 'agent_tracker.rs', 'session_observer.rs',
           'watch_runner.rs', 'agent_lifecycle_runtime.rs', 'agent_hooks.rs']
MUTATIONS = [
    ('busy-stop veto', 'if i.fully_idle == Some(false) {', 'if false {'),
    ('settle interval', 'pub const SETTLE_MS: u64 = 15_000;', 'pub const SETTLE_MS: u64 = 0;'),
    ('durable claim', 't.claimed = true;', 't.claimed = false;'),
    ('sticky failure', 't.failed |= i.ok == Some(false);', 't.failed = i.ok == Some(false);'),
    ('cancellation', 't.phase = Phase::Cancelled; t.observed_ms = at; t.stop_ms=None;', 't.observed_ms = at;'),
    ('authoritative proof', 'else if i.authoritative &&', 'else if true &&'),
    ('restart drops stale holds', 't.phase = Phase::Unknown; t.stop_ms = None;', 't.stop_ms = None;'),
]

def run(cwd):
    return subprocess.run(['cargo', 'test', '--manifest-path',
        'scripts/agent-watch-harness/Cargo.toml', '--locked', '--offline', '-q',
        '--target-dir', str(ROOT / 'scripts/agent-watch-harness/target/mutations')],
        cwd=cwd, capture_output=True, text=True, encoding='utf-8', errors='replace')

def main():
    results = []
    with tempfile.TemporaryDirectory(prefix='nexus-agent-mutations-') as temp:
        root = Path(temp)
        dest = root / 'src-tauri/src'
        dest.mkdir(parents=True)
        for name in SOURCES:
            shutil.copy2(ROOT / 'src-tauri/src' / name, dest / name)
        harness = root / 'scripts/agent-watch-harness'
        harness.mkdir(parents=True)
        for source in (ROOT / 'scripts/agent-watch-harness').iterdir():
            if source.is_file():
                shutil.copy2(source, harness / source.name)
        baseline = run(root)
        if baseline.returncode:
            raise RuntimeError('mutation baseline failed:\n' + baseline.stdout + baseline.stderr)
        path = dest / 'agent_lifecycle.rs'
        original = path.read_text(encoding='utf-8')
        for name, before, after in MUTATIONS:
            if original.count(before) != 1:
                raise RuntimeError('mutation anchor moved: ' + name)
            path.write_text(original.replace(before, after), encoding='utf-8')
            result = run(root)
            output = result.stdout + result.stderr
            killed = result.returncode != 0 and 'test result: FAILED.' in output and 'error[E' not in output
            results.append({'rule': name, 'killed_by_assertion': killed})
            print(json.dumps(results[-1]), flush=True)
            path.write_text(original, encoding='utf-8')
            if not killed:
                raise RuntimeError('mutation survived or failed to compile:\n' + output)
    print(json.dumps({'passed': len(results), 'total': len(MUTATIONS)}))
    report = {'passed': len(results), 'total': len(MUTATIONS), 'results': results,
              'source_sha256': {name: hashlib.sha256((ROOT / 'src-tauri/src' / name).read_bytes()).hexdigest() for name in SOURCES}}
    target = ROOT / 'docs/testing/runs/agent-watch-mutations-20261105.json'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

if __name__ == '__main__':
    main()
