#!/usr/bin/env node
// P1 turn-end choke-point gate: every orb turn-end must flow through the
// ghost-aware path (endGhostTurn/endTurn/maybeGhostRelisten), never a raw
// store.reset(). Deliberate raw resets (silent park, explicit cancel,
// no-input timeout, first-run greeting) must carry a
// `turn-end:keep-raw` marker on the same line or the 2 lines above.
//
// Usage: node scripts/check-turn-ends.mjs  (exit 1 on violation)
// CI: frontend-check job.
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const FILES = [
  "frontend/src/audio/recorder.ts",
  "frontend/src/net/orchestrator.ts",
  "frontend/src/net/wsBridge.ts",
  "frontend/src/main.tsx",
  "frontend/src/App.tsx",
];

const RESET_RE = /reset\(\)/;
const ALLOW_RE = /turn-end:keep-raw|endGhostTurn|endTurn\(\)|maybeGhostRelisten/;

let violations = [];
for (const rel of FILES) {
  const lines = readFileSync(join(ROOT, rel), "utf8").split("\n");
  lines.forEach((line, i) => {
    const trimmed = line.trim();
    // Skip comments and doc text mentioning reset().
    if (trimmed.startsWith("//") || trimmed.startsWith("*") || trimmed.startsWith("/*")) return;
    if (!RESET_RE.test(line)) return;
    const ctx = [line, lines[i - 1] ?? "", lines[i - 2] ?? ""].join("\n");
    if (!ALLOW_RE.test(ctx)) violations.push(`${rel}:${i + 1}: ${line.trim()}`);
  });
}

if (violations.length > 0) {
  console.error("turn-end gate FAILED — raw reset() without ghost-aware path:");
  for (const v of violations) console.error(`  ${v}`);
  console.error("Fix: route through endGhostTurn(), or mark deliberate sites with turn-end:keep-raw.");
  process.exit(1);
}
console.log(`turn-end gate OK (${FILES.length} files scanned)`);
