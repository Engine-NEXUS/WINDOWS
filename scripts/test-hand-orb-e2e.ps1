<#
  Hand-mode orb check on the REAL release exe: the orb stays on the display and is a circle, a triangle or a square.

  Usage (the exe must be built; NEXUS must not be running):
    pwsh scripts/test-hand-orb-e2e.ps1 [-Exe <path to nexus.exe>] [-Out <folder>] [-Port 9336]

  It does not use the camera. It sends the `hand:orb` events the app sends (hand mode on, pointer
  armed, pointer switched off by a held palm) and captures the top-centre of the real desktop each time:
    1. no hand mode: the orb is not on the display (nothing was said to it);
    2. hand mode on, pointer off: an orb is shown (circle);
    3. pointer armed: the picture changes (triangle);
    4. held palm: the picture changes again (square);
    5. armed again: back to the triangle picture;
    6. hand mode over (`hand:stopped`): the orb is gone again.
  Pictures are written to -Out so the shapes can be looked at. Exit code 0 only when every check passes.
#>
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\..\ULTRON-pr46\target\release\nexus.exe'),
    [string]$Out = (Join-Path $env:TEMP 'hand-orb-e2e'),
    [int]$Port = 9336
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class HandOrbGdi {
  [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
  [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleBitmap(IntPtr dc, int w, int h);
  [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc, IntPtr o);
  [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr d, int x, int y, int w, int h, IntPtr s, int sx, int sy, uint rop);
  [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
  public static IntPtr Grab(int x, int y, int w, int h) {
    IntPtr sc = GetDC(IntPtr.Zero); IntPtr mc = CreateCompatibleDC(sc); IntPtr bm = CreateCompatibleBitmap(sc, w, h);
    IntPtr old = SelectObject(mc, bm);
    BitBlt(mc, 0, 0, w, h, sc, x, y, 0x00CC0020u | 0x40000000u); // SRCCOPY | CAPTUREBLT (layered windows too)
    SelectObject(mc, old); DeleteDC(mc); ReleaseDC(IntPtr.Zero, sc);
    return bm;
  }
}
'@
$results = New-Object System.Collections.ArrayList
function Check($name, [bool]$ok, $detail) {
    [void]$results.Add([pscustomobject]@{ name = $name; ok = $ok; detail = "$detail" })
    Write-Host ("[{0}] {1} {2}" -f $(if ($ok) { 'PASS' } else { 'FAIL' }), $name, $detail)
}
New-Item -ItemType Directory -Force $Out | Out-Null
if (Get-Process nexus -ErrorAction SilentlyContinue) { throw 'NEXUS is running: stop it first.' }

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
# send an app-wide event, as the hand window does
function Emit($ws, $event, $payloadJson) {
    try { $hit = @(Page-List | Where-Object { $_.url -match 'stage.html' }); if ($hit.Count) { $ws = [string]$hit[0].webSocketDebuggerUrl } } catch {}
    [void](Cdp-Eval $ws "window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'$event',payload:$payloadJson}).then(()=>'ok').catch(e=>String(e))")
}
# Emit the state again and again, as the hand window does while it runs (the orb treats a state older than 5 s as gone)
function Hold($ws, $payloadJson, $ms) {
    $end = (Get-Date).AddMilliseconds($ms)
    while ((Get-Date) -lt $end) { Emit $ws 'hand:orb' $payloadJson; Start-Sleep -Milliseconds 700 }
}
# a freshly loaded stage page: the listeners of the page that was loaded while the app was still starting can be missing
function Reload-Stage($ws) {
    $c = New-Object System.Net.WebSockets.ClientWebSocket
    $c.ConnectAsync([Uri]$ws, [Threading.CancellationToken]::None).Wait()
    $b = [Text.Encoding]::UTF8.GetBytes('{"id":1,"method":"Page.reload","params":{}}')
    $c.SendAsync([ArraySegment[byte]]$b, 'Text', $true, [Threading.CancellationToken]::None).Wait()
    Start-Sleep -Milliseconds 300
    $c.Dispose()
    Start-Sleep -Seconds 6
}
$sw = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width
$x0 = [int](($sw - 420) / 2); $w = 420; $h = 170
function Shot($name) {
    Start-Sleep -Milliseconds 300
    $img = [System.Drawing.Bitmap][System.Drawing.Image]::FromHbitmap([HandOrbGdi]::Grab($x0, 0, $w, $h))
    $img.Save((Join-Path $Out $name)); $img
}
# a rough, comparable fingerprint of a picture: its bright pixels' bounding box and a 12x8 brightness grid
function Fingerprint($img) {
    $minx = 9999; $maxx = 0; $miny = 9999; $maxy = 0; $n = 0
    $cells = New-Object 'double[]' 96
    for ($y = 8; $y -lt 112; $y += 2) {
        for ($x = 110; $x -lt 310; $x += 2) {
            $c = $img.GetPixel($x, $y)
            $l = ($c.R + $c.G + $c.B) / 3
            if ($l -gt 200) { $n++; if ($x -lt $minx) { $minx = $x }; if ($x -gt $maxx) { $maxx = $x }; if ($y -lt $miny) { $miny = $y }; if ($y -gt $maxy) { $maxy = $y } }
            $cells[[int]([math]::Floor(($y - 8) * 8 / 104)) * 12 + [int]([math]::Floor(($x - 110) * 12 / 200))] += $l
        }
    }
    # the orb sits in a black capsule: how much of the middle of the picture is black
    $dark = 0; $tot = 0
    for ($y = 12; $y -lt 90; $y += 3) { for ($x = 150; $x -lt 270; $x += 3) { $c = $img.GetPixel($x, $y); $tot++; if (($c.R + $c.G + $c.B) / 3 -lt 12) { $dark++ } } }
    [pscustomobject]@{ bright = $n; dark = [math]::Round($dark / $tot, 2); box = "$minx,$miny-$maxx,$maxy"; cells = $cells }
}
function Distance($a, $b) { $d = 0.0; for ($i = 0; $i -lt 96; $i++) { $d += [math]::Abs($a.cells[$i] - $b.cells[$i]) }; $d }

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
$p = Start-Process $Exe -PassThru -RedirectStandardOutput (Join-Path $Out 'run.log') -RedirectStandardError (Join-Path $Out 'run.err')
try {
    $end = (Get-Date).AddSeconds(150); $ws = $null
    while ((Get-Date) -lt $end -and -not $ws) {
        try { $hit = @(Page-List | Where-Object { $_.url -match 'stage.html' }); if ($hit.Count) { $ws = [string]$hit[0].webSocketDebuggerUrl } } catch {}
        if (-not $ws) { Start-Sleep -Milliseconds 500 }
    }
    if (-not $ws) { throw 'the stage page did not appear' }
    Start-Sleep -Seconds 15   # the startup greeting ends; the orb hides again, and the page has registered its listeners

    # the startup greeting's orb leaves by itself; wait for it (up to 60 s) before taking the baseline
    $wait = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $wait) { $probe = Shot 'probe.png'; $fpp = Fingerprint $probe; $probe.Dispose(); if ($fpp.dark -lt 0.3) { break }; Start-Sleep -Seconds 2 }
    $base = Shot '0-before.png'; $fp0 = Fingerprint $base; $base.Dispose()
    Check '1 without hand mode no orb is on the display' ($fp0.dark -lt 0.3) "black capsule share: $($fp0.dark)"

    Hold $ws '{"active":true,"armed":false,"palm_paused":false}' 2600
    $i1 = Shot '1-circle.png'; $f1 = Fingerprint $i1; $i1.Dispose()
    Check '2 hand mode on: the orb is shown (circle)' ($f1.dark -gt 0.6 -and $f1.bright -gt 20) "black capsule share: $($f1.dark), bright pixels: $($f1.bright)"

    Hold $ws '{"active":true,"armed":true,"palm_paused":false}' 2600
    $i2 = Shot '2-triangle.png'; $f2 = Fingerprint $i2; $i2.Dispose()
    Check '3 pointer armed: the picture is a different shape (triangle)' ((Distance $f1 $f2) -gt 3000) "difference $([int](Distance $f1 $f2)), box $($f2.box)"

    Hold $ws '{"active":true,"armed":false,"palm_paused":true}' 2600
    $i3 = Shot '3-square.png'; $f3 = Fingerprint $i3; $i3.Dispose()
    Check '4 held palm: another shape (square)' ((Distance $f2 $f3) -gt 3000 -and (Distance $f1 $f3) -gt 3000) "difference from triangle $([int](Distance $f2 $f3)), from circle $([int](Distance $f1 $f3)), box $($f3.box)"

    Hold $ws '{"active":true,"armed":true,"palm_paused":false}' 2600
    $i4 = Shot '4-triangle-again.png'; $f4 = Fingerprint $i4; $i4.Dispose()
    Check '5 armed again: the triangle picture is back' ((Distance $f2 $f4) -lt (Distance $f2 $f3) / 2) "difference from first triangle $([int](Distance $f2 $f4))"

    Emit $ws 'hand:stopped' 'null'
    Start-Sleep -Milliseconds 1000   # the triangle morphs back to the circle first
    $ix = Shot '4b-exit-circle.png'; $fx = Fingerprint $ix; $ix.Dispose()
    Check '6a exiting hand mode: the orb is still shown and is the circle again' ($fx.dark -gt 0.6 -and (Distance $f1 $fx) -lt (Distance $f2 $fx)) "black capsule share: $($fx.dark), distance to circle $([int](Distance $f1 $fx)), to triangle $([int](Distance $f2 $fx))"
    Start-Sleep -Seconds 3
    $i5 = Shot '5-after.png'; $f5 = Fingerprint $i5; $i5.Dispose()
    Check '6b then the orb has left the screen' ($f5.dark -lt 0.3) "black capsule share: $($f5.dark)"
}
finally {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
$results | ConvertTo-Json | Set-Content (Join-Path $Out 'results.json')
$failed = @($results | Where-Object { -not $_.ok }).Count
Write-Host "$($results.Count - $failed) of $($results.Count) checks passed. Pictures, log and results: $Out"
exit ([int]($failed -gt 0))
