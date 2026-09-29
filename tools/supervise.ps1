# tools/supervise.ps1 — 监督 agent 的子进程本体(心跳循环)
#
# 谁起它：lib\supervisor.ps1 的 Start-GdSupervisor(主流程在 Start-GdRunLock 之后开一个隐藏子进程)。
# 谁停它：主流程收尾调 Stop-GdSupervisor(写 state\supervisor.stop → 它每 5 秒瞄一次，看到就自己退)；
#         主进程被硬杀、标记没人写时，它按"owner 进程消失"自退，不会变成孤儿监工。
# 为什么不并进主流程：主流程那两个等待循环(Invoke-MaaRun / M9A 那路)正是"既干活又盯进度"的裁判本人，
#   监督要防的就是**它自己挂住** —— 同一条线程上的旁观者不算旁观者。
#
# 手动用法(排查/自检)：
#   powershell -NoProfile -File tools\supervise.ps1 -OwnerPid <活进程pid> -Games all -Once
#   -Once 只拍一拍就退，用来验"采集/记账这条链通不通"，不长期驻留。
# 退出码：0=正常收工(停拍标记/owner 消失/-Once/到达存活上限)；3=起不来(没配置/没有可监督的路线)。
[CmdletBinding()]
param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [string]$Games = 'all',
    [int]$CapMinutes = 480,          # 存活上限：主流程僵而不死、一直没走到收尾时，监督也不该永续
    [switch]$Once
)

$ErrorActionPreference = 'Stop'
# 顶层 dot-source 的顺序和 daily-clear.ps1 一致：内核三件套 + 监督库，再按清单把各路的业务库点进来
# (在函数里点的会连函数定义一起消失，钩子稍后就调不到 —— 见 modules.ps1 里 Import-GdModules 的注释)。
. (Join-Path $PSScriptRoot '..\lib\common.ps1')
. (Join-Path $PSScriptRoot '..\lib\decisions.ps1')
. (Join-Path $PSScriptRoot '..\lib\modules.ps1')
. (Join-Path $PSScriptRoot '..\lib\supervisor.ps1')
[void](Import-GdModules -Quiet)
foreach ($libPath in (Get-GdModuleLibPaths)) { . $libPath }

Start-GdTranscript -Tag 'supervise'

# 内核接线(接线批)：监督是**长命宿主**(一轮里每隔几十秒拍一次，读盘全走 Read-GdJson/Get-GdSettings)，
# 所以它是七个函数在真实那一轮里最密集的用处 —— 起跑先编一次，之后整轮都吃这一口。
$null = Add-GdCore
$coreNote = Get-GdCoreStatus
if ($coreNote) { Write-GdLog $coreNote -Level WARN }
Write-GdLog (Get-GdCoreReadout)

# 监督只旁观，配置缺哪项都够它把心跳拍下去；但 settings.json 整个读不到就没什么可监督的(主流程也起不来)。
$S = $null
try { $S = Get-GdSettings -Quiet } catch { Write-GdLog "配置读不到，监督不起：$($_.Exception.Message)" -Level ERROR; exit 3 }
$null = Sync-GdCoreOff -Settings $S

# 要盯的路线：注册表里"装了 + 给了 SupervisorEvidence 钩子"的那几路，再按 -Games 收窄。
$want = @($Games -split '\+' | Where-Object { $_ -and $_ -ne 'all' })
$mods = @()
foreach ($m in @(Get-GdModules -AvailableOnly)) {
    if ($null -eq $m.Hooks.SupervisorEvidence) { continue }     # 没声明可看证据的路不监督(可选钩子口径)
    if ($want.Count -and ($want -notcontains [string]$m.Id)) { continue }
    $mods += $m
}
if (-not $mods.Count) {
    Write-GdLog "没有可监督的路线(Games=${Games}，注册表里没有一路给 SupervisorEvidence 钩子，或那几路没装)" -Level ERROR
    exit 3
}

