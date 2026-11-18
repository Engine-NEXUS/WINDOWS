# scripts/installer-smoke.ps1 - silent install / check / uninstall of the NEXUS installer (plan 194, P4 and P5).
#
# Run it ONLY on a clean machine: a Windows Sandbox, a spare VM, a spare Windows account, or a CI runner.
# It refuses to run when NEXUS is running or already installed, so it cannot disturb a real install.
#
#   powershell -ExecutionPolicy Bypass -File scripts\installer-smoke.ps1 `
#       -Installer C:\kit\installer\NEXUS_0.1.0_x64-setup.exe -ExpectedSha256 <hash> -ReportPath C:\kit\results\report.json
#
# It installs per user (no administrator rights) into %LOCALAPPDATA%\NEXUS, checks files, registry, shortcut,
# deep link, Python and the pip packages, then uninstalls and checks that nothing is left. Exit code 0 = all
# checks passed, 1 = at least one failed, 2 = refused to run. Works in Windows PowerShell 5.1 and PowerShell 7.

param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [string]$ExpectedSha256 = "",
  [string]$ReportPath = "",
  [string]$ExpectedVersion = "",
  [int]$InstallTimeoutMinutes = 40,
  [switch]$SkipUninstall
)

$ErrorActionPreference = "Stop"
$checks = New-Object System.Collections.ArrayList
$timings = [ordered]@{}

function Add-Check([string]$Name, [bool]$Ok, [string]$Detail = "") {
  [void]$checks.Add([pscustomobject]@{ name = $Name; ok = $Ok; detail = $Detail })
  $mark = if ($Ok) { "PASS" } else { "FAIL" }
  $color = if ($Ok) { "Green" } else { "Red" }
  Write-Host ("[{0}] {1}  {2}" -f $mark, $Name, $Detail) -ForegroundColor $color
}

function Finish([int]$Code) {
  $failed = @($checks | Where-Object { -not $_.ok }).Count
  $report = [ordered]@{
    installer = $Installer
    machine   = $env:COMPUTERNAME
    os        = (Get-CimInstance Win32_OperatingSystem).Caption
    date      = (Get-Date).ToString("s")
    passed    = @($checks | Where-Object { $_.ok }).Count
    failed    = $failed
    timings_s = $timings
    checks    = $checks
  }
  if ($ReportPath) {
    $dir = Split-Path -Parent $ReportPath
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
    ($report | ConvertTo-Json -Depth 5) | Set-Content -Encoding UTF8 $ReportPath
    Write-Host "report written to $ReportPath"
  }
  Write-Host ("{0} passed, {1} failed" -f $report.passed, $failed)
  exit $Code
}

$UninstKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NEXUS"
$DeepKey = "HKCU:\Software\Classes\nexus\shell\open\command"
$InstallDir = Join-Path $env:LOCALAPPDATA "NEXUS"

# ---- refuse to touch a real install -------------------------------------------------------------
if (-not (Test-Path -LiteralPath $Installer)) { Write-Host "installer not found: $Installer"; exit 2 }
if (Get-Process -Name nexus -ErrorAction SilentlyContinue) { Write-Host "REFUSED: NEXUS is running on this machine. Use a clean VM, Sandbox or spare account."; exit 2 }
if ((Test-Path $UninstKey) -or (Test-Path -LiteralPath (Join-Path $InstallDir "nexus.exe")) -or (Test-Path $DeepKey)) {
  Write-Host "REFUSED: NEXUS is already installed for this user. Use a clean VM, Sandbox or spare account."
  exit 2
}

# ---- 1. the file is the one that was built -----------------------------------------------------
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Installer).Hash.ToLower()
if ($ExpectedSha256) { Add-Check "installer SHA-256 matches" ($actual -eq $ExpectedSha256.ToLower()) $actual } else { Add-Check "installer SHA-256 recorded" $true $actual }
$sig = Get-AuthenticodeSignature -LiteralPath $Installer
Add-Check "installer signature (informational)" $true ("status=" + $sig.Status)

# ---- 2. install ---------------------------------------------------------------------------------
$sw = [Diagnostics.Stopwatch]::StartNew()
$proc = Start-Process -FilePath $Installer -ArgumentList @("/S", "/D=$InstallDir") -PassThru
# Progress trace every 30 s: which child processes the installer has right now (curl, python, pip ...), so a
# hang can be diagnosed from the report folder instead of guessed.
$progressPath = if ($ReportPath) { Join-Path (Split-Path -Parent $ReportPath) "install-progress.log" } else { "" }
$deadline = (Get-Date).AddMinutes($InstallTimeoutMinutes)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
  [void]$proc.WaitForExit(30000)
  if ($progressPath) {
    $kids = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
      Where-Object { $_.Name -match "curl|python|pip|powershell|certutil|makensis|Setup|NEXUS|msiexec|MicrosoftEdge" } |
      ForEach-Object { "{0}({1})" -f $_.Name, $_.ProcessId }
    ("{0}  t+{1}s  alive={2}  procs: {3}" -f (Get-Date -Format "HH:mm:ss"), [int]$sw.Elapsed.TotalSeconds, (-not $proc.HasExited), ($kids -join " ")) |
      Add-Content -Encoding UTF8 $progressPath
  }
}
if (-not $proc.HasExited) {
  try { $proc.Kill() } catch {}
  Add-Check "install finished in time" $false "timed out after $InstallTimeoutMinutes min"
  Finish 1
}
$sw.Stop()
$timings["install"] = [math]::Round($sw.Elapsed.TotalSeconds, 1)
Add-Check "install exit code 0" ($proc.ExitCode -eq 0) ("exit " + $proc.ExitCode + ", " + $timings["install"] + " s")

