# Starts a CONTRIBUTOR-profile NEXUS (separate identity, data folder, keychain and session lock) on its own hidden Windows
# desktop, in quiet mode, WITHOUT touching a NEXUS the owner is running: unlike run-hidden-test.ps1 it never stops
# another nexus process.
#
#   build:  cd src-tauri; $env:CARGO_TARGET_DIR='D:\nexus-screen-target'; cargo build --features custom-protocol,contributor
#   start:  scripts/run-isolated-test.ps1
#   stop:   scripts/run-isolated-test.ps1 -Stop        (stops only the pid this script started)
#
# Its data lives in %APPDATA%\com.nexus.assistant.contributor, so it has NO API keys of the owner's: anything that needs a
# Groq / Deepgram / Gemini key is not exercised. It will refuse to start if one of the sidecar ports (39217-39219,
# 39222) is already in use (the build's own rule), i.e. when the owner's speech server happens to be up.
param([switch]$Stop, [string]$Exe = "D:\nexus-screen-target\debug\nexus.exe", [int]$Port = 9333, [string]$Log = "D:\nexus-isolated.log")

$pidFile = Join-Path $env:TEMP "nexus-isolated-test.pid"
if ($Stop) {
    if (Test-Path $pidFile) {
        $p = [int](Get-Content $pidFile)
        $exePath = (Resolve-Path $Exe).Path
        $mine = Get-Process nexus -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exePath }
        foreach ($m in $mine) { Stop-Process -Id $m.Id -Force; Write-Output "stopped isolated test instance $($m.Id)" }
        $wrapper = Get-Process -Id $p -ErrorAction SilentlyContinue
        if ($wrapper -and $wrapper.ProcessName -eq "cmd") { Stop-Process -Id $p -Force }
        if (-not $mine) { Write-Output "no isolated test instance was running (the owner's NEXUS is never touched: only $exePath is stopped)" }
        Remove-Item $pidFile -Force
    } else { Write-Output "no isolated test instance recorded" }
    return
}
if (-not (Test-Path $Exe)) { throw "build first: $Exe not found" }
$exeFull = (Resolve-Path $Exe).Path
if (Get-Process nexus -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exeFull }) { throw "an instance of $exeFull is already running" }
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class IsolatedDesktop {
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
  public struct STARTUPINFO { public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
    public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
    public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError; }
  [StructLayout(LayoutKind.Sequential)]
  public struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr CreateDesktop(string name, string device, IntPtr devmode, int flags, uint access, IntPtr sa);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool CreateProcess(string app, string cmd, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env,
    string cwd, ref STARTUPINFO si, out PROCESS_INFORMATION pi);
  public static int Start(string cmdline, string cwd, string desktop) {
    IntPtr d = CreateDesktop(desktop, null, IntPtr.Zero, 0, 0x10000000, IntPtr.Zero);
    if (d == IntPtr.Zero) throw new Exception("CreateDesktop failed: " + Marshal.GetLastWin32Error());
    var si = new STARTUPINFO(); si.cb = Marshal.SizeOf(si); si.lpDesktop = "WinSta0\\" + desktop;
    PROCESS_INFORMATION pi;
    if (!CreateProcess(null, cmdline, IntPtr.Zero, IntPtr.Zero, false, 0, IntPtr.Zero, cwd, ref si, out pi))
      throw new Exception("CreateProcess failed: " + Marshal.GetLastWin32Error());
    return pi.dwProcessId;
  }
}
"@
# The isolated copy has its own data folder but the SAME home folder as the owner's NEXUS: left alone, its "connect coding
# agents automatically" step (agentAutoHook, 25 s after start) would look at the owner's real ~/.claude, ~/.cursor, ...
# settings. Found 2026-11-24 in the test log ("[AUTOHOOK] Codex not hooked up"); nothing was changed (every hook was
# already installed), but a test copy must never write there. Switch it off in the test profile BEFORE it starts.
$profileDir = Join-Path $env:APPDATA "com.nexus.assistant.contributor"
New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
$settingsFile = Join-Path $profileDir "settings.json"
$cfg = if (Test-Path $settingsFile) { try { Get-Content $settingsFile -Raw | ConvertFrom-Json } catch { $null } } else { $null }
if (-not $cfg) { $cfg = [pscustomobject]@{} }
$cfg | Add-Member -NotePropertyName agentAutoHook -NotePropertyValue $false -Force
$cfg | Add-Member -NotePropertyName agentMonitor -NotePropertyValue $false -Force
# UTF-8 WITHOUT a byte-order mark: Windows PowerShell 5.1 Set-Content -Encoding UTF8 writes one, and a BOM stopped the app starting
[System.IO.File]::WriteAllText($settingsFile, ($cfg | ConvertTo-Json -Depth 20), (New-Object System.Text.UTF8Encoding($false)))
$env:NEXUS_QUIET = "1"
$env:RUST_LOG = "info,nexus=debug"
$env:NEXUS_PROFILE = "contributor"
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
# cmd.exe writes the app's own log lines to a file, so a test can read what NEXUS says
if (Test-Path $Log) { Remove-Item $Log -Force }
$cmdline = 'cmd.exe /c ""' + $exeFull + '" > "' + $Log + '" 2>&1"'
$procId = [IsolatedDesktop]::Start($cmdline, (Split-Path $exeFull), "NexusIsolatedTest")
Set-Content -Path $pidFile -Value $procId
Write-Output "isolated NEXUS (contributor profile) started on hidden desktop 'NexusIsolatedTest', pid $procId, debug port $Port, quiet mode, log $Log"