$interval = Get-GdSupervisorIntervalSec -S $S
$quietSec = Get-GdSupervisorQuietSec -S $S
$maxIntv = Get-GdSupervisorMaxInterventions -S $S
$ctx = @{ asst_offset = @{}; task_seen = @{} }                  # 跨拍的读日志偏移/上下文，按路线 id 分格
$everAlive = @{}; $finished = @{}; $quietStreak = @{}; $notified = @{}; $lastAsk = @{}
$interventions = 0
$actions = @()                                                   # 整轮累计的干预流水，进 supervision.json
$startedAt = Get-Date
$meta = [ordered]@{ pid = $PID; owner = $OwnerPid; started = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'); games = $Games }
$gameIds = (@($mods | ForEach-Object { [string]$_.Id }) -join '+')
Write-GdLog "监督心跳开始(间隔 ${interval}s，静止阈值 ${quietSec}s，games=$gameIds，owner PID $OwnerPid)"

function Test-GdSupervisorStopRequested {
    <# 每 5 秒瞄一次的"该不该下播"：停拍标记(主流程正常收尾) 或 owner 进程消失(被硬杀)。
       返回 '' = 继续。用命名函数而不是内联，好让 -Once 之外的三条退出路径都走同一个判据。 #>
    param([int]$Owner)
    if (Test-Path -LiteralPath (Get-GdSupervisorStopPath)) { return 'stop' }
    try { $p = Get-Process -Id $Owner -ErrorAction Stop; $p.Dispose() } catch { return 'owner' }
    return ''
}

$leave = $false
while (-not $leave) {
    # 主日志路径现拍现取：running.json 登记的就是主流程自己那份 transcript(它的 LastWriteTime 即"整体还在动")
    $dailyLog = ''
    try { $lock = Get-GdRunLock; if ($lock) { $dailyLog = [string]$lock.Log } } catch { }

    $perGame = @()
    foreach ($m in $mods) {
        $id = [string]$m.Id
        $state = 'busy'; $why = ''; $actionKind = 'none'; $screenNote = ''
        $ev = $null; $screen = $null
        try {
            $spec = Invoke-GdModuleHook -Module $m -Hook SupervisorEvidence -Params @{ S = $S }
            $ev = Collect-GdSupervisorEvidence -S $S -Spec $spec -Ctx $ctx -DailyLogPath $dailyLog -Game $id
            if ($ev.AssistantAlive) { $everAlive[$id] = $true }
            if ([bool]$finished[$id]) {
                $state = 'done'      # 完成线见过一次就定格：之后助手自己退了不算"人间蒸发"
            }
            else {
                $state = Get-GdSupervisorState -Evidence $ev -QuietSec $quietSec -EverAlive ([bool]$everAlive[$id])
                if ($state -eq 'done') { $finished[$id] = $true }
            }
            if ($state -eq 'quiet') { $quietStreak[$id] = [int]$quietStreak[$id] + 1 } else { $quietStreak[$id] = 0 }
            # 复核只在"规则连续两拍说可疑"时发生，且同一路线两次问模型至少隔 5 分钟 —— 平时零 token。
            if ($state -eq 'quiet' -and [int]$quietStreak[$id] -ge 2) {
                $askNow = $true
                if ($lastAsk.ContainsKey($id) -and $lastAsk[$id] -and ((Get-Date) - $lastAsk[$id]).TotalSeconds -lt 300) { $askNow = $false }
                if ($askNow) {
                    $lastAsk[$id] = Get-Date
                    $screen = Request-GdSupervisorScreen -S $S -Module $m -Game $id
                }
            }
            $act = Get-GdSupervisorAction -State $state -QuietStreak ([int]$quietStreak[$id]) -Screen $screen `
                                          -CanRestart ($null -ne $m.Hooks.SupervisorRestart) `
                                          -Interventions $interventions -MaxInterventions $maxIntv `
                                          -AlreadyNotified ([bool]$notified[$id + ':' + $state])
            $actionKind = [string]$act.Kind
            $why = [string]$act.Why
            if ($null -ne $screen) {
                $screenNote = if ($screen.Ok) { if ($screen.Blocking) { 'blocking' } else { 'clear' } } else { 'ask-failed' }
            }
            switch ($actionKind) {
                'restart_assistant' {
                    $ok = Restart-GdSupervisorAssistant -S $S -Module $m
                    $interventions++
                    # pscustomobject 而不是有序字典：hashtable 走 += 进数组会被摊开成字典项(5.1/7 同坑)
                    $actions += [pscustomobject]@{ ts = (Get-Date -Format 'HH:mm:ss'); game = $id; kind = 'restart_assistant'; ok = [bool]$ok; why = $why }
                    $quietStreak[$id] = 0; $lastAsk[$id] = $null
                    Send-GdNotice -Title "监督介入：${id}助手已重启" -Text $why -Level WARN
                }
                'notify' {
                    $notified[$id + ':' + $state] = $true
                    $actions += [pscustomobject]@{ ts = (Get-Date -Format 'HH:mm:ss'); game = $id; kind = 'notify'; ok = $true; why = $why }
                    Send-GdNotice -Title "监督提醒：$id 这一路不对劲" -Text ("$state ： $why") -Level WARN
                }
            }
        }
        catch {
            # 采集这一拍没成：记 busy 太乐观、记 dead 太吓人，就记个带原因的 unknown，下一拍继续。
            # 原因截到 300 字：PowerShell 的绑参错误消息会把整个对象打印进去(实测一条 4MB)。
            $w = [string]$_.Exception.Message
            if ($w.Length -gt 300) { $w = $w.Substring(0, 300) + '…' }
            $state = 'unknown'; $why = $w
        }
        $aliveRow = $false; $ageRow = -1
        if ($null -ne $ev) { $aliveRow = [bool]$ev.AssistantAlive; $ageRow = [int]$ev.AsstAgeSec }
        $perGame += [pscustomobject]@{
            game     = $id
            state    = $state
            alive    = $aliveRow
            asst_age = $ageRow
            done     = [bool]$finished[$id]
            screen   = $screenNote
            action   = $actionKind
            why      = $why
        }
    }

    try { Write-GdSupervisorBeat -Meta $meta -PerGame $perGame -Actions $actions }
    catch { Write-GdLog "这一拍的记录没写进去：$($_.Exception.Message)" -Level WARN }

    if ($Once) { Write-GdLog '-Once：拍完这一拍就收工'; break }
    if (((Get-Date) - $startedAt).TotalMinutes -gt $CapMinutes) {
        Write-GdLog "到达存活上限 ${CapMinutes} 分钟，主动收工(主流程没走到收尾？去看它)" -Level WARN
        break
    }
    # 睡眠切片：每 5 秒瞄一次停拍标记/owner，Stop-GdSupervisor 的宽限因此用不到一整拍
    $sleepUntil = (Get-Date).AddSeconds($interval)
    while ((Get-Date) -lt $sleepUntil) {
        Start-Sleep -Seconds 5
        $why2 = Test-GdSupervisorStopRequested -Owner $OwnerPid
        if ($why2) {
            Write-GdLog $(if ($why2 -eq 'stop') { '收到停拍标记，监督收工' } else { 'owner 进程消失了，监督下班' })
            $leave = $true
            break
        }
    }
}

# 自己退场前把 stopped 写回最终读数，面板念得出"几点下班"而不是"进程没了(崩?)"
try {
    $p = Get-GdSupervisorStatePath
    if (Test-Path -LiteralPath $p) {
        $txt = ''
        $fs = [System.IO.File]::Open($p, 'Open', 'Read', 'ReadWrite')
        try { $txt = (New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8)).ReadToEnd() } finally { $fs.Dispose() }
        if ($txt) {
            $doc = $txt | ConvertFrom-Json
            $doc.stopped = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
            $doc | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $p -Encoding utf8
        }
    }
} catch { }
exit 0
