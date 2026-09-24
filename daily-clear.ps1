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
#>
# 双引擎兼容：Windows PowerShell 5.1(系统自带) 与 PowerShell 7 都要能跑，别再加 #Requires 版本门把访客挡在门外。
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
. (Join-Path $PSScriptRoot 'lib\m9a.ps1')
. (Join-Path $PSScriptRoot 'lib\mumu.ps1')
. (Join-Path $PSScriptRoot 'lib\r1999-daily.ps1')

Start-GdTranscript -Tag "daily-$Tag"
Write-GdLog "=== 1999 日常开始(tag=$Tag, dryRun=$DryRun)===" -Level STEP

# 没配置这件事由脚本自己用日志的格式说清楚就走(-TellAndExit)：裸 throw 到了顶层是一屏
# "Exception: C:\...\common.ps1:120 / Line | ..." 那种内部报错，双击的人只会以为程序坏了。
$S = Get-GdSettings -TellAndExit
$envOk = @()
$envBad = @()
foreach ($probe in @(
        @{ N = 'M9A 目录'; P = $S.paths.m9aDir },
        @{ N = 'M9A appsettings'; P = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aAppSettings)) },
        @{ N = 'M9A 实例目录'; P = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aInstancesDir)) },
        @{ N = 'MuMu Manager'; P = (Join-GdPath $S.paths.mumuDir @($S.paths.mumuManager)) }
    )) {
    if (Test-GdPathReady $probe.P) { $envOk += $probe.N } else { $envBad += "$($probe.N) → $(if ($probe.P) { $probe.P } else { '(没填)' })" }
}
Write-GdLog "环境检查：$($envOk.Count) 项就绪"
if ($envBad.Count) {
    $envBad | ForEach-Object { Write-GdLog "缺失：$_" -Level ERROR }
    throw '环境不完整，见上面缺失项(可跑 setup.ps1 或改 config\settings.json 的路径)'
}

# ---------- 活动确认门 / 商店领空标记(具体动作在 lib\r1999-daily.ps1) ----------
if ($Ack) { Set-GdR1999Ack -S $S }
if ($ShopDone -or $ShopReset) { Set-GdR1999ShopState -S $S -Done:$ShopDone }
if ($Ack -or $ShopDone -or $ShopReset) {
    if ($DryRun) { Write-GdLog '[dry-run] 仅记录确认/领空状态，结束'; exit 0 }
    Write-GdLog '状态已更新；继续执行本轮日常'
}

# ---------- 判定：今天用哪个 M9A 实例 ----------
$rd = Resolve-GdR1999Decision -S $S -Plan $Plan -Force:$Force
$r1999 = $rd.Decision
$inst = $rd.Instance

if ($DryRun) {
    Write-GdLog '[dry-run] 预览将要下发的配置，不启动任何程序' -Level STEP
    Show-GdR1999Preview -S $S -Instance $inst
    Write-GdLog '[dry-run] 完成' -Level OK
    exit 0
}

# ---------- 运行 ----------
if ($S.runtime.blockSleep) { Enter-GdNoSleep -KeepScreenOn ([bool]$S.runtime.keepScreenOn) }
$fatal = $null
try {
    $r = Start-GdR1999Daily -S $S -Instance $inst
} catch {
    $fatal = $_.Exception.Message
    Write-GdLog "本轮异常中断：$fatal" -Level ERROR
    $r = @{ Status = 'error'; Detail = $fatal; Duration = 0 }
} finally {
    if (-not $SkipCleanup) {
        Write-GdLog '--- 收尾 ---' -Level STEP
        Stop-M9A -S $S
        if ($S.runtime.closeEmulatorAfter) { Stop-GdMuMu -S $S }
    }
    Exit-GdNoSleep
}

# ---------- 汇总 + 历史 + 通知 ----------
$ok = ($r.Status -eq 'ok') -and -not $fatal
$mins = if ($r.Duration) { " $([math]::Round($r.Duration / 60, 1)) 分钟" } else { '' }
$summary = "1999：$($r.Status)$mins — $($r.Detail)"
Add-GdHistory -Game 'r1999' -Status $r.Status -Mode $r1999.Mode -Detail $r.Detail -DurationSec $r.Duration
Write-GdLog "=== 本轮结束：$(if ($ok) { '完成' } else { '需要人工看一眼' }) ===" -Level $(if ($ok) { 'OK' } else { 'WARN' })
Write-Host $summary

$noticeText = "1999：$($r1999.Mode)`n$summary"
if (-not $ok) { $noticeText += "`n(详情见 logs\$([System.IO.Path]::GetFileName($global:GD.LogFile)))" }
Send-GdNotice -Title $(if ($ok) { '1999 日常已完成' } else { '1999 日常需要人工看一眼' }) -Text $noticeText -Level $(if ($ok) { 'OK' } else { 'WARN' })

exit $(if ($ok) { 0 } else { 2 })