# ---- 3. files -----------------------------------------------------------------------------------
$exe = Join-Path $InstallDir "nexus.exe"
$exeSize = if (Test-Path -LiteralPath $exe) { (Get-Item -LiteralPath $exe).Length } else { 0 }
Add-Check "nexus.exe installed" ($exeSize -gt 10MB) ("$exeSize bytes")
foreach ($rel in @(
    "resources\server\stt_server.py", "resources\server\nlu_server.py", "resources\server\requirements-installer.txt",
    "resources\server\nlu\model\nexus_nlu.onnx", "resources\oww\nexus.onnx", "resources\agent_watch_sidecar\server.cjs",
    "uninstall.exe")) {
  Add-Check "file $rel" (Test-Path -LiteralPath (Join-Path $InstallDir $rel))
}

# ---- 4. registry and shortcut -------------------------------------------------------------------
$u = Get-ItemProperty -Path $UninstKey -ErrorAction SilentlyContinue
Add-Check "Add/Remove Programs entry" ($null -ne $u) ($(if ($u) { "name=" + $u.DisplayName + " version=" + $u.DisplayVersion } else { "missing" }))
if ($u) {
  Add-Check "Publisher is set" (-not [string]::IsNullOrWhiteSpace($u.Publisher)) ("'" + $u.Publisher + "'")
  if ($ExpectedVersion) { Add-Check "DisplayVersion matches" ($u.DisplayVersion -eq $ExpectedVersion) $u.DisplayVersion }
}
$deep = (Get-ItemProperty -Path $DeepKey -ErrorAction SilentlyContinue).'(default)'
Add-Check "nexus:// link points at this install" ($deep -and $deep.ToLower().Contains($InstallDir.ToLower())) ("" + $deep)

$lnk = @(
  (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\NEXUS.lnk"),
  (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\NEXUS\NEXUS.lnk")
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($lnk) {
  $target = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk).TargetPath
  Add-Check "Start-menu shortcut targets this install" ($target.ToLower() -eq $exe.ToLower()) $target
} else {
  Add-Check "Start-menu shortcut exists" $false "not found"
}
Add-Check "no desktop shortcut (Start menu only)" (-not (Test-Path (Join-Path ([Environment]::GetFolderPath("Desktop")) "NEXUS.lnk")))

# ---- 5. Python and packages ---------------------------------------------------------------------
$pyExe = $null
foreach ($root in @("HKCU", "HKLM")) {
  foreach ($v in @("3.12", "3.13", "3.11")) {
    $p = (Get-ItemProperty -Path "${root}:\SOFTWARE\Python\PythonCore\$v\InstallPath" -ErrorAction SilentlyContinue).'(default)'
    if (-not $pyExe -and $p -and (Test-Path -LiteralPath (Join-Path $p "python.exe"))) { $pyExe = Join-Path $p "python.exe" }
  }
}
if (-not $pyExe) {
  $cmd = Get-Command python.exe -ErrorAction SilentlyContinue | Where-Object { $_.Source -notmatch "WindowsApps" } | Select-Object -First 1
  if ($cmd) { $pyExe = $cmd.Source }
}
Add-Check "a real Python 3.11+ is installed" ([bool]$pyExe) ("" + $pyExe)
if ($pyExe) {
  $frozen = (& $pyExe -m pip list --format=freeze 2>$null) | ForEach-Object { ($_ -split "==")[0].ToLower().Replace("_", "-") }
  foreach ($pkg in @("fastapi", "uvicorn", "numpy", "onnxruntime", "pydantic", "transformers", "moonshine-voice", "python-multipart", "websockets")) {
    Add-Check "pip package $pkg" ($frozen -contains $pkg)
  }
}
Add-Check "no old Kokoro cache folder created" (-not (Test-Path (Join-Path $env:USERPROFILE ".cache\k")))

# ---- 6. uninstall -------------------------------------------------------------------------------
if (-not $SkipUninstall) {
  $uninstaller = Join-Path $InstallDir "uninstall.exe"
  if (Test-Path -LiteralPath $uninstaller) {
    $sw.Restart()
    # _?= keeps the uninstaller in place so Start-Process can wait for it (it cannot delete itself then).
    $up = Start-Process -FilePath $uninstaller -ArgumentList @("/S", "_?=$InstallDir") -PassThru -Wait
    $sw.Stop()
    $timings["uninstall"] = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    Add-Check "uninstall exit code 0" ($up.ExitCode -eq 0) ("exit " + $up.ExitCode)
    Add-Check "program removed" (-not (Test-Path -LiteralPath $exe))
    $left = @(Get-ChildItem -LiteralPath $InstallDir -Recurse -Force -ErrorAction SilentlyContinue | Where-Object { -not $_.PSIsContainer -and $_.Name -ne "uninstall.exe" })
    Add-Check "no files left behind" ($left.Count -eq 0) ("$($left.Count) left")
    Add-Check "Add/Remove Programs entry removed" (-not (Test-Path $UninstKey))
    Add-Check "nexus:// link removed" (-not (Test-Path $DeepKey))
    if ($lnk) { Add-Check "Start-menu shortcut removed" (-not (Test-Path -LiteralPath $lnk)) }
    $task = Get-ScheduledTask -TaskName "NEXUS" -ErrorAction SilentlyContinue
    Add-Check "no NEXUS scheduled task left" ($null -eq $task)
    Remove-Item -LiteralPath $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
  } else {
    Add-Check "uninstall.exe present" $false "cannot test uninstall"
  }
}

$bad = @($checks | Where-Object { -not $_.ok }).Count
Finish ($(if ($bad -eq 0) { 0 } else { 1 }))
