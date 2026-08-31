#!/usr/bin/env node
// ==============================================================================
// NEXUS — Unified Cross-Platform Developer Command
// ==============================================================================
// One entry point for Windows, macOS, and Linux.
//
//   node nexus.mjs install — install prerequisites + build + global 'nexus' command
//   node nexus.mjs setup   — install prerequisites + build (no global command)
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

import { execSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { platform, arch } from "node:os";

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
  const result = spawnSync(cmd, args, {
    cwd,
    stdio: opts.stdio || "inherit",
    shell: opts.shell || false,
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

// ─── OS-specific prerequisite installation ───────────────────────────────────

function installWindows() {
  info("Installing prerequisites on Windows...");

  // Node.js
  if (!hasNpm()) {
    info("Installing Node.js via winget...");
    run("winget", ["install", "-e", "--id", "OpenJS.NodeJS", "--accept-source-agreements", "--accept-package-agreements"],
      { hint: "If winget fails, download from https://nodejs.org/" });
    // Refresh PATH for this session
    process.env.PATH = execSync('powershell -NoProfile -Command "[Environment]::GetEnvironmentVariable(\'PATH\', \'User\')"', { encoding: "utf-8" }).trim() + ";" + process.env.PATH;
  }

  // Rust
  if (!hasCargo()) {
    info("Installing Rust via rustup...");
    run("winget", ["install", "-e", "--id", "Rustlang.Rustup", "--accept-source-agreements", "--accept-package-agreements"],
      { hint: "If winget fails, run https://win.rustup.rs/x86_64" });
    process.env.PATH = process.env.USERPROFILE + "\\.cargo\\bin;" + process.env.PATH;
  }

  // Python
  if (!hasPython()) {
    info("Installing Python 3.12 via winget...");
    run("winget", ["install", "-e", "--id", "Python.Python.3.12", "--accept-source-agreements", "--accept-package-agreements"],
      { hint: "If winget fails, download from https://python.org/" });
    process.env.PATH = process.env.LOCALAPPDATA + "\\Programs\\Python\\Python312;" + process.env.PATH;
  }

  // LLVM/libclang (needed for bindgen-based crates)
  if (!process.env.LIBCLANG_PATH || !existsSync(process.env.LIBCLANG_PATH + "\\libclang.dll")) {
    const llvmCandidates = [
      "C:\\Program Files\\LLVM\\bin",
      "C:\\LLVM\\bin",
    ];
    let found = llvmCandidates.find(p => existsSync(p + "\\libclang.dll"));
    if (!found) {
      info("Installing LLVM via winget (for libclang)...");
      run("winget", ["install", "-e", "--id", "LLVM.LLVM", "--accept-source-agreements", "--accept-package-agreements"],
        { allowFail: true, hint: "If winget fails, install from https://github.com/llvm/llvm-project/releases" });
      found = "C:\\Program Files\\LLVM\\bin";
    }
    if (existsSync(found + "\\libclang.dll")) {
      process.env.LIBCLANG_PATH = found;
      ok(`LIBCLANG_PATH=${found}`);
    } else {
      warn("libclang.dll not found — Rust build may fail. Install LLVM manually.");
    }
  }

  // MSVC C++ Build Tools
  if (!has("link")) {
    warn("MSVC C++ Build Tools (link.exe) not found in PATH.");
    warn("If the Rust build fails, install via:");
    warn('  winget install Microsoft.VisualStudio.2022.BuildTools --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools"');
  }
}

function installMacOS() {
  info("Installing prerequisites on macOS...");

  // Homebrew
  if (!has("brew")) {
    info("Installing Homebrew...");
    run("/bin/bash", ["-c", 'NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'],
      { hint: "See https://brew.sh" });
  }

  // Node.js
  if (!hasNpm()) {
    info("Installing Node.js via brew...");
    run("brew", ["install", "node"], { hint: "Or download from https://nodejs.org/" });
  }

  // Rust
  if (!hasCargo()) {
    info("Installing Rust via rustup...");
    run("curl", ["--proto", "=https", "--tlsv1.2", "-sSf", "https://sh.rustup.rs", "-o", "/tmp/rustup-init.sh"],
      { stdio: "ignore" });
    run("sh", ["/tmp/rustup-init.sh", "-y"], { hint: "Or visit https://rustup.rs" });
    process.env.PATH = process.env.HOME + "/.cargo/bin:" + process.env.PATH;
  }

  // Python
  if (!hasPython()) {
    info("Installing Python 3.12 via brew...");
    run("brew", ["install", "python@3.12"], { hint: "Or download from https://python.org/" });
  }

  // Xcode Command Line Tools (needed for C/C++ compilation)
  if (!existsSync("/Library/Developer/CommandLineTools/usr/bin/clang")) {
    info("Installing Xcode Command Line Tools...");
    run("xcode-select", ["--install"], { allowFail: true, hint: "This opens a GUI installer — click Install." });
    warn("After the GUI installer finishes, re-run: nexus setup");
  }
}

function installLinux() {
  info("Installing prerequisites on Linux...");

  // Detect package manager
  let pm, pmInstall;
  if (has("apt-get")) {
    pm = "apt-get"; pmInstall = ["install", "-y"];
  } else if (has("dnf")) {
    pm = "dnf"; pmInstall = ["install", "-y"];
  } else if (has("yum")) {
    pm = "yum"; pmInstall = ["install", "-y"];
  } else if (has("pacman")) {
    pm = "pacman"; pmInstall = ["-S", "--noconfirm"];
  } else if (has("zypper")) {
    pm = "zypper"; pmInstall = ["install", "-y"];
  } else {
    err("No supported package manager found (apt/dnf/yum/pacman/zypper).");
    err("Install Rust, Node.js, Python 3.12, and the Tauri system deps manually.");
    err("See: https://tauri.app/start/prerequisites/");
    process.exit(1);
  }

  // System libraries for Tauri on Linux (webkit2gtk-4.1)
  const sysDeps = {
    "apt-get": ["libwebkit2gtk-4.1-dev", "libgtk-3-dev", "libayatana-appindicator3-dev", "librsvg2-dev", "libasound2-dev", "libssl-dev", "pkg-config"],
    "dnf":     ["webkit2gtk4.1-devel", "gtk3-devel", "libappindicator-gtk3-devel", "librsvg2-devel", "alsa-lib-devel", "openssl-devel", "pkgconf-pkg-config"],
    "yum":     ["webkit2gtk4.1-devel", "gtk3-devel", "libappindicator-gtk3-devel", "librsvg2-devel", "alsa-lib-devel", "openssl-devel", "pkgconfig"],
    "pacman":  ["webkit2gtk-4.1", "gtk3", "libappindicator-gtk3", "librsvg", "alsa-lib", "openssl", "pkgconf"],
    "zypper":  ["webkit2gtk4-devel", "gtk3-devel", "libappindicator3-devel", "librsvg2-devel", "alsa-devel", "libopenssl-devel", "pkg-config"],
  };

  info(`Installing system libraries via ${pm}...`);
  if (pm === "apt-get") run("sudo", [pm, "update"]);
  run("sudo", [pm, ...pmInstall, ...sysDeps[pm]],
    { hint: "If this fails, install the packages manually." });

  // Node.js
  if (!hasNpm()) {
    if (pm === "pacman") {
      run("sudo", [pm, ...pmInstall, "nodejs", "npm"]);
    } else {
      // Use NodeSource for a recent version
      info("Installing Node.js 20.x via NodeSource...");
      run("sudo", ["bash", "-c",
        `curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && ${pm} ${pmInstall.join(" ")} nodejs`],
        { shell: true, allowFail: true, hint: "Or download from https://nodejs.org/" });
    }
  }

  // Rust
  if (!hasCargo()) {
    info("Installing Rust via rustup...");
    run("curl", ["--proto", "=https", "--tlsv1.2", "-sSf", "https://sh.rustup.rs", "-o", "/tmp/rustup-init.sh"],
      { stdio: "ignore" });
    run("sh", ["/tmp/rustup-init.sh", "-y"], { hint: "Or visit https://rustup.rs" });
    process.env.PATH = process.env.HOME + "/.cargo/bin:" + process.env.PATH;
  }

  // Python
  if (!hasPython()) {
    const pyPkg = pm === "pacman" ? "python" : "python3";
    run("sudo", [pm, ...pmInstall, pyPkg, "python3-pip"], { hint: "Or download from https://python.org/" });
  }
}

// ─── Commands ────────────────────────────────────────────────────────────────

function cmdSetup() {
  console.log(`\n${C.bold}${C.cyan}╔═══════════════════════════════════════════════════════════╗${C.reset}`);
  console.log(`${C.bold}${C.cyan}║         NEXUS — Cross-Platform Setup & Build               ║${C.reset}`);
  console.log(`${C.bold}${C.cyan}╚═══════════════════════════════════════════════════════════╝${C.reset}\n`);

  // 1. Install OS prerequisites
  if (IS_WIN) installWindows();
  else if (IS_MAC) installMacOS();
  else if (IS_LINUX) installLinux();
  else { err(`Unsupported platform: ${platform()}`); process.exit(1); }

  // 2. Verify tools
  info("Verifying installed tools...");
  const checks = [
    ["npm", hasNpm, "Node.js"],
    ["cargo", hasCargo, "Rust"],
    ["python", hasPython, "Python"],
  ];
  let allOk = true;
  for (const [cmd, check, name] of checks) {
    if (check()) { ok(`${name} found`); }
    else { err(`${name} NOT found — install it manually and re-run`); allOk = false; }
  }
  if (!allOk) {
    err("Prerequisites missing. Please install them and re-run: nexus setup");
    process.exit(1);
  }

  // 3. Install frontend dependencies
  info("Installing frontend dependencies...");
  run("npm", ["--prefix", "frontend", "install"]);

  // 4. Install Worker dependencies (optional, for self-deploy)
  if (existsSync(join(ROOT, "server", "worker", "package.json"))) {
    info("Installing Cloudflare Worker dependencies...");
    run("npm", ["--prefix", "server/worker", "install"], { allowFail: true });
  }

  // 5. Install NLU Python dependencies
  info("Installing NLU server Python dependencies...");
  const nluReq = join(ROOT, "server", "nlu", "requirements.txt");
  if (existsSync(nluReq)) {
    run(pythonCmd(), ["-m", "pip", "install", "-r", nluReq],
      { allowFail: true, hint: "If pip fails, create a venv: python -m venv .venv && activate it" });
  }

  // 6. Verify NLU model is present
  const nluModel = join(ROOT, "server", "nlu", "model", "nexus_nlu.onnx");
  if (existsSync(nluModel)) {
    ok("NLU model found (committed in repo)");
  } else {
    warn("NLU model not found — NLU server will fail. Run: cd server/nlu && python train.py");
  }

  // 7. Build
  info("Building NEXUS...");
  cmdBuild();

  console.log(`\n${C.bold}${C.green}═════════════════════════════════════════════════════════════${C.reset}`);
  ok("NEXUS is set up and ready!");
  console.log(`${C.green}  • Start: ${IS_WIN ? "nexus start" : "./nexus start"}${C.reset}`);
  console.log(`${C.green}  • Dev:   ${IS_WIN ? "nexus dev" : "./nexus dev"}${C.reset}`);
  console.log(`${C.green}  • Hotkey: Ctrl+Shift+Space  •  Wake word: "NEXUS"${C.reset}`);
  console.log(`${C.bold}${C.green}═════════════════════════════════════════════════════════════${C.reset}\n`);
}

// ─── Global command installation ─────────────────────────────────────────────
// Makes `nexus` available from any directory (not just the repo root).

function cmdInstallGlobal() {
  info("Installing global 'nexus' command...");

  if (IS_WIN) {
    // Windows: create nexus.cmd in a directory that's already on PATH.
    // Prefer %USERPROFILE%\.local\bin (created if needed), fall back to
    // %LOCALAPPDATA%\Microsoft\WindowsApps (always on PATH on Win10+).
    const localBin = join(process.env.USERPROFILE || "", ".local", "bin");
    if (!existsSync(localBin)) mkdirSync(localBin, { recursive: true });

    const shimPath = join(localBin, "nexus.cmd");
    const shimContent = `@echo off\r\nnode "${join(ROOT, "nexus.mjs")}" %*\r\n`;
    writeFileSync(shimPath, shimContent);
    ok(`Created ${shimPath}`);

    // Add to user PATH if not already there
    const pathVar = execSync(
      'powershell -NoProfile -Command "[Environment]::GetEnvironmentVariable(\'PATH\',\'User\')"',
      { encoding: "utf-8" }
    ).trim();
    if (!pathVar.split(";").includes(localBin)) {
      info(`Adding ${localBin} to user PATH...`);
      execSync(
        `powershell -NoProfile -Command "[Environment]::SetEnvironmentVariable('PATH','${pathVar};${localBin}','User')"`,
        { stdio: "ignore" }
      );
      ok(`Added ${localBin} to user PATH (restart your terminal to pick it up)`);
    } else {
      ok(`${localBin} already on user PATH`);
    }
    warn("Restart your terminal for the global 'nexus' command to take effect.");

  } else {
    // Unix (macOS + Linux): symlink to /usr/local/bin/nexus (needs sudo)
    // Fall back to ~/.local/bin/nexus if no sudo.
    const target = join(ROOT, "nexus.mjs");
    const globalPaths = ["/usr/local/bin/nexus", "/opt/homebrew/bin/nexus"];
    let installed = false;

    for (const binPath of globalPaths) {
      if (existsSync(dirname(binPath)) || has("sudo")) {
        info(`Symlinking ${binPath} -> ${target}...`);
        const args = ["ln", "-sf", target, binPath];
        if (!has("sudo")) {
          run("ln", ["-sf", target, binPath], { allowFail: true });
        } else {
          run("sudo", args, { allowFail: true });
        }
        if (existsSync(binPath)) {
          ok(`Created ${binPath}`);
          installed = true;
          break;
        }
      }
    }

    if (!installed) {
      // Fall back to ~/.local/bin
      const localBin = join(process.env.HOME || "", ".local", "bin");
      if (!existsSync(localBin)) mkdirSync(localBin, { recursive: true });
      const shimPath = join(localBin, "nexus");
      run("ln", ["-sf", target, shimPath]);
      ok(`Created ${shimPath}`);
      warn(`Add ${localBin} to your PATH if not already there:`);
      warn(`  echo 'export PATH="${localBin}:$PATH"' >> ~/.bashrc  (or ~/.zshrc)`);
    }
  }

  ok("Global 'nexus' command installed. You can now run 'nexus' from any directory.");
}

function cmdInstall() {
  // Full install: setup (prerequisites + build) + global command
  cmdSetup();
  cmdInstallGlobal();

  console.log(`\n${C.bold}${C.green}═════════════════════════════════════════════════════════════${C.reset}`);
  ok("NEXUS installed! The 'nexus' command is now available globally.");
  console.log(`${C.green}  • Start:    nexus start${C.reset}`);
  console.log(`${C.green}  • Dev:      nexus dev${C.reset}`);
  console.log(`${C.green}  • Rebuild:  nexus build${C.reset}`);
  console.log(`${C.green}  • Diagnostics: nexus check${C.reset}`);
  console.log(`${C.green}  • Hotkey: Ctrl+Shift+Space  •  Wake word: "NEXUS"${C.reset}`);
  console.log(`${C.bold}${C.green}═════════════════════════════════════════════════════════════${C.reset}\n`);
}

function cmdBuild() {
  info("Building frontend (Vite)...");
  run("npm", ["--prefix", "frontend", "install"], { allowFail: true, stdio: "ignore" });
  run("npm", ["--prefix", "frontend", "run", "build"]);

  info("Building Rust release binary (custom-protocol)...");
  const cargoEnv = {};
  if (IS_WIN && process.env.LIBCLANG_PATH) cargoEnv.LIBCLANG_PATH = process.env.LIBCLANG_PATH;
  run("cargo", ["build", "--release", "--features", "custom-protocol"],
    { cwd: join(ROOT, "src-tauri"), env: cargoEnv,
      hint: "Make sure LIBCLANG_PATH is set (Windows) or LLVM is installed" });

  // Report binary location
  const ext = IS_WIN ? ".exe" : "";
  const binPath = join(ROOT, "src-tauri", "target", "release", `nexus${ext}`);
  if (existsSync(binPath)) {
    const sizeMB = (statSync(binPath).size / 1024 / 1024).toFixed(1);
    ok(`Binary built: ${binPath} (${sizeMB} MB)`);
  }
}

function cmdDev() {
  info("Starting Tauri dev mode (hot reload via Vite)...");
  // tauri dev runs the Vite dev server + the Rust app with devUrl
  // We need the tauri CLI — check for cargo-tauri or npx tauri
  const tauriCli = has("cargo-tauri") ? ["cargo", "tauri", "dev"]
    : ["npx", "--prefix", join(ROOT, "frontend"), "tauri", "dev"];
  run(tauriCli[0], tauriCli.slice(1), { cwd: join(ROOT, "src-tauri") });
}

function cmdRun() {
  const ext = IS_WIN ? ".exe" : "";
  const binPath = join(ROOT, "src-tauri", "target", "release", `nexus${ext}`);
  if (!existsSync(binPath)) {
    err("Release binary not found. Run: nexus build");
    process.exit(1);
  }
  info(`Launching NEXUS: ${binPath}`);
  run(binPath, [], { stdio: "ignore" });
}

function cmdCheck() {
  console.log(`\n${C.bold}${C.cyan}NEXUS Diagnostics${C.reset}\n`);
  let pass = 0, fail = 0;

  function check(name, fn) {
    try {
      const result = fn();
      if (result) { ok(name); pass++; }
      else { err(name); fail++; }
    } catch (e) {
      err(`${name}: ${e.message}`); fail++;
    }
  }

  // Tools
  check("Node.js / npm", () => hasNpm());
  check("Rust / cargo", () => hasCargo());
  check("Python", () => hasPython());

  // Frontend
  check("frontend/node_modules", () => existsSync(join(ROOT, "frontend", "node_modules")));
  check("frontend/dist", () => existsSync(join(ROOT, "frontend", "dist")));

  // Rust
  check("Rust release binary", () => {
    const ext = IS_WIN ? ".exe" : "";
    return existsSync(join(ROOT, "src-tauri", "target", "release", `nexus${ext}`));
  });

  // NLU
  check("NLU model present", () => existsSync(join(ROOT, "server", "nlu", "model", "nexus_nlu.onnx")));
  check("NLU requirements.txt", () => existsSync(join(ROOT, "server", "nlu", "requirements.txt")));

  // Worker
  check("Worker package.json", () => existsSync(join(ROOT, "server", "worker", "package.json")));

  // Wake word model
  check("Wake word ONNX model", () => existsSync(join(ROOT, "src-tauri", "resources", "oww", "nexus.onnx")));

  console.log(`\n${C.bold}${pass > 0 && fail === 0 ? C.green : C.yellow}${pass} passed, ${fail} failed${C.reset}\n`);
  if (fail > 0) process.exit(1);
}

function cmdClean() {
  info("Cleaning build artifacts...");
  const targets = [
    join(ROOT, "frontend", "dist"),
    join(ROOT, "src-tauri", "target"),
    join(ROOT, "target"),
  ];
  for (const t of targets) {
    if (existsSync(t)) {
      rmSync(t, { recursive: true, force: true });
      ok(`Removed ${t}`);
    }
  }
  ok("Clean complete.");
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

function cmdHelp() {
  console.log(`
${C.bold}NEXUS — Unified Cross-Platform Developer Command${C.reset}

${C.cyan}Usage:${C.reset}
  ${IS_WIN ? "nexus" : "./nexus"} <command> [options]

${C.cyan}Commands:${C.reset}
  ${C.green}install${C.reset}  Install prerequisites + build + global 'nexus' command (first time)
  ${C.green}setup${C.reset}    Install prerequisites + build (no global command)
  ${C.green}build${C.reset}    Build frontend + Rust release binary (custom-protocol)
  ${C.green}dev${C.reset}      Start Tauri dev mode (hot reload via Vite dev server)
  ${C.green}start${C.reset}    Launch the built release binary (alias for 'run')
  ${C.green}run${C.reset}      Launch the built release binary
  ${C.green}check${C.reset}    Run diagnostics (tools, frontend, Rust, NLU, Worker)
  ${C.green}clean${C.reset}    Remove build artifacts (target/, dist/)
  ${C.green}worker${C.reset}   Deploy the Cloudflare Worker (optional, self-host backend)
  ${C.green}help${C.reset}     Show this help

${C.cyan}Examples:${C.reset}
  ${IS_WIN ? "nexus" : "./nexus"} install     # first time: install everything + global command
  ${IS_WIN ? "nexus" : "./nexus"} start       # launch the built app
  ${IS_WIN ? "nexus" : "./nexus"} dev         # develop with hot reload
  ${IS_WIN ? "nexus" : "./nexus"} build       # rebuild after changes

${C.cyan}Environment:${C.reset}
  NEXUS_SERVER_URL   Cloudflare Worker URL (default: baked at build time)
  LIBCLANG_PATH      Path to LLVM bin (Windows, for bindgen)
  NEXUS_STT_PORT     STT port override (legacy, unused with in-process Moonshine)

${C.cyan}Notes:${C.reset}
  • STT uses Moonshine in-process (auto-downloads ONNX model on first use)
  • NLU server is lazy-started Python (BERT-Mini, model committed in repo)
  • TTS uses Fish Audio (set API key in Settings) with Web Speech fallback
  • Wake word uses openWakeWord (ONNX, pure Rust, model in repo)
`);
}

// ─── Main ────────────────────────────────────────────────────────────────────

const command = process.argv[2] || "help";

switch (command) {
  case "install": cmdInstall(); break;
  case "setup":   cmdSetup(); break;
  case "build":   cmdBuild(); break;
  case "dev":     cmdDev(); break;
  case "start":
  case "run":     cmdRun(); break;
  case "check":   cmdCheck(); break;
  case "clean":   cmdClean(); break;
  case "worker":  cmdWorker(); break;
  case "help":
  case "--help":
  case "-h":
    cmdHelp(); break;
  default:
    err(`Unknown command: ${command}`);
    cmdHelp();
    process.exit(1);
}
