param(
    [string]$BaseUrl = 'http://127.0.0.1:8788',
    [Parameter(Mandatory = $true)][string]$GatewayToken
)

$ErrorActionPreference = 'Stop'
$suffix = [Guid]::NewGuid().ToString('N').Substring(0, 12)
$phoneBase = [System.Security.Cryptography.RandomNumberGenerator]::GetInt32(1000000000, 2000000000)
$phoneA = '+1999' + $phoneBase
$phoneB = '+1999' + ($phoneBase + 1)

function Claim-Device([string]$name) {
    Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/profiles/claim" -Headers @{ 'CF-Connecting-IP' = "local-smoke-$suffix" } -ContentType 'application/json' -Body (
        @{ provisional_user_id = $name; device_name = $name; os = 'windows' } | ConvertTo-Json
    )
}

function Device-Headers($device) {
    @{
        Authorization = "Bearer $($device.device_token)"
        'X-Nexus-Profile-Id' = $device.profile_id
        'X-Nexus-Device-Id' = $device.device_id
    }
}

$a = Claim-Device "link-smoke-a-$suffix"
$b = Claim-Device "link-smoke-b-$suffix"
$gatewayHeaders = @{ Authorization = "Bearer $GatewayToken" }
Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/sms/heartbeat" -Headers $gatewayHeaders | Out-Null

foreach ($item in @(@($a, $phoneA), @($b, $phoneB))) {
    $challenge = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/phone/start" -Headers (Device-Headers $item[0]) -ContentType 'application/json' -Body (
        @{ phone = $item[1] } | ConvertTo-Json
    )
    $job = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/sms/next" -Headers $gatewayHeaders
    if (-not $job.pending -or $job.challenge_id -ne $challenge.challenge_id) { throw 'SMS outbox claim failed' }
    Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/sms/sent" -Headers $gatewayHeaders -ContentType 'application/json' -Body (
        @{ challenge_id = $job.challenge_id } | ConvertTo-Json
    ) | Out-Null
    $verified = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/phone/confirm" -Headers (Device-Headers $item[0]) -ContentType 'application/json' -Body (
        @{ challenge_id = $job.challenge_id; code = $job.code } | ConvertTo-Json
    )
    if (-not $verified.verified) { throw 'Phone verification failed' }
}

$request = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/request" -Headers (Device-Headers $a) -ContentType 'application/json' -Body (
    @{ phone = $phoneB } | ConvertTo-Json
)
$pending = Invoke-RestMethod -Method Get -Uri "$BaseUrl/v1/links/pending" -Headers (Device-Headers $b)
if ($pending.requests.Count -ne 1 -or $pending.requests[0].request_id -ne $request.request_id) {
    throw 'Recipient did not receive exactly one request'
}
$accepted = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/accept" -Headers (Device-Headers $b) -ContentType 'application/json' -Body (
    @{ request_id = $request.request_id; code = $request.code } | ConvertTo-Json
)
$list = Invoke-RestMethod -Method Get -Uri "$BaseUrl/v1/links/list" -Headers (Device-Headers $a)
if ($list.links.Count -ne 1 -or $list.links[0].link_id -ne $accepted.link_id) { throw 'Linked pair missing' }
$revoked = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/revoke" -Headers (Device-Headers $a) -ContentType 'application/json' -Body (
    @{ link_id = $accepted.link_id } | ConvertTo-Json
)
if (-not $revoked.revoked) { throw 'Link revocation failed' }
$after = Invoke-RestMethod -Method Get -Uri "$BaseUrl/v1/links/list" -Headers (Device-Headers $a)
if ($after.links.Count -ne 0) { throw 'Revoked link is still listed' }

$second = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/request" -Headers (Device-Headers $a) -ContentType 'application/json' -Body (
    @{ phone = $phoneB } | ConvertTo-Json
)
Invoke-RestMethod -Method Delete -Uri "$BaseUrl/v1/links/phone" -Headers (Device-Headers $b) | Out-Null
try {
    Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/accept" -Headers (Device-Headers $b) -ContentType 'application/json' -Body (
        @{ request_id = $second.request_id; code = $second.code } | ConvertTo-Json
    ) | Out-Null
    throw 'Unverified recipient accepted a link'
} catch {
    if ($_.Exception.Response.StatusCode.value__ -ne 403) { throw }
}
$declined = Invoke-RestMethod -Method Post -Uri "$BaseUrl/v1/links/reject" -Headers (Device-Headers $b) -ContentType 'application/json' -Body (
    @{ request_id = $second.request_id } | ConvertTo-Json
)
if (-not $declined.rejected) { throw 'Recipient could not decline the pending request' }

Write-Output 'Local Worker/D1 Device Links lifecycle passed: two phones, code acceptance, list, revoke, unverified-recipient denial, and decline.'
