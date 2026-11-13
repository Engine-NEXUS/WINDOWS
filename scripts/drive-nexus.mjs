// Drives the real orchestrator_process command inside the running NEXUS (same entry the voice path uses).
import fs from 'node:fs';
const LOG = process.env.APPDATA + '\\com.nexus.assistant\\logs\\nexus.log';
const turns = JSON.parse(process.env.TURNS || process.argv[2]);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const stage = list.find((t) => t.url.includes('stage.html'));
const ws = new WebSocket(stage.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const cdp = (method, params) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
const lines = () => fs.readFileSync(LOG, 'utf8').split('\n');
const panels = async () => (await (await fetch('http://127.0.0.1:9222/json/list')).json()).map((t) => t.url.replace('http://tauri.localhost/', '').split('?')[0] + (t.url.includes('?') ? '?' + t.url.split('?')[1].slice(0, 20) : ''));
for (const t of turns) {
  const from = lines().length;
  const t0 = Date.now();
  const r = await cdp('Runtime.evaluate', {
    expression: `window.__TAURI_INTERNALS__.invoke("orchestrator_process", {transcript: ${JSON.stringify(t)}, dialogContext: null, turnContext: null}).then(v => JSON.stringify(v)).catch(e => "ERR " + e)`,
    awaitPromise: true, returnByValue: true,
  });
  const ms = Date.now() - t0;
  await sleep(Number(process.env.WAIT || 9000));
  const fresh = lines().slice(from).map((l) => l.replace(/^\S+Z\s+/, ''));
  const spoken = fresh.filter((l) => /tts: speaking '/.test(l)).map((l) => l.replace(/^.*tts: speaking /, ''));
  const other = fresh.filter((l) => /directions:|place card|trip reminder|\[MAIN-CMD\]|\[DROP\]|\[SUB-ROUTE\]|maps:|WARN|ERROR/.test(l) && !/audio:|wake:|silence/.test(l)).slice(0, 6).map((l) => l.slice(0, 200));
  console.log(`\n>>> ${t}   (invoke ${ms} ms)`);
  console.log('  result: ' + String(r.result?.result?.value).slice(0, 160));
  for (const s of spoken) console.log('  SAID: ' + s);
  for (const o of other) console.log('  log:  ' + o);
  console.log('  pages: ' + (await panels()).join(', '));
}
ws.close();
