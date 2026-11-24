#!/usr/bin/env node
// Writes featureReadiness.json (registry id -> live-proof percent from docs/testing/scorecard-data.json) using the
// hand-kept map next to it. `--check` fails when the file is stale or a map entry matches no scorecard row.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const hub = join(root, "frontend/src/settings-sidebar/hub");
const score = JSON.parse(readFileSync(join(root, "docs/testing/scorecard-data.json"), "utf8"));
const map = JSON.parse(readFileSync(join(hub, "featureReadiness.map.json"), "utf8"));

export function build(score, map) {
  const out = {};
  const problems = [];
  for (const [id, needle] of Object.entries(map)) {
    const row = score.find((r) => r.feature.includes(needle));
    if (!row) { problems.push(`map entry "${id}" matches no scorecard row ("${needle}")`); continue; }
    out[id] = { feature: row.feature, live: row.L };
  }
  return { out, problems };
}

const { out, problems } = build(score, map);
const text = JSON.stringify(out, null, 2) + "\n";
const target = join(hub, "featureReadiness.json");
if (process.argv.includes("--check")) {
  let current = "";
  try { current = readFileSync(target, "utf8"); } catch { /* missing */ }
  if (current.replace(/\r\n/g, "\n") !== text) problems.push("featureReadiness.json is stale: run node scripts/gen-feature-readiness.mjs");
  if (problems.length) { console.error(problems.join("\n")); process.exit(1); }
  console.log(`Feature readiness: ${Object.keys(out).length} rows in sync with the scorecard.`);
} else {
  if (problems.length) { console.error(problems.join("\n")); process.exit(1); }
  writeFileSync(target, text);
  console.log(`Wrote ${Object.keys(out).length} rows.`);
}
