const test=require('node:test');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const fs=require('node:fs');
const path=require('node:path');
const {EventEmitter}=require('node:events');

function fixture(){
 let handler,manager;
 class Manager extends EventEmitter{
  constructor(){super();manager=this;this.configs=[];this.stops=0;}
  registerAdapter(){}
  async spawn(config){this.configs.push(config);return{id:'session'+this.configs.length};}
  send(){}
  async stop(id){this.stops++;this.emit('session_stopped',{id,exitCode:0},'normal exit');}
 }
 class Adapter{stripAnsi(s){return s;}}
 const token='t'.repeat(64);
 const ctx=vm.createContext({Buffer,URL,console:{log(){},error(){}},setTimeout,
  setInterval(){return{unref(){}}},process:{env:{AGENT_WATCH_TOKEN:token},stdin:{on(){},resume(){}},on(){},exit(){throw Error('unexpected process exit');}},
  require(name){if(name==='pty-manager')return{PTYManager:Manager};if(name==='coding-agent-adapters')return{ADAPTER_TYPES:{claude:Adapter},createAllAdapters:()=>[],BaseCodingAdapter:Adapter};
   if(name==='node:http')return{createServer(fn){handler=fn;return{listen(_p,_h,cb){cb();},close(){}}}};
   return require(name);},
 });
 const source=fs.readFileSync(process.env.NEXUS_SIDECAR_SOURCE||path.join(__dirname,'server.cjs'),'utf8');
 vm.runInContext(source+'\nglobalThis.inspect={manager,watches,AntigravityAdapter,GenericTerminalAdapter};',ctx);
 async function request(route,body={},authorized=true){
  const req=new EventEmitter();Object.assign(req,{method:'POST',url:route,headers:{host:'127.0.0.1:39222','x-nexus-token':authorized?token:'bad'}});
  let status,text;const res={writeHead(s){status=s;},end(t){text=t;}};
  const pending=handler(req,res);queueMicrotask(()=>{req.emit('data',Buffer.from(JSON.stringify(body)));req.emit('end');});await pending;
  return{status,body:JSON.parse(text)};
 }
 return{manager,get watches(){return ctx.inspect.watches;},classes:ctx.inspect,request};
}
test('generic output words and prompts are never completion proof',()=>{
 const f=fixture();for(const C of [f.classes.AntigravityAdapter,f.classes.GenericTerminalAdapter]){
  const a=new C();for(const text of ['Done','Task complete','Finished','\n> ','The test says Done but the agent is still working'])assert.equal(a.isResponseComplete(text),false);
 }
});
test('owned CLI passes its stable watch identity to hook children',async()=>{
 const f=fixture();const r=await f.request('/watch',{type:'claude',prompt:'a fake task'});
 assert.equal(r.status,200);assert.equal(f.manager.configs[0].env.NEXUS_WATCH_ID,r.body.id);
});
test('explicit cancellation wins a synchronous normal-exit race',async()=>{
 const f=fixture();const r=await f.request('/watch',{type:'claude',prompt:'fake'});
 await f.request(`/watch/${r.body.id}/stop`);assert.equal(f.watches.get(r.body.id).status,'stopped');
 f.manager.emit('task_complete',{id:'session1'});assert.equal(f.watches.get(r.body.id).status,'stopped');
});
test('hundred duplicated completion callbacks do not change a terminal record',async()=>{
 const f=fixture();const r=await f.request('/watch',{type:'claude',prompt:'fake'});
 f.manager.emit('task_complete',{id:'session1'});const ended=f.watches.get(r.body.id).endedAt;
 for(let n=0;n<100;n++)f.manager.emit('task_complete',{id:'session1'});
 assert.equal(f.watches.get(r.body.id).status,'done');assert.equal(f.watches.get(r.body.id).endedAt,ended);
});
test('verified-hook acknowledgement requires authentication and never stops a CLI',async()=>{
 const f=fixture();const r=await f.request('/watch',{type:'claude',prompt:'fake'});const route=`/watch/${r.body.id}/observed`;
 assert.equal((await f.request(route,{status:'done'},false)).status,403);
 assert.equal((await f.request(route,{status:'anything'})).status,400);
 assert.equal((await f.request(route,{status:'done'})).status,200);
 assert.equal(f.watches.get(r.body.id).status,'done');assert.equal(f.manager.stops,0);
});
