"""Two unchanged-source offline passes. Does not build or launch the NEXUS app."""
import hashlib
import json
from pathlib import Path
import subprocess
import time

ROOT=Path(__file__).resolve().parent.parent
FILES=['src-tauri/src/'+n for n in ['desktop_control.rs','desktop_native.rs','desktop_choices.rs','ghost_center.rs','ghost_learning.rs','dictation.rs','orchestrator.rs','shortcuts/router.rs','shortcuts/exec.rs','commands.rs','ghost.rs','lib.rs']]
FILES += ['frontend/src/sidebar/'+n for n in ['DesktopChoices.tsx','desktopChoiceModel.ts','desktopChoiceModel.test.ts','desktop-choices.css','UnifiedSidebar.tsx']]
FILES += ['frontend/src/'+n for n in ['audio/recorder.ts','stage/orbRuntime.ts','net/orchestrator.ts','net/orchestrator.test.ts']]
FILES += ['scripts/'+n for n in ['desktop-control-contract.test.mjs','desktop-control-mutations.py','desktop_control_trials.py','test_desktop_control_trials.py','verify-desktop-control.py']]
FILES += ['docs/testing/desktop-control/matrix.json','docs/testing/desktop-control/fixture.html','.github/workflows/desktop-control-offline.yml']
FILES += [str(p.relative_to(ROOT)).replace('\\','/') for p in (ROOT/'scripts/desktop-control-harness').rglob('*') if p.is_file() and 'target' not in p.relative_to(ROOT/'scripts/desktop-control-harness').parts]
def fingerprints():return {p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest()for p in sorted(FILES)}
def main():
    before=fingerprints();run_dir=ROOT/'docs/testing/runs/desktop-control-offline-20261106';run_dir.mkdir(exist_ok=True)
    commands=[
      ('rust_offline',['cargo','test','--offline','--locked','--manifest-path','scripts/desktop-control-harness/Cargo.toml','-q']),
      ('trial_reporter',['python','-m','unittest','discover','-s','scripts','-p','test_desktop_control_trials.py']),
      ('wiring_contracts',['node','--test','scripts/desktop-control-contract.test.mjs']),
      ('frontend_types',['node','frontend/node_modules/typescript/bin/tsc','--noEmit','-p','frontend/tsconfig.json']),
      ('frontend',['node','frontend/node_modules/vitest/vitest.mjs','run','--root','frontend','--reporter=dot']),
      ('worker',['node','server/worker/node_modules/vitest/vitest.mjs','run','--root','server/worker','--reporter=dot']),
      ('ipc',['node','scripts/check-ipc-contract.mjs']),
      ('turn_ends',['node','scripts/check-turn-ends.mjs']),
      ('hygiene',['node','scripts/check-repo-hygiene.mjs']),
      ('colors',['node','scripts/check-hardcoded-colors.mjs']),
      ('opacity',['node','scripts/check-window-opacity.mjs']),
      ('contrast',['node','scripts/check-theme-contrast.mjs']),
      ('whitespace',['git','diff','--check']),
    ]
    results=[]
    for repeat in (1,2):
      for name,cmd in commands:
        start=time.monotonic()
        run=subprocess.run(cmd,cwd=ROOT,capture_output=True,encoding='utf-8',errors='replace',timeout=600)
        (run_dir/f'{repeat}-{name}.log').write_text(run.stdout+run.stderr,encoding='utf-8')
        results.append({'pass':repeat,'check':name,'exit_code':run.returncode,'seconds':round(time.monotonic()-start,3)})
        print(f'{repeat} {name}: {run.returncode}',flush=True)
        if run.returncode:break
      if results[-1]['exit_code']:break
    after=fingerprints()
    revision=subprocess.run(['git','rev-parse','HEAD'],cwd=ROOT,capture_output=True,text=True,check=True).stdout.strip()
    manifest_hash=hashlib.sha256(json.dumps(before,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    result={'scope':'isolated offline helpers and source checks; no NEXUS build, input or launch','head':revision,'dirty_worktree':True,'source_manifest_sha256':manifest_hash,'files':before,'source_unchanged':before==after,'checks':results,'not_run':['full application native library/integration','compile-dependent dead-code guard','release build/provenance','real device matrix','real owner voice pass','remote CI']}
    (run_dir/'assessment.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    if before!=after or len(results)!=26 or any(r['exit_code']for r in results):raise SystemExit(1)
if __name__=='__main__':main()
