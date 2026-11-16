# Compatibility packaging wrapper. Local compilation: node nexus.mjs build.
param([string]$Target = "", [string]$Bundles = "", [string]$Profile = "personal", [switch]$Yes)
$cliArgs = @("$PSScriptRoot/../nexus.mjs", "package", "--profile", $Profile)
if ($Target) { $cliArgs += @("--target", $Target) }
if ($Bundles) { $cliArgs += @("--bundles", $Bundles) }
if ($Yes) { $cliArgs += "--yes" }
& node @cliArgs
exit $LASTEXITCODE
