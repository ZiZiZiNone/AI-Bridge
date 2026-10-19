# tools/build-installer.ps1 —— 桌面端 release 构建 + 安装包（MSI / NSIS）
#
# 用法：
#   powershell -File tools/build-installer.ps1                 # 构建并打包（需联网：首次拉取 NSIS / WiX）
#   powershell -File tools/build-installer.ps1 -NoBundle       # 只构建 release exe，不产安装包
#
# 前置：
#   - tauri-cli **不在**离线 vendor 中（`.cargo-home/vendor` 无 tauri-cli）。
#     本脚本经 `npx -y @tauri-apps/cli@2` 取用，因此**需要网络**（首次会下载 CLI、
#     NSIS 3.13 与 WiX 3.14 到 %LOCALAPPDATA%\tauri）。离线环境无法打包，这是环境限制。
#   - cargo 不在 PATH：脚本自带工具链与 CARGO_HOME 注入（本机约定）。
#
# 为什么设置 CARGO_BUILD_JOBS=2：
#   `[profile.release]` 里 lto = true + codegen-units = 1 是项目既定配置，保留不动。
#   本机在 16 逻辑核 / 约 4 GB 空闲内存下并行跑 release 会触发 rustc ICE
#   （`error: the compiler unexpectedly panicked` 于 ghash 等 crate）。
#   限制并行度即稳定通过；产物与优化配置不变，仅编译并发度不同。

param(
    [switch]$NoBundle
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

$env:PATH = "C:\Users\HKX\AppData\Local\puccinialin\puccinialin\Cache\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;$env:PATH"
$env:CARGO_HOME = Join-Path $repo ".cargo-home"
$env:CARGO_BUILD_JOBS = "2"

Push-Location (Join-Path $repo "desktop")
try {
    # 注意：不要用 $args —— 它是 PowerShell 自动变量，赋值不生效且会把参数弄丢。
    $tauriArgs = @("build")
    if ($NoBundle) { $tauriArgs += "--no-bundle" }
    & npx -y @tauri-apps/cli@2 @tauriArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAIL  桌面端 release 构建/打包失败（退出码 $LASTEXITCODE）"
        exit 1
    }
}
finally {
    Pop-Location
}

$exe = Join-Path $repo "desktop/src-tauri/target/release/ab-desktop.exe"
if (-not (Test-Path $exe)) {
    Write-Host "FAIL  未产出 $exe"
    exit 1
}
$exeItem = Get-Item $exe
Write-Host ("PASS  release exe：{0:N2} MB  {1}" -f ($exeItem.Length / 1MB), $exe)

if ($NoBundle) { exit 0 }

$bundleRoot = Join-Path $repo "desktop/src-tauri/target/release/bundle"
$artifacts = Get-ChildItem $bundleRoot -Recurse -File -Include *.msi, *.exe -ErrorAction SilentlyContinue |
    Sort-Object Length -Descending
if (@($artifacts).Count -eq 0) {
    Write-Host "FAIL  未产出安装包（$bundleRoot 下无 .msi / .exe）"
    exit 1
}
foreach ($a in $artifacts) {
    $hash = (Get-FileHash $a.FullName -Algorithm SHA256).Hash
    Write-Host ("PASS  安装包：{0}  {1:N2} MB  sha256={2}" -f $a.Name, ($a.Length / 1MB), $hash)
}
exit 0
