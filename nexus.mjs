#!/usr/bin/env node
// ==============================================================================
// NEXUS — Unified Cross-Platform Developer Command
// ==============================================================================
// One entry point for Windows, macOS, and Linux.
//
//   node nexus.mjs install — prepare environment + global 'nexus' command
//   node nexus.mjs setup   — prepare prerequisites and dependencies (no global command)
//   node nexus.mjs build   — build frontend + Rust release binary
//   node nexus.mjs dev     — tauri dev (hot reload via Vite)
//   node nexus.mjs start   — launch the built release binary (alias: run)
//   node nexus.mjs run     — launch the built release binary
//   node nexus.mjs check   — diagnostics (tools, frontend, Rust, NLU)
//   node nexus.mjs clean   — remove build artifacts
//   node nexus.mjs worker  — deploy the Cloudflare Worker (optional)
//   node nexus.mjs help    — show this help
//
// On Unix:  ./nexus setup     (symlink/shim with shebang)
// On Win:   nexus setup       (nexus.cmd shim)
// ==============================================================================

import { execSync, spawn, spawnSync } from "node:child_process";
// Suppress the Node DEP0190 deprecation warning about shell:true with args.
// We control all arguments (no user input) — the warning is a false positive
// for our use case of launching .cmd shims like npm.cmd on Windows.
process.removeAllListeners("warning");
import { existsSync, mkdirSync, rmSync, statSync, writeFileSync, copyFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { platform, arch } from "node:os";
import { runContributor } from "./scripts/contributor-cli.mjs";

const __filename = fileURLToPath(import.meta.url);
const ROOT = resolve(dirname(__filename));
const IS_WIN = platform() === "win32";
const IS_MAC = platform() === "darwin";
const IS_LINUX = platform() === "linux";

// ─── Helpers ─────────────────────────────────────────────────────────────────

const C = {
  reset: "\x1b[0m", bold: "\x1b[1m", dim: "\x1b[2m",
  red: "\x1b[31m", green: "\x1b[32m", yellow: "\x1b[33m",
  blue: "\x1b[34m", magenta: "\x1b[35m", cyan: "\x1b[36m",
};

function log(tag, msg, color = "cyan") {
  const ts = new Date().toTimeString().slice(0, 8);
  console.error(`${C.dim}[${ts}]${C.reset} ${C[color]}${tag.padEnd(5)}${C.reset} ${msg}`);
}

function ok(msg)   { log("OK",   msg, "green"); }
function info(msg) { log("==>",  msg, "cyan"); }
function warn(msg) { log("WARN", msg, "yellow"); }
function err(msg)  { log("ERR",  msg, "red"); }

function run(cmd, args, opts = {}) {
  const cwd = opts.cwd || ROOT;
  const label = opts.label || `${cmd} ${args.join(" ")}`;
  if (opts.silent !== true) info(label);
  // On Windows, npm/cargo/python are .cmd/.bat/.exe shims. Node's spawnSync
  // cannot execute .cmd/.bat files without shell:true. We resolve the full
  // path via `where`, then use shell:true only when needed (.cmd/.bat).
  // For .exe files (cargo, python, nexus.exe) we use shell:false to avoid
  // the Node DEP0190 arg-injection deprecation warning.
  let resolvedCmd = cmd;
  let needsShell = opts.shell || false;
  if (IS_WIN && !opts.shell) {
    try {
      const lines = execSync(`where ${cmd}`, { stdio: "pipe", encoding: "utf-8" }).trim().split(/\r?\n/);
      const priority = [".cmd", ".bat", ".exe"];
      const best = lines.find(l => priority.some(ext => l.toLowerCase().endsWith(ext)));
      if (best) {
        resolvedCmd = best;
        // .cmd and .bat files require shell:true to execute
        if (best.toLowerCase().endsWith(".cmd") || best.toLowerCase().endsWith(".bat")) {
          needsShell = true;
        }
      } else if (lines[0]) {
        resolvedCmd = lines[0];
      }
    } catch {
      // `where` failed — cmd may be a full path already; fall through
    }
  }
  // When the resolved command is a .cmd/.bat with spaces in the path
  // (e.g. "C:\Program Files\nodejs\npm.cmd"), spawnSync with shell:true
  // fails because cmd.exe splits on spaces. In that case, use the bare
  // command name (npm, not the full path) — the directory is already on
  // PATH, so cmd.exe will find it.
  let result;
  let finalCmd = resolvedCmd;
  if (needsShell && IS_WIN && resolvedCmd.includes(" ")) {
    // Extract just the filename (npm.cmd -> npm) — cmd.exe resolves via PATH
    finalCmd = cmd; // use the original bare name, not the full path
  }
  result = spawnSync(finalCmd, args, {
    cwd,
    stdio: opts.stdio || "inherit",
    shell: needsShell,
    env: { ...process.env, ...opts.env },
    encoding: "utf-8",
  });
  if (result.status !== 0 && !opts.allowFail) {
    err(`Command failed (exit ${result.status}): ${label}`);
    if (opts.hint) warn(opts.hint);
    process.exit(result.status ?? 1);
  }
  return result;
}

function has(cmd) {
  try {
    execSync(`${IS_WIN ? "where" : "which"} ${cmd}`, { stdio: "ignore", shell: true });
    return true;
  } catch {
    return false;
  }
}

function hasNpm() { return has("npm"); }
function hasCargo() { return has("cargo"); }
function hasPython() { return has("python") || has("python3"); }
function pythonCmd() { return has("python") ? "python" : "python3"; }

// `nexus notify <scenario>`: run one notification scenario in the RUNNING NEXUS (second launch with a private flag;
// the running instance picks it up through its single-instance callback). Starts/stops/rebuilds nothing.
// Keep in sync with SCENARIOS in src-tauri/src/notify_selftest.rs (a Rust test checks every name appears here).
const NOTIFY_SCENARIOS = {
  "pr-failed": 'spoken "PR failed in selftest" + a Windows notification; click it for the animation and the CodeRabbit sidebar',
  "build-failed": 'spoken "Build failed in selftest-build" + a Windows notification',
  "still-failing": "NO new notification or speech; the open alert then shows 2 failing runs",
  "recovered": 'spoken "selftest is green again" + a Windows notification; detail shows Recovered',
  "expired-soon": 'notification now; clicking it after 15 s shows "This alert has expired."',
  "same-name": "two notifications alice/selftest and bob/selftest; each opens its own owner",
  "ambiguous": "spoken line only, no notification (a repo with several owners is not clickable)",
  "open": "no toast needed: plays the click animation and opens the latest alert in the sidebar",
};

function cmdNotify() {
  const name = process.argv[3];
  if (!name || name === "list" || name === "--help" || name === "-h") {
    console.log("Usage: nexus notify <scenario>   (NEXUS must already be running via 'nexus start')");
    console.log("Run in this order for the full cycle: pr-failed, still-failing, recovered.");
    console.log("");
    for (const [n, what] of Object.entries(NOTIFY_SCENARIOS)) console.log(`  ${n.padEnd(14)} ${what}`);
    return;
  }
  if (!(name in NOTIFY_SCENARIOS)) {
    err(`Unknown scenario '${name}'. Run: nexus notify list`);
    process.exit(1);
  }
  const ext = IS_WIN ? ".exe" : "";
  const binPath = join(ROOT, "src-tauri", "target", "release", `nexus${ext}`);
  if (!existsSync(binPath)) {
    err("Release binary not found. Run: nexus build");
    process.exit(1);
  }
  if (IS_WIN) {
    let running = "";
    try { running = execSync('tasklist /FI "IMAGENAME eq nexus.exe" /NH', { encoding: "utf-8" }); } catch { /* treated as not running */ }
    if (!/nexus\.exe/i.test(running)) {
      err("NEXUS is not running. Start it first with: nexus start");
      process.exit(1);
    }
  }
  info(`Sending scenario '${name}' to the running NEXUS (watch its console for [NOTIFY-TEST] lines)`);
  const child = spawn(binPath, ["--notify-test", name], { detached: true, stdio: "ignore" });
  child.unref();
}

function cmdWorker() {
  const workerDir = join(ROOT, "server", "worker");
  if (!existsSync(join(workerDir, "package.json"))) {
    err("Worker directory not found: server/worker/");
    process.exit(1);
  }
  info("Installing Worker dependencies...");
  run("npm", ["install"], { cwd: workerDir });

  if (!has("wrangler") && !existsSync(join(workerDir, "node_modules", ".bin", "wrangler"))) {
    err("wrangler not found. Run: npm install wrangler --save-dev (in server/worker)");
    process.exit(1);
  }

  info("Deploying Cloudflare Worker...");
  info("Make sure you have set up wrangler.toml + D1 database + OAuth secrets.");
  info("See: server/worker/README.md");
  run("npx", ["wrangler", "deploy"], { cwd: workerDir });
}

/// Train or retrain the BERT-Mini NLU model.
/// Runs the full pipeline: clean dataset → generate examples → merge →
/// train → export ONNX → sync to resources.
/// Contributors just run: nexus train
function cmdTrain() {
  const trainScript = join(ROOT, "server", "nlu", "train_all.py");
  if (!existsSync(trainScript)) {
    err("Training script not found: " + trainScript);
    err("Ensure you have the latest code: git pull");
    process.exit(1);
  }

  // Check Python is available
  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }

  // Check training dependencies (torch, transformers)
  info("Checking training dependencies (PyTorch, Transformers)...");
  const depCheck = spawnSync(py, ["-c", "import torch; import transformers; print('ok')"], {
    encoding: "utf-8",
  });
  if (depCheck.stdout?.trim() !== "ok") {
    info("Installing training dependencies...");
    const reqPath = join(ROOT, "server", "nlu", "requirements-train.txt");
    if (existsSync(reqPath)) {
      run(py, ["-m", "pip", "install", "-r", reqPath], {
        allowFail: true,
        hint: "If pip fails, create a venv: python -m venv .venv && activate it",
      });
    } else {
      warn("requirements-train.txt not found — install manually: pip install torch transformers onnx");
    }
  } else {
    ok("Training dependencies OK");
  }

  // Run the unified training pipeline
  info("Starting NLU training pipeline...");
  const args = [trainScript];
  // Pass through --clean-only or --skip-train if provided
  const extraArgs = process.argv.slice(3);
  args.push(...extraArgs);

  run(py, args, { cwd: join(ROOT, "server", "nlu") });

  console.log(`\n${C.bold}${C.green}═════════════════════════════════════════════════════════════${C.reset}`);
  ok("NLU training complete!");
  console.log(`${C.green}  The new model is synced to src-tauri/resources/.${C.reset}`);
  console.log(`${C.green}  Run '${IS_WIN ? "nexus" : "./nexus"} build' to bundle it into the installer.${C.reset}`);
  console.log(`${C.bold}${C.green}═════════════════════════════════════════════════════════════${C.reset}\n`);
}

