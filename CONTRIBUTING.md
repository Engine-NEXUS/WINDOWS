# Develop NEXUS

The contributor workflow is implemented in source and covered by offline tests. Clean-machine builds and real-device proofs are still pending. A working build does not mean every assistant feature works on that operating system.

## First build

Install **Git and Node 22** (Node 24 is also accepted). Clone this repository and, from its root, run:

```text
node nexus.mjs doctor
node nexus.mjs build
node nexus.mjs start
```

`doctor` is read-only. `build` checks prerequisites, displays required system changes and asks before installing them. It prepares Python 3.12 in `.venv`, installs frontend dependencies from their lockfile, verifies runtime resources and compiles using Rust 1.93.1. You may need to finish a system installer and open a new terminal before retrying. Declining an installation does not terminate an existing application.

`start` requires the matching build record and managed Python. It does not rebuild, replace another running NEXUS, clear logs or stop sidecars. Stop another session yourself before starting this one. Building into the separate contributor output is allowed while another app is running; a locked output causes a build error rather than process termination.

For unattended prerequisite installation, `--yes` approves the displayed installation plan. Do not use it unless you intend those changes. On macOS without Homebrew, install Python 3.12 yourself; the CLI will explain this requirement rather than install a package manager silently.

## Supported targets and honest limits

| Target | Preparation path | Verification status |
|---|---|---|
| Windows x64 | MSVC C++ tools, Windows SDK, WebView2, LLVM; winget installation plan | Source/offline checks only; clean-machine/device checks pending |
| macOS 15+ Apple Silicon | Apple command-line tools, Python 3.12, Rust; Homebrew Python if available | Source/offline checks only; clean-machine/device checks pending |
| Ubuntu 24.04 LTS x64, X11 | Compiler, WebKitGTK, audio, D-Bus and other development libraries; apt installation plan | Source/offline checks only; headless CI is not an X11 device proof |
| Intel/older macOS, Windows ARM, other Linux distributions, Wayland | Experimental or missing compatible local-speech dependency | Not qualified; do not promise feature parity |

The pinned Moonshine 0.1.5 macOS wheel requires macOS 15+ and Apple Silicon. Linux needs a desktop Secret Service for persistent credentials. A locked/unavailable service must not be mistaken for a working credential store. Microphone, Accessibility, Screen Recording and other OS permissions remain real-device checks.

## Commands

| Command | Behavior |
|---|---|
| `node nexus.mjs setup` | Prepare prerequisites/dependencies, without compiling or launching |
| `node nexus.mjs build` | Prepare and build the isolated contributor executable |
| `node nexus.mjs start` | Verify and launch that existing build |
| `node nexus.mjs dev` | Prepare and run Tauri/Vite development mode; refuses another active session |
| `node nexus.mjs doctor` / `check` | Read-only environment/capability report; no cloud account probes |
| `node nexus.mjs test` | CLI/provenance tests, frontend tests, then Rust library/integration tests; run after preparation/build |
| `node nexus.mjs clean` | Remove only the selected profile output after checking for an active session; preserve personal installation and venv |
| `node nexus.mjs install` | Prepare and register a user-local `nexus` launcher; no build or launch |
| `node nexus.mjs package --profile personal` | Build a distributable personal installer using the lockfile-pinned Tauri CLI |

Registration writes `~/.local/bin/nexus` or `%USERPROFILE%\.local\bin\nexus.cmd`. Add that directory to your user PATH and reopen the terminal if necessary. It does not overwrite system PATH. The command points at this checkout; moving it requires registration again. `node nexus.mjs` remains the portable entry point, and `sh ./nexus` works without executable permission.

`start --build` is deliberately rejected. Run `build` and `start` separately. The old `scripts/run.ps1` is a compatibility wrapper; `-Build` explicitly runs those two commands in sequence. Windows start no longer requires PowerShell 7 or a browser debugging port. Legacy live/training/feature utilities outside this table have not been qualified for contributor use.

## Personal data stays separate

The default contributor build has:

- application/credential identity `com.nexus.assistant.contributor`;
- deep-link scheme `nexus-dev`;
- its own settings, accounts, voice profile, memory encryption keys, logs and WebView storage;
- output under `src-tauri/target-contributor/`;
- no `admin-brain` feature and no personal autostart-task registration/removal.

No personal data is copied or migrated. Contributor data is shared between this user's contributor checkouts; multiple concurrent desktop sessions are unsupported. The CLI checks existing NEXUS processes and known sidecar ports, and contributor binaries reserve a session port at startup. An unknown listener is a conflict, not a helper to adopt.

`--profile personal` is explicit and uses the existing personal application/credential identity, with local build output under `src-tauri/target-personal/`. Do not launch it unless you intend to use your personal data. `--admin-brain` additionally requires `--profile personal`; private admin scripts/models are never prerequisites for contributors.

Checkout builds embed the absolute `.venv` interpreter path. Moving the checkout requires rebuilding. Distributable packaging requires `--profile personal` and does **not** embed the build machine's Python path. Recipient-side local speech still requires a separately available compatible Python environment; this change does not bundle Python. Packaging options are `--target <triple>` and `--bundles <formats>`. Cross-target SDKs/toolchains must already be installed. For macOS signing, supply your own `APPLE_SIGNING_IDENTITY` and signing/notarization credentials in the release environment. The placeholder identity in the base config is overridden for local builds.

## Accounts, services and models

No API keys, owner login, Worker deployment or private training data is required to compile or run offline tests. Real cloud features need your own keys/accounts and, where applicable, a development Worker/OAuth configuration. Contributor OAuth uses `nexus-dev`; public production callbacks are not evidence of developer-account support. Never request or copy the owner's credentials.

The runtime wake/NLU models are tracked. Do not train models to fix a missing checkout resource. Local training outputs are not promoted during setup/build. Hand Mode assets are optional downloads; missing assets must be reported as unavailable. Packaging maps current server entrypoints to their resource destinations without rewriting tracked models.

The Python runtime lock pins 45 direct/transitive distributions, with Windows/macOS/Linux marker branches, based on the inspected Python 3.12 environment plus official uvloop metadata. Setup audits installed versions and retries on drift; pip check and required imports must pass before a success stamp is written. Fresh installations on all three platforms remain unverified; a lockfile is not installation proof.

## Tests and contribution rules

For the launcher-only work, without compiling or launching the application:

```text
node --test scripts/contributor-cli.test.mjs scripts/contributor-contract.test.mjs scripts/build-provenance.test.mjs
node scripts/contributor-mutation-check.mjs
```

UI-only contributors can run `npm --prefix frontend ci` and `npm --prefix frontend test` without a native build. Keep changes narrow, preserve unrelated working-tree edits, test the relevant subsystem and state what you did not verify. Pull requests should explain the observed problem, resulting behavior and tests.

The Contributor setup workflow runs offline contracts on Node 22/24 across three operating systems. Its manual qualification job builds twice from a clean checkout and runs native/frontend tests. It never launches the assistant or claims voice/device proof. Device acceptance and phase evidence: [implementation record](docs/changes/2026-11-05-contributor-setup.md).
