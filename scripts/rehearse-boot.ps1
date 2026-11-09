<#
  Rehearsal of the logon sequence on the REAL exe: early hello (new orb) -> prepared question -> an answer -> apps open.
  Measures the gaps from the timeline marks the app writes itself.

  What it does to the machine (so it only runs when you say so):
    * it SPEAKS aloud (the greeting, the question, the acknowledgement) and shows the greeting overlay at the top of the screen;
    * NEXUS must not be running: stop yours first (the script refuses otherwise, and does not start it again afterwards);
    * it opens Calculator once (the "previous session" is one harmless app: NEXUS_BOOT_REHEARSAL), then closes it.
    Your real pending session and today's briefing record are not touched.

  Usage:
    pwsh scripts/rehearse-boot.ps1 -Exe <path to nexus.exe> [-Answer "yes"] [-Briefing] [-Out <folder>] [-Port 9334]
  -Briefing also runs the daily briefing path (calendar, health, session) although it was already spoken today.
  -Answer is typed into the normal turn path (orchestrator_process), the same place a spoken answer arrives after speech-to-text.
#>
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Answer = 'yes',
    [switch]$Briefing,
    [string]$Out = (Join-Path $env:TEMP 'boot-rehearsal'),
    [int]$Port = 9334
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class RehearsalGdi {
  [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
  [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleBitmap(IntPtr dc, int w, int h);
  [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc, IntPtr o);
  [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr d, int x, int y, int w, int h, IntPtr s, int sx, int sy, uint rop);
  [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
  // SRCCOPY | CAPTUREBLT: without CAPTUREBLT layered windows (the overlay) are left out of the picture
  public static IntPtr Grab(int x, int y, int w, int h) {
    IntPtr sc = GetDC(IntPtr.Zero); IntPtr mc = CreateCompatibleDC(sc); IntPtr bm = CreateCompatibleBitmap(sc, w, h);
    IntPtr old = SelectObject(mc, bm);
    BitBlt(mc, 0, 0, w, h, sc, x, y, 0x00CC0020u | 0x40000000u);
    SelectObject(mc, old); DeleteDC(mc); ReleaseDC(IntPtr.Zero, sc);
    return bm;
  }
}
'@
New-Item -ItemType Directory -Force $Out | Out-Null
function Shot($x, $y, $w, $h, $name) { $img = [System.Drawing.Image]::FromHbitmap([RehearsalGdi]::Grab($x, $y, $w, $h)); $img.Save((Join-Path $Out $name)); $img.Dispose() }
function Page-List { $t = (Invoke-WebRequest "http://127.0.0.1:$Port/json/list" -UseBasicParsing).Content; foreach ($e in ($t | ConvertFrom-Json)) { $e } }
function Cdp-Eval($ws, $expr) {
    $c = New-Object System.Net.WebSockets.ClientWebSocket
    $c.ConnectAsync([Uri]$ws, [Threading.CancellationToken]::None).Wait()
    $m = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = $expr; returnByValue = $true; awaitPromise = $true } } | ConvertTo-Json -Depth 8 -Compress
    $b = [Text.Encoding]::UTF8.GetBytes($m)
    $c.SendAsync([ArraySegment[byte]]$b, 'Text', $true, [Threading.CancellationToken]::None).Wait()
    $sb = New-Object System.Text.StringBuilder
    do { $buf = New-Object byte[] 262144; $r = $c.ReceiveAsync([ArraySegment[byte]]$buf, [Threading.CancellationToken]::None).Result; [void]$sb.Append([Text.Encoding]::UTF8.GetString($buf, 0, $r.Count)) } while (-not $r.EndOfMessage)
    $c.Dispose()
    ($sb.ToString() | ConvertFrom-Json).result.result.value
}
function Wait-Log($pattern, $seconds) {
    $end = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $end) {
        $t = (Get-Content (Join-Path $Out 'run.log') -Raw -ErrorAction SilentlyContinue)
        if ($t -and $t -match $pattern) { return $true }
        Start-Sleep -Milliseconds 100
    }
    $false
}