function cmdAudit() {
  const auditScript = join(ROOT, "server", "nlu", "audit_nlu.py");
  if (!existsSync(auditScript)) {
    err("NLU audit script not found: " + auditScript);
    process.exit(1);
  }
  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }
  const extraArgs = process.argv.slice(3);
  info("Auditing NLU dataset and ONNX model...");
  run(py, [auditScript, ...extraArgs], { cwd: ROOT });
}

/// Collect real voice samples for NLU training.
/// Prompts you with phrases to speak, records your voice, transcribes via STT,
/// and saves the real transcripts as training data for BERT-Mini.
/// Contributors run: nexus collect
function cmdCollect() {
  const collectScript = join(ROOT, "scripts", "collect_nlu_samples.py");
  if (!existsSync(collectScript)) {
    err("Collection script not found: " + collectScript);
    err("Ensure you have the latest code: git pull");
    process.exit(1);
  }

  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }

  // Check audio recording deps
  info("Checking audio recording dependencies (sounddevice, numpy, scipy)...");
  const depCheck = spawnSync(py, ["-c", "import sounddevice; import numpy; import scipy; print('ok')"], {
    encoding: "utf-8",
  });
  if (depCheck.stdout?.trim() !== "ok") {
    info("Installing audio recording dependencies...");
    run(py, ["-m", "pip", "install", "sounddevice", "numpy", "scipy"], {
      allowFail: true,
      hint: "If pip fails, create a venv: python -m venv .venv && activate it",
    });
  } else {
    ok("Audio recording dependencies OK");
  }

  // Pass through all extra args to the Python script
  const extraArgs = process.argv.slice(3);
  info("Starting voice sample collector...");
  info("Make sure NEXUS is running (for STT) or GROQ_API_KEY is set.");
  run(py, [collectScript, ...extraArgs], { cwd: ROOT });
}

