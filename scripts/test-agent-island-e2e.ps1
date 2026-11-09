<#
  Agent Island end-to-end check on the REAL release exe: real mouse input, real desktop pixels, the page state over CDP.

  Usage (the exe must be built; NEXUS must not be running):
    pwsh scripts/test-agent-island-e2e.ps1 [-Exe <path to nexus.exe>] [-Out <folder for screenshots>] [-Port 9333]

  It starts the exe with the island left capturable (NEXUS_ISLAND_DEBUG=1) and a debug port, shows the island preview
  (two demo agents) and then checks:
    1. the pill is drawn and the desktop around it is untouched (no frame, no clipped halo box);
    2. the hot zone: a cursor 20 px outside the pill lifts it, a cursor 80 px away does not;
    3. hover on the pill opens the full island and the picture changes a lot (filled panel);
    4. a click inside does not close it; leaving closes it again;
    5. while the cursor is far away, a click reaches the app behind the (invisible) window.
  Exit code 0 only when every check passes. Screenshots and a log are written to -Out.
#>
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\src-tauri\target\release\nexus.exe'),
    [string]$Out = (Join-Path $env:TEMP 'island-e2e'),
    [int]$Port = 9333
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class IslandIn {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] i, int size);
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx, dy; public uint data, flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public MOUSEINPUT mi; }
  // absolute move through the input stack (a real mouse event, not just a cursor warp), 0..65535 over the primary screen
  public static void Move(int x, int y, int sw, int sh) {
    var i = new INPUT[1]; i[0].type = 0; i[0].mi.dx = (int)((x * 65535L) / (sw - 1)); i[0].mi.dy = (int)((y * 65535L) / (sh - 1)); i[0].mi.flags = 0x8001; // MOVE | ABSOLUTE
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
  public static void Click() {
    var i = new INPUT[2]; i[0].type = 0; i[0].mi.flags = 0x0002; i[1].type = 0; i[1].mi.flags = 0x0004;
    SendInput(2, i, Marshal.SizeOf(typeof(INPUT)));
  }
}
'@
$results = New-Object System.Collections.ArrayList
function Check($name, [bool]$ok, $detail) {
    [void]$results.Add([pscustomobject]@{ name = $name; ok = $ok; detail = "$detail" })
    $mark = if ($ok) { 'PASS' } else { 'FAIL' }
    Write-Host ("[{0}] {1} {2}" -f $mark, $name, $detail)
}
New-Item -ItemType Directory -Force $Out | Out-Null

