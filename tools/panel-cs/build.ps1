# tools/panel-cs/build.ps1 —— 用系统自带的 csc.exe(.NET Framework 4)把 Panel.cs 编成 panel-cs.exe。
# 不依赖 dotnet SDK：玩家机器只要有 .NET Framework 4(Win10/11 出厂自带)就能跑产物。
# 用法： powershell -File tools\panel-cs\build.ps1 [-Verify] [-ConsoleTest] [-Quiet]
#   -Verify      = 编完顺手跑 --selftest(布局/主题链)
#   -ConsoleTest = 再跑一遍 --consoletest(主控台四页：取数、写配置的转义与备份、代跑参数、诊断包不含 key)
#   -Quiet       = 只留失败信息，给 panel.cmd 自动编译用
[CmdletBinding()]
param([switch]$Verify, [switch]$ConsoleTest, [switch]$Quiet)

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$cs   = Join-Path $here 'Panel.cs'
$exe  = Join-Path $here 'panel-cs.exe'
$icon = Join-Path (Split-Path $here) 'assets\icon.png'

# 64 位优先；极端机器没有 Framework64 再退回 32 位
$csc = Join-Path $env:SystemRoot 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $csc)) { $csc = Join-Path $env:SystemRoot 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
if (-not (Test-Path -LiteralPath $csc)) {
    if ($Quiet) { exit 3 }   # 自动编译场合：这台机器真没有系统 csc，安静让路
    throw '找不到系统自带 csc.exe(.NET Framework 4 编译器)'
}

$args = @(
    '/nologo', '/target:winexe', '/optimize+', '/codepage:65001',   # 源码是无 BOM 的 UTF-8，别按机器 ANSI 码页猜中文
    "/out:$exe",
    '/reference:System.Windows.Forms.dll', '/reference:System.Drawing.dll'
)
if (Test-Path -LiteralPath $icon) { $args += "/resource:$icon,gdpanel.icon.png" }
$args += $cs

if (-not $Quiet) { Write-Host ('csc: ' + $csc) }
& $csc @args
if ($LASTEXITCODE -ne 0) {
    if ($Quiet) { exit $LASTEXITCODE }   # 自动编译失败：安静退，让 panel.cmd 回落 PowerShell 面板
    throw ('编译失败，退出码 ' + $LASTEXITCODE)
}
if (-not $Quiet) { Write-Host ('产物: ' + $exe + ' (' + (Get-Item -LiteralPath $exe).Length + ' bytes)') }

if ($Verify) {
    # 自检不进屏幕：读一次 -DumpSummary 协议 + 渲染 + 逐页布局断言 + 主题切换链
    & $exe --selftest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --selftest 有红项' }
    Write-Host 'panel-cs --selftest OK'
}

if ($ConsoleTest) {
    # 主控台四页：转义往返 / 只在 settings 临时副本上写 / 四页版式 / 代跑参数怎么拼 / 诊断包里搜不到 key
    & $exe --consoletest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --consoletest 有红项' }
    Write-Host 'panel-cs --consoletest OK'
}
