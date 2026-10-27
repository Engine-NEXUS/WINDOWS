#!/usr/bin/env node
// Release identity shared by `nexus start` and the direct Windows launcher.
// A missing or mismatched record fails closed before either launcher stops the app.
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync, renameSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const binary = join(root, "src-tauri", "target", "release", process.platform === "win32" ? "nexus.exe" : "nexus");
const recordPath = join(root, "src-tauri", "target", "release", "nexus-build.json");
const sourceRoots = ["src-tauri/src", "src-tauri/resources/server", "frontend/src", "frontend/public", "scripts"];
const sourceFiles = ["nexus.mjs", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/build.rs", "src-tauri/tauri.conf.json", "frontend/package.json", "frontend/package-lock.json", "frontend/index.html", "frontend/tsconfig.json", "frontend/tsconfig.node.json", "frontend/vite.config.ts",
  ...["melspectrogram.onnx", "embedding_model.onnx", "nexus.onnx", "model_manifest.json", "acoustic_profile.json", "comfort_embedding.npy", "nexus_config.yaml"].map(name => `src-tauri/resources/oww/${name}`)];
const excludedDirs = new Set(["node_modules", "target", "dist", "hand", ".git", "__pycache__", ".pytest_cache"]);

function hashFile(path) { return createHash("sha256").update(readFileSync(path)).digest("hex"); }
function walk(dir, files) {
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!excludedDirs.has(entry.name)) walk(join(dir, entry.name), files);
    } else if (entry.isFile()) {
      files.push(join(dir, entry.name));
    }
  }
}
function treeHash(files) {
  const digest = createHash("sha256");
  for (const file of [...files].filter(existsSync).sort()) {
    digest.update(relative(root, file).replaceAll("\\", "/"));
    digest.update("\0");
    digest.update(hashFile(file));
    digest.update("\n");
  }
  return digest.digest("hex");
}
export function sourceHash() {
  const files = sourceFiles.map(x => join(root, x));
  for (const dir of sourceRoots) walk(join(root, dir), files);
  return treeHash(files);
}
export function frontendHash() {
  const files = [];
  walk(join(root, "frontend", "dist"), files);
  if (!files.length) throw new Error("frontend/dist is missing; run nexus build");
  return treeHash(files);
}
export function recordBuild(expectedSource = sourceHash()) {
  if (!existsSync(binary)) throw new Error(`release executable missing: ${binary}`);
  const actualSource = sourceHash();
  if (actualSource !== expectedSource) throw new Error("source changed during build; rebuild after edits finish");
  const record = { schema: 1, builtAt: new Date().toISOString(), source: actualSource,
    frontend: frontendHash(), executable: hashFile(binary), executablePath: binary };
  record.identity = createHash("sha256").update(`${record.source}:${record.frontend}:${record.executable}`).digest("hex").slice(0, 16);
  const temp = `${recordPath}.${process.pid}.tmp`;
  writeFileSync(temp, JSON.stringify(record, null, 2) + "\n");
  renameSync(temp, recordPath);
  return record;
}
export function verifyBuild({ warnStale = false } = {}) {
  if (!existsSync(recordPath)) throw new Error("release build record missing; run nexus build or nexus start --build");
  if (!existsSync(binary)) throw new Error(`release executable missing: ${binary}`);
  const record = JSON.parse(readFileSync(recordPath, "utf8"));
  if (record.schema !== 1 || record.executablePath !== binary || !record.identity) throw new Error("release build record invalid; run nexus build");
  const warnings = [];
  const stale = (message) => { if (!warnStale) throw new Error(message); warnings.push(message); };
  if (record.source !== sourceHash()) stale("release source changed since build; run nexus build or nexus start --build");
  if (record.frontend !== frontendHash()) stale("frontend bundle changed since build; run nexus build or nexus start --build");
  if (record.executable !== hashFile(binary)) stale("release executable changed since build; run nexus build or nexus start --build");
  return warnings.length ? { ...record, warnings } : record;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const op = process.argv[2];
    if (op === "source") console.log(sourceHash());
    else if (op === "record") console.log(JSON.stringify(recordBuild(process.argv[3] || sourceHash())));
    else if (op === "verify") console.log(JSON.stringify(verifyBuild({ warnStale: process.argv.includes("--warn-stale") })));
    else throw new Error("usage: node scripts/build-provenance.mjs source|record [expected-source]|verify");
  } catch (error) {
    console.error(`NEXUS build check: ${error.message}`);
    process.exitCode = 1;
  }
}
