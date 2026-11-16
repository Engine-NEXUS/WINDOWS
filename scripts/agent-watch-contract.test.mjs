import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import vm from 'node:vm';
const read = p => readFileSync(process.env.NEXUS_AGENT_CONTRACT_ROOT ? join(process.env.NEXUS_AGENT_CONTRACT_ROOT,p) : new URL('../'+p,import.meta.url),'utf8');
test('background completion uses the shared lifecycle, not raw stop receipt time',()=>{
 const s=read('src-tauri/src/agent_attention.rs');
 assert.ok(s.includes('agent_lifecycle_runtime::poll'));
 assert.ok(!s.includes('format!("{} has finished", a.label())'));
});
test('Windows power assertions are serialized on a dedicated owner thread',()=>{
 assert.ok(read('src-tauri/src/keep_awake.rs').includes('PowerThread'));
});
test('file quiescence cannot announce that an agent finished',()=>{
 const s=read('src-tauri/src/agent_watch.rs');
 assert.ok(!s.includes('last_activity.elapsed() >= Duration::from_secs(6)'));
 assert.ok(!s.includes('response is ready, sir. It has finished running.'));
});
test('hook identity is carried all the way into the shared lifecycle',()=>{
 assert.ok(read('src-tauri/src/webhook.rs').includes('agent_lifecycle_runtime::record_hook'));
 assert.ok(read('src-tauri/src/agent_hooks.rs').includes('pub identity: crate::agent_lifecycle::Identity'));
});
for (const name of ['OPENCODE','KILO']) test(`${name} actual plugin preserves identity on start, wait and stop and excludes children`,async()=>{
 const text=read('src-tauri/src/agent_hooks.rs');
 const marker=`const ${name}_PLUGIN_TEMPLATE: &str = r###"`;
 const raw=text.split(marker)[1]?.split('"###;')[0];assert.ok(raw);
 const posts=[];
 const source=raw.replace('export const NexusAgentEvent','const NexusAgentEvent').replace(/export default[^;]+;/g,'');
 const context=vm.createContext({process:{env:{NEXUS_WATCH_ID:'owned-1'}},AbortSignal,
  fetch:async(url,request)=>{posts.push({url,body:JSON.parse(request.body)});}});
 const factory=vm.runInContext(source+`; ${name==='KILO'?'server':'NexusAgentEvent'}`,context);
 const client={session:{get:async({path})=>{if(path.id==='unknown')throw Error('offline');return{data:{parentID:path.id==='child'?'root':undefined}};},messages:async()=>({data:[]})}};
 const hooks=await factory({client});
 for(const sessionID of ['root','child','unknown']) {
  await hooks.event({event:{type:'message.updated',properties:{info:{sessionID,role:'user',id:'m1'}}}});
  await hooks.event({event:{type:'permission.asked',properties:{sessionID}}});
  await hooks.event({event:{type:'session.idle',properties:{sessionID}}});
 }
 assert.equal(posts.length,3);
 assert.deepEqual(posts.map(p=>p.body.session_id),['root','root','root']);
 assert.ok(posts.every(p=>p.body.nexus_watch_id==='owned-1'));
 assert.equal(posts[0].body.event_id,'user:m1');
});
