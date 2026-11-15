@echo off
setlocal
cd /d "%~dp0"

:: Check for Administrator privileges
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo [NEXUS] Requesting Administrator privileges...
    powershell -NoProfile -Command "Start-Process cmd.exe -ArgumentList '/c `\"%~f0`\"' -Verb RunAs"
    exit /b
)

title NEXUS Single-OS Disk Consolidation
echo ====================================================================
echo   NEXUS STORAGE MERGE LAUNCHER (RUNNING ELEVATED)
echo ====================================================================
echo.

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\PROJECTS\ULTRON\scripts\merge_windows_partitions.ps1"
pause
