#!/usr/bin/env node
// Mutation check: apply each single-line mutation to a source file, run one named test, and require it to FAIL.
// The file is restored after every mutation (and on exit). Results go to a JSON file.
//
//   node scripts/mutate-check.mjs <spec.json> <out.json>
//
// spec.json: { "file": "src-tauri/src/x.rs", "cwd": "src-tauri", "targetDir": "D:/target",
//              "testArgs": ["--lib", "x::tests::name", "--", "--test-threads=1"],
//              "mutations": [ { "name": "...", "find": "exact text", "replace": "text" } ] }
import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const [specPath, outPath] = process.argv.slice(2);
const spec = JSON.parse(readFileSync(specPath, "utf8"));
// A mutation may name its own `file`; every touched file is restored after each mutation and on exit.
const originals = new Map();
const load = (f) => {
  if (!originals.has(f)) originals.set(f, readFileSync(f, "utf8"));
  return originals.get(f);
};
load(spec.file);
const restore = () => originals.forEach((text, f) => writeFileSync(f, text));
process.on("exit", restore);
process.on("SIGINT", () => process.exit(130));

const results = [];
for (const m of spec.mutations) {
  const file = m.file || spec.file;
  const original = load(file);
  const CRLF = String.fromCharCode(13, 10);
  const eol = (x) => (original.includes(CRLF) ? x.split(String.fromCharCode(13)).join("").split(String.fromCharCode(10)).join(CRLF) : x);
  const find = eol(m.find);
  const count = original.split(find).length - 1;
  if (count !== 1) {
    results.push({ name: m.name, status: "BAD_SPEC", detail: `find text occurs ${count} times` });
    continue;
  }
  writeFileSync(file, original.replace(find, () => eol(m.replace)));
  // spec.command (e.g. ["node", "scripts/check-window-opacity.mjs"]) runs a gate instead of a cargo test;
  // a non-zero exit means the mutation was caught.
  const r = spec.command
    ? spawnSync(spec.command[0], spec.command.slice(1), { cwd: spec.cwd || ".", encoding: "utf8", maxBuffer: 64 * 1024 * 1024 })
    : spawnSync("cargo", ["test", ...spec.testArgs], {
        cwd: spec.cwd,
        env: { ...process.env, CARGO_TARGET_DIR: spec.targetDir },
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
      });
  const out = (r.stdout || "") + (r.stderr || "");
  let status;
  if (!spec.command && !/test result/.test(out)) status = "COMPILE_ERROR";
  else if (r.status !== 0) status = "KILLED";
  else status = "SURVIVED";
  const panic = (out.match(/panicked at [^\n]*\n[^\n]*/) || [""])[0].replace(/\s+/g, " ").slice(0, 200);
  results.push({ name: m.name, status, detail: status === "KILLED" ? panic : "" });
  restore();
  console.log(`${status.padEnd(9)} ${m.name}`);
}
restore();
writeFileSync(outPath, JSON.stringify({ file: spec.file, test: (spec.command || spec.testArgs).join(" "), results }, null, 2));
const bad = results.filter((r) => r.status !== "KILLED");
console.log(`${results.length - bad.length}/${results.length} killed`);
process.exit(bad.length ? 1 : 0);
