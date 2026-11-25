# Contributor setup implementation — 2026-11-05

## Authorization and scope

Implemented the approved cross-platform contributor setup plan in source. The owner controls application builds and runtime. No NEXUS executable was built, started, stopped or restarted, and no runtime setting, account, voice profile or personal data was changed. Unrelated dirty files were preserved. This record does not advance the production-readiness queue or establish OS feature parity.

User choices: Git + Node bootstrap, all three OS paths staged, separate contributor data by default. The portable first commands are `node nexus.mjs build` and `node nexus.mjs start`; a global `nexus` command requires registration/PATH configuration.

## Changes by phase

### 1. Shared CLI foundation

- Extracted lifecycle operations from `nexus.mjs` into `scripts/contributor-cli.mjs`, with injectable probes, runners, filesystem preparation and provenance operations.
- Added doctor/check, explicit command options, clear failure handling and regression fixtures. The initial two contract tests failed against the original source before implementation.
- Removed broad process killing, stale-build launch permission, implicit rebuilding and default `admin-brain`. Start checks profile-aware provenance before any launch.

### 2. Repeatable setup

- Pinned Rust 1.93.1; Node 22 baseline/24 accepted; managed Python 3.12. Direct Python versions match the locally inspected 3.12.7 environment and their releases were checked against PyPI. A 45-distribution transitive runtime lock includes OS marker branches; uvloop, absent on Windows, was selected from official PyPI metadata. Read-only metadata auditing detects installed-version drift; pip check and imports gate the setup stamp. This is not a fresh-install test.
- Added Windows VS/SDK/WebView2/LLVM probes and approved winget plans, macOS tools/Python guidance and Rust preparation, Ubuntu compiler/WebKit/audio/D-Bus/Python plans.
- Setup displays system changes and requires interactive consent or explicit `--yes`. Required command/import/resource failures are fatal. Failed/interrupted setup does not write a success stamp; incompatible existing venvs are not deleted.
- Uses npm ci and Cargo --locked, does not deploy the Worker, train models or promote ignored local model output. Optional Hand Mode downloads remain visibly optional.
- Rust dependency metadata added Linux persistent Secret Service support and its lockfile dependencies without compiling the app.

### 3. Application identity and isolation

- Added compile-time contributor identity, separate credential/encryption namespaces, fallback storage, WebView identity, native notification identity and `nexus-dev` registration. Personal builds retain their existing identity.
- Routed exact hard-coded application-directory/keychain literals through the identity, preserving surrounding code and concurrent edits. No personal account migration occurs.
- Contributor startup and autostart settings cannot create/delete the personal scheduled task. A shared Windows boot check prevents touching the personal WebView; a contributor session-port reservation and sidecar-port checks reject conflicting sessions.
- Managed interpreter selection fails closed for checkout builds. No fallback to an unrelated system interpreter. Explicit personal admin builds also use the selected interpreter.
- Build script validates contributor Tauri identity and absolute managed interpreter. Local outputs use target-contributor/target-personal, preserving the directory depth expected by existing resource resolvers.

### 4. Build/start/package consistency

- Build prepares and compiles; start verifies an existing matching record and supervises the process via Node. No PowerShell 7 dependency or automatic CDP monitoring in the default launcher.
- Provenance schema 2 includes profile, feature set, architecture, platform, toolchain, output and managed Python, while preserving the old schema 1 helper for historical records. Source hashing includes the new build inputs and canonical server entrypoints.
- Installer creation always uses the frontend lockfile-pinned Tauri CLI. Removed binary-only fallback from packaging wrappers. Old shell/PowerShell scripts delegate to the shared implementation.
- Packaging maps canonical server scripts directly into resources rather than modifying tracked copies. Local macOS builds override placeholder signing configuration; explicit packaging can use APPLE_SIGNING_IDENTITY.
- Packaging requires explicit personal profile. Contributor binaries are checkout-bound; installed personal packages do not embed a CI Python path. Python bundling/recipient environment provisioning is outside this change and remains documented.

### 5. Documentation and CI

