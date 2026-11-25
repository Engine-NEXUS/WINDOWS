# Compatibility packaging wrapper. Local compilation: node nexus.mjs build.
# Default is the FAMILY installer (personal profile, no admin-brain). -Admin adds the admin-brain feature.
param([string]$Target = "", [string]$Bundles = "", [string]$Profile = "personal", [switch]$Yes, [switch]$Admin)
$cliArgs = @("$PSScriptRoot/../nexus.mjs", "package", "--profile", $Profile)
if ($Target) { $cliArgs += @("--target", $Target) }
if ($Bundles) { $cliArgs += @("--bundles", $Bundles) }
if ($Yes) { $cliArgs += "--yes" }
if ($Admin) { $cliArgs += "--admin-brain" }
& node @cliArgs
exit $LASTEXITCODE
