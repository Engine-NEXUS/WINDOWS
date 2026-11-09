// Gate for the bundled logo files (offline guarantee + provenance).
//   node scripts/check-activity-icons.mjs             check
//   node scripts/check-activity-icons.mjs --backfill  add provenance entries for files that have none
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL("..", import.meta.url));
const dir = join(root, "frontend", "public", "activity-logos");
const sourcesPath = join(dir, "SOURCES.json");
const sources = existsSync(sourcesPath) ? JSON.parse(readFileSync(sourcesPath, "utf8")) : {};
const files = readdirSync(dir).filter((f) => f.endsWith(".svg")).map((f) => f.slice(0, -4));
const sha = (id) => createHash("sha256").update(readFileSync(join(dir, `${id}.svg`), "utf8")).digest("hex");
const problems = [];

if (process.argv.includes("--backfill")) {
  for (const id of files) {
    if (!sources[id]) sources[id] = { source: "existing (theSVG / Iconify / Lobe Icons / NEXUS)", sha256: sha(id) };
  }
  writeFileSync(sourcesPath, JSON.stringify(Object.fromEntries(Object.entries(sources).sort()), null, 2) + "\n");
}

for (const id of files) {
  const svg = readFileSync(join(dir, `${id}.svg`), "utf8");
  const refs = [...svg.matchAll(/(?:xlink:)?href\s*=\s*["']([^"']+)["']/gi)].map((m) => m[1]);
  if (!svg.includes("<svg")) problems.push(`${id}.svg is not an SVG`);
  if (/<script|<foreignObject|\son[a-z]+\s*=|javascript:|url\(\s*["']?https?:/i.test(svg)
      || refs.some((r) => !r.startsWith("#") && !/^data:image\/png;base64,/.test(r))) {
    problems.push(`${id}.svg has scripts or external references (the app must work offline)`);
  }
  const entry = sources[id];
  if (!entry) problems.push(`${id}.svg has no provenance entry in SOURCES.json (run --backfill after review)`);
  else if (entry.sha256 !== sha(id)) problems.push(`${id}.svg changed since its provenance entry (sha mismatch)`);
}
for (const id of Object.keys(sources)) if (!files.includes(id)) problems.push(`SOURCES.json lists ${id} but ${id}.svg is missing`);

// Every id the UI can ask for has a file.
const referenced = new Set(Object.keys(JSON.parse(readFileSync(join(root, "frontend/src/settings-sidebar/hub/activityIconColors.json"), "utf8"))));
const known = readFileSync(join(root, "frontend/src/notify-toast/ProviderLogo.tsx"), "utf8").match(/KNOWN_ICONS[\s\S]*?\]\)/)?.[0] ?? "";
for (const m of known.matchAll(/"([a-z0-9]+)"/g)) referenced.add(m[1]);
const island = readFileSync(join(root, "frontend/src/agent-island/agentIslandModel.ts"), "utf8");
for (const m of island.matchAll(/src:\s*"\/activity-logos\/([a-z0-9]+)\.svg"/g)) referenced.add(m[1]);
for (const id of referenced) if (!files.includes(id)) problems.push(`"${id}" is referenced by the UI but ${id}.svg does not exist`);

if (problems.length) {
  console.error(problems.map((p) => `  - ${p}`).join("\n"));
  console.error(`check-activity-icons: ${problems.length} problem(s)`);
  process.exit(1);
}
console.log(`check-activity-icons: ${files.length} logos, all with provenance, no external references.`);