- Added CONTRIBUTING.md, updated build/setup guidance and a three-platform Node 22/24 offline workflow. Manual native qualification runs first/repeat builds and frontend/native tests without launching the application.
- Updated release/macOS runner labels to macos-15 and Linux release baseline to Ubuntu 24.04. Release packaging uses personal output paths and explicit approved preparation.
- Official runner reference consulted: [GitHub runner labels](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job). The checked Moonshine release offers Apple Silicon macOS 15+, Windows x64 and Linux wheels: [PyPI release metadata](https://pypi.org/pypi/moonshine-voice/0.1.5/json).

## Editor attribution and review findings

The requested OpenCode `opencode/glm-5.3-flash-free` model was listed as available. A small isolated helper task was submitted in a temporary directory containing only its brief and local instructions. It returned no response/files; only the brief and instructions existed at inspection. That editor session was interrupted, not NEXUS. The authorized supervisor fallback implemented the changes. No GLM-authored code is claimed.

Review caught and corrected the personal logon-task overwrite risk, new-output directory-depth mismatch, stale packaging server-copy risk, Windows SDK detection requiring an actual SDK, and the risk of embedding a build-machine interpreter in distributable packages. A provisional nonexistent Moonshine version was corrected before tests/installation to the inspected 0.1.5 release. No package was installed by this work.

## Verification and failures retained

| Check | Observed result |
|---|---|
| Initial red regressions | 2 failures on original code, as intended |
| CLI/contracts/provenance | 30/30 passing |
| Safety mutations | 17/17 killed (12 behavioral CLI + 5 native source-contract removals); temporary copies only |
| Python lock tests/audit | 2/2 structural tests; installed Windows Python 3.12 metadata matches the active lock branch |
| Standalone Rust identity helpers | Personal 2/2, contributor 3/3; no application build |
| Frontend | 831/831 tests, 91 files |
| Worker | 145/145 tests, 11 files |
| IPC contract, turn-end, repo hygiene | Passed |
| Cargo metadata --locked --no-deps | Passed; no compile |
| Rust parsing | Changed Rust files parsed with existing stable rustfmt, emitting to stdout only |
| Full application Rust/integration suite | Not run: requires application compilation, reserved to owner/CI |
| Clean Windows/macOS/Linux onboarding | Not run; CI definition is not CI execution evidence |
| Native launch, voice, permissions, credentials, restart, installer | Not run; owner-operated verification remains pending |

An attempted offline full lockfile regeneration failed on an unrelated yanked chacha20/rand resolution. It was not used to replace the dependency set. Cargo metadata had already resolved the targeted Secret Service additions and the retained lockfile then passed locked metadata validation. No application compilation was attempted. Pinned-toolchain rustfmt was absent; parsing was repeated successfully with the already-installed stable rustfmt. Neither condition is hidden as a successful build.

## Owner-operated acceptance checklist

Use a separate test machine/OS account; do not copy personal data into the contributor profile.

1. Clone with only Git/Node ready. Run doctor, decline setup once, then approve the displayed preparation. Retain logs and exact source revision.
2. Complete any OS installer, rerun if asked, and build twice. Confirm Cargo/npm lockfiles remain unchanged and no private data/training repository is required.
3. Confirm contributor data/keychain/WebView identity and outputs differ from personal. Existing personal files and logon task must remain unchanged.
4. Start contributor with no other session. Confirm visible UI/logs, managed Python selection and sensible missing-service explanations.
5. With another NEXUS/known sidecar present, start must refuse without terminating or adopting it. Repeat with an unknown listener and failed process enumeration.
6. Edit source: start must reject stale provenance. Replace executable: start must reject it. Wrong-profile records must be refused.
7. Verify development mode, keyboard recovery and OS permissions on actual hardware. Test persistent credentials across restart only with disposable own accounts.
8. Package personal builds separately; verify installation/resources and recipient Python behavior on a clean machine. macOS signing is a separate release-secret check.

Windows x64, macOS 15+ Apple Silicon and Ubuntu 24.04 x64 X11 remain qualification targets, not proven support. Intel/older macOS, Windows ARM, other distributions and Wayland are unqualified. Fresh cross-platform Python installation, portable contributor artifacts, simultaneous desktop profiles and production OAuth callback compatibility are not claimed.

## Final handoff notes

Frontend/Worker TypeScript checks passed. PowerShell and Unix wrappers parsed without execution. Rust source parsed without application compilation. Unix executable-bit repairs for `nexus` and `install.sh` are staged together with their new safe wrapper contents; the other source changes remain in normal working-tree edits and no commit was created. Pre-existing concurrent source edits were preserved; HEAD changed during the session due to other work, so this is a dirty-tree source checkpoint, not a frozen assessed release.

The native source-contract mutations prove the inspected guard/namespace removals are detected, not that the complete native application passes type checking or live tests. The standalone Rust identity tests compile only the small path/identity helper. All other device, package, signing and full-native gates above remain pending.
