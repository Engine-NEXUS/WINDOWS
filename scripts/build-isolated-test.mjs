// Builds the CONTRIBUTOR-profile debug binary used by scripts/run-isolated-test.ps1, with the same Tauri overlay
// configuration `node nexus.mjs build` uses, but without that command's refusal to run while a NEXUS is running.
// Output: D:\nexus-screen-target\debug\nexus.exe (about 2 minutes incremental). It never starts or stops anything.
import { configuration } from './contributor-cli.mjs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const targetDir = process.env.CARGO_TARGET_DIR || 'D:\nexus-screen-target';
const cfg = configuration(root, { profile: 'contributor', admin: false });
const env = {
  ...process.env, CARGO_TARGET_DIR: targetDir, TAURI_CONFIG: JSON.stringify(cfg.config),
  NEXUS_MANAGED_PYTHON: cfg.python, NEXUS_PROFILE: 'contributor',
};
console.log('identifier', cfg.config.identifier, '| target', targetDir);
const r = spawnSync('cargo', ['build', '--features', 'custom-protocol,contributor'], { cwd: join(root, 'src-tauri'), env, stdio: 'inherit' });
process.exit(r.status ?? 1);
