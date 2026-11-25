"""Read-only application audit: desired-behavior probes in temporary copies.

Exit 1 means an edge remains open. This never builds or runs the NEXUS app.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
RUST_PROBE = r'''
#[test] fn audit_busy_sibling_started_before_watch_must_still_veto() {
    let mut r=Registry::default();
    event(&mut r,&id("already-running"),Kind::Start,1,None);
    // A different session finishes after a watch begins at 50.
    event(&mut r,&id("finishing"),Kind::Stop,100,Some(true));r.tick(20_000);
    assert!(r.active());assert_eq!(r.ready().len(),1);
    assert!(r.busy_since("antigravity",50),"older active sibling is excluded by the watch-start filter");
}
'''
NODE_PROBES = r'''
test('audit definitive process exit survives an earlier provisional done',async()=>{
 const f=fixture();const r=await f.request('/watch',{type:'claude',prompt:'fake'});
 f.manager.emit('task_complete',{id:'session1'});
 f.manager.emit('session_stopped',{id:'session1',exitCode:0},'normal exit');
 assert.equal(f.watches.get(r.body.id).detail.reason,'process_exit');
});
for(const name of ['OPENCODE','KILO'])test(`audit ${name} permission resume reports activity`,async()=>{
 const text=fs.readFileSync(process.env.NEXUS_HOOKS_SOURCE,'utf8');
 const raw=text.split(`const ${name}_PLUGIN_TEMPLATE: &str = r###"`)[1].split('"###;')[0];
 const posts=[];const source=raw.replace('export const NexusAgentEvent','const NexusAgentEvent').replace(/export default[^;]+;/g,'');
 const ctx=vm.createContext({process:{env:{}},AbortSignal,fetch:async(url,r)=>posts.push({url,body:JSON.parse(r.body)})});
 const factory=vm.runInContext(source+`; ${name==='KILO'?'server':'NexusAgentEvent'}`,ctx);
 const client={session:{get:async()=>({data:{}}),messages:async()=>({data:[]})}};
 const hooks=await factory({client});
 await hooks.event({event:{type:'message.updated',properties:{info:{sessionID:'s',role:'user',id:'m'}}}});
 await hooks.event({event:{type:'permission.asked',properties:{sessionID:'s'}}});
 const checkpoint=posts.length;
 // Simulated resume inputs: no new user prompt is required to approve a permission.
 await hooks.event({event:{type:'session.status',properties:{sessionID:'s',status:{type:'busy'}}}});
 await hooks.event({event:{type:'message.updated',properties:{info:{sessionID:'s',role:'assistant',id:'reply'}}}});
 assert.ok(posts.slice(checkpoint).some(p=>/[?&]event=(start|message)/.test(p.url)),'no resumed-activity event reaches NEXUS');
});
'''

def run(command, cwd, env=None):
    p = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True, encoding='utf-8', errors='replace')
    return {'exit_code': p.returncode, 'output': p.stdout + p.stderr}

def main():
    from importlib.util import spec_from_file_location, module_from_spec
    spec=spec_from_file_location('mutations',ROOT/'scripts/agent-watch-mutations.py')
    helper=module_from_spec(spec);spec.loader.exec_module(helper)
    fingerprints={name:hashlib.sha256((ROOT/'src-tauri/src'/name).read_bytes()).hexdigest() for name in helper.SOURCES}
    with tempfile.TemporaryDirectory(prefix='nexus-agent-audit-') as temp:
        root=Path(temp);dest=root/'src-tauri/src';dest.mkdir(parents=True)
        for name in helper.SOURCES: shutil.copy2(ROOT/'src-tauri/src'/name,dest/name)
        harness=root/'scripts/agent-watch-harness';harness.mkdir(parents=True)
        for file in (ROOT/'scripts/agent-watch-harness').iterdir():
            if file.is_file(): shutil.copy2(file,harness/file.name)
        with (harness/'tests.rs').open('a',encoding='utf-8') as f:f.write(RUST_PROBE)
        native=run(['cargo','test','--manifest-path',str(harness/'Cargo.toml'),'--locked','--offline',
                    '--target-dir',str(ROOT/'scripts/agent-watch-harness/target/audit'),'audit_','--','--nocapture'],root)
        fixture=(ROOT/'src-tauri/resources/agent_watch_sidecar/lifecycle.test.cjs').read_text(encoding='utf-8').split("test('generic output")[0]
        node_path=root/'audit.cjs';node_path.write_text(fixture+NODE_PROBES,encoding='utf-8')
        env=dict(os.environ,NEXUS_SIDECAR_SOURCE=str(ROOT/'src-tauri/resources/agent_watch_sidecar/server.cjs'),NEXUS_HOOKS_SOURCE=str(ROOT/'src-tauri/src/agent_hooks.rs'))
        node=run(['node','--test',str(node_path)],root,env)
    report={'scope':'isolated desired-behavior probes, no application/runtime operations',
            'native_probe':native,'sidecar_template_probes':node,'source_sha256':fingerprints,
            'interpretation':'Nonzero assertion failures reproduce open issues; compilation or harness failures are not confirmation.'}
    target=ROOT/'docs/testing/runs/agent-watch-edge-audit-20261106.json'
    target.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'native_exit':native['exit_code'],'node_exit':node['exit_code'],'report':str(target)}))
    return 1 if native['exit_code'] or node['exit_code'] else 0

if __name__=='__main__':raise SystemExit(main())
