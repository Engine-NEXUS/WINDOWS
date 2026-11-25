# Plan: Installer / release pipeline from 2.35 stars to above 4

Date: 2026-11-16. Status: **P0, P1, P2 and part of P4 done in source on 2026-11-16 ([change 198](../changes/201-installer-pipeline-p0-p1-p2.md)); nothing built, installed or run.** Windows only (macOS and Linux are out of scope for the installer). Source of the numbers: `docs/testing/scorecard-data.json` and work card 02. Runtime and builds stay with the owner.

| Phase | State |
| --- | --- |
| P0 inventory | Done: [194a](194a-installer-inventory.md) |
| P1 one build recipe | Done in source and the NSIS template compiled on 2026-11-17 (admin variant, installer not run): family build by default (`-Admin` for yours), one Windows `release.yml`, `ci.yml` uses the same script, checksums plus provenance on a draft release, version gate, stale text and shortcut fixed. Not compiled. |
| P2 safe install steps | Partly: Python download is https-only with a checksum check in the template, unpinned 337 MB model download removed. Python hash pinned (signature verified), pip packages version-bounded in one file (exact lock open), old cache cleaned on uninstall, uninstall cleanup open. |
| P3 signing, updater | Not started (needs D1, D4) |
| P4 tests | Partly: 19 node tests, 14 mutations killed. No silent-install smoke or upgrade test yet. |
| P5 live proof | Not started: script in [installer-testing-checklist.md](../installer-testing-checklist.md) |
| P6 comparison | Not started |

Scorecard now 51.75% (B 75, T 45, L 35, S 65, I 50); the raw numbers in section 1 below are the starting point.

## 1. Where it stands

Raw score **47.0%**, **2.35 stars**, queue position 2 of 68 (only Speaker verification is lower).

| Axis | Weight | Now | Gate | Plan target |
| --- | ---: | ---: | ---: | ---: |
| B Built | 20% | 70 | 90 | **95** |
| T Tested | 20% | 30 | 70 | **80** |
| L Live-proven | 30% | 35 | 70 | **80** |
| S Safety | 15% | 60 | 80 | **90** |
| I Industry comparison | 15% | 50 | 60 | **70** |
| **Raw** | | **47.0%** | 90% | **83.0% = 4.15 stars** |

"Above 4 stars" means raw above 80%. The floor that still clears it is B 90, T 75, L 80, S 88, I 70 = 80.7%. **L is the biggest lever** (+13.5 points on its own); it needs real clean-machine installs, so the owner has to run them.

Known gap in the scorecard: *NSIS broken locally; MSI built; start-menu shortcut points at a stale exe.* Change 154 fixed the NSIS build (missing `RestartManager.nsh` include), so that line is partly out of date; the other two parts are open.

## 2. What the code and CI show today (read, not run)

