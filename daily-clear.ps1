<#
  daily-clear.ps1 — 《重返未来：1999》MuMu + M9A 每日清体力(单游戏仓库入口)

  .\daily-clear.ps1 -DryRun          # 只看今天会怎么跑，不碰任何程序
  .\daily-clear.ps1                  # 完整跑一轮
  .\daily-clear.ps1 -Ack             # 确认"新版本的活动本我手动打过了"→ 之后每天自动跑活动方案
  .\daily-clear.ps1 -ShopDone        # 确认"活动商店已换空"→ 回落到长草方案
  .\daily-clear.ps1 -ShopReset       # 撤销领空标记，恢复跑活动
  .\daily-clear.ps1 -Plan rerun      # 临时强制方案(idle / event / rerun)，不传就是 auto
  .\daily-clear.ps1 -SkipCleanup     # 跑完不关 M9A / 模拟器(自己接着玩)

  计划任务每日按 config\settings.json 的 schedule.times 调用：daily-clear.ps1 -Tag auto
  权限：这一路不需要管理员(MuMu 和 M9A 都在普通权限下跑)，所以没有 UAC。
  退出码：0=这一轮判定为完成；2=没完成(细节看 logs\ 里那一轮的日志和通知气泡)。

  —— 本文件现在是什么 ——
  和 ark 那份薄壳同构：只把 -Ack / -ShopDone / -Plan 翻译成"给 r1999 模块的哪道门、哪个方案"，
  整轮流程交给 lib\rust-round.ps1 的 Invoke-GdDailyRoundDispatch。这一枚调度器把整轮交给核内十段骨架
  (gdcore.exe --daily-round)，而这个仓库按口径只带 Rust 源码、不带编译产物 ⇒ 克隆侧要真跑一轮得先自己编
  内置件(pwsh -File tools\rustcore\build.ps1，要 cargo)；编不出来 / 拿不到内置件就是**拒跑(2)**——
  PS 十段兜底已于 2026-10-20 乙批② 退役，不再有"换人跑"这一档。
  要不要管理员这件事由清单的 RequiresAdmin 说了算，
  所以这个入口连 -NoElevate 参数都没有(它那一路根本提不了什么权)。
#>
# 运行环境：PowerShell 7(pwsh)。本仓库只支持 7，不再为系统自带的 Windows PowerShell 5.1 留兼容退路。
[CmdletBinding()]
param(
    [ValidateSet('auto', 'manual')][string]$Tag = 'manual',
    [switch]$DryRun,
    [Alias('ConfirmEvent')][switch]$Ack,
    [Alias('MarkShopDone')][switch]$ShopDone,
    [switch]$ShopReset,
    [switch]$SkipCleanup,
    [switch]$Force,
    [ValidateSet('auto', 'idle', 'event', 'rerun')][string]$Plan = 'auto'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\common.ps1')
. (Join-Path $PSScriptRoot 'lib\decisions.ps1')      # 门面：仓库里有哪些判定文件就点哪些
. (Join-Path $PSScriptRoot 'lib\modules.ps1')
. (Join-Path $PSScriptRoot 'lib\rust-round.ps1')   # 入口转接层：整轮交给核内十段骨架，拿不到内置件就整轮拒跑(乙批② 起没有 PS 十段回退)
# 监督面自 11-02 刀③ 起不在这里装载(旧 dot-source 那行已随 lib\supervisor.ps1 退役；真身 tools\panel-cs\Supervisor.Host.cs，轮档段 7 现问 start-supervisor)

Start-GdTranscript -Tag "daily-$Tag"
Write-GdLog "=== 1999 日常开始(tag=$Tag, dryRun=$DryRun)===" -Level STEP

# ---------- 内核接线(源码 tools\gdcore\*.cs，装载器与契约号在 lib\common.ps1 末尾那块) ----------
# 必须排在 Get-GdSettings 之前(内核化的七件里就有读配置那两件)；编不成一律静默回退 PS 原身，内核不是硬依赖。
$null = Add-GdCore
$coreT0 = Get-Date
$coreNote = Get-GdCoreStatus
if ($coreNote) { Write-GdLog $coreNote -Level WARN }
Write-GdLog (Get-GdCoreReadout)

[void](Import-GdModules)
foreach ($libPath in (Get-GdModuleLibPaths)) { . $libPath }
# 11-01 丙形真转接(§11.176 ⑥)：单游戏仓库里现问注册表走回落那一支，提示语保持本入口真认的干净短名，同步件已退役。

# 没配置这件事由脚本自己用日志的格式说清楚就走(-TellAndExit)：裸 throw 到了顶层是一屏内部报错，
# 双击的人只会以为程序坏了。
$S = Get-GdSettings -TellAndExit

# settings.runtime.coreOff：单个内核函数出 bug，改一行配置就把那一件退回 PS 原身(当日止血，不挡其余)

$gates = @()
if ($Ack) { $gates += @{ Id = 'r1999'; Verb = 'Ack' } }
if ($ShopDone) { $gates += @{ Id = 'r1999'; Verb = 'ShopDone' } }
if ($ShopReset) { $gates += @{ Id = 'r1999'; Verb = 'ShopReset' } }

$code = Invoke-GdDailyRoundDispatch -S $S -ElevateScript (Join-Path $PSScriptRoot 'daily-clear.ps1') -BoundParams $PSBoundParameters -Gates $gates -Opt @{ Plan = $Plan } -Force:$Force -DryRun:$DryRun -SkipCleanup:$SkipCleanup -NameField ShortName -NoticeOwner '1999' -Tag $Tag
Write-GdLog ('收尾 ' + (Get-GdCoreReadout -ElapsedSec ((Get-Date) - $coreT0).TotalSeconds))
exit $code
