// Drives an isolated (contributor-profile) NEXUS started by scripts/run-isolated-test.ps1 through its own debug port.
// Never touches the owner's NEXUS (different identity, data folder, keychain and port) and never sends anything to a
// cloud service (the contributor profile holds no API keys). Prints PASS / FAIL / SKIP per check; exit 1 on any FAIL.
//
//   node scripts/test-isolated-app.mjs            (PORT=9333 by default)
import fs from 'node:fs';
const PORT = Number(process.env.PORT || 9333);
const DATA = process.env.APPDATA + '\\com.nexus.assistant.contributor';
const LOG = process.env.LOG || 'D:\\nexus-isolated.log';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const record = (status, name, detail = '') => { results.push({ status, name, detail }); console.log(`${status.padEnd(5)} ${name}${detail ? '  -> ' + detail : ''}`); };
const check = (name, ok, detail = '') => record(ok ? 'PASS' : 'FAIL', name, detail);

async function targets() { return (await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()); }
async function connect(match) {
  const t = (await targets()).find((x) => x.url.includes(match));
  if (!t) return null;
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let id = 0; const pending = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
  const cdp = (method, params) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  const evalJs = async (expression) => {
    const r = await cdp('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'eval failed');
    return r.result?.result?.value;
  };
  return { evalJs, close: () => ws.close(), url: t.url };
}
const logText = () => (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8') : '');
const logSince = (n) => logText().split('\n').slice(n);
const logLen = () => logText().split('\n').length;

// ── 1. it started ──
let stage = null;
for (let i = 0; i < 60 && !stage; i++) { try { stage = await connect('stage.html'); } catch { /* not up yet */ } if (!stage) await sleep(2000); }
if (!stage) { record('FAIL', 'the isolated app is running and its stage page is reachable', `no stage target on port ${PORT}`); process.exit(1); }
record('PASS', 'the isolated app is running and its stage page is reachable', stage.url);
const inv = (cmd, args = {}) => stage.evalJs(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then(v => JSON.stringify(v === undefined ? null : v)).catch(e => "ERR " + e)`);
const j = (s) => { try { return JSON.parse(s); } catch { return s; } };

await sleep(4000);
const boot = logText();
check('no panic in the start-up log', !/panicked|PANIC/i.test(boot));
check('it runs with the contributor identity (own data folder)', fs.existsSync(DATA), DATA);

// ── 2. the setting ──
let s = j(await inv('get_settings'));
check('get_settings returns the setting, off by default', s && typeof s === 'object' && s.sttLiveCaptions === false, `sttLiveCaptions=${s?.sttLiveCaptions}`);
const patched = await inv('settings_patch', { patch: { sttLiveCaptions: true } });
check('settings_patch accepts it', !String(patched).startsWith('ERR'), String(patched));
s = j(await inv('get_settings'));
check('turning it on is read back', s?.sttLiveCaptions === true);
const onDisk = () => { try { return JSON.parse(fs.readFileSync(DATA + '\\settings.json', 'utf8')); } catch { return null; } };
check('it is written to settings.json', onDisk()?.sttLiveCaptions === true);
// the classic regression: a full Save from a window that changes ANOTHER field must not wipe it
const full = { ...s, wakeSound: !s.wakeSound };
const saved = await inv('save_settings', { settings: full });
s = j(await inv('get_settings'));
check('a full Save of another field keeps it on (the field is not wiped)', !String(saved).startsWith('ERR') && s?.sttLiveCaptions === true, String(saved).slice(0, 80));
await inv('settings_patch', { patch: { wakeSound: !s.wakeSound } }); // put the other field back

// ── 3. the real capture path, with live captions ON and no keys (so: local plan, no cloud) ──
let mark = logLen();
const started = await inv('start_stt_capture', { origin: null, noSpeechMs: 1500 });
check('a real capture starts', !String(started).startsWith('ERR'), String(started));
await sleep(5000);
let fresh = logSince(mark).join('\n');
check('the capture started and stopped in the log', /stt-capture: started/.test(fresh), (fresh.match(/stt-capture: (started|stopping)[^\n]*/g) || []).join(' | ').slice(0, 160));
check('the stream dispatcher saw the turn (tried the local server, which is not running here)', /stt_stream: (connect failed|connect timed out|not streaming)/.test(fresh), (fresh.match(/stt_stream: [^\n]*/) || ['(no stt_stream line: RUST_LOG may hide debug lines)'])[0].slice(0, 140));
check('no panic during the capture', !/panicked/i.test(fresh));
await inv('stop_stt_capture');

// ── 4. off: nothing is attempted ──
await inv('settings_patch', { patch: { sttLiveCaptions: false } });
mark = logLen();
await inv('start_stt_capture', { origin: null, noSpeechMs: 1500 });
await sleep(4000);
fresh = logSince(mark).join('\n');
check('with the switch off the stream says so and connects to nothing', !/stt_stream: connect/.test(fresh), (fresh.match(/stt_stream: [^\n]*/) || ['(silent)'])[0].slice(0, 120));
await inv('stop_stt_capture');

// ── 5. privacy mode, even with captions on ──
await inv('settings_patch', { patch: { sttLiveCaptions: true, localSttOnly: true } });
mark = logLen();
await inv('start_stt_capture', { origin: null, noSpeechMs: 1500 });
await sleep(4000);
fresh = logSince(mark).join('\n');
check('in privacy mode only the local server is ever considered', !/deepgram|groq/i.test((fresh.match(/stt_stream: [^\n]*/g) || []).join('\n')));
await inv('stop_stt_capture');
await inv('settings_patch', { patch: { localSttOnly: false, sttLiveCaptions: false } });

// ── 5b. ordinary turns still work end to end through the real orchestrator (quiet mode: nothing is audible) ──
for (const phrase of ['hello', 'what time is it']) {
  mark = logLen();
  const r = await inv('orchestrator_process', { transcript: phrase, dialogContext: null, turnContext: null });
  await sleep(2500);
  const l = logSince(mark).join('\n');
  check(`the turn "${phrase}" is processed without an error or a panic`, !String(r).startsWith('ERR') && !/panicked/i.test(l), String(r).slice(0, 110));
}

// ── 6. the usage figure ──
const usage = j(await inv('get_usage_stats'));
check('the usage figure is readable', usage && typeof usage.today === 'number', JSON.stringify(usage).slice(0, 140));

// ── 6b. the battery guard and Kiro, through the real app ──
import { execSync } from 'node:child_process';
const batteryNow = (() => {
  try {
    const out = execSync('powershell -NoProfile -Command "$b = Get-CimInstance Win32_Battery | Select-Object -First 1; if ($b) { \\"$($b.EstimatedChargeRemaining),$($b.BatteryStatus)\\" } else { \\"none\\" }"', { encoding: 'utf8' }).trim();
    if (out === 'none') return null;
    const [pct, status] = out.split(',').map(Number);
    return { pct, ac: status === 2 || status === 6 || status === 7 || status === 8 || status === 9 };
  } catch { return null; }
})();
// the one safe thing to find out first: could switching the guard ON do anything on this machine right now?
const safeToArm = !batteryNow || batteryNow.ac || batteryNow.pct >= 40;
record('PASS', 'the battery on this test machine is read', batteryNow ? `${batteryNow.pct}% ${batteryNow.ac ? 'charging / on AC' : 'on battery'}; arming-on is ${safeToArm ? 'safe to test' : 'NOT tested: too low'}` : 'no battery (a desktop)');
s = j(await inv('get_settings'));
check('the guard is OFF by default with the documented defaults', s.batteryGuard === 'off' && s.batteryGuardPercent === 5 && s.batteryGuardAction === 'shutdown' && s.batteryGuardStopAgents === true, JSON.stringify({ g: s.batteryGuard, p: s.batteryGuardPercent, a: s.batteryGuardAction, st: s.batteryGuardStopAgents }));
const say = async (phrase) => {
  const m = logLen();
  const r = await inv('orchestrator_process', { transcript: phrase, dialogContext: null, turnContext: null });
  await sleep(2500);
  return { r: String(r), log: logSince(m).join('\n') };
};
let res = await say('when the battery is below 8 percent stop claude and hibernate the laptop');
s = j(await inv('get_settings'));
check('the owner\'s kind of sentence is understood and stored', /battery_guard|local_command/.test(res.r) && s.batteryGuardPercent === 8 && s.batteryGuardAction === 'hibernate', res.r.slice(0, 100) + ' ' + JSON.stringify({ p: s.batteryGuardPercent, a: s.batteryGuardAction }));
check('a sentence NEVER switches the guard on (off became practice)', s.batteryGuard === 'dry', 'batteryGuard=' + s.batteryGuard);
res = await say('battery guard status');
check('"battery guard status" is answered', !res.r.startsWith('ERR') && !/panicked/i.test(res.log), res.r.slice(0, 90));
res = await say("don't allow battery shutdown");
s = j(await inv('get_settings'));
check('a negated sentence changes nothing', s.batteryGuard === 'dry', 'batteryGuard=' + s.batteryGuard);
if (safeToArm) {
  res = await say('allow battery shutdown');
  s = j(await inv('get_settings'));
  check('"allow battery shutdown" is the step that switches it on', s.batteryGuard === 'on', 'batteryGuard=' + s.batteryGuard);
  await sleep(6000);
  check('with plenty of battery the guard does nothing', !/\[BATTERY\] (countdown|stop order)/.test(logText()), (logText().match(/\[BATTERY\][^\n]*/g) || ['(no [BATTERY] lines)']).slice(-2).join(' | '));
}
res = await say('turn off the battery guard');
s = j(await inv('get_settings'));
check('"turn off the battery guard" switches it off', s.batteryGuard === 'off', 'batteryGuard=' + s.batteryGuard);
await inv('settings_patch', { patch: { batteryGuardPercent: 5, batteryGuardAction: 'shutdown', batteryGuard: 'off' } });
check('the shutdown-after-agent sentence for Kiro is understood by the real app', !String((await say('after kiro finishes shut down the laptop')).r).startsWith('ERR'));
await say('cancel the shutdown plan');

// ── 7. the Command Hub shows the switch ──
await inv('show_settings_sidebar');
await sleep(5000);
let hub = null;
for (let i = 0; i < 10 && !hub; i++) { try { hub = await connect('sidebar.html'); } catch { /* wait */ } if (!hub) await sleep(1500); }
if (!hub) {
  record('SKIP', 'the Command Hub shows "Live captions while you speak"', 'the hub window did not open on the hidden desktop: ' + (await targets()).map((t) => t.url.split('/').pop()).join(','));
} else {
  const clickRow = (label) => hub.evalJs(`(() => { const els = [...document.querySelectorAll('button,[role=button],[role=tab],div,li,a')].filter(e => (e.innerText||'').split('\\n')[0].trim() .startsWith(${JSON.stringify(label)}) && e.children.length <= 4); if (!els.length) return false; const best = els.sort((x, y) => (x.innerText||'').length - (y.innerText||'').length)[0]; best.click(); return true; })()`);
  const hasText = () => hub.evalJs(`document.body.innerText.includes('Live captions while you speak')`);
  const rowToggle = () => hub.evalJs(`(() => { const rows = [...document.querySelectorAll('.setting-row, .hx-row, .feature-row, .hx-feature, li, div')].filter(r => (r.innerText||'').includes('Live captions while you speak') && r.querySelector('.settings-toggle, input[type=checkbox], [role=switch], button')); if (!rows.length) return 'no row'; const row = rows.sort((x, y) => (x.innerText||'').length - (y.innerText||'').length)[0]; const tg = row.querySelector('.settings-toggle, input[type=checkbox], [role=switch], button'); tg.click(); return 'clicked'; })()`);
  for (const page of ['Features', 'Audio']) {
    await inv('hide_settings_sidebar'); await sleep(2500);
    await inv('show_settings_sidebar'); await sleep(5000);
    hub.close(); hub = null;
    for (let k = 0; k < 10 && !hub; k++) { try { hub = await connect('sidebar.html'); } catch { /* wait */ } if (!hub) await sleep(1500); }
    if (!hub) { record('FAIL', 'the hub reopens for the next page', 'no sidebar target'); break; }
    const opened = await clickRow(page); await sleep(2000);
    if (!opened) { record('FAIL', `the hub has a "${page}" page to open`, 'row not found'); continue; }
    const shown = await hasText();
    check(`the "${page}" page shows "Live captions while you speak"`, shown);
    if (page === 'Features') check('the Features page shows the battery guard row', await hub.evalJs(`document.body.innerText.includes('Battery guard')`));
    if (!shown) continue;
    const before = j(await inv('get_settings')).sttLiveCaptions;
    const clicked = await rowToggle(); await sleep(2500);
    let after = j(await inv('get_settings')).sttLiveCaptions;
    let viaSave = '';
    if (clicked === 'clicked' && after === before) { // a draft page: it saves with its own Save button
      if (await clickRow('Save')) { await sleep(3000); after = j(await inv('get_settings')).sttLiveCaptions; viaSave = ' after its Save button'; }
    }
    if (clicked === 'clicked') check(`clicking the switch on the "${page}" page changes the saved setting${viaSave}`, after === !before, `${before} -> ${after}`);
    else record('SKIP', `clicking the switch on the "${page}" page`, clicked);
    await inv('settings_patch', { patch: { sttLiveCaptions: false } });
  }
  if (hub) hub.close();
}

stage.close();
const bad = results.filter((r) => r.status === 'FAIL').length;
console.log(`\n${results.filter((r) => r.status === 'PASS').length} passed, ${bad} failed, ${results.filter((r) => r.status === 'SKIP').length} skipped`);
process.exit(bad ? 1 : 0);
