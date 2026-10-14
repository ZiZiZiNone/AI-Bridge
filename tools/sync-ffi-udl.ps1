# tools/sync-ffi-udl.ps1 -- keep the UDL single source of truth (mobile/ffi/aibridge.udl)
#
# The uniffi-bindgen CLI `generate` subcommand requires the UDL to live under
# `<crate>/src/` to locate the crate root (guess_crate_root), so
# core/ffi/src/aibridge.udl holds a build copy. This script syncs source ->
# copy; use -Check to verify consistency only (for CI / verify-all).

param(
    [switch]$Check
)

$repo = Split-Path -Parent $PSScriptRoot
$source = Join-Path $repo "mobile/ffi/aibridge.udl"
$copy = Join-Path $repo "core/ffi/src/aibridge.udl"

if ($Check) {
    $same = ((Get-FileHash $source).Hash -eq (Get-FileHash $copy).Hash)
    if (-not $same) {
        Write-Host "FAIL  core/ffi/src/aibridge.udl differs from mobile/ffi/aibridge.udl -- run tools/sync-ffi-udl.ps1 to sync"
        exit 1
    }
    Write-Host "PASS  FFI UDL copy matches the source of truth"
    exit 0
}

Copy-Item $source $copy -Force
Write-Host "synced mobile/ffi/aibridge.udl -> core/ffi/src/aibridge.udl"
