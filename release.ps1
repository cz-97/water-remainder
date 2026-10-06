# 构建、部署与发布。
#
# 两种模式，由是否传版本号决定（对应 AGENTS.md 的「部署」与「发布」）：
#
#   .\release.ps1                只部署：release 构建 + 复制到本机部署目录
#   .\release.ps1 0.11.0         发布：部署 + 同步 Cargo.toml + 提交 + 推 tag
#                                （tag 推送后由 GitHub Actions 建发行版）
#
# 加 -NoPause 可在非交互场景（CI、被别的脚本调用）下运行，结束时不再等待按键。
#
# 为什么版本号要由脚本写进 Cargo.toml：GitHub Actions 的第一步会核对
# tag 与 Cargo.toml 的 version，不一致直接失败。手改容易漏，而漏了要等一轮CI
# 才发现。脚本改完立刻能对照。
#
# 为什么 tag 必须在提交之后推：Actions 读的是 **tag 指向的那个 commit** 里的
# workflow 文件。先推 tag 再改 workflow，跑到的是旧版步骤。

param(
    # 目标版本号，如 0.11.0。省略时只做本地部署。
    [Parameter(Position = 0)]
    [string]$Version,
    [switch]$NoPause
)

# 遇到错误立即停止，否则后续步骤会在错误状态上继续跑。
# 但外部命令（cargo / git）的正常输出会走 stderr，PowerShell 在
# ErrorActionPreference=Stop 下会把那当成终止性错误抛出来 —— cargo 编译成功时
# 也会先打印 "Compiling ..." 到 stderr。调用外部命令时临时放宽，
# 改用 $LASTEXITCODE 判断成败。
$ErrorActionPreference = "Stop"

# 切到项目根目录（脚本自己所在目录），这样从别处调用也能跑
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $scriptDir

# 本机部署目录。改这里就改了部署位置。
$DeployDir = "D:\executable"

function Stop-WithPause {
    param([int]$Code)
    if (-not $NoPause) { Read-Host "`n按 Enter 键退出" }
    exit $Code
}

function Step {
    param([string]$Text)
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Fail {
    param([string]$Text)
    Write-Host "错误：$Text" -ForegroundColor Red
    Stop-WithPause 1
}

# 跑外部命令并把**退出码**返回给调用方，输出直接显示。
# 两个坑：
# 1. `$ErrorActionPreference = "Continue"` 是必要的 —— cargo 与 git 都把进度信息写到
#    stderr，在 Stop 模式下 PowerShell 会把它当成异常抛出来（编译成功也会抛）。
# 2. 用 `Write-Host` 而不是管道：退出码必须是唯一的返回值。若把输出也写进管道，
#    调用方的 `$x -ne 0` 就会拿数组和数字比较，语义全错。
# 需要读输出（如 `git status --porcelain`）时用 `Get-ExternalOutput`。
function Invoke-External {
    param([string]$Command, [string[]]$Arguments)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $Command @Arguments 2>&1 | ForEach-Object { Write-Host $_ }
        return $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
}

# 跑外部命令并**返回输出行**（供需要读结果的调用方）。
# 同上要放宽 ErrorActionPreference，理由相同。
function Get-ExternalOutput {
    param([string]$Command, [string[]]$Arguments)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        return @(& $Command @Arguments 2>&1 |
            Where-Object { $_ -is [string] -and $_.Trim().Length -gt 0 } |
            ForEach-Object { $_.ToString().Trim() })
    } finally {
        $ErrorActionPreference = $previous
    }
}

Write-Host "========================================" -ForegroundColor Cyan
if ($Version) {
    Write-Host "  构建、部署与发布  $Version" -ForegroundColor Cyan
} else {
    Write-Host "  构建与部署" -ForegroundColor Cyan
}
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# ---------------------------------------------------------------- 前置检查

