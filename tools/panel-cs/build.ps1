# tools/panel-cs/build.ps1 —— 用系统自带的 csc.exe(.NET Framework 4)把 Panel.cs 编成 panel-cs.exe。
# 不依赖 dotnet SDK：玩家机器只要有 .NET Framework 4(Win10/11 出厂自带)就能跑产物。
# 用法： powershell -File tools\panel-cs\build.ps1 [-Verify]      (-Verify = 编完顺手跑 --selftest)
[CmdletBinding()]
param([switch]$Verify)

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$cs   = Join-Path $here 'Panel.cs'
$exe  = Join-Path $here 'panel-cs.exe'
$icon = Join-Path (Split-Path $here) 'assets\icon.png'

# 64 位优先；极端机器没有 Framework64 再退回 32 位
$csc = Join-Path $env:SystemRoot 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $csc)) { $csc = Join-Path $env:SystemRoot 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
if (-not (Test-Path -LiteralPath $csc)) { throw '找不到系统自带 csc.exe(.NET Framework 4 编译器)' }

$args = @(
    '/nologo', '/target:winexe', '/optimize+',
    "/out:$exe",
    '/reference:System.Windows.Forms.dll', '/reference:System.Drawing.dll'
)
if (Test-Path -LiteralPath $icon) { $args += "/resource:$icon,gdpanel.icon.png" }
$args += $cs

Write-Host ('csc: ' + $csc)
& $csc @args
if ($LASTEXITCODE -ne 0) { throw ('编译失败，退出码 ' + $LASTEXITCODE) }
Write-Host ('产物: ' + $exe + ' (' + (Get-Item -LiteralPath $exe).Length + ' bytes)')

if ($Verify) {
    # 自检不进屏幕：读一次 -DumpSummary 协议 + 渲染 + 逐页布局断言 + 主题切换链
    & $exe --selftest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --selftest 有红项' }
    Write-Host 'panel-cs --selftest OK'
}
