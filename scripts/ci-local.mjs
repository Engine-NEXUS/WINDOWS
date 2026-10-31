#!/usr/bin/env node
// Runs every check that .github/workflows/ci.yml runs, on this machine, so a push never finds out first.
//
//   node scripts/ci-local.mjs                 # everything (a few minutes; the Rust tests are the slow part)
//   node scripts/ci-local.mjs --only frontend,worker
//   node scripts/ci-local.mjs --fast          # skips the Rust test run (compile checks still run)
//   node scripts/ci-local.mjs --list          # print the checks and exit
//   node scripts/ci-local.mjs --ci            # also `npm ci` first (a clean install, as on the runner)
//   node scripts/ci-local.mjs --installer     # also the NSIS installer build of build-windows.yml (slow, ~15 min)
//
// Jobs: frontend, worker, voice (wake data + tools), python, rust.
// The Rust steps also FAIL on any compiler warning: a green build with warnings is not what you asked for.
// Exit code 0 only when every selected check passed. Nothing here starts, stops or rebuilds NEXUS.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const IS_WIN = process.platform === "win32";
const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const value = (n) => {
  const i = args.indexOf(n);
  return i >= 0 ? args[i + 1] : undefined;
};

/** @type {{job:string,name:string,cwd:string,cmd:string,noWarnings?:boolean,slow?:boolean,optIn?:boolean,info?:boolean}[]} */
const CHECKS = [];
const add = (job, name, cwd, cmd, extra = {}) => CHECKS.push({ job, name, cwd, cmd, ...extra });

// ── frontend-check ───────────────────────────────────────────────────────────
if (flag("--ci")) add("frontend", "npm ci", "frontend", "npm ci");
add("frontend", "TypeScript check", "frontend", "npx tsc --noEmit");
add("frontend", "Build frontend", "frontend", "npm run build");
add("frontend", "Frontend tests", "frontend", "npm test");
add("frontend", "Turn-end choke-point gate", "frontend", "node ../scripts/check-turn-ends.mjs");
add("frontend", "Hygiene gate self-test", "frontend", "node ../scripts/check-repo-hygiene.mjs --selftest");
add("frontend", "Hygiene gate", "frontend", "node ../scripts/check-repo-hygiene.mjs");
add("frontend", "Theme contrast gate", "frontend", "node ../scripts/check-theme-contrast.mjs");
add("frontend", "Hard-coded colour ratchet", "frontend", "node ../scripts/check-hardcoded-colors.mjs");
add("frontend", "IPC + event contract gate", "frontend", "node ../scripts/check-ipc-contract.mjs");
add("frontend", "Window opacity registry", ".", "node scripts/check-window-opacity.mjs");
add("frontend", "Work-card drift", ".", "node scripts/make-work-cards.mjs --check");

// ── worker-check ─────────────────────────────────────────────────────────────
if (flag("--ci")) add("worker", "npm ci", "server/worker", "npm ci");
add("worker", "TypeScript check", "server/worker", "npx tsc --noEmit");
add("worker", "Worker tests", "server/worker", "npm test");

// ── voice-accuracy-tools ─────────────────────────────────────────────────────
// CI has no wake_word_data/ (git-ignored), so it checks the model fingerprints only. The same here; the health of the
// local audio manifest is reported separately below and never fails the run.
add("voice", "Wake model manifest + data gate", ".", "python scripts/wake_data_foundation.py validate --clean-checkout");
add("voice", "Local wake audio manifest (informational)", ".", "python scripts/wake_data_foundation.py validate", { info: true });
for (const t of ["voice_eval_tools", "wake_dsp", "wake_score", "wake_improve", "wake_train_lib", "wake_label_audit", "wake_data_fetch", "wake_data_synth"]) {
  add("voice", `unittest ${t}`, ".", `python -m unittest scripts/test_${t}.py`);
}

// ── python-check ─────────────────────────────────────────────────────────────
const parses = (file) => `python -c "import ast; ast.parse(open('${file}', encoding='utf-8').read())"`;
add("python", "NLU server parses", "server", parses("nlu_server.py"));
add("python", "STT server parses", "server", parses("stt_server.py"));
add("python", "NLU train script parses", "server/nlu", parses("train.py"));
add(
  "python",
  "Sidecar modules parse",
  "server/sidecar",
  `python -c "import ast; [ast.parse(open(f, encoding='utf-8').read()) for f in ['sidecar.py','tts.py','oauth.py','db.py','n8n_client.py']]"`,
);
add(
  "python",
  "n8n blueprints are valid JSON",
  "server/n8n",
  `python -c "import json; [json.load(open(f, encoding='utf-8')) for f in ['master_supervisor.blueprint.json','email_summarize.blueprint.json','github_pr_check.blueprint.json','calendar_peek.blueprint.json','general_chat.blueprint.json']]"`,
);
add("python", "NLU data foundation", ".", "python server/nlu/data_foundation.py validate");
add("python", "Agent + memory unit tests", "server", "python -m pytest agent/tests memory/tests -q");

