# NEXUS development, build, start and packaging

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for the full workflow and supported qualification targets.

| Command | Contract |
|---|---|
| `node nexus.mjs build` | Prepare prerequisites, locked frontend dependencies, managed Python and assets; compile an isolated contributor release |
| `node nexus.mjs start` / `run` | Verify the existing profile-aware build record and launch; never rebuild, clear logs or terminate another instance |
| `node nexus.mjs dev` | Prepare and run Tauri/Vite hot reload; another active NEXUS/sidecar causes refusal |
| `node nexus.mjs test` | Offline CLI/provenance + frontend + full Rust library/integration tests after environment preparation |
| `node nexus.mjs package --profile personal` | Create distributable installers using the frontend lockfile-pinned Tauri CLI, without a binary-only fallback |

Contributor output is `src-tauri/target-contributor/`; explicit local personal output is `src-tauri/target-personal/`. `--admin-brain` is opt-in and requires personal profile. A stale/missing build requires an explicit build command; `start --build` is rejected. Windows startup uses Node rather than requiring PowerShell 7.

Local macOS builds override the placeholder signing identity. Packaging can use `APPLE_SIGNING_IDENTITY` and owner-provided release credentials; local development requires none. Installer packaging does not bake in a CI Python path or bundle Python. Recipient local speech still needs a suitable runtime.

Clean-machine, permissions, actual launch and feature parity are unverified until owner/CI evidence exists. [Implementation record](../changes/2026-11-05-contributor-setup.md).
