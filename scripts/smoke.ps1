# PowerShell smoke test for novel-style-converter release build.
# Replaces `timeout 4 ./target/release/novel-style-converter.exe` (GNU timeout is unavailable on Windows).
# Verifies the binary launches and survives 4 seconds without panic.
#
# Usage:
#   pwsh scripts/smoke.ps1
#   powershell -ExecutionPolicy Bypass -File scripts/smoke.ps1

$ErrorActionPreference = "Continue"
$ScriptRoot = $PSScriptRoot
$RepoRoot = Split-Path -Parent $ScriptRoot
$ExePath = Join-Path $RepoRoot "target\release\nsc-desktop.exe"

if (-not (Test-Path $ExePath)) {
    Write-Error "release build missing at $ExePath — run pnpm tauri build --bundles msi first"
    exit 1
}

$outFile = Join-Path $ScriptRoot "smoke.out"
$errFile = Join-Path $ScriptRoot "smoke.err"
Remove-Item $outFile, $errFile -ErrorAction SilentlyContinue

$proc = Start-Process -FilePath $ExePath `
    -PassThru `
    -RedirectStandardOutput $outFile `
    -RedirectStandardError $errFile `
    -WindowStyle Hidden

Write-Host "Started novel-style-converter PID $($proc.Id), sleeping 4s..."
Start-Sleep -Seconds 4

$running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue

if ($null -ne $running) {
    Stop-Process -Id $proc.Id -Force
    Write-Host "OK: app launched and ran 4s without panic"
    exit 0
} else {
    $code = $proc.ExitCode
    Write-Error "FAIL: app exited with code $code before 4s"
    if (Test-Path $errFile) {
        Write-Host "--- last 20 lines of stderr ---"
        Get-Content $errFile -Tail 20 -ErrorAction SilentlyContinue
    }
    exit 1
}
