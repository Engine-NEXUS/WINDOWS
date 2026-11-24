"""Offline safety mutation checks in a disposable mirror; never edits the checkout."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
RULES = [
    ('voice_choice_capture', 'desktop_choices.rs', 'matches!(reason, "hotkey-wake" | "wake-fire" | "barge" | "continue")', 'false'),
    ('unique_target', 'desktop_control.rs', '2..=5 => Resolution::Clarify(c)', '2..=5 => Resolution::Unique(c[0].clone())'),
    ('protected_control', 'desktop_control.rs', '&& !self.password', '&& true'),
    ('current_window', 'desktop_control.rs', 'if before.window != after.window {', 'if false {'),
    ('cancellation', 'desktop_control.rs', 'if env.cancelled() {', 'if false {'),
    ('effect_success', 'desktop_control.rs', 'if effect.observed(before, &after) {', 'if true {'),
    ('stale_choice', 'desktop_control.rs', 'if p.token != token {', 'if false {'),
    ('expired_choice', 'desktop_control.rs', 'if age >= CHOICE_MS {', 'if false {'),
    ('keyboard_process_lifetime', 'shortcuts/router.rs', 'if let Some(expected) = identity {', 'if let Some(expected) = None::<crate::desktop_control::WindowIdentity> {'),
    ('single_dispatch', 'shortcuts/router.rs', 'if let Err(e) = env.send(&chord) {', 'let _ = env.send(&chord); if let Err(e) = env.send(&chord) {'),
    ('request_cancellation', 'shortcuts/router.rs', 'self.1.load(std::sync::atomic::Ordering::Relaxed) || self.0.stop_requested()', 'self.0.stop_requested()'),
    ('late_dispatch', 'shortcuts/router.rs', 'if env.now_ms().saturating_sub(t0) > crate::desktop_control::VERIFY_MS {', 'if false {'),
]

def main():
    results = []
    with tempfile.TemporaryDirectory(prefix='nexus-desktop-mutations-') as tmp:
        mirror = Path(tmp)
        harness = mirror/'scripts/desktop-control-harness'
        shutil.copytree(ROOT/'scripts/desktop-control-harness', harness, ignore=shutil.ignore_patterns('target'))
        paths = ['desktop_control.rs','desktop_native.rs','desktop_choices.rs','ghost_center.rs','ghost_learning.rs','dictation.rs','dictation_text.rs','shortcuts/router.rs','shortcuts/map.rs','shortcuts/keys.rs','live/safety.rs','shortcuts/seed_windows.json','../tests/fixtures/dictation_corpus.tsv','orchestrator.rs']
        for name in paths:
            dest=mirror/'src-tauri/src'/name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT/'src-tauri/src'/name, dest)
        env=dict(os.environ, CARGO_TARGET_DIR=str(ROOT/'scripts/desktop-control-harness/target/mutations'))
        for name, relative, old, new in RULES:
            path=mirror/'src-tauri/src'/relative
            original=path.read_text(encoding='utf-8')
            if old not in original: raise RuntimeError('mutation anchor missing: '+name)
            path.write_text(original.replace(old,new), encoding='utf-8')
            try:
                run=subprocess.run(['cargo','test','--offline','--locked','--manifest-path',str(harness/'Cargo.toml'),'-q'],cwd=ROOT,env=env,capture_output=True,text=True,timeout=600)
                output=run.stdout+run.stderr
                killed=run.returncode!=0 and 'test result: FAILED' in output and 'could not compile' not in output
                results.append({'rule':name,'killed_by_assertion':killed})
                print(name+(': killed' if killed else ': NOT KILLED'), flush=True)
                if not killed: print(output[-2000:])
            finally: path.write_text(original,encoding='utf-8')
    target=ROOT/'docs/testing/runs/desktop-control-mutations-20261106.json'
    target.write_text(json.dumps({'scope':'offline disposable mirror; no desktop actions','results':results},indent=2)+'\n',encoding='utf-8')
    if not all(r['killed_by_assertion']for r in results):raise SystemExit(1)

if __name__=='__main__':main()
