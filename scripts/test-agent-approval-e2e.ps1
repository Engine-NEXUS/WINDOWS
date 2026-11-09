<#
  Agent approval end-to-end check on the REAL release exe, through the same webhook the agents' hook scripts use.

  Usage (the exe must be built; NEXUS must not be running; the agent hooks must have been installed once so the hook
  script with the webhook port and token exists):
    pwsh scripts/test-agent-approval-e2e.ps1 [-Exe <path to nexus.exe>] [-Out <folder>] [-Port 9335]

  It sends fake agent events (no real agent is started, nothing is executed) and checks:
    A. a request shows on the island with its real text; Allow on the island returns the "allow" answer to the hook
       and the card leaves the island;
    B. the same with Deny;
    C. answered in the agent itself (the agent reports the tool finished): the card leaves the island at once and the
       waiting hook returns nothing;
    D. a new turn / a stop of the session also clears a waiting request;
    E. another tool finishing does NOT clear a request of a different tool;
    F. a plan (ExitPlanMode) arrives as the readable text of the plan, not JSON;
    G. the island slides below the wake orb when the orb reports its capsule and back when it leaves.
  Exit code 0 only when every check passes.
#>
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\nexus.exe'),
    [string]$Out = (Join-Path $env:TEMP 'approval-e2e'),
    [int]$Port = 9335
)
$ErrorActionPreference = 'Stop'
$results = New-Object System.Collections.ArrayList
function Check($name, [bool]$ok, $detail) {
    [void]$results.Add([pscustomobject]@{ name = $name; ok = $ok; detail = "$detail" })
    Write-Host ("[{0}] {1} {2}" -f $(if ($ok) { 'PASS' } else { 'FAIL' }), $name, $detail)
}
New-Item -ItemType Directory -Force $Out | Out-Null
if (Get-Process nexus -ErrorAction SilentlyContinue) { throw 'NEXUS is running: stop it first.' }

$hook = Join-Path $env:APPDATA 'com.nexus.assistant\agent_hooks\nexus-agent-event.cmd'
if (-not (Test-Path $hook)) { throw "hook script not found: $hook (hook an agent up once)" }
$txt = Get-Content $hook -Raw
$hp = [regex]::Match($txt, '127\.0\.0\.1:(\d+)/agent-event').Groups[1].Value
$tok = [regex]::Match($txt, 'Bearer ([A-Za-z0-9_\-]+)').Groups[1].Value
if (-not $hp -or -not $tok) { throw 'could not read the webhook port/token from the hook script' }
$base = "http://127.0.0.1:$hp"

function Page-List { $t = (Invoke-WebRequest "http://127.0.0.1:$Port/json/list" -UseBasicParsing).Content; foreach ($e in ($t | ConvertFrom-Json)) { $e } }
function Get-Page($pattern) { $h = @(Page-List | Where-Object { $_.url -match $pattern }); if ($h.Count -eq 0) { $null } else { [string]$h[0].webSocketDebuggerUrl } }
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
function Invoke-Tauri($ws, $cmd, $argsJson = '{}') {
    Cdp-Eval $ws "window.__TAURI_INTERNALS__.invoke('$cmd',$argsJson).then(r=>JSON.stringify(r)).catch(e=>'ERR '+String(e))"
}
function Post-Event($event, $json) {
    Invoke-WebRequest "$base/agent-event?agent=claude&event=$event" -Method Post -Headers @{ Authorization = "Bearer $tok" } -ContentType 'application/json' -Body $json -UseBasicParsing -TimeoutSec 5 | Out-Null
}
# a permission request held open by a background job, exactly as the hook's curl does it
function Start-Request($tool, $session, $inputJson) {
    $body = '{"session_id":"' + $session + '","tool_name":"' + $tool + '","tool_input":' + $inputJson + '}'
    Start-Job -ArgumentList $base, $tok, $body -ScriptBlock {
        param($b, $t, $body)
        $sw = [Diagnostics.Stopwatch]::StartNew()
        try { $r = Invoke-WebRequest "$b/agent-approval?agent=claude" -Method Post -Headers @{ Authorization = "Bearer $t" } -ContentType 'application/json' -Body $body -UseBasicParsing -TimeoutSec 115; $out = $r.Content } catch { $out = 'ERR ' + $_ }
        [pscustomobject]@{ body = $out; ms = $sw.ElapsedMilliseconds }
    }
}
function Wait-Approvals($ws, $count, $seconds = 10) {
    $end = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $end) {
        $s = (Invoke-Tauri $ws 'agent_island_snapshot') | ConvertFrom-Json
        if (@($s.approvals).Count -eq $count) { return $s }
        Start-Sleep -Milliseconds 150
    }
    (Invoke-Tauri $ws 'agent_island_snapshot') | ConvertFrom-Json
}