/// Verify every registered MCP server end to end (read-only, no sends).
/// Registry check + reachability probe + one read-only tool call +
/// one expected failure per server. Exit 0 only if all are reachable.
/// Contributors run: nexus mcp check
function cmdMcpCheck() {
  const checkScript = join(ROOT, "scripts", "mcp_check.py");
  if (!existsSync(checkScript)) {
    err("MCP check script not found: " + checkScript);
    process.exit(1);
  }
  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }
  run(py, [checkScript], { cwd: ROOT });
}

/// TEMPORARY ghost voice-entry tracer (delete after the fix lands).
/// Shows every pipeline stage and pinpoints the missing link — statically
/// (is the code there?) and from a real run log (did each stage fire?).
/// Read-only: touches no app code, no settings, no devices.
/// Contributors run: nexus trace-ghost [--log run.log]
function cmdTraceGhost() {
  const traceScript = join(ROOT, "scripts", "trace_ghost.py");
  if (!existsSync(traceScript)) {
    err("Trace script not found: " + traceScript);
    process.exit(1);
  }
  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }
  // Pass through --log <file> (and nothing else executes anything).
  run(py, [traceScript, ...process.argv.slice(3)], { cwd: ROOT });
}

/// Inspect training dataset category coverage, voice sample health, and targeted recommendations.
/// Contributors run: nexus stats
function cmdStats() {
  const statsScript = join(ROOT, "scripts", "nlu_stats.py");
  if (!existsSync(statsScript)) {
    err("NLU stats script not found: " + statsScript);
    process.exit(1);
  }
  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }
  const extraArgs = process.argv.slice(3);
  run(py, [statsScript, ...extraArgs], { cwd: ROOT });
}

