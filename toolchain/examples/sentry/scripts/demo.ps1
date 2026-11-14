# SENTRY demo: check + deterministic replay + emit, one command (Windows PowerShell).
# Usage:  pwsh -File scripts\demo.ps1 [-OutRoot D:\tmp\sentry-demo]
param(
    [string]$OutRoot = (Join-Path ([System.IO.Path]::GetTempPath()) "sentry-demo")
)

$ErrorActionPreference = "Stop"
$toolchain = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
Set-Location $toolchain

if (-not (Get-Command bun -ErrorAction SilentlyContinue)) {
    throw "bun not found in PATH (dhv-ts needs bun: https://bun.sh)"
}
if (-not (Get-Command bash -ErrorAction SilentlyContinue)) {
    throw "bash not found in PATH (sentry's bash tool needs a real bash, e.g. Git Bash; on Windows C:\Windows\System32\bash.exe is a WSL stub)"
}

Write-Host "== 1/3 check ==" -ForegroundColor Cyan
bun dhv-ts/src/main.ts check examples/sentry/sentry.hsl
if ($LASTEXITCODE -ne 0) { throw "check failed" }

$ws = Join-Path $OutRoot "ws"
$out = Join-Path $OutRoot "out"
$emit = Join-Path $OutRoot "emit"
if (Test-Path $OutRoot) { Remove-Item -Recurse -Force $OutRoot }
Copy-Item -Recurse "examples/sentry/workspace" $ws

Write-Host "== 2/3 scripted run (deterministic replay) ==" -ForegroundColor Cyan
bun dhv-ts/src/main.ts run examples/sentry/sentry.hsl `
    --workspace $ws `
    --task "checkout 支付链路自 02:13 起大面积超时，请分诊并给出复盘报告" `
    --model scripted `
    --fixture examples/sentry/fixtures/triage-demo.json `
    --out $out
if ($LASTEXITCODE -ne 0) { throw "run failed" }

Write-Host "-- run-report.md --" -ForegroundColor DarkGray
Get-Content -Encoding UTF8 (Join-Path $out "run-report.md") | Select-Object -First 14

Write-Host "== 3/3 emit (projection) ==" -ForegroundColor Cyan
bun dhv-ts/src/main.ts emit examples/sentry/sentry.hsl --out $emit
if ($LASTEXITCODE -ne 0) { throw "emit failed" }

Write-Host ""
Write-Host "OK. artifacts under $OutRoot" -ForegroundColor Green
