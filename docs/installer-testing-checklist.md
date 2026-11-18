# NEXUS installer: live test script (plan 194, P5)

Rewritten 2026-11-16. The old checklist described Piper-era voices, a retired branch and "Python 3.10+ already installed", and could not be used as written. This is the owner-run script behind the **Live-proven** score of the installer. Plan: [194](features/194-installer-release-pipeline-above-4-stars-plan.md). What the installer does: [194a](features/194a-installer-inventory.md).

**Rules:** use Windows Sandbox, a fresh VM, or a spare account for the clean-machine cases; never test on a machine whose settings you cannot lose without a backup. Record each case as one line in `docs/testing/live-proof-ledger.md` with the build identity and the log under `docs/testing/runs/<date>-installer-<case>.log`.

## Fast path: the automated smoke script (cases 1, 3 and 8 in one run)

`scripts/installer-smoke.ps1` installs silently, checks about 30 things (files, Add/Remove entry and Publisher, `nexus://` link, Start-menu shortcut, Python, the pip packages, no old Kokoro cache), uninstalls, and checks that nothing is left. It writes a JSON report and exits 0 only when every check passes. **It refuses to run when NEXUS is running or already installed**, so it cannot touch a real install.

* **Windows Sandbox (recommended).** One-time, in an administrator PowerShell, then restart: `Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All`. Needs virtualization on in the BIOS. Then double-click `docs/testing/installer-sandbox.wsb` (it maps the installer read-only, the script read-only and `D:
exus-installer-results` for the report). Update the installer path and hash in that file after each rebuild.
* **Spare VM (Hyper-V, VirtualBox, VMware).** Copy the installer and `scripts\installer-smoke.ps1` into it and run: `powershell -ExecutionPolicy Bypass -File installer-smoke.ps1 -Installer <path> -ExpectedSha256 <hash> -ReportPath report.json`. Use a snapshot so each run starts clean.
* **CI.** The `windows-installer` job in `ci.yml` runs the same script after the build (informational until it has passed once) and uploads `installer-smoke-report`.

The script covers a clean online install. Cases 2, 4, 5, 6, 7, 9 and 10 still need the manual steps below.

## Before you start

* [ ] Take the installer from the `NEXUS-windows` artifact of a `release.yml` run (or `ci.yml` on `main`) and `SHA256SUMS.txt` from the same run.
* [ ] `certutil -hashfile <installer> SHA256` matches `SHA256SUMS.txt`.
* [ ] `PYTHON_SHA256` in the template is filled (it is, since 2026-11-16); case 6 needs a separate test build with a deliberately wrong value.
* [ ] Write down: installer name, size, build identity (`nexus-build.json`), date, machine.

## Cases (pass means exactly what is written)

| # | Case | Steps | Pass means |
| --- | --- | --- | --- |
| 1 | Clean Windows 11, no Python, online | Run the installer. Let it finish. Start NEXUS. Say the wake word and "what time is it". | Installs without an administrator prompt; Python 3.12 installed per user; first-run setup opens; the command is answered; time from double-click to first answer noted. |
| 2 | Store Python stub present, no real Python | Same as 1 on a machine where `python.exe` only opens the Microsoft Store. | The stub is not mistaken for Python; real Python is installed. |
| 3 | Python already installed (3.11, then 3.12, then 3.13) | Run the installer on each. | Reuses the existing Python; no second Python appears; STT and NLU start. |
| 4 | Offline install | Disconnect the network, run the installer. | Honest message naming what is missing (WebView2 or Python or pip packages); the app starts in its offline mode; nothing half-installed. |
| 5 | Interrupted download | Start the installer online, cut the network during the Python download. | A retry works; no partial Python installer is run; no stale file in `%TEMP%`. |
| 6 | Wrong checksum | Use a test build with a deliberately wrong `PYTHON_SHA256`. | The installer says the checksum did not match and **does not run** the Python installer; NEXUS itself is installed. |
| 7 | Upgrade over the old install on the owner's laptop | Back up `%APPDATA%\com.nexus.assistant\` first. Install over the 2026-08-31 build. | New exe runs; the Start-menu shortcut and the `NEXUS` scheduled task point at the new exe; settings, memory and the Voice Lock profile are intact; wake word still works. |
| 8 | Uninstall | Uninstall twice: once keeping app data, once with the delete box ticked. | No NEXUS process, shortcut or scheduled task left; app data kept or removed as chosen; the Add/Remove entry is gone. |
| 9 | Ten install and uninstall cycles | Repeat 1 and 8 ten times. | Same result each time; time per cycle recorded; no growth in leftover files. |
| 10 | SmartScreen and Defender | Download the installer through a browser and run it. | The warning (if any) is recorded exactly; with signing, the publisher name is shown. |

Cases 1, 3, 4, 7, 8 and 9 must pass on one assessed build before the installer's Live-proven score is raised.

## Quick checks after any install

* [ ] `%LOCALAPPDATA%\Programs\NEXUS` (or the chosen folder) holds `nexus.exe` and the `resources` folder.
* [ ] Start menu shows one NEXUS entry; it opens the exe in that folder.
* [ ] `schtasks /Query /TN NEXUS` shows the task (after the first start) with the same exe path.
* [ ] `%APPDATA%\com.nexus.assistant\` exists after the first start.
* [ ] The log has `autostart: scheduled task 'NEXUS' created` and no installer errors.

## Notes to fill in per run

Build identity: ______  Installer SHA-256: ______  Machine: ______  Case: ______  Result: ______  Log: ______
