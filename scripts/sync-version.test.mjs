import test from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { checkVersions, readVersions, setVersions, tagToVersion } from "./sync-version.mjs";

function put(root, name, content) {
  const path = join(root, name);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

function fixture({ tauri = "0.1.0", cargo = "0.1.0", frontend = "0.1.0" } = {}) {
  const root = mkdtempSync(join(tmpdir(), "nexus-version-"));
  put(root, "src-tauri/tauri.conf.json", `{\n  "productName": "NEXUS",\n  "version": "${tauri}",\n  "bundle": { "x": { "version": "9.9.9" } }\n}\n`);
  put(
    root,
    "src-tauri/Cargo.toml",
    `[package]\nname = "nexus"\nversion = "${cargo}"\nedition = "2021"\n\n[dependencies]\nserde = { version = "1.0.0" }\n\n[build-dependencies]\ntauri-build = "2"\n`,
  );
  put(root, "frontend/package.json", `{\n  "name": "frontend",\n  "version": "${frontend}",\n  "dependencies": { "react": "18.3.1" }\n}\n`);
  return root;
}

test("consistent versions pass", () => {
  const root = fixture();
  assert.deepEqual(checkVersions(root), []);
  assert.deepEqual(readVersions(root), { tauri: "0.1.0", cargo: "0.1.0", frontend: "0.1.0" });
});

test("a file that disagrees is reported by name", () => {
  const root = fixture({ cargo: "0.2.0" });
  const problems = checkVersions(root);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /Cargo\.toml=0\.2\.0/);
});

test("a release tag that differs from the files is refused", () => {
  const root = fixture();
  assert.match(checkVersions(root, "v0.3.0").join("\n"), /tag v0\.3\.0 says 0\.3\.0/);
  assert.deepEqual(checkVersions(root, "v0.1.0"), []);
});

test("setVersions writes all three and touches nothing else", () => {
  const root = fixture();
  setVersions(root, "1.4.2");
  assert.deepEqual(readVersions(root), { tauri: "1.4.2", cargo: "1.4.2", frontend: "1.4.2" });
  // the nested bundle version, the dependency versions and the formatting stay as they were
  assert.match(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"), /"x": \{ "version": "9\.9\.9" \}/);
  const cargo = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
  assert.match(cargo, /serde = \{ version = "1\.0\.0" \}/);
  assert.match(cargo, /edition = "2021"/);
});

test("a dependency version is never mistaken for the package version", () => {
  const root = mkdtempSync(join(tmpdir(), "nexus-version-"));
  put(root, "src-tauri/tauri.conf.json", `{ "version": "0.1.0" }`);
  put(root, "frontend/package.json", `{ "version": "0.1.0" }`);
  put(root, "src-tauri/Cargo.toml", `[package]\nname = "nexus"\nedition = "2021"\n\n[dependencies]\nserde = { version = "1.0.0" }\nversion = "7.7.7"\n`);
  assert.throws(() => readVersions(root), /\[package\] has no version/);
});

test("bad versions and tags are rejected", () => {
  const root = fixture();
  assert.throws(() => setVersions(root, "1.2"), /not a version/);
  assert.throws(() => tagToVersion("latest"), /not a version/);
  assert.equal(tagToVersion("refs/tags/v2.0.1"), "2.0.1");
  assert.equal(tagToVersion("v1.0.0-rc.1"), "1.0.0-rc.1");
  const bad = fixture({ tauri: "banana" });
  assert.match(checkVersions(bad).join("\n"), /not a version/);
});

test("a version in a table before [package] is not read or rewritten", () => {
  const root = mkdtempSync(join(tmpdir(), "nexus-version-"));
  put(root, "src-tauri/tauri.conf.json", `{ "version": "0.1.0" }`);
  put(root, "frontend/package.json", `{ "version": "0.1.0" }`);
  put(root, "src-tauri/Cargo.toml", '[lib]\nversion = "7.7.7"\n\n[package]\nname = "nexus"\nversion = "0.1.0"\n');
  assert.equal(readVersions(root).cargo, "0.1.0");
  setVersions(root, "0.2.0");
  const cargo = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
  assert.ok(cargo.includes('[lib]\nversion = "7.7.7"'));
  assert.ok(cargo.includes('name = "nexus"\nversion = "0.2.0"'));
});
