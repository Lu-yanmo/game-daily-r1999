# tools/panel-cs/build.ps1 —— 用系统自带的 csc.exe(.NET Framework 4)把本目录的 .cs 编成 panel-cs.exe。
# 不依赖 dotnet SDK：玩家机器只要有 .NET Framework 4(Win10/11 出厂自带)就能跑产物。
# 源码从 2026-09-26 起是一个 partial class 的多份(Panel.cs + Panel.*.cs)，这里**按目录扫 *.cs 全给 csc**，
# 新加一份就自动进编译，不需要回来改这条命令行。
# 用法： pwsh -File tools\panel-cs\build.ps1 [-Verify] [-ConsoleTest] [-Quiet]
#   -Verify      = 编完顺手跑 --selftest(布局/主题链)
#   -ConsoleTest = 再跑一遍 --consoletest(主控台四页：取数、写配置的转义与备份、代跑参数、诊断包不含 key)
#   -ExpTest     = 再跑一遍 --exptest(「实验功能」三个布尔键的读写回环，只打在 %TEMP% 副本上)
#   -Quiet       = 只留失败信息，给 panel.cmd 自动编译用
[CmdletBinding()]
param([switch]$Verify, [switch]$ConsoleTest, [switch]$ExpTest, [switch]$Quiet)

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$cs   = @(Get-ChildItem -LiteralPath $here -Filter '*.cs' -File | Sort-Object Name | ForEach-Object { $_.FullName })
if ($cs.Count -eq 0) { throw "本目录里没有 .cs 源码：$here" }

