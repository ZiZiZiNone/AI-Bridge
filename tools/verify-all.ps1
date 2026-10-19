# tools/verify-all.ps1 —— 全仓一键验证（终验/回归共用）
#
# 逐套执行仓库全部验证入口，收集退出码，任何一项失败退出码为 1。
# 用法：
#   powershell -File tools/verify-all.ps1              # 全部套件
#   powershell -File tools/verify-all.ps1 -Exclude desktop   # 排除（如 desktop 正在改）
#
# 工具链前置（本机约定）：cargo 不在 PATH，需先注入 rustup 工具链与离线 CARGO_HOME。
# desktop-structure 在 desktop/ 内执行 `npm run check:structure`（npm-run 需要
# package.json 的 cwd，Push-Location 段内跑）。

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

# 预期失败门禁（P1-10）：命令以非零退出为**正确**行为（如占位签名被拒载）。
# 退出码 0 反而是假绿灯（校验器没拦住占位数据）。
function Invoke-Check-ExpectFail([string]$name, [string[]]$cmd, [string]$why) {
    if ($Exclude -contains $name) {
        Write-Host "SKIP  $name"
        return
    }
    Write-Host "RUN   $name（预期失败：$why）"
    & $cmd[0] $cmd[1..($cmd.Length - 1)]
    $rejected = ($LASTEXITCODE -ne 0)
    $script:results += [pscustomobject]@{ Name = $name; Pass = $rejected }
    Write-Host ("{0}  {1}" -f ($(if ($rejected) {
        "PASS（正确拒载）"
    } else {
        "FAIL（假绿灯：校验器未拒绝）"
    }), $name))
}

# 1. 契约与工具链自检
Invoke-Check "contracts"    @("python", "tools/check-contracts.py")
Invoke-Check "tools-selftest" @("python", "tools/check-contracts.py", "--self-test")
Invoke-Check "contract-fixtures" @("node", "tools/contract-check/check.mjs")
Invoke-Check "vendor-selftest" @("python", "tools/fetch-vendor.py", "--selftest")
Invoke-Check "compat-lock"  @("node", "tests/contract-compat/check-compat.mjs", "--selftest")
Invoke-Check "ffi-udl-sync" @("powershell", "-ExecutionPolicy", "Bypass", "-File", "tools/sync-ffi-udl.ps1", "-Check")

# 2. 核心层 + 契约兼容
Invoke-Check "workspace"    @("cargo", "test", "--workspace", "--quiet")
Invoke-Check "contract-compat" @("cargo", "test", "--manifest-path", "tests/contract-compat/Cargo.toml", "--quiet")
Invoke-Check "recovery-drill" @("cargo", "test", "--manifest-path", "tests/recovery-drill/Cargo.toml", "--quiet")
Invoke-Check "e2e-pipeline" @("cargo", "test", "--manifest-path", "tests/e2e-pipeline/Cargo.toml", "--quiet")

# 3. 转换器管线
# qq 占位签名被正确拒载（预期行为）：全零 module_sha256/signature 是设计内未签名状态，
# 契约层（check-contracts --data 与 core/contract 同规则）必须拒绝它——拒绝 = 门禁 PASS。
Invoke-Check-ExpectFail "qq-manifest-reject" `
    @("python", "tools/check-contracts.py", "--data", "converters/qq/manifest.json") `
    "qq 占位签名（全零）应被契约层拒载"
Invoke-Check "sdk-conformance-qq" @("node", "converters/sdk/tests/conformance.mjs", "converters/qq")
Invoke-Check "sdk-schema-qq" @("python", "converters/sdk/tests/schema-check.py", "converters/qq")
Invoke-Check "converter-fixtures" @("node", "tests/converter-fixtures/run.mjs")

# 4. 扩展层
Invoke-Check "extensions-tsc" @("npx", "tsc", "--noEmit", "-p", "extensions/tsconfig.json")
Invoke-Check "extensions-test" @("node", "--test", "--experimental-test-isolation=none",
    "extensions/shared/test/*.test.ts", "extensions/chromium/test/*.test.ts")

# 5. 桌面端
Invoke-Check "desktop"       @("cargo", "test", "--manifest-path", "desktop/src-tauri/Cargo.toml", "--quiet")
Push-Location desktop
Invoke-Check "desktop-structure" @("npm", "run", "check:structure")
Invoke-Check "desktop-ui"    @("npm", "run", "check")
Invoke-Check "desktop-tsc"   @("npm", "run", "typecheck")
Pop-Location

