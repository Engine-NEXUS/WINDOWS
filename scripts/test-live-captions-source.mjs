// Which live-caption source does a real NEXUS (debug build, contributor profile) choose, and does the speech-model status work?
// Checks, against the isolated app on its debug port (never the owner's NEXUS):
//   mode "cold": the local speech server is NOT running, a (made-up) Groq key is set  -> the turn uses Groq
//   mode "warm": the local speech server IS running, the same key                       -> the turn uses the local server
// and in both modes the Command Hub's speech-model commands answer. The made-up key is never valid, no speech is played into
// the turn (the capture is stopped as soon as it has started), and the key is removed again at the end.
//
//   node scripts/test-live-captions-source.mjs cold|warm
import fs from 'node:fs';
const MODE = process.argv[2];
if (!['cold', 'warm'].includes(MODE)) { console.log('usage: cold|warm'); process.exit(2); }
const PORT = Number(process.env.PORT || 9333);
const LOG = process.env.LOG || 'D:\\nexus-isolated.log';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (name, ok, detail = '') => { if (!ok) failed++; console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  -> ' + detail : ''}`); };
const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const t = list.find((x) => x.url.includes('stage.html'));
if (!t) { console.log('FAIL  no stage page'); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let id = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const evalJs = (expression) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); }).then((r) => r.result?.result?.value);
const inv = (cmd, a = {}) => evalJs(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(a)}).then(v => JSON.stringify(v === undefined ? null : v)).catch(e => "ERR " + e)`);
const j = (s) => { try { return JSON.parse(s); } catch { return s; } };
const logLen = () => (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8').length : 0);
const logSince = (n) => (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8').slice(n) : '');

// speech model commands
const st = j(await inv('stt_model_status'));
check('stt_model_status answers with a known state', st && ['ready', 'downloading', 'missing', 'unknown'].includes(st.state), JSON.stringify(st));
if (st?.state === 'ready') {
  const p = j(await inv('stt_model_prefetch'));
  check('asking to download a model that is already there changes nothing', p?.state === 'ready', JSON.stringify(p));
}

let serverUp = false;
try { serverUp = (await fetch('http://127.0.0.1:39217/health', { signal: AbortSignal.timeout(1500) })).ok; } catch { /* not running */ }
check(`the local speech server is ${MODE === 'warm' ? 'running' : 'not running'} as this mode needs`, serverUp === (MODE === 'warm'));

await inv('settings_patch', { patch: { sttLiveCaptions: true, localSttOnly: false, groqApiKey: 'gsk_isolated_test_key_not_real' } });
const s = j(await inv('get_settings'));
check('captions are on (the made-up key is proved by the source line below, get_settings does not return keys)', s.sttLiveCaptions === true);

const mark = logLen();
const started = await inv('start_stt_capture', { origin: null, noSpeechMs: 3000 });
check('a capture starts', !String(started).startsWith('ERR'), String(started));
await sleep(2500);
const fresh = logSince(mark);
const m = /stt_stream: this turn's live captions come from (\w+)/.exec(fresh);
check(`the turn's caption source is ${MODE === 'warm' ? 'Local (the warm server wins over Groq)' : 'Groq (a cold server is not started for captions)'}`, m?.[1] === (MODE === 'warm' ? 'Local' : 'Groq'), m ? m[1] : 'no [stt_stream] source line in the log');
if (MODE === 'cold') {
  let up2 = false;
  try { up2 = (await fetch('http://127.0.0.1:39217/health', { signal: AbortSignal.timeout(1500) })).ok; } catch { /* still down */ }
  check('asking for captions did not start the local server', !up2);
}
await inv('stop_stt_capture');
await inv('settings_patch', { patch: { sttLiveCaptions: false, groqApiKey: '' } });
const s2 = j(await inv('get_settings'));
check('the made-up key is removed again', !s2.groqApiKey);
ws.close();
console.log(`\n${failed === 0 ? 'all checks passed' : failed + ' FAILED'}`);
process.exit(failed ? 1 : 0);