# 版本号格式先查：拼错成 0.11 或 v0.11.0 会在 Actions那步才失败，
# 而那时已经花掉一次 release 构建。放在构建之前一秒就能报错。
if ($Version -and $Version -notmatch '^\d+\.\d+\.\d+$') {
    Fail "版本号格式不对：'$Version'。应为三段数字，如 0.11.0（不要带 v 前缀）"
}

# 脏工作区检查只在发布模式生效。发布要提交并打 tag，带着无关改动提交会把它们
# 一起带上；纯部署不碰 git，因此不该拦。
if ($Version) {
    Step "检查工作区状态"
    $dirty = @(Get-ExternalOutput "git" @("status", "--porcelain"))
    if ($dirty.Count -gt 0) {
        Write-Host "有未提交的改动：" -ForegroundColor Yellow
        $dirty | ForEach-Object { Write-Host "  $_" -ForegroundColor Yellow }
        Fail "请先提交或暂存这些改动再发布（只做部署可忽略此检查）"
    }
    Write-Host "工作区干净" -ForegroundColor Green

    $branch = @(Get-ExternalOutput "git" @("rev-parse", "--abbrev-ref", "HEAD"))
    if ($branch.Count -eq 0 -or $branch[0] -ne "master") {
        Fail "当前分支是 '$($branch -join '')'，请先切到 master 再发布"
    }
}

# ---------------------------------------------------------------- 构建

Step "release 构建"
# --offline 是本机用的：AGENTS.md 约定「联网受限时优先走本地缓存」。
# 缺依赖时报错会明确提示，不会静默降级。
$code = Invoke-External "cargo" @("build", "--release", "--offline")
if ($code -ne 0) {
    Fail "编译失败，请检查上面的错误信息"
}

$exe = "target\release\water-remainder.exe"
if (-not (Test-Path $exe)) {
    Fail "找不到构建产物 $exe"
}
$size = (Get-Item $exe).Length
Write-Host ("构建成功：{0}（{1:N2} MiB）" -f $exe, ($size / 1MB)) -ForegroundColor Green

# ---------------------------------------------------------------- 部署

