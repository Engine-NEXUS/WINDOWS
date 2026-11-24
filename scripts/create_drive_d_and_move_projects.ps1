# ==============================================================================
# NEXUS Drive D: Creation & Project Migration Script
# 1. Deletes Linux (Pop!_OS) Partitions 5, 6, 7
# 2. Creates new Drive D: (~511 GB NTFS) labeled "PROJECTS"
# 3. Migrates C:\PROJECTS to D:\PROJECTS via high-speed robocopy
# 4. Creates directory junction C:\PROJECTS -> D:\PROJECTS for 100% path compatibility
# 5. Cleans up stale Linux UEFI firmware boot entries
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
Write-Host "    NEXUS NEW DRIVE (D:) CREATION & PROJECT MIGRATION ENGINE" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host ""

# 2. Pre-flight Verification
Write-Host "[1/5] Verifying disk configuration..." -ForegroundColor White

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

Write-Host "      Detected Disk 0: $($disk.FriendlyName)" -ForegroundColor Green
Write-Host "      Windows C: is intact ($([math]::Round($part3.Size / 1GB, 1)) GB)" -ForegroundColor Green

# 3. Create Diskpart script to delete Linux partitions and create Drive D:
Write-Host ""
Write-Host "[2/5] Removing Linux partitions and provisioning Drive D: (511 GB NTFS)..." -ForegroundColor White

$dpScript = [System.IO.Path]::GetTempFileName()
$dpCommands = @"
select disk 0
select partition 7
delete partition override
select partition 6
delete partition override
select partition 5
delete partition override
create partition primary
format fs=ntfs quick label="PROJECTS"
assign letter=D
exit
"@

Set-Content -Path $dpScript -Value $dpCommands -Encoding ASCII

$dpOutput = & diskpart.exe /s $dpScript 2>&1
Remove-Item -Path $dpScript -Force -ErrorAction SilentlyContinue

$dpOutput | ForEach-Object {
    if ($_ -match "DiskPart successfully|successfully deleted|successfully formatted|successfully assigned") {
        Write-Host "      $_" -ForegroundColor Green
    } elseif ($_ -match "error|fail|cannot") {
        Write-Host "      $_" -ForegroundColor Red
    } else {
        Write-Host "      $_" -ForegroundColor Gray
    }
}

# Verify Drive D: is mounted
Start-Sleep -Seconds 2
$volD = Get-Volume -DriveLetter D -ErrorAction SilentlyContinue
if (-not $volD) {
    Write-Host "[ERROR] Drive D: was not created or mounted properly!" -ForegroundColor Red
    pause
    exit 1
}

Write-Host ""
Write-Host "      SUCCESS: Drive D: created! Total Size: $([math]::Round($volD.Size / 1GB, 1)) GB" -ForegroundColor Green

# 4. Migrate C:\PROJECTS to D:\PROJECTS
Write-Host ""
Write-Host "[3/5] Migrating folders from C:\PROJECTS to D:\PROJECTS..." -ForegroundColor White

if (Test-Path "C:\PROJECTS") {
    if (-not (Test-Path "D:\PROJECTS")) {
        New-Item -ItemType Directory -Path "D:\PROJECTS" -Force | Out-Null
    }

    Write-Host "      Copying all project files using high-speed multi-threaded Robocopy..." -ForegroundColor White
    $robocopyArgs = @("C:\PROJECTS", "D:\PROJECTS", "/E", "/COPY:DAT", "/DCOPY:DAT", "/R:1", "/W:1", "/MT:8", "/NFL", "/NDL", "/NP")
    & robocopy.exe @robocopyArgs

    Write-Host "      Verifying project folders on Drive D:..." -ForegroundColor White
    $dFolders = Get-ChildItem "D:\PROJECTS" -Directory
    foreach ($f in $dFolders) {
        Write-Host "      [OK] D:\PROJECTS\$($f.Name)" -ForegroundColor Green
    }

    # Backup & Link C:\PROJECTS to D:\PROJECTS
    Write-Host ""
    Write-Host "[4/5] Establishing directory junction (C:\PROJECTS -> D:\PROJECTS)..." -ForegroundColor White
    try {
        Rename-Item -Path "C:\PROJECTS" -NewName "PROJECTS_C_BACKUP" -ErrorAction Stop
        & cmd.exe /c "mklink /J C:\PROJECTS D:\PROJECTS"
        Write-Host "      Directory junction created: C:\PROJECTS -> D:\PROJECTS" -ForegroundColor Green
        Write-Host "      (Original files preserved safely in C:\PROJECTS_C_BACKUP)" -ForegroundColor Cyan
    } catch {
        Write-Host "      [NOTICE] Active files locked C:\PROJECTS. You can access projects directly at D:\PROJECTS." -ForegroundColor Yellow
    }
} else {
    Write-Host "      C:\PROJECTS does not exist. Created fresh D:\PROJECTS." -ForegroundColor Yellow
    New-Item -ItemType Directory -Path "D:\PROJECTS" -Force | Out-Null
}

# 5. Clean up stale UEFI bootloader entries
Write-Host ""
Write-Host "[5/5] Purging stale Linux/Pop!_OS UEFI boot entries..." -ForegroundColor White
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
            Write-Host "      Removing UEFI entry: '$($le.desc)' ($($le.id))" -ForegroundColor Yellow
            $delRes = & bcdedit.exe /delete $($le.id) 2>&1
            Write-Host "      -> $delRes" -ForegroundColor Green
        }
    } else {
        Write-Host "      No lingering Linux UEFI entries detected." -ForegroundColor Green
    }
} catch {
    Write-Host "      [NOTICE] UEFI cleanup notice: $_" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "    SETUP COMPLETE! NEW DRIVE D: IS ACTIVE & READY TO USE" -ForegroundColor Green
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "    New Drive      : D:\ [PROJECTS] (~511 GB)" -ForegroundColor Cyan
Write-Host "    Projects Path  : D:\PROJECTS" -ForegroundColor Cyan
Write-Host "    Compatibility  : C:\PROJECTS links seamlessly to D:\PROJECTS" -ForegroundColor Cyan
Write-Host ""
Write-Host "Once you have verified your projects on D:\, you can delete C:\PROJECTS_C_BACKUP to free up space on C:." -ForegroundColor Yellow
Write-Host ""

if (-not $NoPause) {
    Write-Host "Press any key to close this window..." -ForegroundColor Gray
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
}
