// Verification helper only: native IPC readback, no speech or profile content.
import { writeFileSync } from "node:fs";
const started = performance.now();
const endpoints = await (await fetch("http://127.0.0.1:9236/json")).json();
const legacy = process.argv.includes("--settings");
const stage = process.argv.includes("--stage");
const matches = endpoints.filter(x => stage ? /\/stage\.html/.test(x.url) : legacy ? /\/settings\.html/.test(x.url) : /sidebar\.html/.test(x.url) && !/settings-sidebar/.test(x.url));
if (matches.length !== 1) throw new Error(`Expected one sidebar target, found ${matches.length}`);
const ws = new WebSocket(matches[0].webSocketDebuggerUrl);
await new Promise((resolve,reject) => { ws.onopen=resolve; ws.onerror=reject; });
let id = 0;
const pending = new Map();
ws.onmessage = e => { const r = JSON.parse(e.data); const entry = pending.get(r.id); if (entry) { pending.delete(r.id); clearTimeout(entry.timer); r.error ? entry.reject(new Error(JSON.stringify(r.error))) : entry.resolve(r.result); } };
function rpc(method, params) { return new Promise((resolve,reject) => { const n=++id; const timer=setTimeout(()=>{pending.delete(n);reject(new Error(method+" timed out after 15 seconds"));},15000); pending.set(n,{resolve,reject,timer}); ws.send(JSON.stringify({id:n,method,params})); }); }
ws.onclose = () => { for(const entry of pending.values()) { clearTimeout(entry.timer); entry.reject(new Error("Verification WebView closed before reply")); } pending.clear(); };
const allowed = new Set(["show_settings_sidebar", "hide_settings_sidebar", "open_settings_window", "close_settings_window", "pause_wakeword", "resume_wakeword"]);
const command = process.argv.slice(2).find(x => !x.startsWith("--"));
const faultModes = new Set(["fault-save-refusal", "fault-save-ack", "fault-save-delay", "fault-profile", "guard-autostart", "restore-ipc", "inspect-ipc"]);
const diagnosticModes = new Set(["diagnostics-arm-one", "diagnostics-off", "diagnostics-status"]);
const lockModes = new Set(["lock-test-on", "lock-restore-off"]);
if (command && !allowed.has(command) && !faultModes.has(command) && !diagnosticModes.has(command) && !lockModes.has(command) && command !== "capture") throw new Error("Only verification commands are allowed");
let expression = command
  ? `window.__TAURI__.core.invoke(${JSON.stringify(command)})`
  : `(async () => {
      const call = window.__TAURI__.core.invoke;
      const [enabled,settings,profile] = await Promise.all([call("voice_lock_enabled"),call("get_settings"),call("get_voice_profile_status")]);
      return { effectiveVoiceLock: enabled, savedVoiceLock: settings.speakerVerification, profileEnrolled: profile.enrolled, clips: profile.num_clips };
    })()`;
