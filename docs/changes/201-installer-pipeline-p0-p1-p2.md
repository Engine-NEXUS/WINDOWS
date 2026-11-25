# 201: Installer / release pipeline, first source pass (plan 194, P0, P1, P2, part of P4)

Date: 2026-11-16. **Source only: nothing was built, installed or run on a real machine.** The owner controls builds.

## What changed

| Area | Change |
| --- | --- |
| Build recipe | **Re-ported 2026-11-18 after merging origin/main.** Upstream turned `scripts/build.ps1` into a wrapper over `node nexus.mjs package --profile personal` (pinned Tauri CLI from `frontend/node_modules`, output in `src-tauri/target-personal`). That flow already builds the family installer (`custom-protocol`, no `admin-brain`) and already runs a real `tauri build`, so the earlier `npx @tauri-apps/cli` fallback is no longer needed. Kept from this work: a `-Admin` switch on the wrapper that passes `--admin-brain`. |
| Workflows | `release.yml` keeps upstream's three-platform matrix (Windows bundles now `nsis,msi`) and adds, before the build, the version gate and the download-pin gate (`--strict` on tags); a new `draft-release` job (tags only) collects the installers, writes `SHA256SUMS.txt` and creates a **draft** GitHub release. `build-windows.yml` removed. `ci.yml`: installer job runs on `main` only, uses `build.ps1`, and now reads `src-tauri/target-personal/release/bundle`; new steps run the version gate, the pin gate, their 19 tests, and `cargo check --lib --features custom-protocol` (the family build). The old `nexus-build.json` copy is gone because `package` writes no build record. |
| One version | new `scripts/sync-version.mjs` (`--check`, `--set`, `--from-tag`) for `tauri.conf.json`, `Cargo.toml` `[package]`, `frontend/package.json`. |
| Download pins | new `scripts/check-installer-pins.mjs`: one `PYTHON_SHA256` define (64 hex or `UNSET`), only allow-listed https hosts, no moving-branch downloads, no stale backend text. |
| NSIS template | Python download: https only, fail on HTTP error, time limit, partial file removed before the fallback; SHA-256 check (`certutil` + `findstr`) before the installer is run, skipped only when the pin is `UNSET`; silent mode no longer hangs on the message box. The Kokoro pre-cache (337 MB, unpinned, unused) is removed. Existing Start-menu shortcuts are repointed at the new exe. |
| pip packages | the two unbounded `pip install` lines are replaced by one `pip install -r` of the new bundled `src-tauri/resources/server/requirements-installer.txt` (bounded ranges; `uvicorn[standard]`; no unconditional `pip --upgrade pip`). A failed install now shows a message. The gate refuses any other `pip install` in the template and any requirement without an upper bound or `==`. |
| Uninstall | removes the two files the old Kokoro pre-cache wrote and the folder if empty (not during an update). Python and packages stay. |
| Product text | `shortDescription` / `longDescription` no longer mention n8n/Ollama. |
| Docs | `docs/features/194a-installer-inventory.md` (P0), live script `docs/installer-testing-checklist.md` rewritten, scorecard row and work card updated. |

## Checks run

* `node --test scripts/sync-version.test.mjs scripts/check-installer-pins.test.mjs`: 19 tests pass (7 version, 12 download/pip pins).
* Mutation checks (rule broken in a temp copy, a test must fail): allow-list, moving-branch rule, strict mode, hash format, plain http, version mismatch, tag mismatch, `[package]` scoping. First run left one survivor (`[package]` scoping); a test was added and the mutant is now killed.
* `node scripts/sync-version.mjs --check` and `node scripts/check-installer-pins.mjs`: OK, and `check-installer-pins.mjs --strict` passes now that the pin is set (before that it only warned).
* YAML of all workflows parses; `build.ps1` parses. After the re-port: `sync-version --check`, `check-installer-pins --strict`, and 44 node tests (version, pins, contributor CLI) pass. **The merged workflows and `nexus package` have not run on a runner or a real build.**