/// Record wake word audio samples or view wake dataset statistics.
/// Examples:
///   nexus wake record 300
///   nexus wake record positive 300
///   nexus wake record negative 100
///   nexus wake stats
function cmdWake() {
  const wakeScript = join(ROOT, "scripts", "record_wake_samples.py");
  if (!existsSync(wakeScript)) {
    err("Wake recorder script not found: " + wakeScript);
    process.exit(1);
  }

  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }

  // Check audio recording dependencies
  info("Checking wake word recording dependencies (sounddevice, numpy, scipy)...");
  const depCheck = spawnSync(py, ["-c", "import sounddevice; import numpy; import scipy; print('ok')"], {
    encoding: "utf-8",
  });
  if (depCheck.stdout?.trim() !== "ok") {
    info("Installing audio recording dependencies...");
    run(py, ["-m", "pip", "install", "sounddevice", "numpy", "scipy"], {
      allowFail: true,
      hint: "If pip fails, create a venv: python -m venv .venv && activate it",
    });
  } else {
    ok("Audio recording dependencies OK");
  }

  const sub = process.argv[3]?.toLowerCase();
  let args = [];

  if (sub === "probe" || sub === "calibrate") {
    const probeScript = join(ROOT, "scripts", "probe_microphone.py");
    if (!existsSync(probeScript)) {
      err("Probe script not found: " + probeScript);
      process.exit(1);
    }
    info("Starting microphone hardware acoustic prober...");
    run(py, [probeScript], { cwd: ROOT });
    return;
  }

  // ── Phase E: data audit, reusable data, scoring (see docs/research/voice-accuracy/02-runbook-…) ──
  const scriptsDir = join(ROOT, "scripts");
  const needScript = (name) => {
    const f = join(scriptsDir, name);
    if (!existsSync(f)) {
      err("Script not found: " + f);
      process.exit(1);
    }
    return f;
  };
  const ensurePy = (mod, pipName, why) => {
    const chk = spawnSync(py, ["-c", `import ${mod}`], { encoding: "utf-8" });
    if (chk.status !== 0) {
      info(`Installing ${pipName} (${why})...`);
      run(py, ["-m", "pip", "install", pipName], { allowFail: true, hint: "If pip fails, create a venv: python -m venv .venv" });
    }
  };

  if (sub === "improve") {
    // nexus wake improve [--skip-fetch] [--skip-synth] [--promote]
    // One command: before-score -> data -> audit (stops for your by-ear review) -> train -> after-score -> verdict report.
    ensurePy("onnxruntime", "onnxruntime", "wake front end");
    ensurePy("torch", "torch", "classifier training");
    ensurePy("onnx", "onnx", "model export");
    run(py, [needScript("wake_improve.py"), ...process.argv.slice(4)], { cwd: ROOT, allowFail: true });
    return;
  }

  if (sub === "audit") {
    // nexus wake audit            -> inventory + out-of-fold scoring, writes the review queue
    // nexus wake audit review     -> listen to flagged clips (worst first), decide by ear
    // nexus wake audit status     -> how many flagged clips still need a decision (exit 1 while any do)
    ensurePy("onnxruntime", "onnxruntime", "wake front end");
    const act = process.argv[4]?.toLowerCase();
    const audit = needScript("wake_label_audit.py");
    if (act === "status") {
      run(py, [audit, "status"], { cwd: ROOT });
    } else if (act === "review") {
      ensurePy("sounddevice", "sounddevice", "audio playback");
      run(py, [needScript("wake_audition.py"), "review", "--queue", join(ROOT, "wake_word_data", "audit_flags.jsonl")], { cwd: ROOT });
    } else {
      info("Step 1/2: inventory (by origin; no Whisper)...");
      run(py, [audit, "inventory"], { cwd: ROOT });
      info("Step 2/2: out-of-fold scoring (finds negatives that sound like NEXUS, positives that do not)...");
      run(py, [audit, "score", ...process.argv.slice(4)], { cwd: ROOT });
    }
    return;
  }

  if (sub === "fetch") {
    // nexus wake fetch list | get <id…> [--max-gb N] | prepare [--max-hours N] | sources
    const act = process.argv[4] || "list";
    if (act === "prepare") {
      ensurePy("soundfile", "soundfile", "read LibriSpeech FLAC");
    }
    run(py, [needScript("wake_data_fetch.py"), act, ...process.argv.slice(5)], { cwd: ROOT });
    return;
  }

  if (sub === "synth" && ["voices", "positives", "negatives"].includes(process.argv[4])) {
    ensurePy("piper", "piper-tts", "synthetic wake-word voices (GPL-3 tool; its audio output is not affected)");
    run(py, [needScript("wake_data_synth.py"), ...process.argv.slice(4)], { cwd: ROOT });
    return;
  }

  if (sub === "score") {
    // nexus wake score [--model <candidate.onnx>] [--json before.json] [--baseline before.json] [--stage2]
    ensurePy("onnxruntime", "onnxruntime", "wake front end");
    run(py, [needScript("wake_score.py"), ...process.argv.slice(4)], { cwd: ROOT });
    return;
  }

  if (sub === "threshold" || sub === "verify") {
    // nexus wake threshold <wake_sweep.jsonl>     pick kws_threshold from a `wake_eval --sweep` file
    // nexus wake verify <wake_stage2.jsonl>       pick the speech-check policy from a `wake_eval --stage2` file
    const extra = process.argv.slice(4);
    run(py, [needScript("wake_calibrate.py"), ...extra, ...(sub === "verify" ? ["--verify"] : [])], { cwd: ROOT });
    return;
  }

  if (sub === "devices") {
    info("Note: these 'devices' are SYNTHETIC filters applied to your training clips (bluetooth band-pass, fan hum, far-field) - not real microphones. For real-device accuracy record with scripts/voice_eval_set.py on each mic.");
    run(py, [needScript("test_device_invariance.py"), ...process.argv.slice(4)], { cwd: ROOT });
    return;
  }

  if (sub === "compare" || (sub === "test" && process.argv.slice(4).includes("--compare"))) {
    const compareScript = join(ROOT, "scripts", "test_wake_compare.py");
    if (!existsSync(compareScript)) {
      err("Compare script not found: " + compareScript);
      process.exit(1);
    }
    const cleanArgs = process.argv.slice(4).filter(a => a !== "--compare");
    info("Starting simultaneous wake word & live ASR comparison...");
    run(py, [compareScript, ...cleanArgs], { cwd: ROOT });
    return;
  }

  if (sub === "replay") {
    // Source of truth for wake accuracy: the real Rust engine over recordings.
    if (!hasCargo()) {
      err("cargo not found: the replay harness runs the Rust wake engine.");
      process.exit(1);
    }
    let replayArgs = process.argv.slice(4);
    if (!replayArgs.some(a => a === "--manifest" || a === "--pos" || a === "--neg" || a === "--help" || a === "-h")) {
      replayArgs = ["--manifest", join(ROOT, "wake_word_data", "manifest.jsonl"), ...replayArgs];
    }
    info("Scoring the real wake engine over recordings (stage 1)...");
    run("cargo", ["run", "--release", "--quiet", "--example", "wake_replay", "--", ...replayArgs],
      { cwd: join(ROOT, "src-tauri") });
    return;
  }

  if (sub === "test" || sub === "live" || sub === "benchmark") {
    const testScript = join(ROOT, "scripts", "test_wake_live.py");
    if (!existsSync(testScript)) {
      err("Test script not found: " + testScript);
      process.exit(1);
    }
    const testArgs = process.argv.slice(4);
    if (sub === "benchmark" && !testArgs.includes("--batch")) {
      testArgs.push("--batch");
    }
    info("Starting wake word live tester / benchmark...");
    run(py, [testScript, ...testArgs], { cwd: ROOT });
    return;
  }

  if (sub === "train" || sub === "retrain") {
    const trainScript = join(ROOT, "scripts", "train_local_wakeword.py");
    if (!existsSync(trainScript)) {
      err("Train script not found: " + trainScript);
      process.exit(1);
    }
    info("Starting wake word neural classifier training (writes a CANDIDATE; add --promote to replace the live model, --dry-run to only count)...");
    run(py, [trainScript, ...process.argv.slice(4)], { cwd: ROOT });
    return;
  }

  if (sub === "synth-accents" || sub === "synth" || sub === "synthesize") {
    const synthScript = join(ROOT, "scripts", "synthesize_wake_accents.py");
    if (!existsSync(synthScript)) {
      err("Synth script not found: " + synthScript);
      process.exit(1);
    }
    const synthArgs = process.argv.slice(4);
    info("Starting multi-accent synthetic wake word generator (Alexa/Google standard)...");
    run(py, [synthScript, ...synthArgs], { cwd: ROOT });
    return;
  }

  if (sub === "ingest" || sub === "noise") {
    const ingestScript = join(ROOT, "scripts", "ingest_opensource_noise.py");
    if (!existsSync(ingestScript)) {
      err("Ingest script not found: " + ingestScript);
      process.exit(1);
    }
    info("Starting multi-source noise ingestion & screening...");
    run(py, [ingestScript], { cwd: ROOT });
    return;
  }

  if (!sub || sub === "record") {
    // Default to positive 300 if no extra arg, or check if next arg is a number/mode
    const nextArg = process.argv[4];
    const thirdArg = process.argv[5];
    if (!nextArg) {
      args = ["positive", "300"];
    } else if (!isNaN(Number(nextArg))) {
      args = ["positive", nextArg];
    } else if (["positive", "negative", "free", "background"].includes(nextArg.toLowerCase())) {
      args = [nextArg.toLowerCase(), thirdArg || "300"];
    } else {
      args = [nextArg, thirdArg || "300"];
    }
  } else if (["positive", "negative", "free", "background", "stats"].includes(sub)) {
    args = process.argv.slice(3);
  } else {
    err(`Unknown wake sub-command: ${sub}`);
    info("Usage:");
    info("  nexus wake probe                 (probe microphone hardware & calibrate acoustic profile)");
    info("  nexus wake test                  (real-time live microphone listening test)");
    info("  nexus wake test --compare        (simultaneous wake test & live speech transcription comparison)");
    info("  nexus wake test --batch          (benchmark model against all 3,000 dataset WAVs)");
    info("  nexus wake devices               (benchmark model across 5 SYNTHETIC mic filters - not real devices)");
    info("  nexus wake replay [--split test] (score the REAL Rust engine: recall, false fires/hour, thresholds)");
    info("  nexus wake improve [--promote]   (EVERYTHING: score shipped -> data -> audit -> train -> score candidate -> verdict report)");
    info("  nexus wake audit                 (find mislabeled clips by audio, not Whisper) / audit review / audit status");
    info("  nexus wake fetch list|get <id>|prepare   (permissively-licensed public speech/noise/TV data)");
    info("  nexus wake synth voices|positives|negatives   (Piper TTS voices: thousands of synthetic speakers)");
    info("  nexus wake train [--dry-run] [--promote]  (refuses until the audit queue is reviewed)");
    info("  nexus wake score [--model cand.onnx] [--json f] [--baseline f]   (REAL accuracy: recall + false alarms/hour)");
    info("  nexus wake threshold <sweep.jsonl> / verify <stage2.jsonl>   (pick threshold / speech-check policy from data)");
    info("  nexus wake synth-accents         (older: multi-accent edge-tts samples)");
    info("  nexus wake ingest                (ingest & screen open-source noise corpora)");
    info("  nexus wake record 300            (record 300 positive wake words)");
    info("  nexus wake record negative 100   (record 100 negative soundalikes)");
    info("  nexus wake stats                 (show sample counts & size)");
    process.exit(1);
  }

  info(`Starting wake recorder: ${args.join(" ")}...`);
  run(py, [wakeScript, ...args], { cwd: ROOT });
}