// ── rust-check-windows ───────────────────────────────────────────────────────
add("rust", "Cargo check (default)", "src-tauri", "cargo check", { noWarnings: true });
add("rust", "Cargo check (mock-wake)", "src-tauri", "cargo check --features mock-wake", { noWarnings: true });
add("rust", "Cargo check (shipped features, all targets)", "src-tauri", "cargo check --all-targets --features custom-protocol,admin-brain", { noWarnings: true });
add("rust", "Dead-code ratchet", ".", "node scripts/dead-code-audit.mjs");
add("rust", "Cargo test (lib + integration)", "src-tauri", "cargo test --tests --features custom-protocol,admin-brain -- --test-threads=1", { noWarnings: true, slow: true });

// ── build-windows.yml (opt-in) ───────────────────────────────────────────────
add("installer", "NSIS installer build", ".", "npx --yes @tauri-apps/cli build --bundles nsis --features custom-protocol", { noWarnings: true, slow: true, optIn: true });

if (flag("--list")) {
  for (const c of CHECKS) console.log(`${c.job.padEnd(10)} ${c.name}${c.optIn ? "  (only with --installer)" : ""}${c.slow ? "  [slow]" : ""}`);
  process.exit(0);
}

const only = (value("--only") ?? "").split(",").filter(Boolean);
const skip = (value("--skip") ?? "").split(",").filter(Boolean);
const selected = CHECKS.filter(
  (c) =>
    (!c.optIn || flag("--installer")) &&
    (only.length === 0 || only.includes(c.job)) &&
    !skip.includes(c.job) &&
    !(flag("--fast") && c.slow),
);

// The Rust build embeds frontend/dist; CI makes a placeholder, here the real build normally exists. The placeholder
// is only written when there is no build at all, and removed again at the end so it never ships in a real bundle.
let placeholder = null;
const distDir = join(ROOT, "frontend", "dist");
if (selected.some((c) => c.job === "rust") && !existsSync(join(distDir, "stage.html"))) {
  placeholder = join(distDir, "index.html");
  if (!existsSync(placeholder)) {
    mkdirSync(distDir, { recursive: true });
    writeFileSync(placeholder, "<!doctype html><title>ci</title>");
    console.log("(no frontend build found: wrote the same placeholder CI uses; it is removed at the end)");
  } else {
    placeholder = null;
  }
}

/** Compiler warnings in captured output, ignoring the summary and the build-failed echo. */
function warningLines(output) {
  return output
    .split(/\r?\n/)
    .filter((l) => /^warning(\[[^\]]+\])?: /.test(l))
    .filter((l) => !/generated \d+ warnings?/.test(l) && !/^warning: build failed/.test(l));
}

const results = [];
const t0 = Date.now();
for (const c of selected) {
  const start = Date.now();
  process.stdout.write(`▶ [${c.job}] ${c.name} ... `);
  const r = spawnSync(c.cmd, { cwd: join(ROOT, c.cwd), shell: true, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, env: { ...process.env, FORCE_COLOR: "0", NO_COLOR: "1" } });
  const out = `${r.stdout ?? ""}\n${r.stderr ?? ""}`;
  const secs = ((Date.now() - start) / 1000).toFixed(1);
  const warns = c.noWarnings ? warningLines(out) : [];
  const ok = r.status === 0 && warns.length === 0;
  if (c.info) {
    // Informational: local-only state that CI never sees. Shown, never counted as a failure.
    results.push({ ...c, ok: true, info: true, secs, warns, status: r.status });
    const localProblems = out.split(String.fromCharCode(10)).filter((l) => l.startsWith("ERROR")).length;
    console.log(ok ? `ok (${secs}s)` : `NOTE (${secs}s): ${localProblems} local data problem(s), not checked by CI`);
    continue;
  }
  results.push({ ...c, ok, secs, warns, status: r.status });
  if (ok) {
    console.log(`ok (${secs}s)`);
  } else {
    console.log(`FAILED (${secs}s)${r.status === 0 ? " - compiler warnings" : ` - exit ${r.status}`}`);
    if (warns.length) {
      console.log("   warnings:");
      for (const w of [...new Set(warns)].slice(0, 20)) console.log(`     ${w}`);
    }
    if (r.status !== 0) {
      const tail = out.split(/\r?\n/).filter((l) => l.trim()).slice(-25);
      for (const l of tail) console.log(`   | ${l}`);
    }
  }
}

if (placeholder) rmSync(placeholder, { force: true });
console.log("\n─── summary ───");
for (const r of results) console.log(`${r.ok ? "PASS" : "FAIL"}  [${r.job}] ${r.name} (${r.secs}s)`);
const failed = results.filter((r) => !r.ok);
const mins = ((Date.now() - t0) / 60000).toFixed(1);
if (failed.length === 0) {
  console.log(`\nALL ${results.length} CI CHECKS PASS (${mins} min)`);
  process.exit(0);
}
console.log(`\n${failed.length} of ${results.length} CI checks FAILED (${mins} min):`);
for (const f of failed) console.log(`  - [${f.job}] ${f.name}`);
process.exit(1);
