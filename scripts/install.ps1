# HSL/DHV 一键安装器（Windows x86_64）
#
# 用法（PowerShell，支持 irm | iex 管道）：
#   irm https://raw.githubusercontent.com/myh2026/harness-specification-language/main/scripts/install.ps1 | iex
#
# 可选环境变量（两种用法均生效）：
#   $env:HSL_VERSION = "0.2.56"   指定版本，默认 latest
#   $env:HSL_BIN_DIR  = "C:\bin"  安装目录，默认 %USERPROFILE%\bin

$ErrorActionPreference = "Stop"
$Repo = "myh2026/harness-specification-language"
$Target = "windows-x86_64"
$Version = "${env:HSL_VERSION}"
$BinDir = "${env:HSL_BIN_DIR}"

if (-not $Version) {
    Write-Host "→ 解析最新版本…"
    $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $rel.tag_name.TrimStart("v")
}
Write-Host "→ 版本: v$Version"

if (-not $BinDir) {
    $BinDir = Join-Path $env:USERPROFILE "bin"
}

$Asset = "dhv-v$Version-$Target.zip"
$Url = "https://github.com/$Repo/releases/download/v$Version/$Asset"

# 下载（走临时目录）
$Tmp = Join-Path $env:TEMP ("hsl-install-" + [System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Force -Path $Tmp | Out-Null

Write-Host "→ 下载 $Asset …"
try {
    Invoke-WebRequest -Uri $Url -OutFile (Join-Path $Tmp $Asset)
} catch {
    Write-Error "✗ 下载失败（v$Version 可能还没有 $Target 产物）：$_"
    exit 1
}

# sha256 校验（若发布附带 sha256sums.txt）
$SumUrl = "https://github.com/$Repo/releases/download/v$Version/sha256sums.txt"
$SumPath = Join-Path $Tmp "sha256sums.txt"
$Expect = $null
try {
    Invoke-WebRequest -Uri $SumUrl -OutFile $SumPath
    $line = (Get-Content $SumPath | Where-Object { $_ -match ("\s" + [regex]::Escape($Asset) + "\s*$") }) | Select-Object -First 1
    if ($line) { $Expect = ($line -split "\s+")[0] }
} catch {
    Write-Host "⚠ 该版本无 sha256sums.txt，跳过校验"
}

if ($Expect) {
    $Actual = (Get-FileHash -Path (Join-Path $Tmp $Asset) -Algorithm SHA256).Hash.ToLower()
    if ($Actual -ne $Expect.ToLower()) {
        Write-Error "✗ sha256 校验失败（期望 $Expect，实际 $Actual）"
        exit 1
    }
    Write-Host "✓ sha256 校验通过"
}

# 解压安装
Expand-Archive -Path (Join-Path $Tmp $Asset) -DestinationPath $Tmp -Force
if (-not (Test-Path (Join-Path $Tmp "dhv.exe"))) {
    Write-Error "✗ zip 中未找到 dhv.exe"
    exit 1
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item (Join-Path $Tmp "dhv.exe") (Join-Path $BinDir "dhv.exe") -Force
Write-Host "✓ 已安装 $(Join-Path $BinDir 'dhv.exe')"

Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue

# 自检
& (Join-Path $BinDir "dhv.exe") --version

# PATH 提示
$PathEnv = [Environment]::GetEnvironmentVariable("Path", "User")
if ($PathEnv -notlike "*$BinDir*") {
    Write-Host "⚠ $BinDir 不在用户 PATH 中，请手动加入（系统设置 → 环境变量），或运行："
    Write-Host "    [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path','User') + ';$BinDir', 'User')"
}

Write-Host ""
Write-Host "完成。用法：dhv check <file.hsl> / dhv run / dhv emit --out <dir>"
