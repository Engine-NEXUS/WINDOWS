// Native contributor workflow. Importing this module performs no setup or process operations.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync, mkdirSync, chmodSync, realpathSync, rmSync, lstatSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline/promises';
import { createConnection } from 'node:net';
import { createHash } from 'node:crypto';
import { sourceHash, recordBuild, verifyBuild } from './build-provenance.mjs';

export const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
export const RUST = '1.93.1';
export const PORTS = [39216, 39217, 39218, 39219, 39222];
export function resourceMap(resources) {
  const mapped = {};
  for (const resource of resources) {
    if (resource === 'resources/server/stt_server.py' || resource === 'resources/server/nlu_server.py') {
      mapped['../server/' + resource.split('/').at(-1)] = resource;
    } else mapped[resource] = resource.includes('*') ? resource.slice(0, resource.lastIndexOf('/') + 1) : resource;
  }
  return mapped;
}
export function options(args = []) {
  const result = { profile: 'contributor', yes: false, admin: false };
  for (let i = 0; i < args.length; i++) {
    if (['--profile', '--target', '--bundles'].includes(args[i]) && (!args[i + 1] || args[i + 1].startsWith('--'))) throw new Error(`Missing value for ${args[i]}`);
    if (args[i] === '--profile') result.profile = args[++i];
    else if (args[i] === '--yes') result.yes = true;
    else if (args[i] === '--admin-brain') result.admin = true;
    else if (args[i] === '--target') result.target = args[++i];
    else if (args[i] === '--bundles') result.bundles = args[++i];
    else throw new Error(`Unknown option ${args[i]}. Build explicitly before start; --build is not supported.`);
  }
  if (!['contributor', 'personal'].includes(result.profile)) throw new Error('Profile must be contributor or personal.');
  if (result.admin && result.profile !== 'personal') throw new Error('--admin-brain requires --profile personal.');
  if (result.target && !/^(aarch64|x86_64)-(apple-darwin|pc-windows-msvc|unknown-linux-gnu)$/.test(result.target)) throw new Error('Unsupported packaging target.');
  if (result.bundles && !/^(nsis|msi|dmg|app|appimage|deb|rpm)(,(nsis|msi|dmg|app|appimage|deb|rpm))*$/.test(result.bundles)) throw new Error('Unsupported bundle format.');
  return result;
}
export function configuration(root, opt, platform = process.platform, arch = process.arch) {
  const id = `com.nexus.assistant${opt.profile === 'contributor' ? '.contributor' : ''}`;
  const targetDir = join(root, 'src-tauri', `target-${opt.profile}`);
  const python = join(root, '.venv', platform === 'win32' ? 'Scripts/python.exe' : 'bin/python');
  const config = { identifier: id, productName: opt.profile === 'contributor' ? 'NEXUS Contributor' : 'NEXUS',
    bundle: { macOS: { signingIdentity: null, providerShortName: null,
      ...(opt.profile === 'contributor' ? { infoPlist: 'Info.contributor.plist' } : {}) } },
    plugins: { 'deep-link': { desktop: { schemes: [opt.profile === 'contributor' ? 'nexus-dev' : 'nexus'] } } } };
  const baseConfig = join(root, 'src-tauri', 'tauri.conf.json');
  if (existsSync(baseConfig)) config.bundle.resources = resourceMap(JSON.parse(readFileSync(baseConfig, 'utf8')).bundle.resources);
  const features = [...(opt.profile === 'contributor' ? ['contributor'] : []), ...(opt.admin ? ['admin-brain'] : [])];
  return { ...opt, id, targetDir, python, config, platform, arch, features,
    binary: join(targetDir, 'release', platform === 'win32' ? 'nexus.exe' : 'nexus'),
    metadata: { profile: opt.profile, features: ['custom-protocol', ...features], platform, arch, targetDir,
      rust: RUST, managedPython: python } };
}
export function context(root = ROOT, overrides = {}) {
  const ctx = { root, platform: process.platform, arch: process.arch, env: { ...process.env },
    provenance: { sourceHash, recordBuild, verifyBuild },
    mkdir: path => mkdirSync(path, { recursive: true }),
    exists: existsSync, read: p => readFileSync(p, 'utf8'), log: console.log,
    exec: (cmd, args = [], opts = {}) => spawnSync(cmd, args, { cwd: root, encoding: 'utf8', windowsHide: true,
      timeout: 30000, ...opts, env: { ...ctx.env, ...opts.env }, shell: false }), ...overrides };
  ctx.probe ??= (cmd, args = []) => {
    const r = ctx.exec(cmd, args);
    return r.status === 0 ? `${r.stdout || ''}${r.stderr || ''}`.trim() : '';
  };
  ctx.run ??= (cmd, args, opts = {}) => {
    ctx.log(`> ${cmd} ${args.join(' ')}`);
    const r = ctx.exec(cmd, args, { stdio: 'inherit', timeout: undefined, ...opts });
    if (r.error || r.status !== 0) throw new Error(`${cmd} failed (${r.error?.message || r.status}). Fix the error above and rerun; no app was terminated.`);
    return r;
  };
  return ctx;
}
export function platformLabel(ctx) {
  if (ctx.platform === 'linux') {
    const os = ctx.exists('/etc/os-release') ? ctx.read('/etc/os-release') : '';
    const ubuntu = /^ID=ubuntu$/m.test(os) && /^VERSION_ID="24\.04"$/m.test(os);
    return ubuntu && ctx.arch === 'x64' && ctx.env.XDG_SESSION_TYPE === 'x11'
      ? 'Ubuntu 24.04 x64 X11: qualification candidate, device proof pending'
      : 'Linux experimental: initial qualification target is Ubuntu 24.04 x64 X11 (Wayland separate)';
  }
  return `${ctx.platform}/${ctx.arch}: clean-machine/device proof pending`;
}
export function capabilities(ctx) {
  return [
    'Voice: microphone permission and real owner speech checks pending; local fallback needs the managed Python runtime.',
    `Desktop control: ${ctx.platform === 'win32' ? 'Windows implementation, device proof pending' : ctx.platform === 'darwin' ? 'experimental macOS port; no Windows feature-parity claim' : 'limited/experimental; X11 and Wayland need separate qualification'}.`,
    `Credentials: ${ctx.platform === 'linux' ? 'requires an unlocked desktop Secret Service' : ctx.platform === 'darwin' ? 'native Keychain backend configured; persistence proof pending' : 'native Windows credential backend configured; separate contributor namespace'}.`,
    'Cloud features: own keys/accounts/development backend required; not a setup prerequisite.',
  ];
}
export function inspect(ctx) {
  const checks = [];
  const add = (name, ok, fix, install) => checks.push({ name, ok: Boolean(ok), fix, install });
  add('Node 22.x or 24.x', /^v(?:22|24)\.\d+\.\d+$/.test(ctx.probe(process.execPath, ['--version'])), 'Install Node 22 LTS (24 supported).');
  add('Git', ctx.probe('git', ['--version']), 'Install Git.');
  add(`Rust ${RUST}`, ctx.probe('rustup', ['run', RUST, 'rustc', '--version']).startsWith(`rustc ${RUST} `),
    `Install rustup, then rustup toolchain install ${RUST} --profile minimal`, ['rustup', ['toolchain', 'install', RUST, '--profile', 'minimal']]);
  let py;
  const pyCandidates = ctx.platform === 'win32' ? [['py', ['-3.12']], ['python', []]] : [['python3.12', []], ['python3', []]];
  if (ctx.platform === 'darwin') pyCandidates.push(['/opt/homebrew/opt/python@3.12/bin/python3.12', []], ['/usr/local/opt/python@3.12/bin/python3.12', []]);
  for (const [cmd, args] of pyCandidates) {
    if (/^Python 3\.12\./.test(ctx.probe(cmd, [...args, '--version']))) { py = [cmd, args]; break; }
  }
  add('Python 3.12', py, 'Install Python 3.12 including venv support.');
  if (ctx.platform === 'win32') {
    add('Local speech architecture', ctx.arch === 'x64', 'The pinned Moonshine wheel currently requires Windows x64. ARM64 is not qualified.');
    const vswhere = join(ctx.env['ProgramFiles(x86)'] || 'C:/Program Files (x86)', 'Microsoft Visual Studio/Installer/vswhere.exe');
    add('MSVC C++ tools', ctx.exists(vswhere) && ctx.probe(vswhere, ['-latest', '-products', '*', '-requires',
      'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath']),
    'Install VS Build Tools with Desktop development with C++ and a Windows SDK.');
    const sdkProbe = `$root=(Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows Kits\\Installed Roots' -ErrorAction SilentlyContinue).KitsRoot10; if ($root -and (Get-ChildItem ($root+'Include\\*\\um\\Windows.h') -ErrorAction SilentlyContinue) -and (Get-ChildItem ($root+'Lib\\*\\um\\x64\\kernel32.lib') -ErrorAction SilentlyContinue)) {'SDK_OK'}`;
    add('Windows SDK', ctx.probe('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', sdkProbe]).includes('SDK_OK'), 'Install a Windows 10/11 SDK through Visual Studio Installer.');
    const ps = `(Get-ChildItem 'HKLM:\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients','HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients','HKCU:\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients' -ErrorAction SilentlyContinue | Get-ItemProperty | Where-Object name -Match 'WebView2' | Select-Object -ExpandProperty pv) -join ','`;
    add('WebView2 runtime', /\d+\.\d+/.test(ctx.probe('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', ps])), 'Install Microsoft Evergreen WebView2 Runtime.');
    const clang = [ctx.env.LIBCLANG_PATH, 'C:/Program Files/LLVM/bin', 'C:/LLVM/bin'].find(p => p && ctx.exists(join(p, 'libclang.dll')));
    add('LLVM libclang', clang, 'Install LLVM and set LIBCLANG_PATH to its bin directory.');
    ctx.libclang = clang;
  } else if (ctx.platform === 'darwin') {
    add('Apple command-line tools', ctx.probe('xcrun', ['--find', 'clang']), 'Run xcode-select --install and finish the Apple installer.', ['xcode-select', ['--install']]);
    const osVersion = Number(ctx.probe('sw_vers', ['-productVersion']).split('.')[0]);
    add('Local speech architecture/OS', ctx.arch === 'arm64' && osVersion >= 15, 'The pinned Moonshine wheel requires Apple Silicon and macOS 15+. Intel/older macOS need a separately qualified speech dependency.');
  } else if (ctx.platform === 'linux') {
    add('C/C++ compiler', ctx.probe('cc', ['--version']) && ctx.probe('c++', ['--version']), 'Install build-essential.');
    add('pkg-config', ctx.probe('pkg-config', ['--version']), 'Install pkg-config.');
    for (const pkg of ['webkit2gtk-4.1', 'gtk+-3.0', 'ayatana-appindicator3-0.1', 'librsvg-2.0', 'alsa', 'openssl', 'xdo', 'dbus-1', 'portaudio-2.0']) {
      add(`Native library ${pkg}`, ctx.probe('pkg-config', ['--modversion', pkg]), `Install the development package providing ${pkg}.`);
    }
  } else throw new Error(`Unsupported OS ${ctx.platform}`);
  return { checks, python: py };
}
export function installationPlan(ctx, report) {
  const missing = report.checks.filter(c => !c.ok);
  const plan = [];
  const need = name => missing.some(c => c.name === name);
  if (ctx.platform === 'win32') {
    const winget = (id, extra = []) => ['winget', ['install', '-e', '--id', id, '--accept-source-agreements', '--accept-package-agreements', '--disable-interactivity', ...extra]];
    if (need('MSVC C++ tools') || need('Windows SDK')) plan.push(winget('Microsoft.VisualStudio.2022.BuildTools', ['--override', '--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended']));
    if (need('WebView2 runtime')) plan.push(winget('Microsoft.EdgeWebView2Runtime'));
    if (need('LLVM libclang')) plan.push(winget('LLVM.LLVM'));
    if (need('Python 3.12')) plan.push(winget('Python.Python.3.12'));
    if (need(`Rust ${RUST}`) && !ctx.probe('rustup', ['--version'])) {
      plan.push(winget('Rustlang.Rustup'));
      if (ctx.env.USERPROFILE) plan.push([join(ctx.env.USERPROFILE, '.cargo/bin/rustup.exe'), ['toolchain', 'install', RUST, '--profile', 'minimal']]);
    }
  } else if (ctx.platform === 'darwin') {
    if (need('Apple command-line tools')) plan.push(['xcode-select', ['--install']]);
    if (need('Python 3.12') && ctx.probe('brew', ['--version'])) plan.push(['brew', ['install', 'python@3.12']]);
  } else if (ctx.platform === 'linux' && /^ID=ubuntu$/m.test(ctx.exists('/etc/os-release') ? ctx.read('/etc/os-release') : '')) {
    if (missing.some(c => /^(C\/C\+\+|pkg-config|Native library|Python)/.test(c.name))) {
      plan.push(['sudo', ['apt-get', 'update']], ['sudo', ['apt-get', 'install', '-y', 'build-essential', 'curl', 'wget', 'file',
        'pkg-config', 'libwebkit2gtk-4.1-dev', 'libgtk-3-dev', 'libayatana-appindicator3-dev', 'librsvg2-dev',
        'libasound2-dev', 'portaudio19-dev', 'libssl-dev', 'libxdo-dev', 'libdbus-1-dev', 'python3.12', 'python3.12-venv']]);
    }
  }
  if (need(`Rust ${RUST}`) && ctx.platform !== 'win32' && !ctx.probe('rustup', ['--version'])) {
    const cpu = ctx.arch === 'arm64' ? 'aarch64' : ctx.arch === 'x64' ? 'x86_64' : null;
    if (cpu) {
      const triple = `${cpu}-${ctx.platform === 'darwin' ? 'apple-darwin' : 'unknown-linux-gnu'}`;
      const installer = join(ctx.root, '.nexus', 'setup', 'rustup-init');
      plan.push(['curl', ['--proto', '=https', '--tlsv1.2', '-fL', `https://static.rust-lang.org/rustup/dist/${triple}/rustup-init`, '-o', installer]],
        ['chmod', ['700', installer]], [installer, ['-y', '--default-toolchain', RUST, '--profile', 'minimal', '--no-modify-path']]);
    }
  }
  if (need(`Rust ${RUST}`) && ctx.probe('rustup', ['--version'])) plan.push(['rustup', ['toolchain', 'install', RUST, '--profile', 'minimal']]);
  return plan;
}
async function approve(ctx, opt, plan) {
  ctx.log('Required system changes:\n' + plan.map(([c, a]) => `  ${c} ${a.join(' ')}`).join('\n'));
  if (opt.yes) return;
  if (!process.stdin.isTTY) throw new Error('System installation needs approval. Rerun interactively or use --yes to approve the displayed installation plan.');
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try { if (!/^y(?:es)?$/i.test((await rl.question('Install these prerequisites? [y/N] ')).trim())) throw new Error('Installation declined. Install the listed prerequisites manually and rerun.'); }
  finally { rl.close(); }
}
export function npmEntry(ctx) {
  // Execute npm's JS entrypoint with Node, never through a shell (paths may contain spaces).
  const candidates = [join(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js')];
  const found = ctx.probe(ctx.platform === 'win32' ? 'where.exe' : 'which', [ctx.platform === 'win32' ? 'npm.cmd' : 'npm']);
  for (const p of found.split(/\r?\n/).filter(Boolean)) {
    candidates.push(join(dirname(p), 'node_modules/npm/bin/npm-cli.js'));
    if (ctx.exists(p)) {
      const actual = realpathSync(p);
      if (actual.endsWith('npm-cli.js')) candidates.push(actual);
    }
  }
  const path = candidates.find(ctx.exists);
  if (!path) throw new Error('npm CLI not found beside Node or on PATH. Install a complete Node distribution.');
  return path;
}
export async function prepare(ctx, opt) {
  let report = inspect(ctx);
  if (report.checks.some(c => !c.ok)) {
    for (const c of report.checks.filter(c => !c.ok)) ctx.log(`${c.name}: ${c.fix}`);
    if (report.checks.some(c => !c.ok && /^(Node|Git)/.test(c.name))) throw new Error('Git and a supported Node version must be installed before preparation.');
    const plan = installationPlan(ctx, report);
    if (plan.length) {
      await approve(ctx, opt, plan);
      ctx.mkdir(join(ctx.root, '.nexus', 'setup'));
      for (const [cmd, args] of plan) ctx.run(cmd, args);
      const home = ctx.env.USERPROFILE || ctx.env.HOME;
      const addedPaths = home ? [join(home, '.cargo', 'bin')] : [];
      if (ctx.platform === 'win32' && ctx.env.LOCALAPPDATA) addedPaths.push(join(ctx.env.LOCALAPPDATA, 'Programs/Python/Python312'));
      ctx.env.PATH = [...addedPaths, ctx.env.PATH || ''].join(ctx.platform === 'win32' ? ';' : ':');
      // Installers may update PATH in another process; never claim success until rediscovery works.
      report = inspect(ctx);
    }
    if (report.checks.some(c => !c.ok)) throw new Error('Prerequisites still missing. Finish installers, open a new terminal and rerun doctor/build.');
  }
  const cfg = configuration(ctx.root, opt, ctx.platform, ctx.arch);
  const stampPath = join(ctx.root, '.venv', 'nexus-environment.json');
  const requirements = join(ctx.root, 'scripts', 'requirements-contributor.txt');
  const lock = join(ctx.root, 'scripts/requirements-contributor-lock.txt');
  const fingerprint = createHash('sha256').update(ctx.read(requirements)).update(ctx.read(join(ctx.root, 'server/nlu/requirements.txt')))
    .update(ctx.exists(lock) ? ctx.read(lock) : '').digest('hex');
  if (!ctx.exists(cfg.python)) ctx.run(report.python[0], [...report.python[1], '-m', 'venv', join(ctx.root, '.venv')]);
  if (!/^Python 3\.12\./.test(ctx.probe(cfg.python, ['--version']))) throw new Error('Existing .venv is not Python 3.12. Rename it and rerun setup; it was not deleted.');
  let stamp = {};
  try { stamp = JSON.parse(ctx.read(stampPath)); } catch { /* first/interrupted setup */ }
  const audit = join(ctx.root, 'scripts/check-contributor-python.py');
  if (stamp.fingerprint !== fingerprint || ctx.probe(cfg.python, [audit]) !== 'ok' ||
      !ctx.probe(cfg.python, ['-c', 'import moonshine_voice,fastapi,uvicorn,websockets,numpy,onnxruntime,transformers;print("ok")'])) {
    ctx.run(cfg.python, ['-m', 'pip', 'install', '-r', requirements]);
    ctx.run(cfg.python, ['-m', 'pip', 'check']);
    if (ctx.probe(cfg.python, [audit]) !== 'ok') throw new Error('Python dependency versions do not match the runtime lock.');
    if (!ctx.probe(cfg.python, ['-c', 'import moonshine_voice,fastapi,uvicorn,websockets,numpy,onnxruntime,transformers;print("ok")'])) throw new Error('Python dependencies failed verification.');
    writeFileSync(stampPath, JSON.stringify({ fingerprint, python: cfg.python }) + '\n');
  }
  const npm = (ctx.npmEntry || npmEntry)(ctx);
  ctx.run(process.execPath, [npm, '--prefix', 'frontend', 'ci']);
  // A fresh clone already includes runtime NLU models; never overwrite them from local training outputs.
  const tauri = JSON.parse(ctx.read(join(ctx.root, 'src-tauri/tauri.conf.json')));
  for (const resource of tauri.bundle.resources.filter(p => !p.includes('*'))) {
    if (!ctx.exists(join(ctx.root, 'src-tauri', resource))) throw new Error(`Required runtime resource missing: ${resource}. Restore the tracked resource; training is not setup.`);
  }
  ctx.run(process.execPath, ['scripts/sync-hand-assets.mjs']);
  ctx.log(platformLabel(ctx));
  for (const line of capabilities(ctx)) ctx.log(line);
  return cfg;
}

export function portBusy(port) {
  return new Promise((resolvePort, reject) => {
      const socket = createConnection({ port, host: '127.0.0.1' });
      socket.setTimeout(500);
      socket.once('connect', () => { socket.destroy(); resolvePort(true); });
      socket.once('error', error => { socket.destroy(); error.code === 'ECONNREFUSED' ? resolvePort(false) : reject(error); });
      socket.once('timeout', () => { socket.destroy(); reject(new Error(`Cannot determine ownership of port ${port}`)); });
  });
}
export async function assertIdle(ctx) {
  // Existing releases and lazy helpers use these ports. An unknown listener is a conflict too.
  for (const port of PORTS) {
    const busy = await (ctx.portBusy || portBusy)(port);
    if (busy) throw new Error(`Port ${port} is occupied. Concurrent NEXUS sessions/sidecars are unsupported. Stop the other session manually when convenient.`);
  }
  const cmd = ctx.platform === 'win32' ? 'tasklist.exe' : 'pgrep';
  const args = ctx.platform === 'win32' ? ['/FI', 'IMAGENAME eq nexus.exe', '/FO', 'CSV', '/NH'] : ['-x', 'nexus'];
  const r = ctx.exec(cmd, args);
  if (r.error || (r.status !== 0 && !(ctx.platform !== 'win32' && r.status === 1))) throw new Error('Could not check existing NEXUS processes; refusing launch.');
  if (ctx.platform === 'win32' ? /^"nexus\.exe"/im.test(r.stdout || '') : Boolean((r.stdout || '').trim())) throw new Error('NEXUS is already running. Leave it running or stop it manually; this command will not stop it.');
}
export function runtimeEnv(cfg) {
  return { CARGO_TARGET_DIR: cfg.targetDir, TAURI_CONFIG: JSON.stringify(cfg.config), NEXUS_MANAGED_PYTHON: cfg.python,
    NEXUS_PROFILE: cfg.profile, RUSTUP_TOOLCHAIN: RUST, RUST_LOG: 'info,nexus=debug' };
}
export async function runContributor(command, args = [], ctx = context()) {
  const opt = options(args);
  if ((opt.target || opt.bundles) && command !== 'package') throw new Error('--target/--bundles are packaging options only.');
  if (command === 'package' && opt.profile !== 'personal') throw new Error('Distributable packaging requires --profile personal. Contributor binaries are checkout-bound; use build for local development.');
  const cfg = configuration(ctx.root, opt, ctx.platform, ctx.arch);
  if (command === 'doctor' || command === 'check') {
    const report = inspect(ctx);
    ctx.log(platformLabel(ctx));
    for (const line of capabilities(ctx)) ctx.log(line);
    for (const c of report.checks) ctx.log(`${c.ok ? 'OK' : 'MISSING'} ${c.name}${c.ok ? '' : ': ' + c.fix}`);
    if (ctx.platform === 'linux') {
      const owner = ctx.probe('gdbus', ['call', '--session', '--dest', 'org.freedesktop.DBus', '--object-path', '/org/freedesktop/DBus',
        '--method', 'org.freedesktop.DBus.NameHasOwner', 'org.freedesktop.secrets']);
      ctx.log(owner.includes('true') ? 'Secret Service is running; actual key persistence still needs a device check.'
        : 'Persistent credentials unavailable/unverified: unlock/start a desktop Secret Service. No mock persistence is used.');
    }
    ctx.log(`Profile: ${cfg.profile}; data identity: ${cfg.id}; output: ${cfg.targetDir}`);
    const managed = ctx.exists(cfg.python) && /^Python 3\.12\./.test(ctx.probe(cfg.python, ['--version'])) &&
      ctx.probe(cfg.python, [join(ctx.root, 'scripts/check-contributor-python.py')]) === 'ok';
    ctx.log(`Managed Python: ${managed ? cfg.python : 'missing/incompatible dependencies; run setup'}`);
    ctx.log('Optional: cloud accounts/keys, Worker deployment, training, native device/permission proofs are separate.');
    return report.checks.every(c => c.ok) && Boolean(managed) ? 0 : 1;
  }
  if (command === 'start' || command === 'run') {
    // A stale build is a reminder, not a refusal: the owner may want to test the binary they already have.
    // A missing or invalid build record, or a different profile/toolchain, still stops the launch.
    const verified = ctx.provenance.verifyBuild({ warnStale: true, directory: join(cfg.targetDir, 'release'), metadata: cfg.metadata });
    const staleWarnings = Array.isArray(verified?.warnings) ? verified.warnings : [];
    for (const w of staleWarnings) ctx.log(`WARNING: ${w}. Starting the existing build anyway.`);
    if (staleWarnings.length) ctx.log('REMINDER: the source changed since the last build. Run `nexus build` when you want those changes in the running app.');
    if (!ctx.exists(cfg.python)) throw new Error('Managed Python is missing; run setup.');
    if (!/^Python 3\.12\./.test(ctx.probe(cfg.python, ['--version']))) throw new Error('Managed Python version changed; run setup.');
    await (ctx.assertIdle || assertIdle)(ctx);
    ctx.run(cfg.binary, [], { env: runtimeEnv(cfg) });
    return 0;
  }
  if (command === 'clean') {
    await (ctx.assertIdle || assertIdle)(ctx);
    if (ctx.exists(cfg.targetDir)) {
      if (lstatSync(cfg.targetDir).isSymbolicLink() || dirname(realpathSync(cfg.targetDir)) !== realpathSync(join(ctx.root, 'src-tauri'))) {
        throw new Error('Refusing to clean a linked or external output directory.');
      }
      rmSync(cfg.targetDir, { recursive: true, force: true });
    }
    ctx.log(`Cleaned only ${cfg.targetDir}; personal installation, venv and shared frontend assets were preserved.`);
    return 0;
  }
  if (command === 'test') {
    const npm = (ctx.npmEntry || npmEntry)(ctx);
    ctx.run(process.execPath, ['--test', 'scripts/contributor-cli.test.mjs', 'scripts/contributor-contract.test.mjs', 'scripts/build-provenance.test.mjs']);
    ctx.run(cfg.python, ['-m', 'unittest', 'scripts/test_contributor_lock.py']);
    ctx.run(process.execPath, [npm, '--prefix', 'frontend', 'test']);
    ctx.run('rustup', ['run', RUST, 'cargo', 'test', '--locked', '--tests', '--features', ['custom-protocol', ...cfg.features].join(','), '--', '--test-threads=1'],
      { cwd: join(ctx.root, 'src-tauri'), env: runtimeEnv(cfg) });
    return 0;
  }
  await (ctx.prepare || prepare)(ctx, opt);
  if (command === 'install') { registerGlobal(ctx); return 0; }
  if (command === 'setup') return 0;
  if (!['build', 'dev', 'package'].includes(command)) throw new Error(`Unsupported contributor command ${command}`);
  if (command === 'dev') await (ctx.assertIdle || assertIdle)(ctx);
  const env = { ...runtimeEnv(cfg), ...(ctx.libclang ? { LIBCLANG_PATH: ctx.libclang } : {}) };
  if (command === 'dev' || command === 'package') {
    const cli = join(ctx.root, 'frontend/node_modules/@tauri-apps/cli/tauri.js');
    const packageConfig = structuredClone(cfg.config);
    if (command === 'package' && ctx.env.APPLE_SIGNING_IDENTITY) packageConfig.bundle.macOS.signingIdentity = ctx.env.APPLE_SIGNING_IDENTITY;
    // Installed personal apps discover Python on the recipient machine; never bake in the CI venv path.
    if (command === 'package') env.NEXUS_MANAGED_PYTHON = undefined;
    ctx.run(process.execPath, [cli, command === 'dev' ? 'dev' : 'build', '--config', JSON.stringify(packageConfig),
      ...(opt.target ? ['--target', opt.target] : []), ...(opt.bundles ? ['--bundles', opt.bundles] : []),
      ...(cfg.features.length ? ['--features', cfg.features.join(',')] : []), '--', '--locked'],
      { cwd: join(ctx.root, 'src-tauri'), env });
    // package deliberately does not fabricate a local build record for a different target/bundle.
    return 0;
  }
  const before = ctx.provenance.sourceHash();
  ctx.run(process.execPath, [(ctx.npmEntry || npmEntry)(ctx), '--prefix', 'frontend', 'run', 'build']);
  ctx.run('rustup', ['run', RUST, 'cargo', 'build', '--locked', '--release', '--features', ['custom-protocol', ...cfg.features].join(',')],
    { cwd: join(ctx.root, 'src-tauri'), env });
  const record = ctx.provenance.recordBuild(before, { directory: join(cfg.targetDir, 'release'), metadata: cfg.metadata });
  ctx.log(`Built ${cfg.profile} ${record.identity}: ${cfg.binary}`);
  return 0;
}
export function registerGlobal(ctx) {
  const home = ctx.env.USERPROFILE || ctx.env.HOME;
  if (!home) throw new Error('Cannot resolve user home for command registration.');
  const bin = join(home, '.local', 'bin');
  mkdirSync(bin, { recursive: true });
  const script = join(ctx.root, 'nexus.mjs');
  const path = join(bin, ctx.platform === 'win32' ? 'nexus.cmd' : 'nexus');
  if (ctx.platform === 'win32') {
    if (/[%!\r\n]/.test(script + process.execPath)) throw new Error('Cannot safely register a Windows shim in a path containing %, ! or newlines. Use node nexus.mjs.');
    writeFileSync(path, `@echo off\r\n"${process.execPath}" "${script}" %*\r\n`);
  } else {
    const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
    writeFileSync(path, `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`);
    chmodSync(path, 0o755);
  }
  ctx.log(`Registered ${path}. Add ${bin} to your user PATH if needed; open a new terminal. No system PATH was overwritten.`);
}