# --- CDP helpers
# the debugging page list as one object per page (Invoke-RestMethod can hand back the whole array as ONE object)
function Page-List { $t = (Invoke-WebRequest "http://127.0.0.1:$Port/json/list" -UseBasicParsing).Content; foreach ($e in ($t | ConvertFrom-Json)) { $e } }
function Get-Page($pattern) {
    $hit = @(Page-List | Where-Object { $_.url -match $pattern })
    if ($hit.Count -eq 0) { return $null }
    [string]$hit[0].webSocketDebuggerUrl
}
function Get-Pages($pattern) {
    @(Page-List | Where-Object { $_.url -match $pattern } | ForEach-Object { [string]$_.webSocketDebuggerUrl })
}
function Cdp-Eval($ws, $expr) {
    if ($ws -is [array]) { throw "Cdp-Eval got an array of $($ws.Count): $($ws -join ' | ') (expr: $expr)" }
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
# the preview lasts 10 s; each phase asks for it again so the island stays up for the whole test
function Keep-Preview { Cdp-Eval $script:stage "window.__TAURI_INTERNALS__.invoke('agent_island_preview').then(()=>'ok')" | Out-Null }
# a closed island window can linger in the page list: use the first page that still answers
function Island-Eval($expr) {
    for ($try = 0; $try -lt 20; $try++) {
        foreach ($u in (Get-Pages 'agent-island')) { try { return (Cdp-Eval $u $expr) } catch { $script:lastErr = $_.ToString() } }
        Start-Sleep -Milliseconds 500   # the page is listed before it accepts connections
    }
    $listing = (Page-List | ForEach-Object { "$($_.type) $($_.url) $($_.webSocketDebuggerUrl)" }) -join "`n"
    throw "no live island page (last error: $script:lastErr)`n$listing"
}
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public class IslandWin {
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(P p, IntPtr l);
  [DllImport("user32.dll")] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  // state of the window titled "NEXUS Agents": visible flag and extended style
  public static string State() { string o = "none"; EnumWindows((h, l) => { var s = new StringBuilder(80); GetWindowText(h, s, 80); if (s.ToString() == "NEXUS Agents") { o = IsWindowVisible(h) + "|" + GetWindowLong(h, -20); return false; } return true; }, IntPtr.Zero); return o; }
}
'@
function Island-Win { $t = [IslandWin]::State() -split '\|'; if ($t.Count -lt 2) { return [pscustomobject]@{ visible = $false; ex = 0 } }; [pscustomobject]@{ visible = ($t[0] -eq 'True'); ex = [int]$t[1] } }
function Island-State($ignored) {
    Island-Eval "(()=>{const e=document.querySelector('.ai-island');if(!e)return 'none';const r=e.getBoundingClientRect();return e.className+'|'+Math.round(r.width)+'x'+Math.round(r.height)})()"
}

# --- desktop picture of a region. SRCCOPY | CAPTUREBLT: without CAPTUREBLT a plain screen copy leaves out layered
#     windows, and the island is one (it is click-through), so it would simply not be in the picture.
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class IslandGdi {
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
    BitBlt(mc, 0, 0, w, h, sc, x, y, 0x00CC0020u | 0x40000000u);
    SelectObject(mc, old); DeleteDC(mc); ReleaseDC(IntPtr.Zero, sc);
    return bm;
  }
}
'@
function Shot($x, $y, $w, $h, $path) {
    $bmp = [System.Drawing.Image]::FromHbitmap([IslandGdi]::Grab($x, $y, $w, $h))
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    [System.Drawing.Bitmap]$bmp
}
function Diff-Fraction($a, $b) {
    $n = 0; $tot = 0
    for ($y = 0; $y -lt $a.Height; $y += 2) { for ($x = 0; $x -lt $a.Width; $x += 2) {
        $p = $a.GetPixel($x, $y); $q = $b.GetPixel($x, $y); $tot++
        if ([Math]::Abs($p.R - $q.R) + [Math]::Abs($p.G - $q.G) + [Math]::Abs($p.B - $q.B) -gt 24) { $n++ }
    } }
    [double]$n / $tot
}

if (Get-Process nexus -ErrorAction SilentlyContinue) { throw 'NEXUS is running: stop it first (the test starts its own copy).' }
Add-Type -AssemblyName System.Windows.Forms
$sw = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width; $sh = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height

