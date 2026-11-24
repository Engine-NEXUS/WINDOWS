// Copies only the CLI module into a private temporary directory. Never edits application source.
import { mkdtempSync, writeFileSync, readFileSync, rmSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
const source = readFileSync(new URL('./contributor-cli.mjs', import.meta.url), 'utf8');
const provenance = new URL('./build-provenance.mjs', import.meta.url).href;
const cases = [
  ['personal-default', "profile: 'contributor', yes:", "profile: 'personal', yes:"],
  ['shared-identity', "? '.contributor' : ''", "? '' : ''"],
  ['admin-guard', "if (result.admin && result.profile !== 'personal')", 'if (false)'],
  // A stale build is only a reminder now; the safety rule is that a missing or invalid build record still stops a launch.
  ['stale-start', "ctx.provenance.verifyBuild({ warnStale: true, directory: join(cfg.targetDir, 'release'), metadata: cfg.metadata });", 'undefined; /* omitted verification */'],
  ['start-interpreter', "if (!ctx.exists(cfg.python)) throw new Error('Managed Python is missing; run setup.');", '// interpreter check removed'],
  ['start-session', 'await (ctx.assertIdle || assertIdle)(ctx);', '// session check removed'],
  ['unsafe-update', "[npm, '--prefix', 'frontend', 'ci']", "[npm, '--prefix', 'frontend', 'install']"],
  ['unlocked-build', "'cargo', 'build', '--locked', '--release'", "'cargo', 'build', '--release'"],
  ['wrong-interpreter', 'NEXUS_MANAGED_PYTHON: cfg.python', "NEXUS_MANAGED_PYTHON: 'python'"],
  ['ignore-busy-port', 'if (busy) throw new Error', 'if (false) throw new Error'],
  ['ignore-process-error', "if (r.error || (r.status !== 0 && !(ctx.platform !== 'win32' && r.status === 1)))", 'if (false)'],
  ['ignore-process', "if (ctx.platform === 'win32' ? /^\"nexus\\.exe\"/im.test(r.stdout || '') : Boolean((r.stdout || '').trim()))", 'if (false)'],
];
const dir = mkdtempSync(join(tmpdir(), 'nexus-contributor-mutations-'));
let failed = false;
try {
  for (const [name, from, to] of cases) {
    if (!source.includes(from)) throw new Error(`Mutation anchor missing: ${name}`);
    const path = join(dir, name + '.mjs');
    writeFileSync(path, source.replace(from, to).replace("'./build-provenance.mjs'", JSON.stringify(provenance)));
    const r = spawnSync(process.execPath, ['--test', fileURLToPath(new URL('./contributor-cli.test.mjs', import.meta.url))], {
      encoding: 'utf8', timeout: 30000, env: { ...process.env, NEXUS_CLI_TEST_MODULE: pathToFileURL(path).href } });
    // Syntax/import failures are not valid safety proof.
    if (r.error || /SyntaxError|ERR_MODULE_NOT_FOUND/.test((r.stdout || '') + (r.stderr || ''))) throw new Error(`Invalid mutation run: ${name}`);
    const killed = r.status !== 0;
    console.log(`${killed ? 'KILLED' : 'SURVIVED'} ${name}`);
    if (!killed) failed = true;
  }
  const root = join(dir, 'native-contracts');
  const nativeFiles = ['nexus.mjs', 'src-tauri/src/app_dirs.rs', 'src-tauri/src/auth_vault.rs', 'src-tauri/src/voice_profile.rs',
    'src-tauri/src/memcore/crypto.rs', 'src-tauri/src/lib.rs', 'src-tauri/src/commands.rs'];
  for (const file of nativeFiles) {
    mkdirSync(dirname(join(root, file)), { recursive: true });
    writeFileSync(join(root, file), readFileSync(new URL('../' + file, import.meta.url)));
  }
  for (const [name, file, from, to] of [
    ['native-shared-identity', 'app_dirs.rs', 'const APP_ID: &str = "com.nexus.assistant.contributor"', 'const APP_ID: &str = "com.nexus.assistant"'],
    ['native-shared-credentials', 'auth_vault.rs', 'Entry::new(crate::app_dirs::APP_ID, user)', 'Entry::new("com.nexus.assistant", user)'],
    ['native-autostart', 'lib.rs', 'if !cfg!(feature = "contributor") {', 'if true {'],
    ['native-session', 'lib.rs', '!crate::app_dirs::claim_contributor_session()', 'false'],
    ['native-settings-autostart', 'commands.rs', 'if cfg!(feature = "contributor") { return Err("Autostart is disabled', 'if false { return Err("Autostart is disabled'],
  ]) {
    const path = join(root, 'src-tauri/src', file);
    const original = readFileSync(path, 'utf8');
    if (!original.includes(from)) throw new Error(`Native mutation anchor missing: ${name}`);
    writeFileSync(path, original.replace(from, to));
    const r = spawnSync(process.execPath, ['--test', fileURLToPath(new URL('./contributor-contract.test.mjs', import.meta.url))], {
      encoding: 'utf8', timeout: 30000, env: { ...process.env, NEXUS_CONTRACT_ROOT: root } });
    writeFileSync(path, original);
    if (r.error || /SyntaxError|ERR_MODULE_NOT_FOUND/.test((r.stdout || '') + (r.stderr || ''))) throw new Error(`Invalid native mutation run: ${name}`);
    const killed = r.status !== 0;
    console.log(`${killed ? 'KILLED' : 'SURVIVED'} ${name}`);
    if (!killed) failed = true;
  }
} finally { rmSync(dir, { recursive: true, force: true }); }
if (failed) process.exitCode = 1;
