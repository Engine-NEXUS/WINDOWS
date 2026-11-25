"""Repeat offline feature verification; never builds or controls the desktop application."""
import argparse
import datetime
import hashlib
import json
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
COMMANDS = [
    ('native_policy_and_glue_shim', ['cargo', 'test', '--manifest-path', 'scripts/agent-watch-harness/Cargo.toml', '--locked', '--offline', '-q']),
    ('source_and_sidecar', ['node', '--test', 'scripts/agent-watch-contract.test.mjs', 'src-tauri/resources/agent_watch_sidecar/lifecycle.test.cjs', 'src-tauri/resources/agent_watch_sidecar/server.test.cjs']),
    ('frontend', ['node', 'frontend/node_modules/vitest/vitest.mjs', 'run', '--root', 'frontend', '--reporter=dot']),
    ('worker', ['node', 'server/worker/node_modules/vitest/vitest.mjs', 'run', '--root', 'server/worker', '--reporter=dot']),
    ('frontend_types', ['node', 'frontend/node_modules/typescript/bin/tsc', '--noEmit', '--project', 'frontend/tsconfig.json']),
    ('worker_types', ['node', 'server/worker/node_modules/typescript/bin/tsc', '--noEmit', '--project', 'server/worker/tsconfig.json']),
    ('ipc', ['node', 'scripts/check-ipc-contract.mjs']),
    ('turn_ends', ['node', 'scripts/check-turn-ends.mjs']),
    ('hygiene', ['node', 'scripts/check-repo-hygiene.mjs']),
]
SOURCES = [
    'src-tauri/src/' + f + '.rs' for f in ['agent_lifecycle', 'agent_lifecycle_runtime',
    'agent_attention', 'agent_hooks', 'agent_watch', 'keep_awake', 'power_thread',
    'notify_hub', 'watch_runner', 'webhook', 'lib']
] + ['src-tauri/resources/agent_watch_sidecar/server.cjs',
     'src-tauri/resources/agent_watch_sidecar/lifecycle.test.cjs',
     'scripts/agent-watch-contract.test.mjs', 'scripts/agent-watch-mutations.py',
     'scripts/verify-agent-watch.py'] + [str(p.relative_to(ROOT)).replace('\\', '/')
        for p in (ROOT / 'scripts/agent-watch-harness').iterdir() if p.is_file()]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--passes', type=int, default=2, choices=[1, 2])
    parser.add_argument('--output', default='docs/testing/runs/agent-watch-20261105.json')
    args = parser.parse_args()
    hashes = lambda: {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in SOURCES}
    before = hashes()
    evidence = {'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'scope': 'offline helpers and mocked boundaries; no application build or runtime operations',
                'source_sha256': before, 'passes': []}
    for attempt in range(1, args.passes + 1):
        results = []
        for name, command in COMMANDS:
            result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding='utf-8', errors='replace')
            output = result.stdout + result.stderr
            lines = [line for line in output.splitlines() if line.strip()]
            summaries = [line for line in lines if re.search(r'Test Files|Tests\s+\d|test result:|tests \d|pass \d|fail \d|contract FAILED|NEW:|listened-never-emitted|gate OK|hygiene OK', line)]
            results.append({'check': name, 'exit_code': result.returncode, 'summary': '\n'.join(summaries or lines[-8:])})
            print(f'Pass {attempt}: {name}: {"PASS" if result.returncode == 0 else "FAIL"}', flush=True)
        evidence['passes'].append({'pass': attempt, 'results': results})
    evidence['source_unchanged_during_checks'] = before == hashes()
    evidence['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    target = ROOT / args.output
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    return 0 if evidence['source_unchanged_during_checks'] and all(r['exit_code'] == 0 for p in evidence['passes'] for r in p['results']) else 1

if __name__ == '__main__':
    sys.exit(main())