function cmdData() {
  const py = pythonCmd();
  if (!py) {
    err("Python not found. Install Python 3.12+ and add it to PATH.");
    process.exit(1);
  }

  const area = process.argv[3];
  const action = process.argv[4] || "validate";
  if (area === "nlu") {
    if (action === "import-clinc") {
      const importer = join(ROOT, "server", "nlu", "import_clinc150.py");
      run(py, [importer, ...process.argv.slice(5)], { cwd: ROOT });
      return;
    }
    const script = join(ROOT, "server", "nlu", "data_foundation.py");
    const allowed = new Set(["validate", "freeze-evaluation"]);
    if (!allowed.has(action)) {
      err(`Unknown NLU data action: ${action}`);
      process.exit(1);
    }
    run(py, [script, action, ...process.argv.slice(5)], { cwd: ROOT });
    return;
  }
  if (area === "wake") {
    const script = join(ROOT, "scripts", "wake_data_foundation.py");
    const mappedAction = action === "fingerprint" ? "fingerprint-models" : action;
    const allowed = new Set(["validate", "fingerprint-models"]);
    if (!allowed.has(mappedAction)) {
      err(`Unknown wake data action: ${action}`);
      process.exit(1);
    }
    run(py, [script, mappedAction, ...process.argv.slice(5)], { cwd: ROOT });
    return;
  }
  err("Usage: nexus data <nlu|wake> <validate|freeze-evaluation|import-clinc|fingerprint>");
  process.exit(1);
}