# 运行中的进程会锁住目标 exe 导致复制失败，而且症状是Copy-Item 报「另一个
# 程序正在使用此文件」——不提前结束的话很容易误判成编译问题。
Step "结束运行中的实例"
$running = @(Get-CimInstance Win32_Process -Filter "Name='water-remainder.exe'" -ErrorAction SilentlyContinue)
if ($running.Count -gt 0) {
    foreach ($p in $running) {
        Write-Host ("  结束 PID {0}（{1}）" -f $p.ProcessId, $p.ExecutablePath) -ForegroundColor DarkGray
        Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 500
} else {
    Write-Host "  没有运行中的实例" -ForegroundColor DarkGray
}

Step "复制到部署目录"
if (-not (Test-Path $DeployDir)) {
    New-Item -ItemType Directory -Path $DeployDir -Force | Out-Null
    Write-Host "  已创建 $DeployDir"
}
Copy-Item -Path $exe -Destination $DeployDir -Force
$deployedSize = (Get-Item "$DeployDir\water-remainder.exe").Length
if ($deployedSize -ne $size) {
    # 尺寸不等说明复制被截断或写坏了，这种 exe 拷到用户手上会直接跑不起来。
    Fail "复制后大小不一致（源 $size，目标 $deployedSize）"
}
Write-Host ("  已部署（{0:N2} MiB）" -f ($deployedSize / 1MB)) -ForegroundColor Green

# ---------------------------------------------------------------- 发布

# 部署已完成。不带版本号就是纯部署到此为止 —— 顺序上刻意让部署在前，
# 这样「发布」这个动作必然包含一次可用的本地部署。
if (-not $Version) {
    Write-Host ""
    Write-Host "部署完成（未发布）。要发布请带上版本号：.\release.ps1 0.11.0" -ForegroundColor Cyan
    Write-Host ""
    Stop-WithPause 0
}

Step "同步 Cargo.toml 版本号"
$cargoToml = "Cargo.toml"
$content = Get-Content $cargoToml -Raw
# 只替换 [package] 段里那一行，不能全局替换 —— 依赖项也有 version 字段。
# 不要用 Regex.Replace 的 4 参数重载：那个签名第4 参是 RegexOptions 而非
# 「替换次数」，传 1 会被当成 IgnoreCase。这里用 -creplacecount 明确限定只改一处。
$match = [regex]::Match($content, '(?m)^(version\s*=\s*")[^"]+(")')
if (-not $match.Success) {
    Fail "未能在 $cargoToml 里找到 version 字段，格式是否变了？"
}
$currentVersion = $match.Value -replace '^version\s*=\s*"|"$', ''
if ($currentVersion -eq $Version) {
    # 内容和目标版本一致，不是「找不到字段」。这两种情况必须分开报，
    # 否则用户会以为是格式问题，去查一个根本没坏的东西。
    Fail "Cargo.toml 里已经是 $Version 了。若要重新发布该版本，请先手工执行：git tag -d v$Version; git push origin :refs/tags/v$Version"
}

$updated = [regex]::Replace(
    $content,
    '(?m)^(version\s*=\s*")[^"]+(")',
    "`${1}$Version`${2}",
    [System.Text.RegularExpressions.RegexOptions]::None,
    [TimeSpan]::FromSeconds(1)
)
# 确认改到的是 [package] 那一处，而不是某个依赖。
$pkgVersion = (Select-String -Path $cargoToml -Pattern '^version = "(.+)"$').Matches[0].Groups[1].Value
if ($pkgVersion -ne $Version) {
    Fail "改写后读回的版本是 '$pkgVersion'，期望 '$Version'（不要把版本号写进 [package] 以外的段）"
}
[System.IO.File]::WriteAllText($cargoToml, $updated, (New-Object System.Text.UTF8Encoding $false))
Write-Host "  $currentVersion -> $Version" -ForegroundColor Green

if ((Get-ExternalOutput "git" @("status", "--porcelain", $cargoToml)).Count -eq 0) {
    Fail "$cargoToml 没有产生改动，版本号可能已经是 $Version"
}

Step "提交版本号"
if ((Invoke-External "git" @("add", $cargoToml)) -ne 0) { Fail "git add 失败" }
if ((Invoke-External "git" @("commit", "-m", "Bump version to $Version")) -ne 0) {
    Fail "git commit 失败"
}

Step "推送 master"
if ((Invoke-External "git" @("push", "origin", "master")) -ne 0) {
    Fail "git push master 失败（网络问题可重试本脚本；版本号已提交，不会重复改动）"
}

Step "创建标签 v$Version"
$tag = "v$Version"
# 已存在的 tag 不覆盖：覆盖会让 GitHub 上已发布的版本与附件错位。
if ((Get-ExternalOutput "git" @("tag", "-l", $tag)).Count -gt 0) {
    Fail "标签 $tag 已存在。若要重新发布该版本，先手工确认：git tag -d $tag; git push origin :refs/tags/$tag"
}
if ((Invoke-External "git" @("tag", "-a", $tag, "-m", $Version)) -ne 0) {
    Fail "创建标签失败"
}

Step "推送标签（触发 GitHub Actions）"
if ((Invoke-External "git" @("push", "origin", $tag)) -ne 0) {
    Fail "推送标签失败。本地标签已创建，重试请用：git push origin $tag"
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  已推送 $tag，等待 Actions 完成" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "查看进度：gh run watch --repo cz-97/water-remainder" -ForegroundColor White
Write-Host "确认发行：gh release view $tag --repo cz-97/water-remainder" -ForegroundColor White
Write-Host ""
Write-Host "注意：推送成功不等于发布成功。Actions 仍可能失败，" -ForegroundColor Yellow
Write-Host "      务必等 run 结束并确认 release 上出现了附件。" -ForegroundColor Yellow
Write-Host ""

Stop-WithPause 0