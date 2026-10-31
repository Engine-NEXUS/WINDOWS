#!/usr/bin/env node
// Hard-coded colour ratchet for the UI stylesheets.
//
// A hex, rgb(), rgba(), hsl() or hsla() value in a component stylesheet cannot follow the theme: it is right in one of
// Light/Dark and wrong in the other (the CodeRabbit panel, the Assistant view's white text, the dock buttons were all
// this). Colours belong in frontend/src/theme/tokens.css (the only file allowed to define them); everything else uses
// var(--ui-*) / var(--stage-*).
//
// This is a ratchet like the dead-code gate: scripts/baselines/hardcoded-colors.json records how many each stylesheet
// still has. A file may only go DOWN; a new stylesheet starts at zero; --update rewrites the baseline after a cleanup.
//
//   node scripts/check-hardcoded-colors.mjs           # check, exit 1 on any increase
//   node scripts/check-hardcoded-colors.mjs --update  # write the baseline (only ever shrinks unless --force)
//   node scripts/check-hardcoded-colors.mjs --list    # print every file's count, worst first

import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "frontend", "src");
const BASELINE = join(ROOT, "scripts", "baselines", "hardcoded-colors.json");
const ALLOWED = new Set(["frontend/src/theme/tokens.css"]);

function cssFiles(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...cssFiles(p));
    else if (name.endsWith(".css")) out.push(p);
  }
  return out;
}

export function countColors(css) {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return (text.match(/#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/g) ?? []).length;
}

const counts = {};
for (const f of cssFiles(SRC)) {
  const rel = relative(ROOT, f).split("\\").join("/");
  if (ALLOWED.has(rel)) continue;
  const n = countColors(readFileSync(f, "utf8"));
  if (n > 0) counts[rel] = n;
}

if (process.argv.includes("--list")) {
  for (const [f, n] of Object.entries(counts).sort((a, b) => b[1] - a[1])) console.log(String(n).padStart(5), f);
  console.log(String(Object.values(counts).reduce((a, b) => a + b, 0)).padStart(5), "TOTAL");
  process.exit(0);
}

const baseline = existsSync(BASELINE) ? JSON.parse(readFileSync(BASELINE, "utf8")) : {};

if (process.argv.includes("--update")) {
  const next = {};
  for (const [f, n] of Object.entries(counts)) {
    const was = baseline[f];
    if (was !== undefined && n > was && !process.argv.includes("--force")) {
      console.log(`refusing to raise ${f}: ${was} -> ${n} (clean it up, or pass --force on purpose)`);
      process.exit(1);
    }
    if (was === undefined && Object.keys(baseline).length && !process.argv.includes("--force")) {
      console.log(`refusing to add a new stylesheet ${f} (${n}) to the baseline`);
      process.exit(1);
    }
    next[f] = n;
  }
  writeFileSync(BASELINE, JSON.stringify(Object.fromEntries(Object.entries(next).sort()), null, 2) + "\n");
  console.log(`hard-coded colour baseline written: ${Object.keys(next).length} files, ${Object.values(next).reduce((a, b) => a + b, 0)} colours`);
  process.exit(0);
}

const worse = [];
const better = [];
for (const [f, n] of Object.entries(counts)) {
  const was = baseline[f] ?? 0;
  if (n > was) worse.push(`${f}: ${was} -> ${n}`);
}
for (const [f, was] of Object.entries(baseline)) {
  const n = counts[f] ?? 0;
  if (n < was) better.push(`${f}: ${was} -> ${n}`);
}
if (worse.length) {
  console.log("hard-coded colour gate FAILED: more hex/rgb/hsl values than the baseline allows:");
  for (const w of worse) console.log("  +", w);
  console.log("Use var(--ui-*) or var(--stage-*) from frontend/src/theme/tokens.css instead.");
  process.exit(1);
}
if (better.length) {
  console.log("hard-coded colour ratchet: these stylesheets improved. Shrink the baseline:");
  for (const b of better) console.log("  -", b);
  console.log("Run: node scripts/check-hardcoded-colors.mjs --update");
  process.exit(1);
}
console.log(`hard-coded colour gate OK (${Object.values(counts).reduce((a, b) => a + b, 0)} colours in ${Object.keys(counts).length} stylesheets, none new)`);
