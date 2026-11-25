#!/usr/bin/env node
// One version for the installer (plan 194, P1).
//
//   node scripts/sync-version.mjs --check            fail when the three files disagree
//   node scripts/sync-version.mjs --set 0.2.0        write the version into all three files
//   node scripts/sync-version.mjs --from-tag v0.2.0  same, taking the version from a release tag
//   node scripts/sync-version.mjs --check --tag v0.2.0   also fail when the tag differs from the files
//
// Files: src-tauri/tauri.conf.json, src-tauri/Cargo.toml ([package]), frontend/package.json.
// Cargo.lock follows Cargo.toml on the next cargo run; it is not edited here.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

const FILES = {
  tauri: "src-tauri/tauri.conf.json",
  cargo: "src-tauri/Cargo.toml",
  frontend: "frontend/package.json",
};

export function tagToVersion(tag) {
  const v = String(tag).trim().replace(/^refs\/tags\//, "").replace(/^v/, "");
  if (!SEMVER.test(v)) throw new Error(`tag "${tag}" is not a version like v1.2.3`);
  return v;
}

const JSON_VERSION = /("version"\s*:\s*")([^"]+)(")/;

function readJsonVersion(path) {
  const text = readFileSync(path, "utf8");
  const parsed = JSON.parse(text).version;
  if (typeof parsed !== "string") throw new Error(`${path}: no top-level "version"`);
  return parsed;
}

function writeJsonVersion(path, version) {
  const text = readFileSync(path, "utf8");
  const next = text.replace(JSON_VERSION, `$1${version}$3`);
  if (JSON.parse(next).version !== version) throw new Error(`${path}: could not set the top-level "version"`);
  writeFileSync(path, next);
}

// Position of the version line inside the [package] table only (never a dependency's version).
function cargoSpan(text) {
  const pkg = text.search(/^\[package\]\s*$/m);
  if (pkg < 0) throw new Error("Cargo.toml: no [package] table");
  const rest = text.slice(pkg + 9);
  const nextTable = rest.search(/^\[/m);
  const body = nextTable < 0 ? rest : rest.slice(0, nextTable);
  const m = /^(version\s*=\s*")([^"]+)(")/m.exec(body);
  if (!m) throw new Error("Cargo.toml: [package] has no version");
  const start = pkg + 9 + m.index;
  return { start, end: start + m[0].length, head: m[1], value: m[2], tail: m[3] };
}

export function readVersions(root) {
  return {
    tauri: readJsonVersion(join(root, FILES.tauri)),
    cargo: cargoSpan(readFileSync(join(root, FILES.cargo), "utf8")).value,
    frontend: readJsonVersion(join(root, FILES.frontend)),
  };
}

export function setVersions(root, version) {
  if (!SEMVER.test(version)) throw new Error(`"${version}" is not a version like 1.2.3`);
  writeJsonVersion(join(root, FILES.tauri), version);
  writeJsonVersion(join(root, FILES.frontend), version);
  const cargoPath = join(root, FILES.cargo);
  const text = readFileSync(cargoPath, "utf8");
  const s = cargoSpan(text);
  writeFileSync(cargoPath, text.slice(0, s.start) + s.head + version + s.tail + text.slice(s.end));
}

// Returns a list of problems (empty = consistent).
export function checkVersions(root, tag) {
  const v = readVersions(root);
  const problems = [];
  for (const [name, value] of Object.entries(v)) {
    if (!SEMVER.test(value)) problems.push(`${FILES[name]}: "${value}" is not a version like 1.2.3`);
  }
  const distinct = new Set(Object.values(v));
  if (distinct.size > 1) {
    problems.push(`versions disagree: tauri.conf.json=${v.tauri}, Cargo.toml=${v.cargo}, package.json=${v.frontend}`);
  }
  if (tag) {
    const want = tagToVersion(tag);
    if (v.tauri !== want) problems.push(`tag ${tag} says ${want} but tauri.conf.json says ${v.tauri}`);
  }
  return problems;
}

function main(argv) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const arg = (name) => {
    const i = argv.indexOf(name);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  try {
    if (argv.includes("--set") || argv.includes("--from-tag")) {
      const version = argv.includes("--set") ? arg("--set") : tagToVersion(arg("--from-tag"));
      setVersions(root, version);
      console.log(`version set to ${version} in 3 files`);
      return 0;
    }
    if (argv.includes("--check")) {
      const problems = checkVersions(root, arg("--tag"));
      if (problems.length) {
        for (const p of problems) console.error(`FAIL ${p}`);
        return 1;
      }
      console.log(`OK version ${readVersions(root).tauri} is the same in all 3 files`);
      return 0;
    }
    console.error("usage: sync-version.mjs --check [--tag vX.Y.Z] | --set X.Y.Z | --from-tag vX.Y.Z");
    return 2;
  } catch (e) {
    console.error(`FAIL ${e.message}`);
    return 1;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exit(main(process.argv.slice(2)));
}
