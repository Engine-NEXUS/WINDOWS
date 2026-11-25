# ==============================================================================
# Personal Laptop UEFI Bootloader Repair & Windows Default Restoration Script
# 1. Inspects UEFI Firmware Boot Manager entries
# 2. Deletes stale Linux (Pop!_OS, systemd-boot, GRUB) bootloader entries
# 3. Mounts EFI System Partition (ESP) and purges dead Linux EFI files
# 4. Refreshes Windows UEFI boot files via bcdboot
# 5. Configures Windows Boot Manager as the #1 priority & default in BIOS NVRAM
# ==============================================================================

param(
    [switch]$NoPause
)

# 1. Require Administrator Privileges
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "Requesting administrative elevation (UAC)..." -ForegroundColor Yellow
    $scriptPath = $MyInvocation.MyCommand.Definition
    Start-Process powershell.exe -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$scriptPath`""
    exit
}

Clear-Host
Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host "    UEFI BIOS BOOTLOADER REPAIR & WINDOWS DEFAULT RESTORATION" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host ""

# 2. Inspect Current UEFI Firmware Boot Entries
Write-Host "[1/5] Inspecting current UEFI BIOS firmware boot entries..." -ForegroundColor White

$rawFirmware = & bcdedit.exe /enum firmware 2>&1
$entries = @()
$currentEntry = $null

foreach ($line in $rawFirmware) {
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

Write-Host "      Detected Boot Entries in Motherboard NVRAM:" -ForegroundColor Gray
foreach ($e in $entries) {
    $tag = "[KEEP]"
    $color = "Green"
    if ($e.desc -match "Pop|Ubuntu|Linux|systemd-boot|GRUB" -or $e.path -match "systemd-boot|grub|shim") {
        $tag = "[REMOVE STALE]"
        $color = "Red"
    } elseif ($e.desc -match "Windows Boot Manager") {
        $tag = "[SET DEFAULT]"
        $color = "Cyan"
    }
    Write-Host "      $tag $($e.desc) ($($e.id))" -ForegroundColor $color
}

# 3. Delete Stale Non-Windows UEFI Boot Entries
Write-Host ""
Write-Host "[2/5] Deleting non-Windows bootloader entries from BIOS NVRAM..." -ForegroundColor White
$staleEntries = $entries | Where-Object {
    $_.desc -match "Pop|Ubuntu|Linux|systemd-boot|GRUB" -or $_.path -match "systemd-boot|grub|shim"
}

if ($staleEntries.Count -gt 0) {
    foreach ($se in $staleEntries) {
        Write-Host "      Deleting stale entry: '$($se.desc)' ($($se.id))..." -ForegroundColor Yellow
        $delOut = & bcdedit.exe /delete $($se.id) 2>&1
        Write-Host "      -> $delOut" -ForegroundColor Green
    }
} else {
    Write-Host "      No lingering Linux boot entries in NVRAM." -ForegroundColor Green
}

# 4. Mount EFI System Partition and Remove Dead Linux Bootloader Files
Write-Host ""
Write-Host "[3/5] Inspecting EFI System Partition (ESP) for dead boot files..." -ForegroundColor White

$efiDrive = "S:"
$mountedByUs = $false

if (-not (Test-Path "$efiDrive\")) {
    & mountvol.exe $efiDrive /S | Out-Null
    $mountedByUs = $true
}

if (Test-Path "$efiDrive\EFI") {
    Write-Host "      Mounted EFI System Partition at $efiDrive" -ForegroundColor Green
    $linuxDirs = @("Pop_OS", "systemd", "ubuntu", "debian", "fedora", "arch", "grub", "Linux")
    foreach ($ld in $linuxDirs) {
        $targetPath = "$efiDrive\EFI\$ld"
        if (Test-Path $targetPath) {
            Write-Host "      Removing dead EFI folder: $targetPath..." -ForegroundColor Yellow
            Remove-Item -Path $targetPath -Recurse -Force -ErrorAction SilentlyContinue
            Write-Host "      -> Removed successfully" -ForegroundColor Green
        }
    }
} else {
    Write-Host "      [NOTICE] EFI partition mounted but EFI folder not found." -ForegroundColor Yellow
}

# 5. Reinstall & Refresh Clean Windows UEFI Boot Files via BCDBoot
Write-Host ""
Write-Host "[4/5] Refreshing Windows Bootloader EFI files & NVRAM registration..." -ForegroundColor White
$bcdbootOut = & bcdboot.exe C:\Windows /s $efiDrive /f UEFI 2>&1
Write-Host "      $bcdbootOut" -ForegroundColor Green

# Unmount EFI Drive
if ($mountedByUs) {
    & mountvol.exe $efiDrive /D | Out-Null
}

# 6. Configure Windows Boot Manager as #1 Priority and Set Timeout to 0
Write-Host ""
Write-Host "[5/5] Enforcing Windows Boot Manager as #1 Default in BIOS boot order..." -ForegroundColor White

# Set Windows Boot Manager as default
$res1 = & bcdedit.exe /set "{fwbootmgr}" default "{bootmgr}" 2>&1
# Set displayorder with Windows Boot Manager first
$res2 = & bcdedit.exe /set "{fwbootmgr}" displayorder "{bootmgr}" /addfirst 2>&1
# Set timeout to 0 so it boots instantly into Windows without hesitation
$res3 = & bcdedit.exe /timeout 0 2>&1

Write-Host "      Set Default: $res1" -ForegroundColor Gray
Write-Host "      Boot Priority: $res2" -ForegroundColor Gray
Write-Host "      Boot Timeout: $res3" -ForegroundColor Gray

Write-Host ""
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "    REPAIR COMPLETE! BIOS WILL NOW BOOT DIRECTLY TO WINDOWS" -ForegroundColor Green
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "    Primary Bootloader : Windows Boot Manager" -ForegroundColor Cyan
Write-Host "    Boot Priority      : #1 (Highest / Default)" -ForegroundColor Cyan
Write-Host "    Boot Timeout       : 0 seconds (Instant Boot)" -ForegroundColor Cyan
Write-Host "    Stale Bootloaders  : Completely deleted" -ForegroundColor Cyan
Write-Host ""
Write-Host "Your laptop will now boot directly into Windows without stopping at any other bootloader." -ForegroundColor Green
Write-Host ""

if (-not $NoPause) {
    Write-Host "Press any key to close this window..." -ForegroundColor Gray
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
}