# 5b. 桌面端交互验收（真实浏览器逐个点击按钮）
# 为什么必须进这道门禁：check:structure / check:render 只断言「文案存在」，对死按钮完全无感
# （docs/缺陷报告-2026-10-15.md 的 D2）——曾出现「20/20 全绿但界面点不动」。
# 本项自己拉起 dev server、跑完关掉，不依赖外部已启动的服务。
function Invoke-DesktopUiAcceptance {
    if ($Exclude -contains "desktop-ui-acceptance") {
        Write-Host "SKIP  desktop-ui-acceptance"
        return
    }
    Write-Host "RUN   desktop-ui-acceptance"
    $log = Join-Path $env:TEMP ("ab-ui-acceptance-" + [guid]::NewGuid().ToString("N") + ".log")
    $dev = $null
    try {
        # 拉起 dev server（vite 只监听 localhost:1420）
        $dev = Start-Process -FilePath "npm.cmd" -ArgumentList "run", "dev" `
            -WorkingDirectory (Join-Path $repo "desktop") `
            -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput ($log + ".dev.out") `
            -RedirectStandardError ($log + ".dev.err")

        $base = "http://localhost:1420"
        $ready = $false
        # 用 curl.exe 探测而非 Invoke-WebRequest：后者在本机 PowerShell 下偶发
        # 「读取控制台输出缓冲区时 Access is denied」，会把已就绪的服务误判为未就绪。
        for ($i = 0; $i -lt 90; $i++) {
            Start-Sleep -Milliseconds 500
            $code = & curl.exe -s -o NUL -w "%{http_code}" "$base/?window=panel-library" 2>$null
            if ("$code" -eq "200") { $ready = $true; break }
        }
        if (-not $ready) {
            Write-Host "      dev server 90 秒内未就绪（$base），无法执行交互验收"
            $script:results += [pscustomobject]@{ Name = "desktop-ui-acceptance"; Pass = $false }
            Write-Host "FAIL  desktop-ui-acceptance"
            return
        }

        Push-Location (Join-Path $repo "desktop")
        node scripts/check-ui-acceptance.mjs --base $base 2>&1 | ForEach-Object { Write-Host "      $_" }
        $ok = ($LASTEXITCODE -eq 0)
        Pop-Location
        $script:results += [pscustomobject]@{ Name = "desktop-ui-acceptance"; Pass = $ok }
        Write-Host ("{0}  desktop-ui-acceptance" -f $(if ($ok) { "PASS" } else { "FAIL" }))
    } finally {
        if ($dev -and -not $dev.HasExited) {
            # npm run dev 会派生 vite 子进程，必须整棵树杀，否则端口被占、下次门禁起不来
            & taskkill.exe /PID $dev.Id /T /F 2>&1 | Out-Null
        }
        Start-Sleep -Milliseconds 800
        Remove-Item ($log + ".dev.out"), ($log + ".dev.err") -Force -ErrorAction SilentlyContinue
    }
}
Invoke-DesktopUiAcceptance

# 5c. 桌面端真实窗口 E2E（建库 → 新建条目 → 重启后条目仍在）
# 为什么单列一项：check-ui-acceptance 在 dev server 的裸浏览器里点击，只能证明
# 「UI 会调用 invoke」；本项在真实 ab-desktop.exe 窗口内走完整用户路径，并重启进程
# 校验数据真写进了 vault（而不是前端内存假象）。
# 前置：debug exe 已构建。未构建时**不算失败**（跳过并说明），因为 cargo test 已覆盖编译，
# 而真正的构建验收由 build-installer 承担；避免让纯文档/前端改动被 Rust 构建耗时拖住。
function Invoke-DesktopRealWindowE2e {
    if ($Exclude -contains "desktop-e2e-real") {
        Write-Host "SKIP  desktop-e2e-real"
        return
    }
    $exe = Join-Path $repo "desktop/src-tauri/target/debug/ab-desktop.exe"
    if (-not (Test-Path $exe)) {
        Write-Host "SKIP  desktop-e2e-real（未构建 $exe；先跑 cargo build --manifest-path desktop/src-tauri/Cargo.toml --offline --features custom-protocol）"
        return
    }
    Write-Host "RUN   desktop-e2e-real"
    Push-Location (Join-Path $repo "desktop")
    node scripts/e2e-real-window.mjs 2>&1 | ForEach-Object { Write-Host "      $_" }
    $ok = ($LASTEXITCODE -eq 0)
    Pop-Location
    $script:results += [pscustomobject]@{ Name = "desktop-e2e-real"; Pass = $ok }
    Write-Host ("{0}  desktop-e2e-real" -f $(if ($ok) { "PASS" } else { "FAIL" }))
}
Invoke-DesktopRealWindowE2e

$passed = @($results | Where-Object { $_.Pass }).Count
$failed = @($results | Where-Object { -not $_.Pass })
Write-Host ""
Write-Host ("结果：{0} 项，通过 {1}，失败 {2}，跳过 {3}" -f `
    ($passed + $failed.Count + $Exclude.Count), $passed, $failed.Count, $Exclude.Count)
$failed | ForEach-Object { Write-Host ("  FAIL  " + $_.Name) }
if ($failed.Count -gt 0) { exit 1 }
exit 0
