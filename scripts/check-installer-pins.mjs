#!/usr/bin/env node
// Installer download gate (plan 194, P2).
//
//   node scripts/check-installer-pins.mjs            checks the NSIS template; "UNSET" is a warning
//   node scripts/check-installer-pins.mjs --strict   release builds: "UNSET" fails
//
// Rules for src-tauri/installer/nexus-installer.nsi:
//   1. exactly one PYTHON_SHA256 define, either 64 hex characters or "UNSET";
//   2. every https URL used by the installer is on an allow-list of hosts;
//   3. the Python URL is https, on www.python.org and follows PYTHON_VERSION;
//   4. no download of a model from a moving branch ("/raw/main/", "/resolve/main/") comes back;
//   5. stale product text is not left in tauri.conf.json;
//   6. the template never runs "pip install <package>": it installs only from the bundled requirements file;
//   7. every line of that requirements file has an upper bound or "==", and the file is bundled in tauri.conf.json.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const ALLOWED_HOSTS = new Set([
  "www.python.org", // Python installer (hash-pinned)
  "go.microsoft.com", // WebView2 bootstrapper (Microsoft-signed)
  "github.com", // finish-page link only
]);

const STALE_TEXT = [/n8n/i, /ollama/i];
const REQUIREMENTS_RESOURCE = "resources/server/requirements-installer.txt";

export function checkNsi(text, { strict = false } = {}) {
  const errors = [];
  const warnings = [];
  const lines = text.split(/\r?\n/);

  const defines = lines
    .map((l) => /^\s*!define\s+PYTHON_SHA256\s+"([^"]*)"/.exec(l))
    .filter(Boolean)
    .map((m) => m[1]);
  if (defines.length !== 1) {
    errors.push(`expected exactly one PYTHON_SHA256 define, found ${defines.length}`);
  } else if (defines[0] === "UNSET") {
    (strict ? errors : warnings).push("PYTHON_SHA256 is UNSET: the Python installer download is not hash-checked");
  } else if (!/^[0-9a-fA-F]{64}$/.test(defines[0])) {
    errors.push(`PYTHON_SHA256 "${defines[0]}" is not 64 hex characters`);
  }

  const version = /^\s*!define\s+PYTHON_VERSION\s+"([^"]+)"/m.exec(text)?.[1];
  const pythonUrl = /^\s*!define\s+PYTHON_URL\s+"([^"]+)"/m.exec(text)?.[1];
  if (!version) errors.push("PYTHON_VERSION define is missing");
  if (!pythonUrl) {
    errors.push("PYTHON_URL define is missing");
  } else {
    if (!pythonUrl.startsWith("https://www.python.org/")) errors.push(`PYTHON_URL must be https://www.python.org/..., got ${pythonUrl}`);
    if (!pythonUrl.includes("${PYTHON_VERSION}")) errors.push("PYTHON_URL must be built from ${PYTHON_VERSION}");
  }

  lines.forEach((line, i) => {
    if (/^\s*;/.test(line)) return; // comments
    for (const m of line.matchAll(/https?:\/\/[^\s"'\\)]+/g)) {
      const url = m[0];
      let host;
      try {
        host = new URL(url.replace(/\$\{[^}]+\}/g, "x")).hostname;
      } catch {
        errors.push(`line ${i + 1}: unreadable URL ${url}`);
        continue;
      }
      if (url.startsWith("http://")) errors.push(`line ${i + 1}: plain http URL ${url}`);
      if (!ALLOWED_HOSTS.has(host)) errors.push(`line ${i + 1}: host ${host} is not on the installer allow-list`);
      if (/\/(raw|resolve)\/main\//.test(url)) errors.push(`line ${i + 1}: download from a moving branch: ${url}`);
    }
  });

  // Rule 6: pip installs only from the bundled requirements file.
  lines.forEach((line, i) => {
    if (/^\s*;/.test(line)) return;
    if (/DetailPrint|MessageBox/.test(line)) return; // log and message text, not a command
    const m = /\bpip\s+install\b(.*)$/i.exec(line);
    if (!m) return;
    const rest = `${m[1]} `;
    if (!/\s-r\s/.test(rest)) errors.push(`line ${i + 1}: pip install without -r <requirements file>: ${line.trim()}`);
    else if (!/requirements-installer\.txt/.test(rest)) errors.push(`line ${i + 1}: pip -r must name requirements-installer.txt`);
  });

  return { errors, warnings };
}

// Rule 7: every requirement is bounded, so "latest anything" can never be installed.
export function checkRequirements(text) {
  const errors = [];
  let count = 0;
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.replace(/#.*/, "").trim();
    if (!line) return;
    count++;
    if (/^(-|git\+|https?:)/.test(line)) {
      errors.push(`requirements line ${i + 1}: options and URLs are not allowed: ${line}`);
      return;
    }
    const bounded = line.includes("==") || /<\s*\d/.test(line) || line.includes("~=");
    if (!bounded) errors.push(`requirements line ${i + 1}: no upper bound or "==": ${line}`);
  });
  if (count === 0) errors.push("requirements-installer.txt has no requirements");
  return errors;
}

export function checkConfigText(confText) {
  const errors = [];
  let conf;
  try {
    conf = JSON.parse(confText);
  } catch (e) {
    return [`tauri.conf.json is not valid JSON: ${e.message}`];
  }
  if (!(conf.bundle?.resources ?? []).includes(REQUIREMENTS_RESOURCE)) {
    errors.push(`bundle.resources must include ${REQUIREMENTS_RESOURCE} (the installer reads it)`);
  }
  for (const key of ["shortDescription", "longDescription"]) {
    const v = conf.bundle?.[key] ?? "";
    for (const re of STALE_TEXT) if (re.test(v)) errors.push(`bundle.${key} still mentions a retired backend (${re}): "${v}"`);
  }
  return errors;
}

function main(argv) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const strict = argv.includes("--strict");
  const nsi = readFileSync(join(root, "src-tauri/installer/nexus-installer.nsi"), "utf8");
  const conf = readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8");
  const req = readFileSync(join(root, "src-tauri", REQUIREMENTS_RESOURCE), "utf8");
  const { errors, warnings } = checkNsi(nsi, { strict });
  errors.push(...checkConfigText(conf), ...checkRequirements(req));
  for (const w of warnings) console.warn(`WARN ${w}`);
  for (const e of errors) console.error(`FAIL ${e}`);
  if (errors.length) return 1;
  console.log(`OK installer pins${strict ? " (strict)" : ""}`);
  return 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exit(main(process.argv.slice(2)));
}