| Finding | Where | Effect |
| --- | --- | --- |
| Release script always builds with `custom-protocol,admin-brain` | `scripts/build.ps1` lines 58, 67 | The notes say family builds ship **without** `admin-brain`. Every installer built by `release.yml` therefore carries the admin brain code (it is still gated at run time). Needs a decision and a flag, see P1. |
| Three workflows build installers differently | `ci.yml` (nsis+msi, only on main and `phase-1`), `build-windows.yml` (nsis), `release.yml` (matrix incl. dmg, appimage, deb) | No single, tested way to make "the" installer. `phase-1` branch name is stale. |
| `release.yml` builds macOS and Linux installers with placeholder signing | `tauri.conf.json` `macOS.signingIdentity = "Developer ID Application: YOUR_TEAM"` | Out of scope, but a failing or unsigned matrix leg makes the whole release look broken. |
| No Windows code signing | no `signCommand`, no certificate in any workflow | SmartScreen warning on every download. Biggest trust gap. |
| No updater | no `tauri-plugin-updater` in `Cargo.toml` | Every update is a manual reinstall. A signing key variable (`TAURI_SIGNING_PRIVATE_KEY`) is passed in `release.yml` but nothing uses it. |
| NSIS downloads and installs Python 3.12.7 with no hash check | `nexus-installer.nsi` ~lines 726-735 | A tampered or broken download is run as the installer. Safety gap. |
| NSIS pre-caches the Kokoro model | `InstallKokoroModel`, `nexus-installer.nsi` ~line 812 | The notes say Kokoro files are not bundled and download on first online run, and the swap code pins SHA-256. Check the installer step matches (pinned revision and hashes) or remove it. |
| Installer test checklist is stale | `docs/installer-testing-checklist.md` | Mentions voices "Sky, Adam, Emma", Piper-era pre-warm lines, `phase-1`, "Python 3.10+ installed". Cannot be used as a live script as written. |
| Product text is stale | `tauri.conf.json` `longDescription`: "bridges to a remote n8n/Ollama backend" | Shows in installer metadata. |
| Start-menu shortcut points at a stale exe | AGENTS.md: `NEXUS.lnk` -> `%LOCALAPPDATA%\NEXUS\nexus.exe` (2026-08-31 build) | A user can launch an old build after installing a new one. `CreateOrUpdateStartMenuShortcut` exists in the template; verify it actually repoints. |
| Provenance covers the local build, not the shipped installer | `scripts/build-provenance.mjs` writes `nexus-build.json` for `target/release/nexus.exe` | No hash or provenance travels with the installer itself. |
| Version is `0.1.0` | `tauri.conf.json` | One source of truth for the version is missing (tag, `Cargo.toml`, `tauri.conf.json`, `package.json`). |

## 3. Phases

Each phase lists the work, how it moves the score, and the acceptance evidence. Phases P1 to P4 are source and CI work; P5 is the owner's live proof.

### P0. Inventory (read-only, about half a day)

* Write one table of everything the installer does on a machine: files placed, registry keys, scheduled task or autostart entry, shortcuts, downloads (Python, WebView2, Kokoro), `pip` installs, firewall or port use, uninstall behaviour. Source: the NSIS template, `tauri.conf.json`, `agent_autohook`, autostart plugin.
* Decide what must survive an upgrade (settings, keychain entries, memory store, voice profile, `trip_reminders.json`) and what an uninstall removes.
* Acceptance: the table is in this folder as `194a-installer-inventory.md`; every row cites the file and line it came from.

### P1. One build recipe (B up, S up)

1. **One workflow** (`release.yml`) builds the Windows NSIS installer (and the MSI if wanted); `ci.yml` builds the same artifact on every push to `main` so a broken template is caught before a tag. Delete or fold `build-windows.yml`. Remove the macOS and Linux legs from `release.yml` until they are in scope (or mark them `continue-on-error` and exclude from the release).
2. **Decide `admin-brain`.** Add a `-Admin` switch to `scripts/build.ps1`; the default (family) build omits `admin-brain`, the owner's build passes it. Check that `cargo check` and tests also pass without the feature. *Owner decision D2.*
3. **One version.** A small script reads the version from the tag and writes it into `tauri.conf.json`, `Cargo.toml` and `frontend/package.json`; CI fails if they disagree.
4. Fix `longDescription`, `shortDescription` and the stale `phase-1` references.
5. **Shortcut and upgrade path.** The installer repoints the Start-menu shortcut and any autostart entry to the freshly installed exe, closes a running NEXUS first (the `CheckIfAppIsRunning` macro is already there), and leaves user data alone.
6. Release job attaches a `SHA256SUMS.txt` and the `nexus-build.json` provenance record to the GitHub release.

Acceptance: a tag push produces one installer plus checksums with no manual step; `ci.yml` builds the same installer on `main`; a table test over the version files fails when they disagree.

### P2. Safe install steps (S up)

