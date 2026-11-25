// Drives the REAL battery-guard loop of an isolated (contributor-profile) debug NEXUS with a MADE-UP battery reading, so the
// owner can see whether it triggers without any power-off. Start the app with
//   NEXUS_FAKE_BATTERY_FILE=<file> scripts/run-isolated-test.ps1
// (debug builds only; while that variable is set the guard refuses to power anything off, whatever the mode).
// Never touches the owner's NEXUS. Prints PASS / FAIL per check; exit 1 on any FAIL.
//
//   node scripts/test-battery-guard-live.mjs
import fs from 'node:fs';
const PORT = Number(process.env.PORT || 9333);
const LOG = process.env.LOG || 'D:\\nexus-isolated.log';
const FAKE = process.env.NEXUS_FAKE_BATTERY_FILE || 'D:\\nexus-fake-battery.txt';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (name, ok, detail = '') => { if (!ok) failed++; console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  -> ' + detail : ''}`); };

const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const t = list.find((x) => x.url.includes('stage.html'));
if (!t) { console.log('FAIL  no stage page (is the isolated app running?)'); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let id = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const evalJs = (expression) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); }).then((r) => r.result?.result?.value);
const inv = (cmd, args = {}) => evalJs(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then(v => JSON.stringify(v === undefined ? null : v)).catch(e => "ERR " + e)`);
const say = (phrase) => inv('orchestrator_process', { transcript: phrase, dialogContext: null, turnContext: null });
const logText = () => (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8') : '');
const batteryLines = () => logText().split('\n').filter((l) => l.includes('[BATTERY]'));
const waitFor = async (re, from, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = batteryLines().slice(from).find((l) => re.test(l)); if (hit) return hit; await sleep(1000); } return null; };
const patch = (p) => inv('settings_patch', { patch: p });
const setBattery = (text) => fs.writeFileSync(FAKE, text);

if (!process.env.NEXUS_FAKE_BATTERY_FILE && !fs.existsSync(FAKE)) fs.writeFileSync(FAKE, '80');
await patch({ batteryGuard: 'off', batteryGuardPercent: 5, batteryGuardAction: 'shutdown', batteryGuardStopAgents: true });

// 1. practice mode at 4 percent on battery: it speaks what it WOULD do and does nothing
setBattery('80');
await patch({ batteryGuard: 'dry' });
await sleep(3000);
let n = batteryLines().length;
setBattery('4');
let hit = await waitFor(/say: Practice, sir: the battery is at 4 percent/, n);
check('practice mode at 4 percent says what it would do', !!hit, hit || '(no [BATTERY] practice line)');
check('practice mode starts no countdown', !batteryLines().slice(n).some((l) => /countdown started/.test(l)));

// 2. "on" with a charger connected: never acts
setBattery('4,ac');
await patch({ batteryGuard: 'on' });
await sleep(8000);
n = batteryLines().length;
await sleep(6000);
check('on a charger at 4 percent it does nothing', !batteryLines().slice(n).some((l) => /countdown|stop order|say:/.test(l)));

// 3. "on" on battery at 4 percent: warns, orders the stop, starts the 30 s countdown
setBattery('80');
await sleep(6000);
n = batteryLines().length;
setBattery('4');
hit = await waitFor(/countdown started: shutdown in 30 s/, n);
check('on, on battery at 4 percent: the countdown starts', !!hit, hit || '(no countdown)');
check('it said it is about to shut down and how to cancel', batteryLines().slice(n).some((l) => /say: The battery is at 4 percent, sir\..*Say cancel to stop me/.test(l)));

// 4. a spoken "cancel" stops it
const m2 = batteryLines().length;
const r = await say('cancel');
hit = await waitFor(/say: Cancelled, sir\. The battery is at 4 percent/, m2, 15000);
check('saying "cancel" during the countdown cancels it', !!hit, hit || String(r).slice(0, 100));
check('after the cancel nothing is powered off', !batteryLines().slice(m2).some((l) => /countdown finished/.test(l)));

// 5. the charger stands a second countdown down; and a full countdown (test reading) powers nothing off
setBattery('80');
await sleep(65000); // a steady charger or a recovered level re-arms the guard
const m3 = batteryLines().length;
setBattery('4');
hit = await waitFor(/countdown started: shutdown in 30 s/, m3);
check('after a recovery it can start again', !!hit, hit || '(no second countdown)');
const m4 = batteryLines().length;
setBattery('4,ac');
hit = await waitFor(/say: The charger is connected, sir\. I have stood down/, m4, 15000);
check('plugging in the charger during the countdown stands it down', !!hit, hit || '(no stand-down)');

setBattery('80');
await sleep(65000);
const m5 = batteryLines().length;
setBattery('4');
await waitFor(/countdown started/, m5);
hit = await waitFor(/test reading in use: the countdown ended, nothing is powered off/, m5, 45000);
check('a countdown left to run out only prints a line: with a made-up reading nothing is powered off', !!hit, hit || '(no end-of-countdown line)');

await patch({ batteryGuard: 'off' });
setBattery('80');
ws.close();
console.log(`\n${failed === 0 ? 'all checks passed' : failed + ' FAILED'}`);
process.exit(failed ? 1 : 0);
