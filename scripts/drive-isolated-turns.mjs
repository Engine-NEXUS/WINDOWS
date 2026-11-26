// Sends spoken-style turns through the REAL orchestrator_process of an isolated (contributor-profile) NEXUS started by
// scripts/run-isolated-test.ps1, and prints for each turn: the result, what NEXUS said, and the log lines that matter.
// Never touches the owner's NEXUS (own identity, data folder and debug port). Used for the live runs of the reverse
// engineer and Maps features (docs/testing/runs/*).
//
//   node scripts/drive-isolated-turns.mjs turns.json [--wait 9000] [--grep "regex"]
//   turns.json = ["reverse engineer nextjs.org", "directions to Charminar", ...]
//   env: PORT (9333), LOG (D:\nexus-isolated.log)
import fs from 'node:fs';

const PORT = Number(process.env.PORT || 9333);
const LOG = process.env.LOG || 'D:\\nexus-isolated.log';
const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith('--'));
const flag = (name, dflt) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : dflt; };
const WAIT = Number(flag('--wait', 9000));
const GREP = new RegExp(flag('--grep', '\\[REVERSE\\]|directions:|open_routing|place card|trip reminder|\\[MAIN-CMD\\]|\\[DROP\\]|\\[SUB-ROUTE\\]|maps:|location|geocod|render guard|WARN|ERROR|panicked'), 'i');
const turns = JSON.parse(fs.readFileSync(file, 'utf8'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const stage = list.find((t) => t.url.includes('stage.html'));
if (!stage) { console.error('no stage page on port ' + PORT); process.exit(2); }
const ws = new WebSocket(stage.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const cdp = (method, params) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
const lines = () => (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8').split('\n') : []);
const clean = (l) => l.replace(/\x1b\[[0-9;]*m/g, '').replace(/^\S+Z\s+/, '');
const out = [];
for (const t of turns) {
  const from = lines().length;
  const t0 = Date.now();
  const r = await cdp('Runtime.evaluate', {
    expression: `window.__TAURI_INTERNALS__.invoke("orchestrator_process", {transcript: ${JSON.stringify(t)}, dialogContext: null, turnContext: null}).then(v => JSON.stringify(v)).catch(e => "ERR " + e)`,
    awaitPromise: true, returnByValue: true,
  });
  const invokeMs = Date.now() - t0;
  await sleep(WAIT);
  const fresh = lines().slice(from).map(clean);
  const said = fresh.filter((l) => /tts: speaking '|speak_line|\[SAY\]/.test(l)).map((l) => l.replace(/^.*(tts: speaking |speak_line:? )/, '').slice(0, 260));
  const logs = fresh.filter((l) => GREP.test(l) && !/audio:|wake:|silence/.test(l)).map((l) => l.slice(0, 260));
  const rec = { turn: t, invokeMs, result: String(r.result?.result?.value).slice(0, 200), said, logs };
  out.push(rec);
  console.log(`\n>>> ${t}   (invoke ${invokeMs} ms)`);
  console.log('  result: ' + rec.result.slice(0, 160));
  for (const s of said) console.log('  SAID: ' + s);
  for (const l of logs.slice(0, 14)) console.log('  log:  ' + l);
}
const outFile = flag('--out', '');
if (outFile) fs.writeFileSync(outFile, JSON.stringify(out, null, 1));
ws.close();
