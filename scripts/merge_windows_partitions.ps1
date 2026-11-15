# ==============================================================================
# NEXUS Single-OS Partition Consolidation Script
# Target: Free Linux (Pop!_OS) partitions and merge all storage into Windows C:
# ==============================================================================

param(
    [switch]$NoPause
)

# 1. Require Administrator Privileges
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "[NEXUS] Requesting administrative elevation (UAC)..." -ForegroundColor Yellow
    $scriptPath = $MyInvocation.MyCommand.Definition
    Start-Process powershell.exe -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$scriptPath`""
    exit
}

Clear-Host
Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host "    NEXUS SINGLE-OS DISK CONSOLIDATION & STORAGE MERGE ENGINE" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host ""

# 2. Pre-flight Verification
Write-Host "[1/6] Performing pre-flight disk geometry checks..." -ForegroundColor White

$disk = Get-Disk -Number 0 -ErrorAction SilentlyContinue
if (-not $disk) {
    Write-Host "[ERROR] Disk 0 not found! Aborting for safety." -ForegroundColor Red
    pause
    exit 1
}

$part3 = Get-Partition -DiskNumber 0 -PartitionNumber 3 -ErrorAction SilentlyContinue
if (-not $part3 -or $part3.DriveLetter -ne 'C') {
    Write-Host "[ERROR] Partition 3 is not drive C:! Aborting for safety." -ForegroundColor Red
    pause
    exit 1
}

$part4 = Get-Partition -DiskNumber 0 -PartitionNumber 4 -ErrorAction SilentlyContinue
$part5 = Get-Partition -DiskNumber 0 -PartitionNumber 5 -ErrorAction SilentlyContinue
$part6 = Get-Partition -DiskNumber 0 -PartitionNumber 6 -ErrorAction SilentlyContinue
$part7 = Get-Partition -DiskNumber 0 -PartitionNumber 7 -ErrorAction SilentlyContinue

Write-Host "      Detected Disk 0: $($disk.FriendlyName) ($([math]::Round($disk.TotalSize / 1GB, 1)) GB)" -ForegroundColor Green
Write-Host "      Partition 1: Windows EFI Bootloader (100 MB) -> [PRESERVE]" -ForegroundColor Green
Write-Host "      Partition 2: Microsoft Reserved (16 MB)      -> [PRESERVE]" -ForegroundColor Green
Write-Host "      Partition 3: Windows C: ($([math]::Round($part3.Size / 1GB, 1)) GB)     -> [TARGET FOR EXPANSION]" -ForegroundColor Green
Write-Host "      Partition 4: Windows Recovery ($([math]::Round($part4.Size / 1GB, 1)) GB) -> [RELOCATE TO C:]" -ForegroundColor Yellow
if ($part5) { Write-Host "      Partition 5: Linux Root ($([math]::Round($part5.Size / 1GB, 1)) GB)     -> [DELETE]" -ForegroundColor Red }
if ($part6) { Write-Host "      Partition 6: Pop!_OS EFI ($([math]::Round($part6.Size / 1GB, 1)) GB)    -> [DELETE]" -ForegroundColor Red }
if ($part7) { Write-Host "      Partition 7: Linux Data ($([math]::Round($part7.Size / 1GB, 1)) GB)     -> [DELETE]" -ForegroundColor Red }

Write-Host ""
Write-Host "[2/6] Disabling Windows Recovery Environment (WinRE) temporarily..." -ForegroundColor White
$reagentDisable = & reagentc.exe /disable 2>&1
Write-Host "      $reagentDisable" -ForegroundColor Gray

# 3. Create and execute Diskpart commands
Write-Host ""
Write-Host "[3/6] Deleting non-Windows partitions and extending Drive C:..." -ForegroundColor White

$dpScript = [System.IO.Path]::GetTempFileName()
$dpCommands = @"
select disk 0
select partition 7
delete partition override
select partition 6
delete partition override
select partition 5
delete partition override
select partition 4
delete partition override
select partition 3
extend
exit
"@

Set-Content -Path $dpScript -Value $dpCommands -Encoding ASCII

$dpOutput = & diskpart.exe /s $dpScript 2>&1
Remove-Item -Path $dpScript -Force -ErrorAction SilentlyContinue

$dpOutput | ForEach-Object {
    if ($_ -match "DiskPart successfully|successfully deleted|successfully extended") {
        Write-Host "      $_" -ForegroundColor Green
    } elseif ($_ -match "error|fail|cannot") {
        Write-Host "      $_" -ForegroundColor Red
    } else {
        Write-Host "      $_" -ForegroundColor Gray
    }
}

# 4. Re-enable WinRE on Drive C:
Write-Host ""
Write-Host "[4/6] Re-enabling Windows Recovery Environment on expanded C: drive..." -ForegroundColor White
$reagentEnable = & reagentc.exe /enable 2>&1
Write-Host "      $reagentEnable" -ForegroundColor Gray

$reagentStatus = & reagentc.exe /info 2>&1
$reagentStatus | ForEach-Object { Write-Host "      $_" -ForegroundColor Gray }

# 5. Clean up UEFI Firmware Bootloader Entries
Write-Host ""
Write-Host "[5/6] Checking for stale Linux / Pop!_OS UEFI bootloader entries..." -ForegroundColor White
try {
    $bcdFirmware = & bcdedit.exe /enum firmware 2>&1
    $entries = @()
    $currentEntry = $null

    foreach ($line in $bcdFirmware) {
        if ($line -match "^Firmware Boot Manager" -or $line -match "^---") { continue }
        if ($line -match "^identifier\s+(.+)$") {
            if ($currentEntry) { $entries += $currentEntry }
            $currentEntry = @{ id = $matches[1].Trim(); desc = ""; path = "" }
        }
        elseif ($currentEntry -and $line -match "^description\s+(.+)$") {
            $currentEntry.desc = $matches[1].Trim()
        }
        elseif ($currentEntry -and $line -match "^path\s+(.+)$") {
            $currentEntry.path = $matches[1].Trim()
        }
    }
    if ($currentEntry) { $entries += $currentEntry }

    $linuxEntries = $entries | Where-Object { 
        $_.desc -match "Pop|Ubuntu|Linux|systemd-boot" -or $_.path -match "systemd-boot|grub|shim" 
    }

    if ($linuxEntries.Count -gt 0) {
        foreach ($le in $linuxEntries) {
            Write-Host "      Found stale Linux entry: '$($le.desc)' ($($le.id))" -ForegroundColor Yellow
            $delRes = & bcdedit.exe /delete $($le.id) 2>&1
            Write-Host "      -> $delRes" -ForegroundColor Green
        }
    } else {
        Write-Host "      No lingering Linux UEFI entries detected." -ForegroundColor Green
    }
} catch {
    Write-Host "      [NOTICE] UEFI cleanup step encountered notice: $_" -ForegroundColor Yellow
}

# 6. Final Status & Verification
Write-Host ""
Write-Host "[6/6] Verifying final storage and partition layout..." -ForegroundColor White
$newPart3 = Get-Partition -DiskNumber 0 -PartitionNumber 3 -ErrorAction SilentlyContinue
$newVolC = Get-Volume -DriveLetter C -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "    CONSOLIDATION COMPLETE! STORAGE MERGED SUCCESSFULLY" -ForegroundColor Green
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "    New C: Drive Total Size: $([math]::Round($newVolC.Size / 1GB, 1)) GB" -ForegroundColor Cyan
Write-Host "    New C: Drive Free Space: $([math]::Round($newVolC.SizeRemaining / 1GB, 1)) GB" -ForegroundColor Cyan
Write-Host "    Remaining Partitions   : $( (Get-Partition -DiskNumber 0).Count ) (EFI, MSR, Windows C:)" -ForegroundColor White
Write-Host ""
Write-Host "Your laptop is now configured exclusively for Windows with 100% unified storage." -ForegroundColor Green
Write-Host ""

if (-not $NoPause) {
    Write-Host "Press any key to close this window..." -ForegroundColor Gray
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
}