if (Get-Process nexus -ErrorAction SilentlyContinue) { throw 'NEXUS is running: stop it first (this script starts its own copy and does not restart yours).' }
if (Get-Process CalculatorApp -ErrorAction SilentlyContinue) { throw 'Calculator is open: close it first (the rehearsal opens it as the "previous session").' }

$env:NEXUS_BOOT_REHEARSAL = if ($Briefing) { 'briefing' } else { '1' }
$env:RUST_LOG = 'info'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Remove-Item (Join-Path $Out 'run.log'), (Join-Path $Out 'run.err') -ErrorAction SilentlyContinue
$p = Start-Process $Exe -ArgumentList '--background' -PassThru -RedirectStandardOutput (Join-Path $Out 'run.log') -RedirectStandardError (Join-Path $Out 'run.err')
$summary = [ordered]@{ answer = $Answer; briefing = [bool]$Briefing }
try {
    # the overlay of the early hello
    if (Wait-Log 'EARLY-ORB\] first frame' 120) {
        Start-Sleep -Milliseconds 900
        $sw = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width
        Shot ([int](($sw - 700) / 2)) 0 700 150 'early-hello-overlay.png'
        $summary.early_hello_overlay = 'captured'
    } else { $summary.early_hello_overlay = 'the early hello did not start (see run.log)' }

    # the question (or the briefing) is handed to speech and played
    $got = Wait-Log 'BOOT-TIMING\] hello_end -> (question_audio_start|briefing_audio_start): (-?\d+) ms' 90
    $log = Get-Content (Join-Path $Out 'run.log') -Raw
    $m = [regex]::Match($log, 'BOOT-TIMING\] hello_end -> (question_audio_start|briefing_audio_start): (-?\d+) ms')
    if ($m.Success) { $summary.hello_end_to_speech_audio_ms = [int]$m.Groups[2].Value; $summary.first_line = $m.Groups[1].Value } else { $summary.hello_end_to_speech_audio_ms = 'not measured' }
    $q = [regex]::Match($log, '\[BOOT\] (reopen question|briefing)[^\r\n]*'); if ($q.Success) { $summary.line_log = $q.Value }

    # let the line finish, then answer through the normal turn path
    Start-Sleep -Seconds 14
    $stage = $null
    foreach ($e in (Page-List)) { if ($e.url -match 'stage.html') { $stage = [string]$e.webSocketDebuggerUrl } }
    if (-not $stage) { throw 'the stage page is not available over the debug port' }
    $expr = "window.__TAURI_INTERNALS__.invoke('orchestrator_process',{transcript:$(ConvertTo-Json $Answer -Compress),dialogContext:null,turnContext:null}).then(r=>JSON.stringify(r)).catch(e=>String(e))"
    $summary.answer_result = Cdp-Eval $stage $expr
    Start-Sleep -Seconds 4
    $log = Get-Content (Join-Path $Out 'run.log') -Raw
    foreach ($pair in @(@('yes_received -> ack_audio_start', 'ack_audio_ms'), @('yes_received -> apps_started', 'apps_started_ms'), @('apps_started -> apps_done', 'apps_launch_ms'))) {
        $mm = [regex]::Match($log, 'BOOT-TIMING\] ' + [regex]::Escape($pair[0]).Replace('\ ', ' ') + ': (-?\d+) ms')
        $summary[$pair[1]] = if ($mm.Success) { [int]$mm.Groups[1].Value } else { 'not measured' }
    }
    $summary.calculator_opened = [bool](Get-Process CalculatorApp -ErrorAction SilentlyContinue)
}
finally {
    Get-Process CalculatorApp -ErrorAction SilentlyContinue | Stop-Process -Force
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
$summary | ConvertTo-Json | Set-Content (Join-Path $Out 'summary.json')
$summary | ConvertTo-Json
Write-Host "Pictures, log and summary: $Out"
