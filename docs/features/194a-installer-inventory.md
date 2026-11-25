# Installer inventory (plan 194, P0)

Date: 2026-11-16. Read from the source, **not** from a run: `src-tauri/tauri.conf.json`, `src-tauri/installer/nexus-installer.nsi`, `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`, `.github/workflows/*`. Every row names where it came from. Nothing here was built or installed.

## 1. What the installer is

| Item | Value | Source |
| --- | --- | --- |
| Formats | NSIS setup `.exe` (custom template) and MSI | `tauri.conf.json` `bundle.targets = all`, `windows.nsis.template` |
| Install mode | per user (`currentUser`), no administrator rights | `windows.nsis.installMode` |
| Compression | LZMA, English only, no language picker | `windows.nsis` |
| WebView2 | downloaded bootstrapper if WebView2 is missing (skipped on update) | `webviewInstallMode`; NSIS ~line 588 (`go.microsoft.com`) |
| Product name / id / version | NEXUS / `com.nexus.assistant` / `0.1.0` | `tauri.conf.json` |
| Signing | **none** (no `signCommand`, no certificate) | `tauri.conf.json`, workflows |
| Updater | **none** (no `tauri-plugin-updater`) | `Cargo.toml` |

## 2. What it puts on the machine

| Step | What happens | Source |
| --- | --- | --- |
| Running app | closes a running NEXUS first (`CheckIfAppIsRunning`, RestartManager) | NSIS `Section Install`; change 154 |
| Files | `nexus.exe` plus the listed resources (wake word models, VAD, smart-turn, `stt_server.py`, `nlu_server.py`, NLU model, `agent_watch_sidecar`, activity-manager extension) | `bundle.resources` |
| Python | uses a real Python if found (PATH minus the Microsoft Store stub, then registry 3.11/3.12/3.13, then `%LOCALAPPDATA%\Programs\Python\`); otherwise downloads **Python 3.12.7** and installs it per user with `PrependPath=1` | NSIS `InstallPythonAndDeps` |
| Python packages | one `pip install -r` of the bundled `resources/server/requirements-installer.txt` (version-bounded ranges, `uvicorn[standard]` so the STT live-caption WebSocket works). Before plan 194: two `pip install` lines with unbounded names. Exact, hash-checked pins are still open | NSIS `InstallPythonAndDeps`; `requirements-installer.txt` |
| Deep link | registers the `nexus://` protocol (current user) | NSIS, `plugins.deep-link` |
| Uninstaller | `uninstall.exe`, Add/Remove Programs entry (name, version, publisher, size) | NSIS |
| Shortcut | Start menu only, no desktop shortcut; since plan 194 an existing shortcut is repointed at the new exe | NSIS `CreateOrUpdateStartMenuShortcut` |
| Removed in plan 194 | the Kokoro "pre-cache" (~337 MB from `github.com/8b-is/kokoro-tiny/raw/main/...` into `%USERPROFILE%\.cache\k`). Nothing in the app reads that folder; the offline voice comes from `tts_swap.rs` (pinned revision, SHA-256) | NSIS history; `grep` of `src-tauri/src` |

## 3. What the app itself does after install

| Item | What happens | Source |
| --- | --- | --- |
| Autostart | at every start the app registers a scheduled task `NEXUS` (at log on, `--background`) pointing at the **running** exe, unless `autostart` is false in settings.json (default on) | `lib.rs` ~633, `commands.rs` ~3040 |
| Settings, memory, voice profile, keychain entries | created by the app under `%APPDATA%\com.nexus.assistant\` and Windows Credential Manager, not by the installer | AGENTS.md; `memcore`, `auth_vault` |
| Offline voice and model files | downloaded by the app on first online run | `tts_swap.rs` |

## 4. What uninstall does

| Item | Result | Source |
| --- | --- | --- |
| Program files, resources, deep link | removed | NSIS `Section Uninstall` |
| Autostart | `HKCU\...\Run\NEXUS` value and the `NEXUS` scheduled task removed (not during an update) | NSIS ~line 1078 |
| Start menu shortcut | removed only if it points at this install | NSIS ~line 1026 |
| App data | removed only if the "delete app data" box is ticked | NSIS ~line 1090 |
| Python and its pip packages | **left installed** on purpose (other programs may use them) | not touched |
| `%USERPROFILE%\.cache\k` from older installers | the two files the old installer wrote (`0.bin`, `0.onnx`) are deleted, then the folder if empty (not during an update) | NSIS `Section Uninstall` |
| Keychain entries | not touched by the installer | n/a |

## 5. What must survive an upgrade

Settings (`settings.json`), the sealed memory store, voice profile, `trip_reminders.json`, keychain keys, autostart choice, and the Start-menu shortcut (repointed, not recreated). The installer writes none of these, so an in-place upgrade should keep them; **this is untested** (live case 7 in the plan).

## 6. Gaps this inventory found

| # | Gap | Status |
| --- | --- | --- |
| 1 | Release script built every installer with the admin brain | fixed in source: `build.ps1` default is the family build, `-Admin` adds it |
| 2 | `release.yml` called `build.ps1`, which without `cargo-tauri` runs a plain `cargo build` and makes **no installer**; the artifact upload was set to fail on no files | fixed upstream: `build.ps1` now runs `nexus package` (pinned Tauri CLI) |
| 3 | Python installer run with no checksum | source done (download is `https` only, hash check in the template); **hash value still `UNSET`**, see below |
| 4 | Unpinned 337 MB model download from a moving branch into a folder nothing reads | removed |
| 5 | Stale shortcut in another folder was never repointed | fixed in source, **not run** |
| 6 | pip packages unpinned (any version of `transformers`, `onnxruntime`, ... was installed) | bounded in source (`requirements-installer.txt`, gate fails on an unbounded line); exact hash-checked lock still open |
| 7 | Uninstall left the old `.cache\k` files | fixed in source (exact two files + empty folder); Python and pip packages are left on purpose |
| 13 | The `python` on this machine has no `transformers`, but `nlu_server.py` imports it for the tokenizer (`AutoTokenizer`) | **observation, not investigated**: NLU fallback may not work on this laptop; check `lazy_nlu` logs before relying on NLU |
| 8 | Product text said "remote n8n/Ollama backend" | fixed |
| 9 | Three workflows built installers three ways; one named a retired branch | fixed in source: `build-windows.yml` removed, `ci.yml` and `release.yml` share `build.ps1` (-> `nexus package`) |
| 10 | No signing, no updater, no installer smoke test, no live proof | open (P3, P4, P5) |
| 11 | WebView2 bootstrapper aborts the install on a download error with a generic message | open |
| 12 | Failure of `pip install` is logged but the install still reports success | open; should be a spoken line on first start |

## 7. The Python pin

Pinned 2026-11-16 with the owner's go-ahead. The file at `https://www.python.org/ftp/python/3.12.7/python-3.12.7-amd64.exe` was downloaded once (26,538,304 bytes), its Authenticode signature checked (`Valid`, signer Python Software Foundation, Beaverton, Oregon), and its SHA-256 written into `PYTHON_SHA256`:

`1206721601a62c925d4e4a0dcfc371e88f2ddbe8c0c07962ebb2be9b5bde4570`

The file was deleted afterwards. Changing the Python version means re-doing this and editing both `PYTHON_VERSION` and the hash; `node scripts/check-installer-pins.mjs --strict` (run by the release workflow on tags) refuses `UNSET` or a malformed value. The download took about 8 minutes at roughly 55 KB/s on this connection; the template allows 900 seconds, so a slower link would fall back to PowerShell and may still fail, which the installer reports honestly.
