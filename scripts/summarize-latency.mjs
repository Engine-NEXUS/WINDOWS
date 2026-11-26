// Baseline table from a NEXUS log: how long from the end of speech to the action, per stage.
//
//   node scripts/summarize-latency.mjs <log file> [more logs...]
//
// Reads the "[LAT-ACT] <kind> <confirmed|unconfirmed> | endwait .. | stt .. | parse .. | act .. | total .." lines.
// Only confirmed actions are timed; unconfirmed ones are counted. The log holds no words that were said on these lines.
import fs from 'node:fs';

const files = process.argv.slice(2);
if (!files.length) {
  console.error('usage: node scripts/summarize-latency.mjs <log file>...');
  process.exit(2);
}
const groups = new Map();
for (const file of files) {
  // Windows PowerShell redirects write UTF-16; everything else here is UTF-8
  const bytes = fs.readFileSync(file);
  const text = bytes[0] === 0xff && bytes[1] === 0xfe ? bytes.toString('utf16le') : bytes.toString('utf8');
  for (const line of text.split('\n')) {
    const m = line.match(/\[LAT-ACT\] (\w+) (confirmed|unconfirmed) \| (.*)/);
    if (!m) continue;
    const [, kind, state, rest] = m;
    if (!groups.has(kind)) groups.set(kind, { unconfirmed: 0, rows: [] });
    const g = groups.get(kind);
    if (state === 'unconfirmed') {
      g.unconfirmed++;
      continue;
    }
    g.rows.push(Object.fromEntries([...rest.matchAll(/(\w+) (\d+)ms/g)].map((x) => [x[1], Number(x[2])])));
  }
}
if (!groups.size) {
  console.log('No [LAT-ACT] lines found.');
  process.exit(1);
}
const pick = (v, q) => (v.length ? v[Math.min(v.length - 1, Math.ceil(q * v.length) - 1)] : '-');
for (const [kind, g] of groups) {
  console.log(`\n${kind}: ${g.rows.length} confirmed, ${g.unconfirmed} unconfirmed`);
  if (!g.rows.length) continue;
  console.log('stage     median     p90   (ms)');
  for (const stage of ['endwait', 'stt', 'parse', 'act', 'total']) {
    const v = g.rows.map((r) => r[stage]).filter((x) => x !== undefined).sort((a, b) => a - b);
    console.log(`${stage.padEnd(8)}${String(pick(v, 0.5)).padStart(8)}${String(pick(v, 0.9)).padStart(8)}`);
  }
}
