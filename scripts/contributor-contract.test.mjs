import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
const read = p => readFileSync(process.env.NEXUS_CONTRACT_ROOT ? join(process.env.NEXUS_CONTRACT_ROOT, p) : new URL('../' + p, import.meta.url), 'utf8');

test('repository build/start use the safe contributor dispatcher', () => {
  const cli = read('nexus.mjs');
  assert.match(cli, /runContributor/);
  assert.doesNotMatch(cli, /function killRunningNexus/);
  assert.doesNotMatch(cli, /verifyBuild\(\{ warnStale: true/);
});

test('native fallback and credentials use the selected build identity', () => {
  assert.match(read('src-tauri/src/app_dirs.rs'), /feature = "contributor"/);
  assert.match(read('src-tauri/src/app_dirs.rs'), /const APP_ID: &str = "com\.nexus\.assistant\.contributor"/);
  for (const file of ['auth_vault.rs', 'voice_profile.rs', 'memcore/crypto.rs']) {
    assert.doesNotMatch(read('src-tauri/src/' + file), /(?:Entry::new\(|KEYRING_SERVICE: &str = )"com\.nexus\.assistant"/);
  }
});

test('contributor startup and settings cannot modify the personal autostart task or WebView', () => {
  const lib = read('src-tauri/src/lib.rs');
  assert.match(lib, /if !cfg!\(feature = "contributor"\) \{\s*let autostart_enabled/);
  assert.doesNotMatch(lib, /\.join\("com\.nexus\.assistant"\)/);
  assert.match(read('src-tauri/src/commands.rs'), /if cfg!\(feature = "contributor"\) \{ return Err\("Autostart is disabled/);
});

test('native session refusal precedes setup work while permitting single-instance activation', () => {
  const lib = read('src-tauri/src/lib.rs');
  const plugin = lib.indexOf('.plugin(tauri_plugin_single_instance');
  const setup = lib.indexOf('builder.setup(move |app|');
  const guard = lib.indexOf('!crate::app_dirs::claim_contributor_session()', setup);
  const stage = lib.indexOf('stage::stage_show_sync', setup);
  assert.ok(plugin > 0 && setup > plugin && guard > setup && stage > guard);
  assert.doesNotMatch(lib.slice(plugin, setup), /claim_contributor_session\(/);
});