$env:NEXUS_ISLAND_DEBUG = '1'; $env:RUST_LOG = 'info'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
$p = Start-Process $Exe -PassThru -RedirectStandardOutput (Join-Path $Out 'run.log') -RedirectStandardError (Join-Path $Out 'run.err')
try {
    $script:stage = $null; $stage = $null
    for ($i = 0; $i -lt 60 -and -not $stage; $i++) { Start-Sleep 1; try { $stage = Get-Page 'stage.html' } catch {} }
    if (-not $stage) { throw 'the stage page never appeared' }
    Start-Sleep 20   # let the startup work finish before the island window is created
    # the desktop at the top centre BEFORE the island exists (the baseline for the 'nothing else is drawn' check)
    [IslandIn]::Move(200, ($sh - 200), $sw, $sh); Start-Sleep -Milliseconds 600
    $rx = [int](($sw - 800) / 2); $rw = 800; $rh = 460
    $base = Shot $rx 0 $rw $rh (Join-Path $Out '0-baseline.png')
    Cdp-Eval $stage "window.__TAURI_INTERNALS__.invoke('agent_island_preview').then(()=>'ok')" | Out-Null
    $isl = $null
    for ($i = 0; $i -lt 20 -and -not $isl; $i++) { Start-Sleep -Milliseconds 500; try { $isl = Get-Page 'agent-island' } catch {} }
    if (-not $isl) { throw 'the island page never appeared' }
    Start-Sleep -Milliseconds 800

    # geometry of the pill on the real desktop (page coordinates + window origin)
    $geoText = Island-Eval "(()=>{const r=document.querySelector('.ai-island').getBoundingClientRect();return [r.left,r.top,r.width,r.height,window.screenX,window.screenY,innerWidth,innerHeight].join(',')})()"
    $geo = @($geoText -split ',' | ForEach-Object { [double]$_ })
    $px = [int]($geo[4] + $geo[0]); $py = [int]($geo[5] + $geo[1]); $pw = [int][Math]::Ceiling($geo[2]); $ph = [int][Math]::Ceiling($geo[3])
    $wx = [int]$geo[4]; $wy = [int]$geo[5]; $ww = [int]$geo[6]; $wh = [int]$geo[7]
    Write-Host "pill on screen: x=$px y=$py ${pw}x$ph, window: x=$wx y=$wy ${ww}x$wh, screen ${sw}x$sh"
    $cx = $px + [int]($pw / 2)

    # park the cursor far away, take the 'before' picture of the whole window area
    [IslandIn]::Move(200, ($sh - 200), $sw, $sh); Start-Sleep -Milliseconds 700
    $w0 = Island-Win
    Check 'the island window is on screen, topmost-style, click-through while the cursor is away' ($w0.visible -and (($w0.ex -band 0x20) -ne 0) -and (($w0.ex -band 0x80) -ne 0) -and (($w0.ex -band 0x08000000) -ne 0)) ("visible=$($w0.visible) ex=0x$('{0:x}' -f $w0.ex)")
    $before = Shot $wx $wy $ww $wh (Join-Path $Out '1-collapsed.png')
    Check 'collapsed pill is drawn' ((Island-State $isl) -like 'ai-island ai-island--collapsed*') (Island-State $isl)

    Keep-Preview
    # 1. nothing is drawn except the pill and its own shadow: every pixel of the top-centre area that is not within 45 px
    #    of the pill must equal the baseline taken before the island existed (a window frame or a clipped halo box would show here)
    $now = Shot $rx 0 $rw $rh (Join-Path $Out '1b-now.png')
    $ix0 = $px - $rx - 45; $iy0 = $py - 45; $ix1 = $px - $rx + $pw + 45; $iy1 = $py + $ph + 45   # evaluated at the end, see below
    Keep-Preview
    # 2. hot zone: 80 px away does nothing, 20 px away lifts the pill
    [IslandIn]::Move($cx + 80 + [int]($pw / 2), $py + 10, $sw, $sh); Start-Sleep -Milliseconds 500
    Check 'cursor 80 px away: not hot' (-not ((Island-State $isl) -like '*ai-island--hot*')) (Island-State $isl)
    [IslandIn]::Move($px - 20, $py + [int]($ph / 2), $sw, $sh); Start-Sleep -Milliseconds 90
    $hotState = Island-State $isl
    Check 'cursor 20 px outside the pill: lifted (hot) and still collapsed' (($hotState -like '*ai-island--hot*') -and ($hotState -like '*collapsed*')) $hotState
    [void](Shot $wx $wy $ww $wh (Join-Path $Out '2-hot.png'))

    Keep-Preview
    # 3. hover on the pill opens it, the picture changes a lot
    [IslandIn]::Move($cx, $py + [int]($ph / 2), $sw, $sh); Start-Sleep -Milliseconds 900
    $openState = Island-State $isl
    Check 'hover opens the full island' ($openState -like '*expanded*') $openState
    $w1 = Island-Win
    Check 'while open the window is still visible and keeps its tool-window and no-activate style' ($w1.visible -and (($w1.ex -band 0x80) -ne 0) -and (($w1.ex -band 0x08000000) -ne 0)) ("visible=$($w1.visible) ex=0x$('{0:x}' -f $w1.ex)")
    Check 'while open the island takes the mouse (click-through is off)' (($w1.ex -band 0x20) -eq 0) ("ex=0x$('{0:x}' -f $w1.ex)")
    $after = Shot $wx $wy $ww $wh (Join-Path $Out '3-expanded.png')
    $d = Diff-Fraction $before $after
    Check 'the picture changes (more than 30% of the window area differs)' ($d -gt 0.30) ("changed fraction {0:P0}" -f $d)

    Keep-Preview
    # 4. a click inside keeps it open; leaving closes it
    [IslandIn]::Move($wx + [int]($ww / 2), $wy + [int]($wh / 2), $sw, $sh); Start-Sleep -Milliseconds 300
    [IslandIn]::Click(); Start-Sleep -Milliseconds 600
    $kept = (Island-State $isl) -like '*expanded*'
    if (-not $kept) {   # one retry: a person moving the real mouse during the run can disturb an attempt
        Keep-Preview; [IslandIn]::Move($px + [int]($pw / 2), $py + [int]($ph / 2), $sw, $sh); Start-Sleep -Milliseconds 900
        [IslandIn]::Move($wx + [int]($ww / 2), $wy + [int]($wh / 2), $sw, $sh); Start-Sleep -Milliseconds 300
        [IslandIn]::Click(); Start-Sleep -Milliseconds 600; $kept = (Island-State $isl) -like '*expanded*'
    }
    Check 'a click inside keeps it open' $kept (Island-State $isl)
    Keep-Preview; [IslandIn]::Move(200, ($sh - 200), $sw, $sh); Start-Sleep -Milliseconds 1500
    $closed = Island-State $isl
    Check 'leaving closes it' ($closed -like '*collapsed*') $closed
    [void](Shot $wx $wy $ww $wh (Join-Path $Out '4-closed.png'))

    Keep-Preview
    # 1 (evaluated now). Wait for the island to go away, take a second baseline, and ignore every pixel that changed by itself
    #    between the two baselines (the apps under it are alive). A window frame or a clipped halo box would still show.
    Start-Sleep 12
    $base2 = Shot $rx 0 $rw $rh (Join-Path $Out '0b-baseline-after.png')
    # the window outline (a 3 px ring on the window rectangle, minus the pill's own zone) and the rest of the area, separately:
    # a window frame shows as a changed ring, a clipped halo box as changed pixels inside it
    $wl = $wx - $rx; $wt = $wy; $wr = $wl + $ww; $wb = $wt + $wh
    $ringBad = 0; $ringTot = 0; $areaBad = 0; $areaTot = 0
    for ($y = 0; $y -lt $rh; $y++) { for ($x = 0; $x -lt $rw; $x++) {
        if (($x + $y) % 2 -ne 0) { continue }
        if ($x -ge $ix0 -and $x -le $ix1 -and $y -ge $iy0 -and $y -le $iy1) { continue }
        $q = $base.GetPixel($x, $y); $q2 = $base2.GetPixel($x, $y)
        if ([Math]::Abs($q.R - $q2.R) + [Math]::Abs($q.G - $q2.G) + [Math]::Abs($q.B - $q2.B) -gt 24) { continue }   # changed on its own
        $c = $now.GetPixel($x, $y)
        $d = ([Math]::Abs($c.R - $q.R) + [Math]::Abs($c.G - $q.G) + [Math]::Abs($c.B - $q.B)) -gt 24
        $onRing = (($x -ge $wl - 2 -and $x -le $wl + 2) -or ($x -ge $wr - 3 -and $x -le $wr + 2)) -and $y -ge $wt - 2 -and $y -le $wb + 2 -or
                  (($y -ge $wb - 3 -and $y -le $wb + 2) -and $x -ge $wl - 2 -and $x -le $wr + 2)
        if ($onRing) { $ringTot++; if ($d) { $ringBad++ } } else { $areaTot++; if ($d) { $areaBad++ } }
    } }
    $ringFrac = [double]$ringBad / [Math]::Max(1, $ringTot); $areaFrac = [double]$areaBad / [Math]::Max(1, $areaTot)
    Check 'no frame around the window and no halo box (steady pixels: ring under 10%, area under 2% changed)' (($ringFrac -lt 0.10) -and ($areaFrac -lt 0.02)) ("ring {0:P1} of {1}, area {2:P1} of {3}" -f $ringFrac, $ringTot, $areaFrac, $areaTot)

    # 5. the transitions are in the log
    $log = Get-Content (Join-Path $Out 'run.log') -Raw
    Check 'hover transitions are logged' ($log -match 'agent island: Collapsed -> Expanded' -and $log -match 'agent island: Expanded -> Collapsed') ''
}
finally {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
$failed = @($results | Where-Object { -not $_.ok })
$results | ConvertTo-Json | Set-Content (Join-Path $Out 'results.json')
Write-Host ("{0} checks, {1} failed. Pictures and log: {2}" -f $results.Count, $failed.Count, $Out)
exit ([int]($failed.Count -gt 0))
