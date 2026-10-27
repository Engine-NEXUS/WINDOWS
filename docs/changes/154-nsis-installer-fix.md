# NSIS Installer Fix — RestartManager Macro Missing

## Cause

The custom NSIS template (`src-tauri/installer/nexus-installer.nsi`) was forked from an older Tauri default template. The installed Tauri CLI (2.11.4) and its NSIS support code (`CheckIfAppIsRunning` macro + `nsis_tauri_utils` v0.5.3 plugin DLL) expect the `RestartManager_StartSession` / `RestartManager_RegisterFile` / `RestartManager_EndSession` macros, which are defined in `Win\RestartManager.nsh` — a file included by the upstream default template but **missing from our fork**.

When packaging, `makensis` failed with:
```
!insertmacro: macro named "RestartManager_StartSession" not found!
Error in macro CheckIfAppIsRunning on macroline 11
Error in script ...\target\release\nsis\x64\installer.nsi on line 849 -- aborting creation process
failed to bundle project: `Failed to bundle app with makensis`
```

## Versions Tried

| Tauri CLI Version | NSIS Result | MSI Result | Notes |
|-------------------|-------------|------------|-------|
| 2.11.4 (current) | ✅ Success (after fix) | ✅ Success | Fixed by adding missing `!include "Win\RestartManager.nsh"` |
| 2.11.4 (before fix) | ❌ Failed | N/A | Missing RestartManager macros |

The fix is a **small, safe template port** — only one line added. No CLI version pinning needed.

## Fix Applied

**File:** `src-tauri/installer/nexus-installer.nsi`  
**Change:** Added missing include after line 29:

```diff
 !include "Win\COM.nsh"
 !include "Win\Propkey.nsh"
+!include "Win\RestartManager.nsh"
 !include "StrFunc.nsh"
```

This matches the upstream default template (tauri-bundler v2.10.4, line 30) and provides the macros required by `nsis_tauri_utils` v0.5.3's `CheckIfAppIsRunning` macro.

All NEXUS customisations preserved:
- Python detection & installation (`InstallPythonAndDeps`)
- Kokoro TTS model pre-caching (`InstallKokoroModel`)
- White theme customization
- Reinstall/uninstall logic
- File associations & deep links
- Start Menu only (no desktop shortcut)

## Verification

### NSIS Installer
- **Path:** `src-tauri/target/release/bundle/nsis/NEXUS_0.1.0_x64-setup.exe`
- **Size:** 65,127,408 bytes (62.1 MB)
- **SHA-256:** `AD437A78D0465D1421B8335E0C8312B751C6E0F629416F1B7773CA022D80B59F`

### MSI Installer
- **Path:** `src-tauri/target/release/bundle/msi/NEXUS_0.1.0_x64_en-US.msi`
- **Size:** 83,910,656 bytes (80.0 MB)
- **SHA-256:** `4D782583DDDB8BBFADBEA038BE0814007CE8373D9C928442C5F278400A7251D4`

Both installers produced successfully with Tauri CLI 2.11.4.

## Log Files

- `D:\ULTRON-logs\nsis-try-2.11.4.log` — initial reproduction (failed, before fix)
- `D:\ULTRON-logs\nsis-try-2.11.4-fixed.log` — NSIS build after fix (success)
- `D:\ULTRON-logs\nsis-try-2.11.4-msi.log` — MSI build after fix (success)
- `D:\ULTRON-logs\integration-nsis-build-2-ci-method.log` — original CI failure log

## Decision

**Keep `npx --yes @tauri-apps/cli build` in CI** (no version pin). The one-line template fix restores compatibility with the current Tauri CLI.
