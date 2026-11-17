import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const original = fileURLToPath(new URL("./build-provenance.mjs", import.meta.url));
function put(root, name, content) {
  const path = join(root, name);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}
function run(root, ...args) {
  return spawnSync(process.execPath, [join(root, "scripts", "build-provenance.mjs"), ...args], { encoding: "utf8" });
}

test("release identity refuses missing, changed source, bundle and executable", () => {
  const root = mkdtempSync(join(tmpdir(), "nexus-build-record-"));
  try {
    put(root, "scripts/build-provenance.mjs", "");
    copyFileSync(original, join(root, "scripts", "build-provenance.mjs"));
    put(root, "src-tauri/src/main.rs", "version one");
    put(root, "frontend/dist/index.html", "bundle one");
    put(root, `src-tauri/target/release/${process.platform === "win32" ? "nexus.exe" : "nexus"}`, "binary one");
    assert.equal(run(root, "verify").status, 1);
    const before = run(root, "source");
    assert.equal(before.status, 0);
    assert.equal(run(root, "record", before.stdout.trim()).status, 0);
    assert.equal(run(root, "verify").status, 0);
    put(root, "scripts/__pycache__/trial.cpython-312.pyc", "generated cache");
    put(root, "scripts/.pytest_cache/v/cache/nodeids", "generated test cache");
    assert.equal(run(root, "source").stdout.trim(), before.stdout.trim());
    assert.equal(run(root, "verify").status, 0);
    put(root, "src-tauri/resources/oww/embedding_model.onnx", "changed speaker model");
    assert.match(run(root, "verify").stderr, /source changed/);
    put(root, "src-tauri/src/main.rs", "version two");
    assert.match(run(root, "verify").stderr, /source changed/);
    assert.equal(run(root, "record", before.stdout.trim()).status, 1);
    const next = run(root, "source").stdout.trim();
    assert.equal(run(root, "record", next).status, 0);
    put(root, "frontend/dist/index.html", "bundle two");
    assert.match(run(root, "verify").stderr, /frontend bundle changed/);
    assert.equal(run(root, "record", next).status, 0);
    put(root, `src-tauri/target/release/${process.platform === "win32" ? "nexus.exe" : "nexus"}`, "binary two");
    assert.match(run(root, "verify").stderr, /release executable changed/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('profile provenance refuses another identity, feature set, architecture and output', () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-profile-record-'));
  try {
    put(root, 'scripts/build-provenance.mjs', '');
    copyFileSync(original, join(root, 'scripts/build-provenance.mjs'));
    put(root, 'src-tauri/src/main.rs', 'source');
    put(root, 'frontend/dist/index.html', 'frontend');
    put(root, `src-tauri/target/contributor/release/${process.platform === 'win32' ? 'nexus.exe' : 'nexus'}`, 'binary');
    const script = `
      import assert from 'node:assert/strict';
      import { recordBuild, verifyBuild } from './scripts/build-provenance.mjs';
      const directory = ${JSON.stringify(join(root, 'src-tauri/target/contributor/release'))};
      const metadata = {profile:'contributor', features:['custom-protocol','contributor'], arch:'x64', targetDir:directory};
      const recorded = recordBuild(undefined, {directory, metadata});
      assert.equal(recorded.schema, 2);
      assert.equal(verifyBuild({directory, metadata}).identity, recorded.identity);
      for (const changed of [{profile:'personal'}, {features:['custom-protocol','admin-brain']}, {arch:'arm64'}]) {
        assert.throws(() => verifyBuild({directory, metadata:{...metadata,...changed}}), /metadata mismatch/);
      }
      assert.throws(() => verifyBuild(), /missing/);
    `;
    const r = spawnSync(process.execPath, ['--input-type=module', '-e', script], { cwd: root, encoding: 'utf8' });
    assert.equal(r.status, 0, r.stdout + r.stderr);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