## Clean-machine install result (2026-11-17, Windows Sandbox)

* Installer rebuilt with the Publisher fix and quiet curl (`-sS`): 74,991,120 bytes, SHA-256 `d3e4906e20425b6ca35fa2631d66c8248e306e6305103b3d52f7761fdff0ef1f`, unsigned. Report: `D:\nexus-installer-results\report-sandbox.json`.
* **34 passed, 0 failed** in a fresh Windows 11 Enterprise Sandbox: install exit 0, `nexus.exe` and the server/model/sidecar files present, Add/Remove entry (name NEXUS, version 0.1.0, Publisher NEXUS), `nexus://` link, Start-menu shortcut only (no desktop shortcut), Python 3.12 installed per user, all 9 pip packages, no Kokoro cache folder; uninstall exit 0 with nothing left behind (files, registry, link, shortcut, scheduled task).
* History: run 1 timed out at 40 min (Sandbox window closed by hand); run 2 crashed (0xC0000005) at 82 s right after the Python download, fixed by quiet curl output; run 3 (this one) stalled about 20 min inside the Python install while the Sandbox session was disconnected and finished after the owner reconnected. **The recorded install time (2,424 s) is therefore not a real measurement.**
* Not covered: the app was not launched after install, no upgrade over an older install, the family build was not compiled, and the Python/pip steps were not timed on a normal connection.

## Not done / not verified

* **Compile check done (2026-11-17, owner authorised the build):** `tauri build --features custom-protocol,admin-brain --bundles nsis` with `CARGO_TARGET_DIR=D:
exus-installer-target` (so the running NEXUS and its build record were untouched) finished with exit 0 in 42 minutes (34 min Rust). `makensis` accepted the edited template with no warnings. Output `NEXUS_0.1.0_x64-setup.exe`, 74,970,376 bytes, SHA-256 `5e84cd688490a7fe79e0d315bef73c94fd6066dcc0ce1f2c13fbd7a2e335f8ac`, Authenticode `NotSigned`. The generated script contains the pinned Python hash, the bundled `requirements-installer.txt` (and its uninstall delete) and no Kokoro code. **The installer was not run:** a silent install on the owner's laptop would stop the running NEXUS and take over the `nexus://` link, and there is no Windows Sandbox here.
* **Smoke script (2026-11-17):** `scripts/installer-smoke.ps1` (silent install, about 30 checks, uninstall, JSON report; refuses to run if NEXUS is running or installed), `docs/testing/installer-sandbox.wsb` (Windows Sandbox kit) and an informational CI step. The script parses and its refusal guard was tested on this laptop (exit 2, nothing installed); **the install and uninstall paths have never run.** This machine has no Windows Sandbox (feature not enabled), Hyper-V, VirtualBox or VMware, so no clean-machine run was possible; enabling Sandbox needs an administrator PowerShell and a restart, which I did not do.
* **Found in that build:** the installer's Publisher field was empty (`CompanyName` blank); `bundle.publisher = "NEXUS"` added to `tauri.conf.json`, not rebuilt yet. The NSIS toolchain (NSIS 3.11, `nsis_tauri_utils` 0.5.3) downloaded on first use with hash validation.
* The family build (`custom-protocol` only, no `admin-brain`) has not been compiled here; the new CI step will. The 2026-11-17 build was the admin variant.
* **`PYTHON_SHA256` is now pinned** (`120672...4570`, signature checked as Valid / Python Software Foundation, owner approved the download). `check-installer-pins.mjs --strict` passes. The pin is only proven against the file once a real install runs (live case 6 uses a wrong hash on purpose).
* No Authenticode signing, no updater (decisions D1 and D4), no silent-install smoke test, exact hash-checked pip lock (the file is bounded ranges only), and none of the 10 live cases. The pip and requirements rules are covered by tests; 6 further mutations of them were run and all were killed (14 mutations in total).
* The scorecard moves only for what exists as source and tests (Built 70 to 75, Tested 30 to 45, Safety 60 to 65). Live-proven stays 35.