function cmdHelp() {
  console.log(`
${C.bold}NEXUS — Unified Cross-Platform Developer Command${C.reset}

${C.cyan}Usage:${C.reset}
  ${IS_WIN ? "nexus" : "./nexus"} <command> [options]

${C.cyan}Commands:${C.reset}
  ${C.green}install${C.reset}  Prepare environment + register a user-local command
  ${C.green}setup${C.reset}    Prepare environment only (no build or launch)
  ${C.green}build${C.reset}    Prepare + compile an isolated contributor release
  ${C.green}dev${C.reset}      Start Tauri dev mode (hot reload via Vite dev server)
  ${C.green}start${C.reset}    Launch the built release binary (alias for 'run')
  ${C.green}run${C.reset}      Launch the built release binary
  ${C.green}check${C.reset}    Read-only prerequisite and capability report (alias for doctor)
  ${C.green}clean${C.reset}    Remove only the selected profile output (no process termination)
  ${C.green}worker${C.reset}   Deploy the Cloudflare Worker (optional, self-host backend)
  ${C.green}train${C.reset}    Retrain the BERT-Mini NLU model (clean → generate → train → export)
  ${C.green}audit${C.reset}    Audit NLU data/model quality, coverage, leakage, slots, and OOS behavior
  ${C.green}collect${C.reset}  Collect real voice samples for NLU training (speak phrases → transcribe → save)
  ${C.green}wake${C.reset}     Record wake word audio samples for wake model training (nexus wake record 300)
  ${C.green}stats${C.reset}    Inspect dataset category coverage, voice health, and training recommendations
  ${C.green}data${C.reset}     Validate NLU provenance/evaluation locks and wake data/model manifests (structural only - no accuracy; use "wake score")
  ${C.green}mcp check${C.reset} Verify every registered MCP server end to end (read-only, no sends)
  ${C.green}notify${C.reset}   Trigger a test notification in the RUNNING app (nexus notify list)
  ${C.green}help${C.reset}     Show this help

${C.cyan}Examples:${C.reset}
  ${IS_WIN ? "nexus" : "./nexus"} install     # first time: install everything + global command
  ${IS_WIN ? "nexus" : "./nexus"} start       # launch the built app
  ${IS_WIN ? "nexus" : "./nexus"} dev         # develop with hot reload
  ${IS_WIN ? "nexus" : "./nexus"} build       # rebuild after changes
  ${IS_WIN ? "nexus" : "./nexus"} train       # retrain the NLU model with new data
  ${IS_WIN ? "nexus" : "./nexus"} stats       # inspect dataset coverage & weakest categories
  ${IS_WIN ? "nexus" : "./nexus"} audit       # audit the current NLU dataset and ONNX model
  ${IS_WIN ? "nexus" : "./nexus"} collect     # speak phrases → save real voice samples for training
  ${IS_WIN ? "nexus" : "./nexus"} wake record 300 # record 300 real wake word samples ("NEXUS")
  ${IS_WIN ? "nexus" : "./nexus"} wake stats  # inspect wake word audio dataset statistics
  ${IS_WIN ? "nexus" : "./nexus"} wake improve # the whole loop + a before/after verdict report to review
  ${IS_WIN ? "nexus" : "./nexus"} wake audit  # find poisoned/mislabeled wake clips, then "wake audit review"
  ${IS_WIN ? "nexus" : "./nexus"} wake score  # measure real wake-word recall + false alarms per hour
  ${IS_WIN ? "nexus" : "./nexus"} data nlu validate
  ${IS_WIN ? "nexus" : "./nexus"} data wake validate

${C.cyan}Contributor workflow:${C.reset}
  doctor/check       Read-only prerequisites and capability report
  test               Contributor CLI, frontend and Rust tests
  package            Create installers with the pinned Tauri CLI
  --profile contributor|personal   Default: contributor (separate data/output)
  --admin-brain      Opt-in, personal builds only
  --yes              Approve prerequisite installations (for unattended setup)
  --target / --bundles  Packaging options only

${C.cyan}Environment:${C.reset}
  NEXUS_SERVER_URL   Cloudflare Worker URL (default: baked at build time)
  LIBCLANG_PATH      Path to LLVM bin (Windows, for bindgen)
  NEXUS_STT_PORT     STT port override (default: 39217)

${C.cyan}Notes:${C.reset}
  • STT uses Moonshine Streaming local fallback + Groq Whisper cloud primary (lazy Python sidecar on port 39217)
  • NLU server is lazy-started Python (BERT-Mini, model committed in repo)
  • TTS uses edge-tts (cloud) with local Kokoro fallback
  • Wake word uses openWakeWord (ONNX, pure Rust, model in repo)
`);
}

