param(
    [switch]$Check
)

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$SourceUdl = Join-Path $RepoRoot "mobile\ffi\aibridge.udl"
$TargetDir = Join-Path $RepoRoot "core\ffi\src"
$TargetUdl = Join-Path $TargetDir "aibridge.udl"

if (-not (Test-Path $SourceUdl)) {
    Write-Error "缺少 UDL 事实源: $SourceUdl"
    exit 1
}

$SourceBytes = [System.IO.File]::ReadAllBytes($SourceUdl)

if ($Check) {
    if (-not (Test-Path $TargetUdl)) {
        Write-Error "缺少 UDL 构建副本: $TargetUdl"
        exit 1
    }
    $TargetBytes = [System.IO.File]::ReadAllBytes($TargetUdl)
    $SourceHash = (Get-FileHash -Path $SourceUdl -Algorithm SHA256).Hash
    $TargetHash = (Get-FileHash -Path $TargetUdl -Algorithm SHA256).Hash
    if ($SourceBytes.Length -ne $TargetBytes.Length -or $SourceHash -ne $TargetHash) {
        Write-Error "UDL 不同步: mobile/ffi/aibridge.udl ($SourceHash) != core/ffi/src/aibridge.udl ($TargetHash)"
        exit 1
    }
    Write-Host "[OK] UDL 一致 ($SourceHash, $($SourceBytes.Length) bytes)"
    exit 0
}

if (-not (Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
}

[System.IO.File]::WriteAllBytes($TargetUdl, $SourceBytes)
Write-Host "[OK] 已同步 UDL: mobile/ffi/aibridge.udl -> core/ffi/src/aibridge.udl ($($SourceBytes.Length) bytes)"
