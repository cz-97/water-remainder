# 编译 Rust 项目并复制可执行文件
# 用法: 右键选择"使用 PowerShell 运行"，或在终端执行: .\build-and-deploy.ps1

# 设置错误处理：遇到错误立即停止
$ErrorActionPreference = "Stop"

# 切换到当前脚本所在目录（项目根目录）
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $scriptDir

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Rust 项目编译与部署脚本" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 编译 Rust 项目（release 模式）
Write-Host "[1/4] 正在编译 Rust 项目（release 模式）..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "编译失败！请检查错误信息。" -ForegroundColor Red
    Read-Host "按 Enter 键退出"
    exit 1
}
Write-Host "编译成功！" -ForegroundColor Green
Write-Host ""

# 设置源文件和目标目录
$src = "target\release\water-remainder.exe"
$dest = "D:\executable"

# 检查源文件是否存在
if (-not (Test-Path $src)) {
    Write-Host "错误：找不到编译后的文件 $src" -ForegroundColor Red
    Read-Host "按 Enter 键退出"
    exit 1
}

# 如果目标文件夹不存在则创建
Write-Host "[2/4] 检查目标目录..." -ForegroundColor Yellow
if (-not (Test-Path $dest)) {
    Write-Host "目标目录不存在，正在创建: $dest" -ForegroundColor Yellow
    New-Item -ItemType Directory -Path $dest -Force | Out-Null
    Write-Host "目录创建成功" -ForegroundColor Green
} else {
    Write-Host "目标目录已存在" -ForegroundColor Green
}
Write-Host ""

# 复制并覆盖
Write-Host "[3/4] 正在复制文件..." -ForegroundColor Yellow
Copy-Item -Path $src -Destination $dest -Force
Write-Host "文件复制成功！" -ForegroundColor Green
Write-Host ""

# 显示结果
Write-Host "[4/4] 部署完成！" -ForegroundColor Cyan
Write-Host ""
Write-Host "源文件: " -NoNewline; Write-Host $src -ForegroundColor White
Write-Host "目标目录: " -NoNewline; Write-Host $dest -ForegroundColor White
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  编译并复制完成（覆盖模式）" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 暂停等待用户按键
Read-Host "按 Enter 键退出"
