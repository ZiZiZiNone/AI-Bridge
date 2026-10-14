$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$SourceUdl = Join-Path $RepoRoot "mobile\ffi\aibridge.udl"
$TargetDir = Join-Path $RepoRoot "core\ffi\src"
$TargetUdl = Join-Path $TargetDir "aibridge.udl"

if (-not (Test-Path $SourceUdl)) {
    throw "缺少 UDL 事实源: $SourceUdl"
}

if (-not (Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
}

$Bytes = [System.IO.File]::ReadAllBytes($SourceUdl)
[System.IO.File]::WriteAllBytes($TargetUdl, $Bytes)
Write-Host "[OK] 已同步 UDL: mobile/ffi/aibridge.udl -> core/ffi/src/aibridge.udl ($($Bytes.Length) bytes)"