1. **Python.** Preferred: pin the installer download to an exact URL and **verify SHA-256 before running it**; abort with a clear message on mismatch. Better, if the sidecars allow: bundle an embeddable Python so the installer does not run a download at all. *Owner decision D3.*
2. Every network step (Python, WebView2 bootstrapper, Kokoro model) has: a timeout, a retry limit, a hash or signature check where one exists, and a **specific, honest error** (what failed, what still works offline, how to retry). Installer never continues silently after a failed required step.
3. Install **per user, no administrator rights** (already `installMode: currentUser`); confirm nothing writes to `HKLM` or `Program Files`.
4. Installer writes `install.log` into the app data folder (no personal data), and the app's first start reports a failed or partial install in one spoken line.
5. Uninstall removes the program, shortcuts, autostart entry and sidecar caches; **asks** before deleting settings and memory; never touches the OS keychain without asking.

Acceptance: a mutation check per safety rule (wrong hash, blocked URL, killed download, missing file) must make a scripted test fail; the uninstall leaves nothing behind except what the user chose to keep.

### P3. Trust: signing, checksums, updates (S up, I up)

1. **Authenticode signing** of the installer and the exe. Options: SignPath Foundation (free for qualifying open-source projects; eligibility to check), Azure Trusted Signing (paid, small), or a purchased code-signing certificate. Self-signing does not remove the SmartScreen warning. *Owner decision D1.*
2. **Tauri updater** with a minisign key held in GitHub secrets: signed update manifest, check at most once a day, owner can turn it off, never auto-installs without a prompt. Updates reuse the same installer.
3. Provenance: the release carries the build record (source hash, input hash, exe hash) so a user or the owner can verify what they downloaded (`node scripts/build-provenance.mjs verify` against the shipped exe).

Acceptance: installer shows the publisher name instead of "Unknown publisher"; an update from the previous release to the next one installs and keeps settings; a tampered manifest is refused.

### P4. Tests (T up)

The score's weakest axis is T = 30 ("tests: -", none counted today).

1. **Template compile test in CI**: `makensis` runs on the template on every push (already implied by the build, make it an explicit step with the log kept).
2. **Silent-install smoke test** on a `windows-latest` runner: install with `/S` into a temp directory, assert expected files and registry keys, run `nexus.exe --version` (or the existing smoke path) and uninstall; assert nothing is left.
3. **Upgrade test**: install v(N-1), write a fake settings file, install v(N), assert the settings file and shortcut target.
4. **Pure-logic tests** for the new scripts (version sync, checksum file, hash pin check).
5. **Mutation checks**: break each safety rule (skip hash check, point shortcut at old path, delete a required resource) and a test must fail.

Acceptance: at least 25 automated checks counted in the scorecard "tests" column; all mutations killed.

### P5. Live proof on real machines (L up, the 30% axis)

Owner-run, one line per case in `docs/testing/live-proof-ledger.md` with build identity and log under `docs/testing/runs/`:

| # | Case | Pass means |
| --- | --- | --- |
| 1 | Clean Windows 11 (Windows Sandbox or a fresh VM), no Python, online | Installs, first-run setup opens, one voice command works |
| 2 | Same with the Microsoft Store Python stub present | Installer does not mistake the stub for Python |
| 3 | Python already installed (3.11, 3.12, 3.13) | Reuses it, no second Python |
| 4 | Offline install | Honest message naming what is missing; app still starts in its offline mode |
| 5 | Download blocked or interrupted mid-way | Retry works; no half-installed state |
| 6 | Wrong-hash Python file (test build with a bad pin) | Install stops with the mismatch message |
| 7 | Upgrade over the stale 2026-08-31 install on the owner's laptop | New exe runs, shortcut and autostart repointed, settings and Voice Lock profile intact |
| 8 | Uninstall | No process, shortcut or autostart entry left; asks about data |
| 9 | Ten install/uninstall cycles | Same result every time; time per cycle recorded |
| 10 | SmartScreen and antivirus (Defender) on the signed installer | No warning beyond the normal one, or the warning recorded |

Acceptance: cases 1, 3, 4, 7, 8 and 9 pass on one assessed build and the ledger lines exist. This is what moves L from 35 to about 80.

