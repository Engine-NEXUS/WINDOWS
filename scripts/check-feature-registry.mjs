#!/usr/bin/env node
// Every boolean switch in NexusSettings must appear on the Features page (featureRegistry.ts) or be listed, with a
// reason, in featureInternal.json. Every registry key must exist in NexusSettings (or be a documented loose key).
// A new setting cannot silently miss the page, and a row cannot point at a setting that does not exist.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const rust = readFileSync(join(root, "src-tauri/src/commands.rs"), "utf8").replace(/\r\n/g, "\n");
const hub = join(root, "frontend/src/settings-sidebar/hub");
const registry = readFileSync(join(hub, "featureRegistry.ts"), "utf8");
const internal = JSON.parse(readFileSync(join(hub, "featureInternal.json"), "utf8"));

const camel = (s) => s.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

const start = rust.indexOf("pub struct NexusSettings {");
if (start < 0) {
  console.error("check-feature-registry: NexusSettings not found in commands.rs");
  process.exit(2);
}
const end = rust.indexOf("\n}\n", start);
const body = rust.slice(start, end);
const fields = new Map(); // camelCase -> rust type
for (const m of body.matchAll(/^\s{4}pub ([a-z0-9_]+): ([^,]+),/gm)) fields.set(camel(m[1]), m[2].trim());
const bools = [...fields].filter(([, t]) => t === "bool").map(([k]) => k);

const rows = new Set([...registry.matchAll(/\bkey:\s*"([A-Za-z0-9]+)"/g)].map((m) => m[1]));
const skip = new Set(Object.keys(internal.internal));
const loose = new Set(Object.keys(internal.loose));

const problems = [];
for (const k of bools) if (!rows.has(k) && !skip.has(k)) problems.push(`boolean setting "${k}" is neither a Features row nor in featureInternal.json`);
for (const k of rows) if (!fields.has(k) && !loose.has(k)) problems.push(`Features row "${k}" is not a NexusSettings field (add it to featureInternal.json "loose" if it is read as a loose key)`);
for (const k of skip) if (!fields.has(k)) problems.push(`featureInternal.json lists "${k}", which is not a NexusSettings field any more`);
for (const k of skip) if (rows.has(k)) problems.push(`"${k}" is both a Features row and listed as internal`);

if (problems.length > 0) {
  console.error("check-feature-registry: FAILED");
  for (const p of problems) console.error("  - " + p);
  process.exit(1);
}
console.log(`check-feature-registry: ok (${rows.size} rows, ${bools.length} boolean settings, ${skip.size} internal)`);
