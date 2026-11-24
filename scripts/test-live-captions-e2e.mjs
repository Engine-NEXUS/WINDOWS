// End to end test of the LIVE CAPTIONS in a real NEXUS (debug build, contributor profile, hidden desktop), with no person and no
// loudspeaker: speech made by scripts/make-test-speech.ps1 is played through the REAL microphone path by the debug-only hook
// (NEXUS_FAKE_MIC_DIR), a real capture is started, and everything that reaches the screen is recorded with timestamps:
//   * every `stt:partial` event the stage receives, with how many words were confirmed
//   * the text of the caption pill in the page, sampled every 50 ms (the DOM text; a hidden desktop is not a screenshot)
//   * the final `stt:transcript`
// then it reports time to the first caption, per-word lag, whether text only grew, and whether the pill showed it WHILE the
// speech was still being played. Needs: the app started with NEXUS_FAKE_MIC_DIR=<dir> (run-isolated-test.ps1 passes the
// environment on) and the local speech server (server/stt_server.py) listening on 39217; no key is used or needed.
//
//   node scripts/test-live-captions-e2e.mjs D:\nexus-test-speech\normal.wav [more.wav] [--dir D:\nexus-fake-mic]
import fs from 'node:fs';
import path from 'node:path';
const PORT = Number(process.env.PORT || 9333);
const args = process.argv.slice(2);
const dirIdx = args.indexOf('--dir');
const DIR = dirIdx >= 0 ? args.splice(dirIdx, 2)[1] : (process.env.NEXUS_FAKE_MIC_DIR || 'D:\\nexus-fake-mic');
const FILES = args.length ? args : ['D:\\nexus-test-speech\\normal.wav'];
const LOG = process.env.LOG || 'D:\\nexus-isolated.log';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (name, ok, detail = '') => { if (!ok) failed++; console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  -> ' + detail : ''}`); };
const norm = (t) => t.toLowerCase().replace(/[^a-z0-9' ]+/g, ' ').split(/\s+/).filter(Boolean);

const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const t = list.find((x) => x.url.includes('stage.html'));
if (!t) { console.log('FAIL  no stage page (is the isolated app running?)'); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let id = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const evalJs = (expression) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); }).then((r) => { if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'eval failed'); return r.result?.result?.value; });
const inv = (cmd, a = {}) => evalJs(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(a)}).then(v => JSON.stringify(v === undefined ? null : v)).catch(e => "ERR " + e)`);
const j = (s) => { try { return JSON.parse(s); } catch { return s; } };

// recorder inside the page: partial events, transcript events and the pill text
await evalJs(`(() => {
  if (window.__cap) return 'already';
  const cap = window.__cap = { t0: performance.now(), partials: [], finals: [], pill: [] };
  const listen = (event, fn) => window.__TAURI_INTERNALS__.invoke('plugin:event|listen', { event, target: { kind: 'Any' }, handler: window.__TAURI_INTERNALS__.transformCallback(fn) });
  listen('stt:partial', (e) => cap.partials.push({ at: performance.now(), p: e.payload }));
  listen('stt:transcript', (e) => cap.finals.push({ at: performance.now(), p: e.payload }));
  setInterval(() => { const el = document.querySelector('.speech-pill-text'); cap.pill.push({ at: performance.now(), text: el ? el.textContent : null, tail: el ? (el.querySelector('.speech-pill-tail')?.textContent || '') : null }); }, 50);
  return 'installed';
})()`);

const s0 = j(await inv('get_settings'));
check('the isolated app answers', s0 && typeof s0 === 'object');
await inv('settings_patch', { patch: { sttLiveCaptions: true, localSttOnly: false } });
const s1 = j(await inv('get_settings'));
check('live captions are switched on for this run', s1.sttLiveCaptions === true);
fs.mkdirSync(DIR, { recursive: true });

const results = [];
for (const file of FILES) {
  const meta = JSON.parse(fs.readFileSync(file.replace(/\.wav$/i, '.words.json'), 'utf8'));
  await evalJs(`(() => { window.__cap.partials.length = 0; window.__cap.finals.length = 0; window.__cap.pill.length = 0; return 1; })()`);
  const logBefore = fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8').length : 0;
  // 1. a real capture starts (this is the moment the caption stream is asked for)
  const started = await inv('start_stt_capture', { origin: null, noSpeechMs: 6000 });
  check(`[${path.basename(file)}] a real capture starts`, !String(started).startsWith('ERR'), String(started));
  await sleep(500);
  // 2. the speech is "spoken" through the microphone path
  const target = path.join(DIR, `${Date.now()}-${path.basename(file)}`);
  const tWrite = await evalJs('performance.now()');
  fs.copyFileSync(file, target);
  // 3. wait for the final transcript (or give up)
  const audioMs = (fs.statSync(file).size - 44) / 32;
  let final = null;
  for (let i = 0; i < 200 && !final; i++) { await sleep(100); final = j(await evalJs('JSON.stringify(window.__cap.finals[0] || null)')); }
  await sleep(700);
  const cap = j(await evalJs('JSON.stringify(window.__cap)'));
  const log = (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8') : '').slice(logBefore);
  const playedLine = /\[FAKE-MIC\] playing[^\n]*/.exec(log)?.[0];
  check(`[${path.basename(file)}] the test speech was played into the microphone path`, !!playedLine, playedLine || 'no [FAKE-MIC] line in the log (was the app started with NEXUS_FAKE_MIC_DIR?)');

  const parts = cap.partials.map((x) => ({ ms: x.at - tWrite, text: x.p.text || '', stable: x.p.stable_words }));
  check(`[${path.basename(file)}] partial captions arrived`, parts.length >= 2, `${parts.length} partials`);
  if (!parts.length) continue;
  const first = parts[0].ms;
  const lastPartial = parts[parts.length - 1].ms;
  // the speech starts to play about 0..200 ms after the file appeared (the watcher polls every 200 ms)
  console.log(`   first caption ${Math.round(first)} ms after the speech file appeared; ${parts.length} updates; the speech is ${Math.round(audioMs)} ms long`);
  check(`[${path.basename(file)}] words appear WHILE the speech is still playing`, first < audioMs * 0.6, `first at ${Math.round(first)} ms of ${Math.round(audioMs)} ms`);
  check(`[${path.basename(file)}] more than one update arrives before the speech ends`, parts.filter((p) => p.ms < audioMs).length >= 2);
  // text only grows or is revised in its unconfirmed tail: words reported as confirmed must never change later
  let shownStable = []; let broken = 0;
  for (const p of parts) {
    const w = norm(p.text); const st = Math.min(p.stable ?? 0, w.length);
    for (let k = 0; k < Math.min(shownStable.length, w.length); k++) if (shownStable[k] !== w[k]) { broken++; break; }
    shownStable = w.slice(0, st).length >= shownStable.length ? w.slice(0, st) : shownStable;
  }
  check(`[${path.basename(file)}] a word reported as confirmed is never changed later`, broken === 0, `${broken} updates changed a confirmed word`);
  // what the page itself showed
  const pillTexts = cap.pill.filter((x) => x.text && x.at - cap.partials[0].at >= -50);
  const sawPartialInPill = pillTexts.some((x) => parts.some((p) => norm(p.text).slice(-5).join(' ') === norm(x.text).join(' ')));
  check(`[${path.basename(file)}] the pill in the page showed the live text`, sawPartialInPill, `${pillTexts.length} pill samples`);
  const tails = cap.pill.filter((x) => x.tail && x.tail.length > 0).length;
  console.log(`   the pill showed an unconfirmed (dimmed) tail in ${tails} of ${cap.pill.length} samples`);
  // per word lag (how long after it began to be spoken did it first show in the PAGE)
  const speechStartGuess = first - first; // the speech begins when the watcher picked the file up; word times are relative to that
  const lags = [];
  meta.words.forEach((wd, k) => {
    const hit = cap.pill.find((x) => x.text && norm(x.text).length > 0 && cap.partials.length && parts.find((p) => norm(p.text).length > k && p.ms <= x.at - tWrite + 5));
    const firstPartial = parts.find((p) => norm(p.text).length > k);
    if (firstPartial) lags.push(Math.round(firstPartial.ms - wd.startMs));
  });
  lags.sort((a, b) => a - b);
  console.log(`   word lag (event arrival minus the word's start; includes up to 200 ms of watcher pick-up): median ${lags[Math.floor(lags.length / 2)]} ms, worst ${lags[lags.length - 1]} ms`);
  const fin = final && (final.p.text || final.p.raw_text || '');
  check(`[${path.basename(file)}] the final transcript arrived`, !!final, String(fin).slice(0, 100));
  if (fin) {
    const ref = norm(meta.text); const got = norm(fin);
    const exact = ref.join(' ') === got.join(' ');
    console.log(`   spoken : ${meta.text}\n   final  : ${fin}\n   caption: ${parts[parts.length - 1].text}`);
    // KNOWN LIMITATION, reported not failed: the engine's last guess before the end of speech can still be wrong for a short
    // command (the correct words come in its end-of-speech flush, which is dropped on purpose so a late caption can never
    // overwrite the final text). The final transcript replaces it about a second later.
    const lastWords = norm(parts[parts.length - 1].text);
    const wrongWords = lastWords.filter((w, k) => w !== got[k]).length;
    console.log(`   last caption vs final transcript: ${wrongWords} of ${got.length} words differ (known limitation, see docs)`);
    results.push({ file, exact, firstMs: Math.round(first), updates: parts.length, medianLagMs: lags[Math.floor(lags.length / 2)] });
  }
  await sleep(9000); // the app really executes the command and answers aloud; captions are off while it speaks (by design), so wait
}
await inv('settings_patch', { patch: { sttLiveCaptions: false } });
ws.close();
fs.writeFileSync('D:\\nexus-test-speech\\e2e-results.json', JSON.stringify(results, null, 2));
console.log(`\n${failed === 0 ? 'all checks passed' : failed + ' FAILED'}`);
process.exit(failed ? 1 : 0);