function cmdTestWatch() {
  const script = join(ROOT, "scripts", "test_watch_live.ps1");
  const args = ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, ...process.argv.slice(3)];
  run("pwsh", args, { shell: false });
}

// ─── Main ────────────────────────────────────────────────────────────────────

const command = process.argv[2] || "help";

try {
switch (command) {
  case "doctor":
  case "test":
  case "package":
    process.exitCode = await runContributor(process.argv[2], process.argv.slice(3)); break;
  case "install":
    process.exitCode = await runContributor("install", process.argv.slice(3)); break;
  case "setup":
    process.exitCode = await runContributor("setup", process.argv.slice(3)); break;
  case "build":
    process.exitCode = await runContributor("build", process.argv.slice(3)); break;
  case "dev":
    process.exitCode = await runContributor("dev", process.argv.slice(3)); break;
  case "start":
  case "run":
    process.exitCode = await runContributor("start", process.argv.slice(3)); break;
  case "test-watch": cmdTestWatch(); break;
  case "check":
    process.exitCode = await runContributor("check", process.argv.slice(3)); break;
  case "clean":
    process.exitCode = await runContributor("clean", process.argv.slice(3)); break;
  case "worker":  cmdWorker(); break;
  case "train":   cmdTrain(); break;
  case "audit":   cmdAudit(); break;
  case "collect": cmdCollect(); break;
  case "wake":    cmdWake(); break;
  case "notify":  cmdNotify(); break;
  // TEMPORARY: ghost voice-entry tracer (see cmdTraceGhost).
  case "trace-ghost": cmdTraceGhost(); break;
  case "stats":
  case "status":
  case "coverage":
    cmdStats(); break;
  case "mcp":
    if (process.argv[3] === "check") { cmdMcpCheck(); break; }
    err("Usage: nexus mcp check");
    process.exit(1);
  case "data":    cmdData(); break;
  case "help":
  case "--help":
  case "-h":
    cmdHelp(); break;
  default:
    err(`Unknown command: ${command}`);
    cmdHelp();
    process.exit(1);
}
} catch (error) { err(error.message); process.exitCode = 1; }