if (faultModes.has(command)) {
  expression = `(() => {
    const internal = window.__TAURI_INTERNALS__;
    if (!internal?.invoke) throw new Error("No Tauri internal IPC bridge");
    const mode = ${JSON.stringify(command)};
    if (mode === "inspect-ipc") return {fetchInstalled:!!window.__NEXUS_P2_VERIFY_ORIGINAL_FETCH__, stats:window.__NEXUS_P2_VERIFY_STATS__??null};
    if (mode === "restore-ipc") {
      if (window.__NEXUS_P2_VERIFY_ORIGINAL_INVOKE__) internal.invoke = window.__NEXUS_P2_VERIFY_ORIGINAL_INVOKE__;
      delete window.__NEXUS_P2_VERIFY_ORIGINAL_INVOKE__;
      if (window.__NEXUS_P2_VERIFY_ORIGINAL_FETCH__) window.fetch = window.__NEXUS_P2_VERIFY_ORIGINAL_FETCH__;
      delete window.__NEXUS_P2_VERIFY_ORIGINAL_FETCH__;
      return {controlledFault: "restored",stats:window.__NEXUS_P2_VERIFY_STATS__??null};
    }
    if (!window.__NEXUS_P2_VERIFY_ORIGINAL_FETCH__) window.__NEXUS_P2_VERIFY_ORIGINAL_FETCH__ = window.fetch;
    const original = window.__NEXUS_P2_VERIFY_ORIGINAL_FETCH__;
    const stats = window.__NEXUS_P2_VERIFY_STATS__ = {mode,intercepted:0,pending:0};
    const reply=(ok,value)=>Promise.resolve(new Response(JSON.stringify(value),{headers:{'Content-Type':'application/json','Tauri-Response':ok?'ok':'error'}}));
    const wrapper = function(input,options) {
      const url=String(input);
      const is=(cmd)=>url===internal.convertFileSrc(cmd,'ipc');
      if (is('set_autostart') && mode === 'guard-autostart') {stats.intercepted++;return reply(true,null);}
      if (is('get_voice_profile_status') && mode === 'fault-profile') {stats.intercepted++;return reply(false,'P2 controlled profile-status failure; enrolled file untouched');}
      if (is('settings_patch')) {
        stats.intercepted++;
        if (mode === 'fault-save-refusal') return reply(false,'P2 controlled save refusal; native setting unchanged');
        if (mode === 'fault-save-ack') return reply(true,null);
        if (mode === 'fault-save-delay') {stats.pending++;return new Promise((resolve,reject)=>setTimeout(()=>original.call(window,input,options).then(resolve,reject).finally(()=>stats.pending--),8000));}
      }
      return original.call(window,input,options);
    };
    window.fetch=wrapper;
    if(window.fetch!==wrapper)throw new Error('Fault interceptor was not installed');
    return {controlledFault:mode,installed:true};
  })()`;
}
if (diagnosticModes.has(command)) {
  expression = `(async()=>{
    const call=window.__TAURI__.core.invoke;
    const mode=${JSON.stringify(command)};
    const stop=async(reason)=>{
      const state=window.__NEXUS_ONE_CLIP__;
      if(state){clearTimeout(state.timer);state.unlisten?.();state.reason=reason;state.armed=false;}
      await call('settings_patch',{patch:{sttDiagnostics:false}});
      return {enabled:(await call('get_settings')).sttDiagnostics,reason};
    };
    if(mode==='diagnostics-off')return stop('explicit cleanup');
    if(mode==='diagnostics-arm-one'){
      if((await call('get_settings')).sttDiagnostics)throw Error('Diagnostics already enabled; do not overwrite an existing session');
      const state=window.__NEXUS_ONE_CLIP__={armed:true,session:null,reason:'awaiting one capture'};
      state.unlisten=await window.__TAURI__.event.listen('stt:turn_stats',async(event)=>{
        if(!state.armed)return;
        state.session=event.payload?.session??null;
        await stop('first completed capture');
      });
      state.timer=setTimeout(()=>stop('90-second timeout'),90000);
      await call('settings_patch',{patch:{sttDiagnostics:true}});
      return {armed:true,enabled:(await call('get_settings')).sttDiagnostics,timeoutSeconds:90};
    }
    const state=window.__NEXUS_ONE_CLIP__;
    return {enabled:(await call('get_settings')).sttDiagnostics,oneClip:state?{armed:state.armed,session:state.session,reason:state.reason}:null,clips:await call('stt_diagnostics_list')};
  })()`;
}
if(lockModes.has(command)) {
  expression=`(async()=>{
    const call=window.__TAURI__.core.invoke;
    const enabled=${command === "lock-test-on"};
    if(enabled && (await call('get_voice_profile_status')).enrolled!==true)throw Error('Valid enrollment required for the controlled trial');
    await call('settings_patch',{patch:{speakerVerification:enabled}});
    const native=await call('voice_lock_enabled');
    const saved=(await call('get_settings')).speakerVerification;
    if(native!==enabled || saved!==enabled)throw Error('Controlled lock write did not match native readback');
    return {effectiveVoiceLock:native,savedVoiceLock:saved,profileEnrolled:(await call('get_voice_profile_status')).enrolled};
  })()`;
}
try {
  if (command === "capture") {
    const labelArg=process.argv.find(x=>x.startsWith("--label="));
    const label=labelArg?labelArg.slice(8):"audio";
    if(!/^[a-z0-9-]{1,64}$/.test(label))throw new Error("Invalid capture label");
    const result=await rpc("Page.captureScreenshot",{format:"png",captureBeyondViewport:false});
    const output=new URL(`./windows1-20261025-voice-lock-ui-${label}.png`,import.meta.url);
    writeFileSync(output,Buffer.from(result.data,"base64"));
    console.log(JSON.stringify({at:new Date().toISOString(),capture:"browser-rendered webview only; native WDA exclusion unchanged",path:output.pathname},null,2));
  } else {
  const result = await rpc("Runtime.evaluate", { expression, awaitPromise:true, returnByValue:true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text + ": " + (result.exceptionDetails.exception?.description ?? ""));
  console.log(JSON.stringify({at:new Date().toISOString(),command:command??"readback",elapsedMs:Math.round(performance.now()-started),value:result.result.value},null,2));
  }
} finally { ws.close(); }