# B-2b(2026-10-08)：Live.cs 那七枚现场 verb 要用的那批 user32 声明，**真身仍只有一份** =
# ..\tools\gdinterop\GdWin.cs(端游视觉兜底迁移 B0③b 时从 lib\vision-ark.ps1 的 here-string 搬出来的那份，
# PowerShell 侧靠 lib\common.ps1 的 Add-GdInterop 现编)。这里多带一条路径把它编进同一份装配，
# 不在 panel-cs 里抄第二份签名 —— 于是改那份 .cs 从此要过两套装配：PS 现编那侧的门 tools\test-ark.ps1 t14
# 与这条编译。缺文件就当编译失败(不许静默跳过：跳过了 Live.cs 会编不过，报错点反而更难读)。
$gdwin = Join-Path (Split-Path $here) 'gdinterop\GdWin.cs'
if (-not (Test-Path -LiteralPath $gdwin)) {
    if ($Quiet) { exit 4 }
    throw ('缺 tools\gdinterop\GdWin.cs(现场 verb 的 user32 声明，真身只这一份)：' + $gdwin)
}
$cs += $gdwin
# R9-a(2026-10-17)：轮骨架四格收编之后，panel-cs 要多带两份**别处已有唯一真身**的 .cs，
# 理由与上面那条完全同形 —— 宁可在装配里多带一条路径，也不在 tools\panel-cs 抄第二份：
#   ..\tools\gdinterop\GdPower.cs   屏保 SPI 那两枚 P/Invoke(Get-/Set-GdScreensaverActive 的声明本体)，
#                                   新立的 disable-screensaver-for-round 要用；PS 侧靠 Add-GdInterop 现编同一份。
#   ..\tools\gdcore\GdCore.Logs.cs  history.csv 的 CSV 规则(GdCore.Csv.Flatten/BuildRow/DurationText)，
#                                   新立的 append-round-history 要用；PS 侧 Add-GdHistory 走的是同一份内核真身。
# 缺文件一律当编译失败(不许静默跳过：跳过了 Round.cs 会编不过，报错点反而更难读)。
# ⚠ 这一份里还有 GdCore.Versions 与 GdCore.RunLock 两枚类，panel-cs **一格都不消费**(运行锁的 own 判据
#   在 Round.cs:247 自己那份) —— 带进来是"整份文件同进同出"的代价，登记在此免得将来被当成第二处真身。
$gdpower = Join-Path (Split-Path $here) 'gdinterop\GdPower.cs'
if (-not (Test-Path -LiteralPath $gdpower)) {
    if ($Quiet) { exit 4 }
    throw ('缺 tools\gdinterop\GdPower.cs(屏保 SPI 声明，真身只这一份)：' + $gdpower)
}
$cs += $gdpower
$gdlogs = Join-Path (Split-Path $here) 'gdcore\GdCore.Logs.cs'
if (-not (Test-Path -LiteralPath $gdlogs)) {
    if ($Quiet) { exit 4 }
    throw ('缺 tools\gdcore\GdCore.Logs.cs(history.csv 的 CSV 规则，真身只这一份)：' + $gdlogs)
}
$cs += $gdlogs
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
    '/reference:System.Windows.Forms.dll', '/reference:System.Drawing.dll',
    # 批次 0(Core.Json.cs)破例上 JavaScriptSerializer：那次 Panel.Settings.cs 说"为一处自检不值当加引用"判断不变，
    # 但迁移地基上的 JSON 不能靠手写近似。System.Web.Extensions 是 .NET Framework 自带装配(GAC)，玩家机器零新增依赖。
    '/reference:System.Web.Extensions.dll'
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
    # 批次 0 的 Core/契约自检先跑：轻、无窗体、只喂临时树(见 docs\ps-migration-batch0.md §6)
    & $exe --coretest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --coretest 有红项' }
    Write-Host 'panel-cs --coretest OK'
    & $exe --echotest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --echotest 有红项' }
    Write-Host 'panel-cs --echotest OK'
    # 批次 1 判定层：假数据回环，期望文案逐字抄自旧 PS(移植走样在这里就红)
    & $exe --dectest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --dectest 有红项' }
    Write-Host 'panel-cs --dectest OK'
    # 批次 2 探测/状态行：假树回环，期望串逐字抄自旧实现(同上一条纪律)
    & $exe --statetest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --statetest 有红项' }
    Write-Host 'panel-cs --statetest OK'
    # 批次 3 外部进程层：纯函数回环(不起进程/不写文件)，期望串逐字抄自旧实现(同上一条纪律)
    & $exe --proctest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --proctest 有红项' }
    Write-Host 'panel-cs --proctest OK'
    # 批次 4 视觉兜底层：纯规则段回环(不联网/不碰窗口/不起子进程)，期望串逐字抄自旧实现(同上一条纪律)
    & $exe --visiontest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --visiontest 有红项' }
    Write-Host 'panel-cs --visiontest OK'
    # 批次 5 编排/排程层：纯规则段回环(不起进程/不读盘/不写文件)，期望串逐字抄自旧实现(同上一条纪律)
    & $exe --orchtest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --orchtest 有红项' }
    Write-Host 'panel-cs --orchtest OK'
    # 批次 6 监督层纯判定段：四 verb 回环(不读盘/不起进程)，期望串逐字抄自旧实现(同上一条纪律)
    & $exe --svtest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --svtest 有红项' }
    Write-Host 'panel-cs --svtest OK'
    # 批次 7 活段判据：tick 剧本回环(读数/回执场景直喂，不碰挂钟/盘/进程)，期望串逐字抄自旧实现(同上一条纪律)
    & $exe --looptest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --looptest 有红项' }
    Write-Host 'panel-cs --looptest OK'
    # 对话小窗那条链的纯规则段：找解释器的顺序 / CRT 命令行转义 / 信封投影(不起进程、不联网、不要求本机装了 Python)
    & $exe --agenttest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --agenttest 有红项' }
    Write-Host 'panel-cs --agenttest OK'
    # B-2b 现场件七枚(Live.cs)的纯规则段回环：真窗口/真 HTTP/真 key 一个都不碰，期望串逐字抄自旧实现(同上一条纪律)
    & $exe --livetest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --livetest 有红项' }
    Write-Host 'panel-cs --livetest OK'
    # B-3②-a 轮骨架现场件七枚(Round.cs)的纯规则段回环：不弹 UAC/不打 webhook/不起气泡/不真杀进程/不读真任务，
    # 写盘的两枚(通知与运行锁)打在 %TEMP% 临时树上(只读自检不许要求机器现场状态，同上一条纪律)
    & $exe --roundtest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --roundtest 有红项' }
    Write-Host 'panel-cs --roundtest OK'
    # 面板自证：读一次 -DumpSummary 协议 + 渲染 + 逐页布局断言 + 主题切换链
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

if ($ExpTest) {
    # 「实验功能」三个布尔键的读写回环：翻一次读得回来 / 只有那一行动过 / 补出来的那行过得了 ConvertFrom-Json /
    # runtime 形状认不出时必须拒着不写。全程打在 %TEMP% 副本上，收尾逐字节比对确认那份真 settings.json 没动。
    & $exe --exptest
    if ($LASTEXITCODE -ne 0) { throw 'panel-cs --exptest 有红项' }
    Write-Host 'panel-cs --exptest OK'
}
