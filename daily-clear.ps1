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
  整轮流程跑的是 lib\modules.ps1 里那一份实现。要不要管理员这件事由清单的 RequiresAdmin 说了算，
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
. (Join-Path $PSScriptRoot 'lib\supervisor.ps1')   # 监督 agent(可整行删掉=这一轮没人监督，主流程不受影响)

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
Sync-GdEntryVerbs

# 没配置这件事由脚本自己用日志的格式说清楚就走(-TellAndExit)：裸 throw 到了顶层是一屏内部报错，
# 双击的人只会以为程序坏了。
$S = Get-GdSettings -TellAndExit

# settings.runtime.coreOff：单个内核函数出 bug，改一行配置就把那一件退回 PS 原身(当日止血，不挡其余)
$null = Sync-GdCoreOff -Settings $S

$gates = @()
if ($Ack) { $gates += @{ Id = 'r1999'; Verb = 'Ack' } }
if ($ShopDone) { $gates += @{ Id = 'r1999'; Verb = 'ShopDone' } }
if ($ShopReset) { $gates += @{ Id = 'r1999'; Verb = 'ShopReset' } }

$code = Invoke-GdDailyRound -S $S -ElevateScript (Join-Path $PSScriptRoot 'daily-clear.ps1') -BoundParams $PSBoundParameters -Gates $gates -Opt @{ Plan = $Plan } -Force:$Force -DryRun:$DryRun -SkipCleanup:$SkipCleanup -NameField ShortName -NoticeOwner '1999' -Tag $Tag
Write-GdLog ('收尾 ' + (Get-GdCoreReadout -ElapsedSec ((Get-Date) - $coreT0).TotalSeconds))
exit $code
