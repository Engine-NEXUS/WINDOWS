// Spoken commands through the REAL microphone path of an isolated NEXUS, with no person: each speech file (made by
// scripts/make-test-speech.ps1) is played by the debug-only fake microphone (NEXUS_FAKE_MIC_DIR) into a real capture, and
// the transcript, the routed intent and the spoken answer are read from the app's log. Synthetic Windows-voice speech is
// NOT a person's voice: a pass here says the path works, not that a real voice is understood as well.
//
//   node scripts/drive-isolated-speech.mjs a.wav b.wav ... [--out results.json]
//   env: PORT (9333), LOG (D:\nexus-isolated.log), NEXUS_FAKE_MIC_DIR (D:\nexus-fake-mic)
import fs from 'node:fs';
import path from 'node:path';

const PORT = Number(process.env.PORT || 9333);
const LOG = process.env.LOG || 'D:\\nexus-isolated.log';
const DIR = process.env.NEXUS_FAKE_MIC_DIR || 'D:\\nexus-fake-mic';
const args = process.argv.slice(2);
const oi = args.indexOf('--out');
const OUT = oi >= 0 ? args.splice(oi, 2)[1] : '';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const stage = list.find((t) => t.url.includes('stage.html'));
if (!stage) { console.error('no stage page'); process.exit(2); }
const ws = new WebSocket(stage.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const evalJs = (expression) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); });
const inv = (cmd, a = {}) => evalJs(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(a)}).then(v => JSON.stringify(v === undefined ? null : v)).catch(e => "ERR " + e)`);
const text = () => (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8') : '');
const clean = (l) => l.replace(/\x1b\[[0-9;]*m/g, '').replace(/^\S+Z\s+/, '');

await inv('settings_patch', { patch: { localSttOnly: true } });
fs.mkdirSync(DIR, { recursive: true });
const out = [];
for (const file of args) {
  const words = JSON.parse(fs.readFileSync(file.replace(/\.wav$/i, '.words.json'), 'utf8'));
  const said = words.words.map((w) => w.word ?? w.text).join(' ');
  const from = text().length;
  await inv('start_stt_capture', { origin: null, noSpeechMs: 8000 });
  await sleep(600);
  fs.copyFileSync(file, path.join(DIR, `${Date.now()}-${path.basename(file)}`));
  // wait until an answer has been logged (or 40 s), then let it finish
  for (let i = 0; i < 80; i++) { await sleep(500); const f = text().slice(from); if (/\[MAIN-CMD\]|\[DROP\]/.test(f) && /\[TTS\] speak|tts: speaking/.test(f)) break; }
  await sleep(14000);
  const fresh = text().slice(from).split('\n').map(clean);
  const pick = (re) => fresh.filter((l) => re.test(l)).map((l) => l.slice(0, 300));
  const rec = { said, heard: pick(/\[MAIN-CMD\]|\[DROP\]/).slice(0, 3), spoken: pick(/\[TTS\] speak|tts: speaking '/).slice(0, 4), reverse: pick(/\[REVERSE\] target|render guard/) };
  out.push(rec);
  console.log(`\n>>> said: ${said}`);
  for (const k of ['heard', 'spoken', 'reverse']) for (const l of rec[k]) console.log(`  ${k}: ${l}`);
}
if (OUT) fs.writeFileSync(OUT, JSON.stringify(out, null, 1));
ws.close();
