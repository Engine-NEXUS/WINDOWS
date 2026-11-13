# Starts NEXUS for testing without touching the machine's visible desktop, sound or screensaver.
#
#  - Own hidden Windows desktop (CreateDesktop): NEXUS's windows (stage, map panel, place card, sidebar) are created
#    there, never on the desktop the user sees. They take no focus and cause no input.
#  - NEXUS_QUIET=1: every playback is silent, the master volume is not changed, Windows is never asked to keep the
#    screen on, and the display is never turned off (see src-tauri/src/quiet.rs).
#  - A debug port on 127.0.0.1:9222 so a test can call the app's own commands (scripts/drive-nexus.mjs).
#
# Stop with:  scripts/run-hidden-test.ps1 -Stop
param([switch]$Stop, [string]$Exe = "$PSScriptRoot\..\src-tauri\target\release\nexus.exe", [int]$Port = 9222)

$name = "NexusHiddenTest"
if ($Stop) {
    Get-Process nexus -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Output "stopped"
    return
}
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class HiddenDesktop {
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
  public static int Start(string exe, string cwd, string desktop) {
    IntPtr d = CreateDesktop(desktop, null, IntPtr.Zero, 0, 0x10000000, IntPtr.Zero);
    if (d == IntPtr.Zero) throw new Exception("CreateDesktop failed: " + Marshal.GetLastWin32Error());
    var si = new STARTUPINFO(); si.cb = Marshal.SizeOf(si); si.lpDesktop = "WinSta0\\" + desktop;
    PROCESS_INFORMATION pi;
    if (!CreateProcess(exe, null, IntPtr.Zero, IntPtr.Zero, false, 0, IntPtr.Zero, cwd, ref si, out pi))
      throw new Exception("CreateProcess failed: " + Marshal.GetLastWin32Error());
    return pi.dwProcessId;
  }
}
"@
Get-Process nexus -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 2
$exeFull = (Resolve-Path $Exe).Path
$env:NEXUS_QUIET = "1"
$env:RUST_LOG = "info"
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
$procId = [HiddenDesktop]::Start($exeFull, (Split-Path $exeFull), $name)
Write-Output "nexus started on the hidden desktop '$name', pid $procId (quiet mode, debug port $Port)"