$env:NEXUS_ISLAND_DEBUG = '1'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
$p = Start-Process $Exe -ArgumentList '--background' -PassThru -RedirectStandardOutput (Join-Path $Out 'run.log') -RedirectStandardError (Join-Path $Out 'run.err')
try {
    # any command can be invoked from any page; the stage page is always there
    $end = (Get-Date).AddSeconds(150); $stage = $null
    while ((Get-Date) -lt $end -and -not $stage) { try { $stage = Get-Page 'stage.html' } catch {}; if (-not $stage) { Start-Sleep -Milliseconds 500 } }
    if (-not $stage) { throw 'the stage page did not appear' }
    Start-Sleep -Seconds 3
    $ws = $stage

    # A: Allow on the island
    Post-Event 'start' '{"session_id":"sa"}'
    $jobA = Start-Request 'Bash' 'sa' '{"command":"git status"}'
    $s = Wait-Approvals $ws 1
    Check 'A request is on the island with its text' (@($s.approvals).Count -eq 1 -and $s.approvals[0].summary -eq 'git status') ($s.approvals | ConvertTo-Json -Compress)
    $id = $s.approvals[0].id
    $ok = Invoke-Tauri $ws 'agent_approval_decide' "{`"id`":$id,`"decision`":`"allow`"}"
    $ra = Receive-Job $jobA -Wait -AutoRemoveJob
    Check 'A Allow on the island answers the hook' ($ok -eq 'true' -and $ra.body -match 'allow') "$($ra.body) ($($ra.ms) ms)"
    $s = Wait-Approvals $ws 0
    Check 'A the card leaves the island' (@($s.approvals).Count -eq 0) ''

    # B: Deny on the island
    $jobB = Start-Request 'Bash' 'sa' '{"command":"rm -rf x"}'
    $s = Wait-Approvals $ws 1
    $id = $s.approvals[0].id
    [void](Invoke-Tauri $ws 'agent_approval_decide' "{`"id`":$id,`"decision`":`"deny`"}")
    $rb = Receive-Job $jobB -Wait -AutoRemoveJob
    Check 'B Deny on the island answers the hook' ($rb.body -match 'deny') "$($rb.body)"
    $s = Wait-Approvals $ws 0
    Check 'B the card leaves the island' (@($s.approvals).Count -eq 0) ''

    # C: answered in the agent itself -> its tool reports finished
    $jobC = Start-Request 'Bash' 'sa' '{"command":"npm test"}'
    $s = Wait-Approvals $ws 1
    Check 'C a request is waiting' (@($s.approvals).Count -eq 1) ''
    Post-Event 'posttool' '{"session_id":"sa","tool_name":"Bash","tool_input":{"command":"npm test"}}'
    $s = Wait-Approvals $ws 0 4
    Check 'C the card leaves the island when the agent was answered directly' (@($s.approvals).Count -eq 0) ''
    $rc = Receive-Job $jobC -Wait -AutoRemoveJob
    Check 'C the waiting hook is released at once and prints nothing' ([string]::IsNullOrWhiteSpace($rc.body) -and $rc.ms -lt 5000) "body='$($rc.body)' $($rc.ms) ms"

    # E: another tool finishing does not clear it
    $jobE = Start-Request 'Bash' 'sa' '{"command":"ls"}'
    $s = Wait-Approvals $ws 1
    Post-Event 'posttool' '{"session_id":"sa","tool_name":"Read","tool_input":{"file_path":"a.txt"}}'
    Start-Sleep -Milliseconds 800
    $s = (Invoke-Tauri $ws 'agent_island_snapshot') | ConvertFrom-Json
    Check 'E a different tool finishing keeps the request' (@($s.approvals).Count -eq 1) ''
    # D: a stop clears it
    Post-Event 'stop' '{"session_id":"sa"}'
    $s = Wait-Approvals $ws 0 4
    Check 'D a stop of the session clears the request' (@($s.approvals).Count -eq 0) ''
    [void](Receive-Job $jobE -Wait -AutoRemoveJob)

    # F: a plan is readable text
    $plan = "## Plan`n1. Fix the bug`n2. Add a test`n`nRisk: none"
    $planJson = (@{ plan = $plan } | ConvertTo-Json -Compress)
    $jobF = Start-Request 'ExitPlanMode' 'sp' $planJson
    $s = Wait-Approvals $ws 1
    $sum = [string]$s.approvals[0].summary
    Check 'F the plan arrives as the readable text of the plan' ($sum -eq $plan) $sum
    Check 'F it is not JSON' (-not $sum.Contains('{"plan"') -and -not $sum.Contains('\n')) ''
    [void](Invoke-Tauri $ws 'agent_approval_decide' "{`"id`":$($s.approvals[0].id),`"decision`":`"deny`"}")
    [void](Receive-Job $jobF -Wait -AutoRemoveJob)
    Post-Event 'stop' '{"session_id":"sp"}'

    # G: the island slides below the wake orb
    $jobG = Start-Request 'Bash' 'sg' '{"command":"echo hi"}'
    [void](Wait-Approvals $ws 1)
    Start-Sleep -Milliseconds 900
    $islandWs = Get-Page 'agent-island'
    $top0 = [double](Cdp-Eval $islandWs 'window.screenY')
    [void](Invoke-Tauri $ws 'agent_island_set_obstacle' '{"bottom":110}')
    Start-Sleep -Milliseconds 1200
    $top1 = [double](Cdp-Eval $islandWs 'window.screenY')
    Check 'G the island moves down below the orb' (($top1 - $top0) -ge 90) "y $top0 -> $top1"
    [void](Invoke-Tauri $ws 'agent_island_set_obstacle' '{"bottom":0}')
    Start-Sleep -Milliseconds 1200
    $top2 = [double](Cdp-Eval $islandWs 'window.screenY')
    Check 'G and back up when the orb leaves' ([math]::Abs($top2 - $top0) -le 2) "y $top2"
    Post-Event 'stop' '{"session_id":"sg"}'
    [void](Receive-Job $jobG -Wait -AutoRemoveJob)
}
finally {
    Get-Job | Remove-Job -Force -ErrorAction SilentlyContinue
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
$results | ConvertTo-Json | Set-Content (Join-Path $Out 'results.json')
$failed = @($results | Where-Object { -not $_.ok }).Count
Write-Host "$($results.Count - $failed) of $($results.Count) checks passed. Log and results: $Out"
exit ([int]($failed -gt 0))
