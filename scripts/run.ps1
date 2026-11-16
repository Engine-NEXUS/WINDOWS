# Compatibility launcher. Does not stop existing applications, clear logs or rebuild implicitly.
param([switch]$Build, [string]$Profile = "contributor")
if ($Build) {
    & node "$PSScriptRoot/../nexus.mjs" build --profile $Profile
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& node "$PSScriptRoot/../nexus.mjs" start --profile $Profile
exit $LASTEXITCODE
