# tools/verify-all.ps1 —— 全仓一键验证（终验/回归共用）
#
# 逐套执行仓库全部验证入口，收集退出码，任何一项失败退出码为 1。
# 用法：
#   powershell -File tools/verify-all.ps1              # 全部套件
#   powershell -File tools/verify-all.ps1 -Exclude desktop   # 排除（如 desktop 正在改）
#
# 工具链前置（本机约定）：cargo 不在 PATH，需先注入 rustup 工具链与离线 CARGO_HOME。

param(
    [string[]]$Exclude = @(),
    [switch]$NoColor
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

$env:PATH = "C:\Users\HKX\AppData\Local\puccinialin\puccinialin\Cache\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;$env:PATH"
$env:CARGO_HOME = Join-Path $repo ".cargo-home"

$results = @()
function Invoke-Check([string]$name, [string[]]$cmd) {
    if ($Exclude -contains $name) {
        Write-Host "SKIP  $name"
        return
    }
    Write-Host "RUN   $name"
    & $cmd[0] $cmd[1..($cmd.Length - 1)]
    $ok = ($LASTEXITCODE -eq 0)
    $script:results += [pscustomobject]@{ Name = $name; Pass = $ok }
    Write-Host ("{0}  {1}" -f ($(if ($ok) { "PASS" } else { "FAIL" }), $name))
}

# 1. 契约与工具链自检
Invoke-Check "contracts"    @("python", "tools/check-contracts.py")
Invoke-Check "tools-selftest" @("python", "tools/check-contracts.py", "--self-test")
Invoke-Check "vendor-selftest" @("python", "tools/fetch-vendor.py", "--selftest")
Invoke-Check "compat-lock"  @("node", "tests/contract-compat/check-compat.mjs", "--selftest")

# 2. 核心层 + 契约兼容
Invoke-Check "workspace"    @("cargo", "test", "--workspace", "--quiet")
Invoke-Check "contract-compat" @("cargo", "test", "--manifest-path", "tests/contract-compat/Cargo.toml", "--quiet")
Invoke-Check "recovery-drill" @("cargo", "test", "--manifest-path", "tests/recovery-drill/Cargo.toml", "--quiet")
Invoke-Check "e2e-pipeline" @("cargo", "test", "--manifest-path", "tests/e2e-pipeline/Cargo.toml", "--quiet")

# 3. 转换器管线
Invoke-Check "sdk-conformance-qq" @("node", "converters/sdk/tests/conformance.mjs", "converters/qq")
Invoke-Check "sdk-schema-qq" @("python", "converters/sdk/tests/schema-check.py", "converters/qq")
Invoke-Check "converter-fixtures" @("node", "tests/converter-fixtures/run.mjs")

# 4. 扩展层
Invoke-Check "extensions-tsc" @("npx", "tsc", "--noEmit", "-p", "extensions/tsconfig.json")
Invoke-Check "extensions-test" @("node", "--test", "--experimental-test-isolation=none",
    "extensions/shared/test/*.test.ts")

# 5. 桌面端
Invoke-Check "desktop"       @("cargo", "test", "--manifest-path", "desktop/src-tauri/Cargo.toml", "--quiet")
Push-Location desktop
Invoke-Check "desktop-ui"    @("npm", "run", "check")
Invoke-Check "desktop-tsc"   @("npm", "run", "typecheck")
Pop-Location

$failed = @($results | Where-Object { -not $_.Pass })
Write-Host ""
Write-Host ("结果：{0} 项，通过 {1}，失败 {2}，跳过 {3}" -f `
    ($results.Count + $Exclude.Count), $results.Count, $failed.Count, $Exclude.Count)
$failed | ForEach-Object { Write-Host ("  FAIL  " + $_.Name) }
if ($failed.Count -gt 0) { exit 1 }
exit 0
