import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
const m = await import(process.env.NEXUS_CLI_TEST_MODULE || './contributor-cli.mjs');
const { options, configuration, context, inspect, installationPlan, runContributor, prepare, runtimeEnv, platformLabel, assertIdle } = m;

function fixture(fn) {
  const root = mkdtempSync(join(tmpdir(), 'nexus contributor tests '));
  const put = (p, v) => { const f = join(root, p); mkdirSync(dirname(f), { recursive: true }); writeFileSync(f, v); };
  return Promise.resolve().then(() => fn(root, put)).finally(() => rmSync(root, { recursive: true, force: true }));
}
function fake(root, extra = {}) {
  const calls = [];
  const ctx = context(root, { platform: 'linux', arch: 'x64', env: { HOME: root, XDG_SESSION_TYPE: 'x11' },
    log: () => {}, mkdir: () => {}, npmEntry: () => '/tools with spaces/npm-cli.js',
    probe: (cmd, args) => {
      if (cmd === process.execPath) return 'v22.15.0';
      if (cmd === 'rustup') return args[0] === 'run' ? 'rustc 1.93.1 (hash)' : 'rustup 1.28.0';
      if (cmd === 'git') return 'git version 2.49';
      if (cmd === 'python3.12' || cmd.includes('.venv')) return args.includes('--version') ? 'Python 3.12.7' : 'ok';
      if (cmd === 'cc' || cmd === 'c++') return 'cc 13.3';
      if (cmd === 'pkg-config') return '1.0';
      return '';
    },
    run: (cmd, args, opt) => { calls.push({ cmd, args, opt }); },
    assertIdle: async () => { calls.push({ idle: true }); },
    provenance: { sourceHash: () => 'source', recordBuild: (...args) => { calls.push({ record: args }); return { identity: 'fixture' }; },
      verifyBuild: args => { calls.push({ verify: args }); } },
    ...extra });
  return { ctx, calls };
}
test('defaults isolate contributor builds and never include admin brain', () => {
  const c = configuration('/repo with spaces', options(), 'win32', 'x64');
  assert.equal(c.id, 'com.nexus.assistant.contributor');
  assert.deepEqual(c.metadata.features, ['custom-protocol', 'contributor']);
  assert.match(c.binary, /target-contributor[\\/]release[\\/]nexus.exe$/);
  assert.equal(c.config.bundle.macOS.signingIdentity, null);
  assert.deepEqual(c.config.plugins['deep-link'].desktop.schemes, ['nexus-dev']);
});
test('personal is explicit, admin is opt-in, installed paths remain distinct', () => {
  const p = configuration('/repo', options(['--profile', 'personal']), 'darwin', 'arm64');
  assert.equal(p.id, 'com.nexus.assistant');
  assert.deepEqual(p.features, []);
  assert.deepEqual(options(['--profile', 'personal', '--admin-brain']).admin, true);
  assert.throws(() => options(['--admin-brain']), /personal/);
  assert.throws(() => options(['--profile', 'other']), /Profile/);
  assert.throws(() => options(['--build']), /not supported/);
  assert.throws(() => options(['--target', '--yes']), /Missing value/);
  assert.throws(() => options(['--bundles']), /Missing value/);
});
test('doctor is read-only, reports missing tools, does not probe cloud services', async () => {
  const { ctx, calls } = fake('/empty', { probe: () => '', exists: () => false });
  assert.equal(await runContributor('doctor', [], ctx), 1);
  assert.equal(calls.length, 0);
  assert.equal(await runContributor('check', [], ctx), 1);
});
test('version checks reject an incompatible Rust or Python even when a command exists', () => {
  const { ctx } = fake('/repo');
  assert.ok(inspect(ctx).checks.every(c => c.ok));
  const good = ctx.probe;
  ctx.probe = (cmd, args) => cmd === 'rustup' ? 'rustc 1.94.0' : cmd === 'python3.12' ? 'Python 3.13.0' : good(cmd, args);
  const report = inspect(ctx);
  assert.equal(report.python, undefined);
  assert.ok(report.checks.some(c => c.name.startsWith('Rust') && !c.ok));
});
test('Windows requires actual VS installation and SDK, not an arbitrary link command', () => {
  const { ctx } = fake('/repo', { platform: 'win32', exists: () => false });
  const checks = inspect(ctx).checks;
  assert.equal(checks.find(c => c.name === 'MSVC C++ tools').ok, false);
  assert.equal(checks.find(c => c.name === 'Windows SDK').ok, false);
  assert.ok(installationPlan(ctx, { checks }).some(([c, a]) => c === 'winget' && a.includes('Microsoft.VisualStudio.2022.BuildTools')));
});
test('Ubuntu dependency plan includes compilers, secret service headers, audio and Python venv', () => {
  const { ctx } = fake('/repo', { read: () => 'ID=ubuntu\nVERSION_ID="24.04"', exists: () => true, probe: () => '' });
  const plan = installationPlan(ctx, inspect(ctx));
  const packages = plan.flatMap(([, a]) => a);
  for (const p of ['build-essential', 'libdbus-1-dev', 'libasound2-dev', 'python3.12-venv']) assert.ok(packages.includes(p));
  assert.match(platformLabel(ctx), /proof pending/);
  ctx.env.XDG_SESSION_TYPE = 'wayland';
  assert.match(platformLabel(ctx), /experimental/);
});
test('macOS reports command-line tools installation separately', () => {
  const { ctx } = fake('/repo', { platform: 'darwin', probe: () => '' });
  assert.ok(installationPlan(ctx, inspect(ctx)).some(([c, a]) => c === 'xcode-select' && a[0] === '--install'));
});
test('noninteractive setup refuses unapproved installation without any writes or commands', async () => {
  const { ctx, calls } = fake('/repo', { platform: 'win32', exists: () => false,
    probe: cmd => cmd === process.execPath ? 'v22.15.0' : cmd === 'git' ? 'git version 2.49' : '' });
  await assert.rejects(prepare(ctx, options()), /approval|declined/);
  assert.equal(calls.length, 0);
});
test('an approved installer failure is fatal, never a setup success', async () => {
  const { ctx } = fake('/repo', { platform: 'win32', exists: () => false,
    probe: cmd => cmd === process.execPath ? 'v22.15.0' : cmd === 'git' ? 'git version 2.49' : '', run: () => { throw new Error('download failed'); } });
  await assert.rejects(prepare(ctx, options(['--yes'])), /download failed/);
});
test('missing Git/Node never triggers installation even with --yes', async () => {
  const { ctx, calls } = fake('/repo', { probe: () => '' });
  await assert.rejects(prepare(ctx, options(['--yes'])), /must be installed/);
  assert.equal(calls.length, 0);
});
test('setup verifies Python before installing dependencies and leaves an incompatible venv intact', () => fixture(async (root, put) => {
  put('scripts/requirements-contributor.txt', 'package==1'); put('server/nlu/requirements.txt', '');
  put('.venv/bin/python', 'existing interpreter');
  const { ctx, calls } = fake(root);
  const good = ctx.probe;
  ctx.probe = (cmd, args) => cmd.includes('.venv') ? 'Python 3.13.0' : good(cmd, args);
  await assert.rejects(prepare(ctx, options()), /not Python 3.12/);
  assert.equal(calls.length, 0);
  assert.equal(readFileSync(join(root, '.venv/bin/python'), 'utf8'), 'existing interpreter');
}));
test('setup retries interrupted Python preparation and uses npm ci without model training', () => fixture(async (root, put) => {
  put('scripts/requirements-contributor.txt', 'package==1'); put('server/nlu/requirements.txt', ''); put('.venv/bin/python', 'py');
  put('src-tauri/tauri.conf.json', JSON.stringify({ bundle: { resources: ['resources/model.onnx'] } })); put('src-tauri/resources/model.onnx', 'model');
  const { ctx, calls } = fake(root);
  await prepare(ctx, options());
  assert.ok(calls.some(c => c.args.includes('pip')));
  assert.ok(calls.some(c => c.args.includes('ci')));
  assert.ok(existsSync(join(root, '.venv/nexus-environment.json')));
  calls.length = 0;
  await prepare(ctx, options());
  assert.ok(!calls.some(c => c.args.includes('pip')));
  assert.ok(!calls.some(c => /train|worker/.test(c.args.join(' '))));
}));
test('required resource loss fails setup without recommending training', () => fixture(async (root, put) => {
  put('scripts/requirements-contributor.txt', 'package==1'); put('server/nlu/requirements.txt', ''); put('.venv/bin/python', 'py');
  put('src-tauri/tauri.conf.json', JSON.stringify({ bundle: { resources: ['resources/missing.onnx'] } }));
  const { ctx } = fake(root);
  await assert.rejects(prepare(ctx, options()), /Required runtime resource missing/);
}));
test('start verifies exact profile before launch and never prepares or builds', async () => {
  const { ctx, calls } = fake('/repo', { exists: () => true, prepare: () => { throw new Error('must not prepare'); } });
  await runContributor('start', [], ctx);
  assert.ok(calls[0].verify);
  assert.equal(calls.filter(c => c.cmd).length, 1);
  assert.match(calls.at(-1).cmd, /target-contributor[\\/]release/);
  assert.equal(calls.at(-1).opt.env.NEXUS_PROFILE, 'contributor');
});
test('stale source only reminds: the existing build still starts', async () => {
  const logs = [];
  const { ctx, calls } = fake('/repo', { exists: () => true, log: m => logs.push(m) });
  let asked;
  ctx.provenance.verifyBuild = opts => { asked = opts; return { warnings: ['release source changed since build; run nexus build or nexus start --build'] }; };
  await runContributor('start', [], ctx);
  assert.equal(asked.warnStale, true);
  assert.equal(calls.filter(c => c.cmd).length, 1, 'the binary is launched');
  assert.ok(logs.some(l => /REMINDER/.test(l) && /nexus build/.test(l)));
});
test('missing build record, missing Python and occupied session each prevent launch', async () => {
  for (const kind of ['missing', 'python', 'occupied']) {
    const { ctx, calls } = fake('/repo', { exists: () => kind !== 'python' });
    if (kind === 'missing') ctx.provenance.verifyBuild = () => { throw new Error('release build record missing; run nexus build'); };
    if (kind === 'occupied') ctx.assertIdle = async () => { throw new Error('occupied'); };
    await assert.rejects(runContributor('start', [], ctx));
    assert.equal(calls.filter(c => c.cmd).length, 0);
  }
});
test('build uses locked dependencies and profile provenance with no app termination or launch', async () => {
  const { ctx, calls } = fake('/repo with spaces', { prepare: async () => {} });
  await runContributor('build', [], ctx);
  const cargo = calls.find(c => c.cmd === 'rustup');
  assert.ok(cargo.args.includes('--locked'));
  assert.ok(cargo.args.includes('custom-protocol,contributor'));
  assert.ok(!cargo.args.join(' ').includes('admin-brain'));
  assert.equal(calls.at(-1).record[1].metadata.profile, 'contributor');
  assert.ok(!calls.some(c => /kill|nexus\.exe|pwsh/.test(c.cmd || '')));
});
test('build failure cannot record a successful build', async () => {
  const { ctx, calls } = fake('/repo', { prepare: async () => {} });
  ctx.run = () => { throw new Error('compiler failed'); };
  await assert.rejects(runContributor('build', [], ctx), /compiler failed/);
  assert.ok(!calls.some(c => c.record));
});
test('package always uses the pinned npm Tauri CLI, never a binary-only fallback', async () => {
  const { ctx, calls } = fake('/repo', { prepare: async () => {} });
  await runContributor('package', ['--profile', 'personal', '--bundles', 'deb,appimage'], ctx);
  assert.equal(calls.length, 1);
  assert.match(calls[0].args[0], /tauri.js$/);
  assert.ok(calls[0].args.includes('deb,appimage'));
  assert.ok(calls[0].args.includes('--locked'));
  assert.equal(calls[0].opt.env.NEXUS_MANAGED_PYTHON, undefined);
  await assert.rejects(runContributor('package', [], ctx), /checkout-bound/);
  await assert.rejects(runContributor('build', ['--target', 'aarch64-apple-darwin'], ctx), /packaging options/);
});
test('managed interpreter selection is explicit and absolute across platforms', () => {
  for (const platform of ['win32', 'darwin', 'linux']) {
    const cfg = configuration('/repo', options(), platform);
    assert.equal(runtimeEnv(cfg).NEXUS_MANAGED_PYTHON, cfg.python);
    assert.match(cfg.python, /\.venv/);
  }
});
test('real idle guard rejects occupied ports, existing processes and failed process enumeration', async () => {
  for (const state of ['port', 'process', 'query-error', 'idle']) {
    const { ctx } = fake('/repo', { portBusy: async () => state === 'port',
      exec: () => ({ status: state === 'query-error' ? 2 : state === 'process' ? 0 : 1, stdout: state === 'process' ? '1234' : '' }) });
    if (state === 'idle') await assertIdle(ctx);
    else await assert.rejects(assertIdle(ctx), /occupied|running|Could not check/);
  }
});
test('Windows idle guard recognizes a quoted tasklist row without adopting the instance', async () => {
  const { ctx } = fake('/repo', { platform: 'win32', portBusy: async () => false,
    exec: () => ({ status: 0, stdout: '"nexus.exe","1234","Console","1","42 K"' }) });
  await assert.rejects(assertIdle(ctx), /already running/);
});
test('packaging maps current server sources without rewriting tracked runtime models', () => {
  assert.deepEqual(m.resourceMap(['resources/server/stt_server.py', 'resources/oww/nexus.onnx', 'resources/vad/*']), {
    '../server/stt_server.py': 'resources/server/stt_server.py',
    'resources/oww/nexus.onnx': 'resources/oww/nexus.onnx',
    'resources/vad/*': 'resources/vad/',
  });
});
test('process execution preserves literal arguments instead of evaluating shell syntax', () => {
  const ctx = context();
  const args = ['a space', '&echo NEXUS_UNSAFE', '$(echo NEXUS_UNSAFE)', '%NEXUS_UNSET_TEST_VAR%', 'single\'quote'];
  const r = ctx.exec(process.execPath, ['-e', 'process.stdout.write(JSON.stringify(process.argv.slice(1)))', ...args]);
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(JSON.parse(r.stdout), args);
});
test('clean removes only the selected local output and preserves personal output', () => fixture(async (root, put) => {
  put('src-tauri/target-contributor/file', 'contributor'); put('src-tauri/target-personal/file', 'personal');
  const { ctx } = fake(root);
  await runContributor('clean', [], ctx);
  assert.equal(existsSync(join(root, 'src-tauri/target-contributor')), false);
  assert.equal(readFileSync(join(root, 'src-tauri/target-personal/file'), 'utf8'), 'personal');
}));