### P6. Industry comparison (I up)

State the reference and measure it, do not estimate:

* Installer size and install time on a clean machine versus the default Tauri NSIS installer and one comparable desktop assistant.
* Time from double-click to first working voice command, median of 5 runs.
* Silent-install exit codes and documented switches (`/S`, `/D=`) versus the Tauri default.
* SmartScreen outcome with and without signing.

Record numbers with the number of runs. Acceptance: a short table in `docs/testing/runs/` and the scorecard `I` value updated from it.

## 4. Order and effort

| Order | Phase | Effort | Needs owner |
| --- | --- | --- | --- |
| 1 | P0 inventory | small | no |
| 2 | P1 one build recipe | medium | decision D2 |
| 3 | P2 safe install steps | medium | decision D3 |
| 4 | P4 tests (alongside P1/P2) | medium | no |
| 5 | P3 trust: signing and updater | medium | decision D1, a secret in GitHub |
| 6 | P5 live proof | owner time, about one day | yes, real machines |
| 7 | P6 comparison | small | yes, same machines |

Score effect if each phase lands: P1 and P2 take B to about 95 and S to about 85 (+7.5 points); P4 takes T to about 80 (+10); P3 takes S to 90 and helps L and I (+3); P5 takes L to about 80 (+13.5); P6 takes I to about 70 (+3). Total about +37, which reaches the 83% target. If P5 is skipped the best possible is about 69.5% (3.5 stars): **no live proof, no 4 stars.**

## 5. Decisions needed from the owner

1. **D1 Signing.** SignPath Foundation (free, needs an application and an open-source repository), Azure Trusted Signing (paid), or a certificate you buy? Without signing, SmartScreen shows a warning on every download, and the S and I scores stay lower.
2. **D2 `admin-brain`.** Should the public installer be built without it (recommended, matches the family-build rule in the notes), with a separate `-Admin` build for you?
3. **D3 Python.** Keep downloading Python in the installer (add a hash check), or bundle an embeddable Python (bigger installer, no download, fewer failure cases)?
4. **D4 Updater.** Add automatic update checks (off switch in the Hub), or keep manual reinstall for now?
5. **D5 MSI.** Keep the MSI as a second artifact, or ship only the NSIS installer?

## 6. What not to do

* Do not run a build, install or uninstall on the owner's laptop without their go-ahead; the owner controls builds and runtime.
* Do not test the installer on the hidden-desktop runner; use Windows Sandbox, a VM, or the owner's machine.
* Do not store a signing key or certificate in the repository; GitHub secrets only.
* Do not claim a score increase from CI results alone. The L axis needs the live ledger lines.
* Do not delete `build-windows.yml`, move tags or change the release workflow without a PR.

## 7. Done when

- [ ] P0 inventory file exists and every row cites source.
- [ ] One workflow builds the installer; checksums and provenance attached to the release.
- [ ] Python (and any other download) is hash-checked or bundled; failures are specific and honest.
- [ ] Installer, exe and updater manifest are signed (or D1 explicitly decides to ship unsigned and the score is capped accordingly).
- [ ] 25+ automated installer checks, all mutations killed.
- [ ] Live ledger lines for cases 1, 3, 4, 7, 8, 9 on one build.
- [ ] `scorecard-data.json` updated from evidence, `node scripts/make-work-cards.mjs` rerun, queue order rechecked, raw score above 80%.

## Sources (in the repository)

* `docs/testing/work-cards/02-installer-release-pipeline.md`, `docs/testing/scorecard-data.json`, `docs/testing/windows1-production-queue.md`
* `src-tauri/tauri.conf.json`, `src-tauri/installer/nexus-installer.nsi`, `scripts/build.ps1`, `scripts/build-provenance.mjs`
* `.github/workflows/release.yml`, `ci.yml`, `build-windows.yml`
* `docs/changes/154-nsis-installer-fix.md`, `docs/installer-testing-checklist.md`
