# tools/test-r1999.ps1 — 重返未来1999 那一路的纯逻辑自检(只读：不改配置、不启动任何程序)
# 用法: pwsh -NoProfile -File tools\test-r1999.ps1 [-History]      (powershell.exe 5.1 同样能跑)
#       -History 追加"回放本机某天的真实 M9A 日志"那 7 条(断言的是那次真跑的内容，换机器不适用，故默认不跑)。
# 方舟那一路在 tools\test-ark.ps1；两边都跑用 tools\test-decisions.ps1。
# 公共那半套(计数器、临时 state 的存与取、[t1]/[t11])在 tools\test-lib.ps1。
param([switch]$NoExit, [switch]$History)

. (Join-Path $PSScriptRoot 'test-lib.ps1')
Show-GdCommonTimeChecks
Show-GdCommonPathChecks
Show-GdCommonVerbChecks
Show-GdCommonEntryHintChecks
Show-GdCommonScriptHygieneChecks
Show-GdCommonEmptyPathChecks
Show-GdCommonRunLockChecks

# [t4] 往后的每一组都要读 M9A 自己的文件(config\instances\*.json、日志)。paths.m9aDir 还没填时
# 先说人话，别让人对着"参数 'Path' 是空字符串"这种内部报错猜(和 test-ark.ps1 同一处理)。
if (-not (Test-GdPathReady $S.paths.m9aDir)) {
    Write-Host "`n  M9A 目录还没填或不存在 → [t4] 之后的断言跑不了，先跑 setup.ps1 探测安装位置" -ForegroundColor Yellow
    Check '助手目录已填好，这套自检才有意义(先跑 setup.ps1)' $false "paths.m9aDir=[$($S.paths.m9aDir)]"
    $res = Show-GdTestResult -Tag 'r1999'
    if ($NoExit) { return $res }
    exit 1
}

Write-Host "`n[t4] 1999 实例解析"
foreach ($key in @('idle', 'event', 'rerun')) {
    $name = Get-GdMapValue $S.re1999.planToInstance $key
    try {
        $inst = Resolve-R1999Instance -S $S -PlanKey $key
        Check "plan[$key]=$name" ($inst.Name -eq $name) "id=$($inst.Id) 任务 $($inst.Tasks.Count) 个"
    } catch { Check "plan[$key]=$name" $false $_.Exception.Message }
}
$r = Get-R1999Event -S $S
Check '1999 判定返回实例键' (@('idle', 'event', 'rerun') -contains $r.Instance) "mode=$($r.Mode) instance=$($r.Instance)"
Check '1999 判定有理由' ($r.Reason.Length -gt 8)

Write-Host "`n[t6] 用 M9A 历史日志回放解析器(只读，需 -History)"
$hist = Join-Path (Join-GdPath $S.paths.m9aDir @($S.paths.m9aLogDir)) 'log-20260830.log'
if (-not $History) {
    # 这一组断言的是"本机某天那次真跑"的内容(任务序列、失败收尾、长草那次有链路完成)。
    # 换台机器即使有同名日志、内容也是另一回事，会让访客的自检无故变红 —— 所以默认不跑，加 -History 才跑。
    Write-Host '  SKIP 读本机历史日志的那 7 条没跑(要跑：tools\test-r1999.ps1 -History)' -ForegroundColor Yellow
} elseif (Test-Path -LiteralPath $hist) {
    # 实例 id 是 M9A 在你这台机器上生成的短 hash，不能写死在测试里(这文件会进发布仓库)。
    # 从这份真实日志自己的 [inst=实例名/id] 标记里，按 settings 里"活动方案"配的实例名把它认出来。
    $wantName = [string]$S.re1999.planToInstance.event
    $histIds = @{}
    foreach ($l in (Get-Content -LiteralPath $hist -Encoding utf8)) {
        if ($l -match '\[inst=([^/\]]+)/([0-9A-Za-z]+)\]') { $histIds[$Matches[1]] = $Matches[2] }
    }
    $histId = [string]$histIds[$wantName]
    if (-not $histId) {
        Write-Host "  SKIP 这份日志里没有名为[$wantName]的实例(planToInstance 或实例名不同)，实例过滤那 4 条不跑" -ForegroundColor Yellow
    } else {
        $evs = @()
        foreach ($l in (Get-Content -LiteralPath $hist -Encoding utf8)) {
            $e = Read-GdM9ALine -Line $l -InstanceId $histId
            if ($e) { $evs += $e }
        }
        $taskNames = @($evs | Where-Object { $_.Kind -eq 'task' } | ForEach-Object { $_.Text })
        Check '历史日志能解出任务序列' ($taskNames.Count -ge 8) "$($taskNames.Count) 个: $($taskNames -join '>')"
        # 08-30 那次 日常-活动 是"任务运行失败 + 停止前状态：FAILED"收尾的，不该有 chain-done
        Check '失败收尾不会被当成链路完成' (@($evs | Where-Object { $_.Kind -eq 'chain-done' }).Count -eq 0)
        Check '能识别失败行' (@($evs | Where-Object { $_.Kind -eq 'fail' }).Count -ge 1)
        Check '最后一项是关闭游戏' ($taskNames[-1] -match '关闭游戏') $taskNames[-1]
    }
    $allEvs = @()
    foreach ($l in (Get-Content -LiteralPath $hist -Encoding utf8)) {
        $e = Read-GdM9ALine -Line $l          # 不按实例过滤
        if ($e) { $allEvs += $e }
    }
    Check '全文件能认出链路完成(长草那次)' (@($allEvs | Where-Object { $_.Kind -eq 'chain-done' }).Count -ge 1)
    Check '连接阶段不会被当成链路开始' (@($allEvs | Where-Object { $_.Kind -eq 'connect-start' }).Count -ge 1)
    $other = @()
    foreach ($l in (Get-Content -LiteralPath $hist -Encoding utf8)) {
        $e = Read-GdM9ALine -Line $l -InstanceId 'ffffffff'   # 不存在的实例 id 应该全被过滤
        if ($e -and $e.Kind -in @('task', 'chain-done', 'fail')) { $other += $e }
    }
    Check '别的实例日志行会被过滤' ($other.Count -eq 0) "$($other.Count) 条漏进来"
} else {
    Write-Host "  SKIP  没有历史日志可回放($hist)" -ForegroundColor Yellow
}

Write-Host "`n[t7] 活动商店领空 gate(1999：假数据 + 临时 state，跑完还原)"
Save-GdTestState -Names @('r1999-event-ack', 'r1999-shop-done')
$nowMs = Get-GdNowMs
try {
    # 1999：同一套逻辑作用在版本-活动键上
    $rData = New-Object psobject
    $rData | Add-Member -NotePropertyName '9.9' -NotePropertyValue (New-Object psobject -Property @{
            version_name = '测试版本'
            start_time   = $nowMs - 86400000
            end_time     = $nowMs + 864000000
            activity     = (New-Object psobject -Property @{ combat = (New-Object psobject -Property @{ start_time = $nowMs - 86400000; end_time = $nowMs + 864000000; event_type = 'MainStory' }) })
        })
    $x1 = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
    Check '1999 主线未确认 → 长草+pending' ($x1.Mode -eq 'event-pending' -and $x1.Instance -eq 'idle') $x1.Reason
    # 提示语是给用户照抄的：单游戏入口只有 -Ack / -ShopReset，写死 -Ack1999 会让人敲一个不存在的参数
    Check '未确认的提示语用短参数名 -Ack(不是 -Ack1999)' (($x1.Reason -match 'daily-clear\.ps1 -Ack(?!1999)') -and ($x1.Reason -notmatch 'Ack1999')) $x1.Reason
    Set-GdEntryVerb -Game r1999 -Verb Ack -Name '-Ack1999'
    try {
        $x1b = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
        Check '入口覆盖参数名后提示语跟着改(合体工程就是这么跑的)' ($x1b.Reason -match 'daily-clear\.ps1 -Ack1999') $x1b.Reason
    } finally { Set-GdEntryVerb -Game r1999 -Verb Ack -Name '-Ack' }
    Set-GdAckKey -Game 'r1999' -Key '9.9-combat'
    $x2 = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
    Check '1999 确认后 → 活动方案' ($x2.Mode -eq 'event' -and $x2.Instance -eq 'event')
    Set-GdShopDoneKey -Game 'r1999' -Key '9.9-combat'
    $x3 = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
    Check '1999 商店领空 → 长草实例' ($x3.Mode -eq 'idle' -and $x3.Instance -eq 'idle' -and $x3.ShopDone) $x3.Reason
    Check '领空回落的提示语用短参数名 -ShopReset' (($x3.Reason -match 'daily-clear\.ps1 -ShopReset(?!\w)') -and ($x3.Reason -notmatch 'RShop')) $x3.Reason
    Check '1999 领空后仍可撤销' ((Resolve-R1999Plan -S $S -Data $rData -Now $nowMs -IgnoreShopDone).Mode -eq 'event')
    Check '1999 撤销标记生效' ((Remove-GdShopDoneKey -Game 'r1999' -Key '9.9-combat') -and (Resolve-R1999Plan -S $S -Data $rData -Now $nowMs).Mode -eq 'event')
    Check '1999 重复撤销返回 false' ((Remove-GdShopDoneKey -Game 'r1999' -Key '9.9-combat') -eq $false)
} finally {
    Restore-GdTestState
}
Check 'state 文件已还原' (Test-GdTestStateRestored)

Write-Host "`n[t10] M9A 日志事件解析(用真实日志行)"
$P_LONG = '[2026-08-30 12:18:45.984][INF] [cfg=Default][inst=日常-活动/cafebabe][src=Worker][op=ExecuteTaskQueue] >>> 任务 启动任务开始（异步）。'
Check '启动任务开始 只算连接阶段' ((Read-GdM9ALine -Line $P_LONG).Kind -eq 'connect-start')
Check '队列任务开始 才算链路开始' ((Read-GdM9ALine -Line ($P_LONG -replace '启动任务开始', '队列任务开始')).Kind -eq 'chain-start')
Check '任务已全部完成 算链路完成' ((Read-GdM9ALine -Line ($P_LONG -replace '启动任务开始（异步）。', '任务已全部完成！')).Kind -eq 'chain-done')
Check '启动任务完成 只给中性 finish(含义靠上下文定)' ((Read-GdM9ALine -Line ($P_LONG -replace '启动任务开始', '启动任务完成')).Kind -eq 'finish')
Check '单任务开始带中文名' ((Read-GdM9ALine -Line '[..][cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 开始任务：收取荒原').Text -eq '收取荒原')
Check '别的实例的日志行被忽略' ($null -eq (Read-GdM9ALine -Line $P_LONG -InstanceId 'deadbeef'))
Check '同实例的日志行被采纳' ((Read-GdM9ALine -Line $P_LONG -InstanceId 'cafebabe').Kind -eq 'connect-start')
Check '无 inst 标记的行不受过滤影响' ((Read-GdM9ALine -Line '[..][cfg=Default] 任务已全部完成！' -InstanceId 'zzz').Kind -eq 'chain-done')
Check '失败行识别' ((Read-GdM9ALine -Line '[..][inst=x/deadbeef][src=Worker][op=StopTask] 停止前状态：FAILED').Kind -eq 'fail')
Check '资源更新提示算噪音(会续命但不算进度)' ((Read-GdM9ALine -Line '[..][inst=x/deadbeef][src=Agent][op=StdErr] 如前面有提示新资源版本还请更新').Kind -eq 'noise')
Check '无关行返回 null' ($null -eq (Read-GdM9ALine -Line '[..][cfg=Default] [内存管理]释放了11.97 MB（21.1%)'))
Check 'settings 里 1999 起跑超时已收紧' ([int]$S.runtime.r1999StartTimeoutMinutes -le 12) "=$($S.runtime.r1999StartTimeoutMinutes) 分钟"

Show-GdCommonTailChecks

Write-Host "`n[t12] M9A 命令行脚本化入口 + 结果判定"
$inst12 = [pscustomobject]@{ Id = 'deadbeef'; Name = '日常-长草' }
$a1 = @(Get-M9ALaunchArgs -Instance $inst12 -Mode 'id')
Check '命令行入口用长参数(--instance/--autostart/…)' (($a1 -join ' ') -eq '--instance deadbeef --autostart --quit-after-run --force-start') ($a1 -join ' ')
Check '绝不使用短参数(-a/-i/-q/-f 实测不可靠)' (-not ($a1 -contains '-a') -and -not ($a1 -contains '-i') -and -not ($a1 -contains '-q') -and -not ($a1 -contains '-f'))
$a2 = @(Get-M9ALaunchArgs -Instance $inst12 -Mode 'name')
Check '--instance 也能按实例中文名传' (($a2 -join ' ') -eq '--instance 日常-长草 --autostart --quit-after-run --force-start') ($a2 -join ' ')
$a3 = @(Get-M9ALaunchArgs -Instance $inst12 -Mode 'id' -ForceStart $false)
Check '关掉 force-start 时只留三项' (($a3 -join ' ') -eq '--instance deadbeef --autostart --quit-after-run') ($a3 -join ' ')
$a4 = @(Get-M9ALaunchArgs -Instance $inst12 -CliArgs $false)
Check '关掉命令行入口时不加任何参数(回退老路子)' ($a4.Count -eq 0)
Check 'settings 里三个 M9A 开关都在' ($null -ne $S.runtime.m9aCliArgs -and $null -ne $S.runtime.m9aInstanceArg -and $null -ne $S.runtime.m9aForceStart)
Check '默认走命令行入口' ((Get-GdM9ACliEnabled -S $S) -eq $true)

# 真实日志行(09-23 17:30 用 --autostart 成功提交队列那一段)
Check '能识别"用户操作：启动任务"' ((Read-GdM9ALine -Line '[2026-09-23 17:30:40.048][INF] [cfg=Default][inst=日常-长草/deadbeef][src=UI][op=StartTask] 用户操作：启动任务 | controller=Adb, resource=官服, device=<none>, selectedTasks=10').Kind -eq 'ui-start')
Check '能读出战它提交了几个任务' ((Read-GdM9ALine -Line '[..][inst=日常-长草/deadbeef][src=Worker][op=ExecuteTaskQueue] 准备执行任务队列：任务数量=10').Text -eq '10')

function New-M9AState {
    return @{ tasks = @(); chainStarted = $false; chainDone = $false; connectPhase = 'none'; failed = $null;
              lastLog = (Get-Date); queueCount = $null; startRequested = $false; eventCount = 0;
              failedTasks = @(); resCurrent = $null; resLatest = $null; connectFail = $null }
}
$instS12 = [pscustomobject]@{ Id = 'deadbeef'; Name = '日常-长草' }

$sA = New-M9AState
Register-M9AEvent -State $sA -Event @{ Kind = 'connect-start'; Text = '' } | Out-Null
Register-M9AEvent -State $sA -Event @{ Kind = 'finish'; Text = '' }
Register-M9AEvent -State $sA -Event @{ Kind = 'chain-done'; Text = '' }
$rA = Resolve-M9AStatus -State $sA -Elapsed 5 -S $S -Instance $instS12 -LogName 'log-test.log'
Check '连接完就报全部完成、零任务 → empty-queue(不再白等 10 分钟)' ($rA.Status -eq 'empty-queue') "$($rA.Status)"
Check 'empty-queue 会点名短参数 -a 无效这个坑' ($rA.Detail -match '--autostart 没生效') $rA.Detail
$sA0 = New-M9AState
Register-M9AEvent -State $sA0 -Event @{ Kind = 'connect-start'; Text = '' }
Register-M9AEvent -State $sA0 -Event @{ Kind = 'finish'; Text = '' }
Check '「启动任务完成」在没有任务时只算连接完成' ($sA0.connectPhase -eq 'done' -and -not $sA0.chainDone) "chainDone=$($sA0.chainDone)"
$sA2 = New-M9AState
Register-M9AEvent -State $sA2 -Event @{ Kind = 'queue-prepared'; Text = '0' }
Register-M9AEvent -State $sA2 -Event @{ Kind = 'chain-done'; Text = '' }
$rA2 = Resolve-M9AStatus -State $sA2 -Elapsed 5 -S $S -Instance $instS12
Check '任务数量=0 时 empty-queue 会引用 M9A 自己报的数' ($rA2.Detail -match '任务数量=0') $rA2.Detail

$sB = New-M9AState
Register-M9AEvent -State $sB -Event @{ Kind = 'chain-start'; Text = '' }
Register-M9AEvent -State $sB -Event @{ Kind = 'task'; Text = '启动游戏' }
Register-M9AEvent -State $sB -Event @{ Kind = 'chain-done'; Text = '' }
$rB = Resolve-M9AStatus -State $sB -Elapsed 40 -S $S -Instance $instS12 -ProcessGone $true
Check '跑过任务且完成 → ok(进程已因 --quit-after-run 退出也算成功)' ($rB.Status -eq 'ok') "$($rB.Status)"
Check 'ok 里带任务数' ($rB.Detail -match '1 个任务') $rB.Detail

$sC = New-M9AState
Register-M9AEvent -State $sC -Event @{ Kind = 'finish'; Text = '' }
$rC = Resolve-M9AStatus -State $sC -Elapsed 700 -S $S -Instance $instS12 -LogName 'log-test.log'
Check '只连上不跑队列 → not-started' ($rC.Status -eq 'not-started') "$($rC.Status)"
Check 'not-started 会点名命令行 --autostart' ($rC.Detail -match '--autostart')

# 真跑复盘(09-23 17:36→17:43 那次实际成功)：--quit-after-run 下 M9A 只打「启动任务完成」就退出，
# 根本不会有「任务已全部完成！」这一行 → 之前被判成 error，白重试一次
$sF = New-M9AState
Register-M9AEvent -State $sF -Event @{ Kind = 'queue-prepared'; Text = '10' }
foreach ($n in 1..10) { Register-M9AEvent -State $sF -Event @{ Kind = 'task'; Text = "任务$n" } }
Register-M9AEvent -State $sF -Event @{ Kind = 'finish'; Text = '' }
Check '跑满队列声明的任务数 + 见到「启动任务完成」→ chainDone' $sF.chainDone
$rF = Resolve-M9AStatus -State $sF -Elapsed 430 -S $S -Instance $instS12 -ProcessGone $true
Check '真实成功形态：10/10 任务后进程退出 → ok(不再误判 error)' ($rF.Status -eq 'ok') "$($rF.Status)｜$($rF.Detail)"
$sG = New-M9AState
Register-M9AEvent -State $sG -Event @{ Kind = 'queue-prepared'; Text = '10' }
foreach ($n in 1..9) { Register-M9AEvent -State $sG -Event @{ Kind = 'task'; Text = "任务$n" } }
$rG = Resolve-M9AStatus -State $sG -Elapsed 430 -S $S -Instance $instS12 -ProcessGone $true -LogName 'log-test.log'
Check '只差最后一个任务就退出 → 仍判 error(不放过真中断)' ($rG.Status -eq 'error') "$($rG.Status)｜$($rG.Detail)"
$sH = New-M9AState
Register-M9AEvent -State $sH -Event @{ Kind = 'finish'; Text = '' }
$rH = Resolve-M9AStatus -State $sH -Elapsed 30 -S $S -Instance $instS12 -ProcessGone $true -LogName 'log-test.log'
Check '连接完就退出且零任务 → not-started(不烧 10 分钟)' ($rH.Status -eq 'not-started') "$($rH.Status)｜$($rH.Detail)"

Write-Host "`n[t13] 真实日志逐行复盘(09-23 17:36 那一轮，从提交队列到进程退出)"
# 这几行是从 M9A 自己 logs\log-20260923.log 里原样摘出来的
$raw13 = @(
    '[2026-09-23 17:36:22.736][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Worker][op=ExecuteTaskQueue] 准备执行任务队列：任务数量=10',
    '[2026-09-23 17:36:22.770][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Worker][op=ExecuteTaskQueue] >>> 任务 启动任务开始（异步）。',
    '[2026-09-23 17:36:25.126][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 开始任务：启动游戏',
    '[2026-09-23 17:43:18.707][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 开始任务：使用兑换码',
    '[2026-09-23 17:43:33.122][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 开始任务：关闭游戏',
    '[2026-09-23 17:43:33.977][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Worker][op=ExecuteTaskQueue] >>> 任务 启动任务完成（异步）。',
    '[2026-09-23 17:43:33.978][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Worker][op=StopTask] 停止前状态：SUCCEEDED',
    '[2026-09-23 17:43:34.286][INF] [cfg=Default] 应用已关闭'
)
$sI = New-M9AState
foreach ($l in $raw13) {
    $ev = Read-GdM9ALine -Line $l -InstanceId 'deadbeef'
    if ($ev) { Register-M9AEvent -State $sI -Event $ev }
}
Check '队列声明被读到' ($sI.queueCount -eq 10) "queueCount=$($sI.queueCount)"
Check 'SUCCEEDED 不会被当成失败' ($null -eq $sI.failed) "failed=$($sI.failed)"
Check '「启动任务完成」在跑过任务之后就是链路完成' $sI.chainDone
$rI = Resolve-M9AStatus -State $sI -Elapsed 445 -S $S -Instance $instS12 -ProcessGone $true -LogName 'log-20260923.log'
Check '这一轮真实判 ok(曾经误判 error 导致白重试一次)' ($rI.Status -eq 'ok') "$($rI.Status)｜$($rI.Detail)"

$sD = New-M9AState
Register-M9AEvent -State $sD -Event @{ Kind = 'task'; Text = '常规作战' }
Register-M9AEvent -State $sD -Event @{ Kind = 'fail'; Text = '任务运行失败！' }
$rD = Resolve-M9AStatus -State $sD -Elapsed 300 -S $S -Instance $instS12 -ProcessGone $true
Check '进程消失且有失败 → error 并带上失败原因' ($rD.Status -eq 'error' -and $rD.Detail -match '任务运行失败') "$($rD.Status)"
$rE = Resolve-M9AStatus -State (New-M9AState) -Elapsed 60 -S $S -Instance $instS12
Check '还在正常推进时不返回结论' ($null -eq $rE)

Write-Host "`n[t14] 日志标志失配 → unknown-format(不套 not-started/stalled 的猜测)"
$sJ = New-M9AState
$rJ = Resolve-M9AStatus -State $sJ -Elapsed 30 -S $S -Instance $instS12 -ProcessGone $true -LogName 'log-test.log'
Check '进程退出且一条标志都没读到 → unknown-format' ($rJ.Status -eq 'unknown-format') "$($rJ.Status)｜$($rJ.Detail)"
$sK = New-M9AState
$rK = Resolve-M9AStatus -State $sK -Elapsed 700 -S $S -Instance $instS12 -LogName 'log-test.log'
Check '进程还在但过了「该开始」时限仍零标志 → unknown-format' ($rK.Status -eq 'unknown-format') "$($rK.Status)"
Check 'unknown-format 会点名日志格式/实例 id 两种可能' (($rK.Detail -match '日志文案') -and ($rK.Detail -match '实例')) $rK.Detail
Check 'unknown-format 给出可执行的排查入口(replay/probe 工具)' (($rK.Detail -match 'replay-m9a-log') -and ($rK.Detail -match 'probe-m9a-history'))
Check '零标志时不谎称"--autostart 没生效"' (-not ($rK.Detail -match '--autostart 没生效')) $rK.Detail
$sCnt = New-M9AState
Register-M9AEvent -State $sCnt -Event @{ Kind = 'noise'; Text = '' }
Register-M9AEvent -State $sCnt -Event @{ Kind = 'task'; Text = 'x' }
Check '事件计数会被 Register-M9AEvent 累加' ([int]$sCnt.eventCount -eq 2) "eventCount=$($sCnt.eventCount)"
$sL = New-M9AState
Register-M9AEvent -State $sL -Event @{ Kind = 'queue-prepared'; Text = '10' }
foreach ($n in 1..3) { Register-M9AEvent -State $sL -Event @{ Kind = 'task'; Text = "任务$n" } }
$rL = Resolve-M9AStatus -State $sL -Elapsed 5200 -S $S -Instance $instS12
Check '读到过标志时仍按 timeout 判(不被 unknown-format 抢走)' ($rL.Status -eq 'timeout') "$($rL.Status)"
$unparsed = Read-GdM9ALine -Line '[2026-xx][INF] [inst=日常-长草/deadbeef] 任务链已全部执行 SUCCESS' -InstanceId 'deadbeef'
Check '新版文案不认识时 Read-GdM9ALine 返回 $null(而不是硬凑一个事件)' ($null -eq $unparsed) "$($unparsed.Kind)"

# 重试策略跟着"跑一轮"一起搬进了 lib\r1999-daily.ps1，入口脚本只剩参数分发。
# 断言对象随之换文件，断言内容(那条跳过清单)一个字没改。
$r1999Daily = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\r1999-daily.ps1') -Raw -Encoding utf8
Check '1999 重试会跳过 not-started/empty-queue/unknown-format/blocked' ($r1999Daily -match "Status -notin @\('ok', 'not-started', 'empty-queue', 'unknown-format', 'blocked'\)")

Write-Host "`n[t17] 单任务识别失败 / 版本落后 / 连不上模拟器 —— 09-25 19:24 那一轮的复盘"
<#
  那一轮的真实形态(证据：M9A logs\log-20260925.log 1336~1766 行 + 我们自己的
  logs\run-20260925-191135-daily-manual.log 第 36~51 行)：
    模拟器连上了、截图 11ms 正常，但客户端正在下 1.91G 的 Reverse1999_sub1009 更新包，
    界面停在安卓下拉通知栏上 → 「启动游戏」失败，之后每个任务约 22 秒各失败一次，共 8 个。
  我们当时一路报「M9A 进行中[1..8]」，最后只给出「进程提前退出」，然后原样重试一遍又白烧 5 分钟。
  根因是解析器不认「任务失败：<中文名>」这一行(只认「任务运行失败」，差一个字)，
  也不认 M9A 自己喊的版本差(「最新资源版本」还被 noise 那条按子串吞了)。
#>
# 下面这批日志行是从 09-25 19:24 那一轮原样抄来的，只有 [inst=名字/id] 里的 id 换成了合成值：
# 真实实例 id 属于"本机专属值"，写进来会被 lib\common.ps1 的 Test-GdExportLeaks 拦下(它扫的就是
# M9A 实例目录下每个 .json 的文件名)：现在 tools\test-modules.ps1 [m8] 扫源码时就先红一次，
# 不用等导出那一步才发现。导出的 r1999 仓库发不出去是晚了两轮的代价。解析器只按"传进来的 id 和行内 id 对不对得上"过滤，
# 用哪个 id 都不影响断言的意义，所以这里固定用 deadbeef(本文件上面那组断言用的也是它)。
$raw17 = @(
    '[2026-09-25 19:24:21.177][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] [Record] 当前资源版本：v4.7.1',
    '[2026-09-25 19:24:21.185][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] [Record] 最新资源版本：v4.10.0',
    '[2026-09-25 19:24:23.165][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Worker][op=ExecuteTaskQueue] 准备执行任务队列：任务数量=10',
    '[2026-09-25 19:24:25.126][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 开始任务：启动游戏',
    '[2026-09-25 19:26:27.316][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 任务失败：启动游戏',
    '[2026-09-25 19:26:45.511][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 开始任务：收取荒原',
    '[2026-09-25 19:26:49.543][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Monitor][op=MonitorLog] 任务失败：收取荒原',
    '[2026-09-25 19:28:50.180][INF] [cfg=Default][inst=日常-长草/deadbeef][src=Worker][op=StopTask] 停止前状态：NOT_STARTED'
)
Check '「任务失败：<中文名>」认得，且带出任务名' ((Read-GdM9ALine -Line $raw17[4] -InstanceId 'deadbeef').Kind -eq 'task-fail')
Check '「当前资源版本」认得' ((Read-GdM9ALine -Line $raw17[0] -InstanceId 'deadbeef').Kind -eq 'res-current')
Check '「最新资源版本」不再被 noise 按子串吞掉' ((Read-GdM9ALine -Line $raw17[1] -InstanceId 'deadbeef').Kind -eq 'res-latest')
Check '「初始化控制器失败」算连接失败' ((Read-GdM9ALine -Line '[..][inst=日常-长草/deadbeef][src=Worker][op=ExecuteTaskQueue] 初始化控制器失败：message=连接模拟器时发生错误！' -InstanceId 'deadbeef').Kind -eq 'connect-fail')
Check '连接失败只留原因，不带时间戳/字段头' ((Read-GdM9ALine -Line '[2026-09-25 19:29:33.726][INF] [cfg=Default] 按指纹匹配到的设备数量：0' -InstanceId 'deadbeef').Text -eq '按指纹匹配到的设备数量：0')
Check '旧的"新资源版本还请更新"提示仍算噪音(不抢 res-latest)' ((Read-GdM9ALine -Line '[..][inst=x/deadbeef][src=Agent][op=StdErr] 如前面有提示新资源版本还请更新' -InstanceId 'deadbeef').Kind -eq 'noise')

$s17 = New-M9AState
foreach ($l in $raw17) {
    $ev = Read-GdM9ALine -Line $l -InstanceId 'deadbeef'
    if ($ev) { Register-M9AEvent -State $s17 -Event $ev }
}
Check '两个任务失败都累计进来了' (@($s17.failedTasks).Count -eq 2) "failedTasks=$($s17.failedTasks -join ',')"
Check '读到了版本差' ($s17.resCurrent -eq 'v4.7.1' -and $s17.resLatest -eq 'v4.10.0') "$($s17.resCurrent)/$($s17.resLatest)"
$r17 = Resolve-M9AStatus -State $s17 -Elapsed 280 -S $S -Instance $instS12 -ProcessGone $true -LogName 'log-20260925.log'
Check '第一个任务就失败 → blocked(不再报成"进程提前退出"这种没指向的 error)' ($r17.Status -eq 'blocked') "$($r17.Status)｜$($r17.Detail)"
Check 'blocked 点名是第一个任务失败、后面全是空转' ($r17.Detail -match '第一个任务\[启动游戏\]') $r17.Detail
Check 'blocked 把版本差说成可行动的整包更新' (($r17.Detail -match 'v4\.10\.0') -and ($r17.Detail -match '整包更新')) $r17.Detail
Check 'blocked 带上失败任务清单' ($r17.Detail -match '收取荒原') $r17.Detail
Check 'blocked 不是 ok：这一轮不能算清完体力' ($r17.Status -ne 'ok')

# 只有中间某个任务失败(实测「使用兑换码」没码可换就这样)→ 不许整轮判红
$s17b = New-M9AState
foreach ($e in @(@{Kind='queue-prepared';Text='10'}, @{Kind='task';Text='启动游戏'}, @{Kind='task';Text='使用兑换码'},
                 @{Kind='task-fail';Text='使用兑换码'}, @{Kind='chain-done';Text=''})) { Register-M9AEvent -State $s17b -Event $e }
$r17b = Resolve-M9AStatus -State $s17b -Elapsed 300 -S $S -Instance $instS12 -ProcessGone $true
Check '中间任务单独失败 → 仍判 ok，但把失败任务写进告警' (($r17b.Status -eq 'ok') -and ($r17b.Detail -match '1 个任务报过')) "$($r17b.Status)｜$($r17b.Detail)"

# 19:29 那一轮：声明了 10 个任务却一个没开始，真因是连不上模拟器 —— 旧文案说成"队列为空"
$s17c = New-M9AState
foreach ($e in @(@{Kind='queue-prepared';Text='10'}, @{Kind='connect-fail';Text='初始化控制器失败：AdbSerial 为空'}, @{Kind='chain-done';Text=''})) { Register-M9AEvent -State $s17c -Event $e }
$r17c = Resolve-M9AStatus -State $s17c -Elapsed 30 -S $S -Instance $instS12 -LogName 'log-20260925.log'
Check '连不上模拟器时 empty-queue 说清是连接失败' ($r17c.Detail -match '连模拟器这一步就失败') "$($r17c.Status)｜$($r17c.Detail)"
Check '声明了 10 个任务时不再谎称"队列为空"' (-not ($r17c.Detail -match '队列为空')) $r17c.Detail
Check '连接失败会把 adb/虚拟机指向出来' ($r17c.Detail -match 'MuMu') $r17c.Detail

# 老状态包没有新键(比如 replay 工具自己拼的)不许把 @($null) 数成一个失败
$s17d = @{ tasks = @(); chainStarted = $false; chainDone = $false; connectPhase = 'none'; failed = $null;
           lastLog = (Get-Date); queueCount = $null; startRequested = $false; eventCount = 0 }
Register-M9AEvent -State $s17d -Event @{ Kind = 'task-fail'; Text = '启动游戏' }
Check '状态包缺 failedTasks 键时不会把 $null 数成一个失败' (@($s17d.failedTasks).Count -eq 1) "count=$(@($s17d.failedTasks).Count)"
$r17d = Resolve-M9AStatus -State $s17d -Elapsed 60 -S $S -Instance $instS12 -ProcessGone $true
Check '零任务+只有失败行 → 不报"开始过的 0 个任务全部失败"这种话' (-not ($r17d.Detail -match '0 个任务全部')) "$($r17d.Status)｜$($r17d.Detail)"

Write-Host "`n[t18] 1999 这一路也接上视觉兜底(2026-09-26 加强 AI 干预；全程不联网、不起 adb、不点任何东西)"
# 为什么 1999 也要接：M9A 和 MAA 得的是同一种病 —— 模板集里没有的界面它看不见。
# 2026-09-25 19:24 那一轮，1999 客户端在模拟器里下 1.91G 更新包，10 个任务里 8 个「任务失败」，
# 我们的日志只会说"进程提前退出"，而画面上一眼就能看出是"停在更新界面"。
$mumuLib = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\vision-mumu.ps1') -Raw -Encoding utf8
$m9aLib = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\m9a.ps1') -Raw -Encoding utf8
$dailyLib = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\r1999-daily.ps1') -Raw -Encoding utf8
if (Get-Command New-GdVisionTarget-mumu -ErrorAction SilentlyContinue) {
    # 契约：内核(lib\vision-rescue.ps1)只按字段名调它，少一个字段就是"兜底静默不生效"，所以逐字段点名
    $tm = New-GdVisionTarget-mumu -S $S
    $need = @('Kind', 'Name', 'Enabled', 'Why', 'NeedsAdmin', 'AdminWhy', 'Scene', 'UserText', 'Shot', 'Click', 'Alive', 'Grab')
    $missing = @($need | Where-Object { $tm.PSObject.Properties.Name -notcontains $_ })
    Check '模拟器靶子把契约要求的 12 个字段都给齐(内核只按名字调它)' ($missing.Count -eq 0) $(
        if ($missing.Count) { "缺：$($missing -join ',')" } else { '齐' })
    Check '靶子自称 mumu(内核按 New-GdVisionTarget-<kind> 找它，名字错了就找不到)' ($tm.Kind -eq 'mumu') $tm.Kind
    Check '模拟器这一路**不要**管理员(点的是虚拟机，宿主机提不提权与它无关)' (
        -not $tm.NeedsAdmin -and -not $tm.AdminWhy) "NeedsAdmin=$($tm.NeedsAdmin)"
    Check '没有"抢前台"这一步(点击进的是虚拟机，碰不到用户正在用的窗口，所以 Grab 给空)' ($null -eq $tm.Grab)
    Check '提示词开头换成 1999/模拟器那套文案(不沿用端游那句，否则模型会去找端游的弹窗)' (
        $tm.Scene -match '1999' -and $tm.Scene -match 'MuMu|模拟器' -and $tm.Scene -notmatch '端游')
    Check '靶子自己带 Shot/Click/Alive 三个动作(内核不认识 adb)' (
        $tm.Shot -is [scriptblock] -and $tm.Click -is [scriptblock] -and $tm.Alive -is [scriptblock])

    # 开关关掉必须"一次都不试"，而且那句原因要带 settings 里的键名 —— 玩家对着日志才知道改哪儿
    $rtOff = [pscustomobject](Convert-GdMapToHashtable $S.runtime)
    Set-GdMapValue -Map $rtOff -Key 'mumuVisionRescue' -Value $false | Out-Null
    $tmOff = New-GdVisionTarget-mumu -S ([pscustomobject]@{ runtime = $rtOff; paths = $S.paths })
    Check 'mumuVisionRescue=false → 这一路关闭且原因里带键名' (-not $tmOff.Enabled -and $tmOff.Why -match 'mumuVisionRescue') $tmOff.Why
    $rtArk = [pscustomobject](Convert-GdMapToHashtable $S.runtime)
    Set-GdMapValue -Map $rtArk -Key 'arkVisionRescue' -Value $false | Out-Null
    $tmOther = New-GdVisionTarget-mumu -S ([pscustomobject]@{ runtime = $rtArk; paths = $S.paths })
    Check '端游那个开关管不着模拟器(arkVisionRescue=false 时模拟器靶子照常开着)' $tmOther.Enabled

    # adb 端点三级来源：① 实例配置里的 AdbDevice ② MuMuManager adb -v ③ adb devices 里恰好一台
    # 为什么不能写死端口：16384/7555 换个 MuMu 版本或实例编号就错，而且本机专属值不许进源码(test-modules 的 m8)。
    # "注释里写着不该有的东西"和"代码里真的那么干了"是两回事：Get-Command 的 Definition 连着
# 注释一起给，所以"不许写死端口/不许走 exec-out"这类**否定式**断言必须先把注释剥掉再判 ——
# 否则文件里多写一句解释就把自检弄红(2026-09-26 第一次跑就是这两条假红)。
function Get-GdTestCodeOnly {
    param([string]$Text)
    return (($Text -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' ')
}
$fnEp = Get-GdTestCodeOnly (Get-Command Get-GdMuMuAdbEndpoint).Definition
    Check 'adb 端点第一选择是调用方给的 Conn(那一路真要用的设备)' ($fnEp -match '\$Conn\.Adb' -and $fnEp -match '\$Conn\.Serial')
    Check '第二选择问 MuMuManager 的 adb -v，且只抠 ip:port 不认字段名(输出形态各家版本不一)' (
        $fnEp -match "'adb', '-v'" -and $fnEp -match 'd\{1,3\}')
    Check '第三选择 adb devices，但**恰好一台**才认(挑错设备等于往别人的模拟器里点)' ($fnEp -match '\$ons\.Count -eq 1')
    Check '源码里不写死 adb 端口(换了 MuMu 版本/实例编号就错)' ($fnEp -notmatch '16384|7555|127\.0\.0\.1:\d')
    Check '拿不到 adb 时先说"跑 setup.ps1"，不拿相对路径硬跑' ($fnEp -match 'setup\.ps1')
    Check '问 MuMuManager 之前先探路 Get-Command(这份库单独被拿去用时没有 mumu.ps1 也能跑)' (
        $fnEp -match 'Get-Command Get-GdMuMu')
    $fnCap = (Get-Command Save-GdMuMuShot).Definition
    Check '拍屏走"设备里落一张 + pull"，不走 exec-out 接 stdout(文本管道会把 PNG 编码弄废)' (
        $fnCap -match 'screencap' -and $fnCap -match "'pull'" -and (Get-GdTestCodeOnly $mumuLib) -notmatch 'exec-out')
    Check '模拟器后端里一个端游的字眼都没有(两路后端互不认识，导出才能各带一份)' (
        (Get-GdTestCodeOnly $mumuLib) -notmatch 'arknightsExe|PrintWindow|Send-GdArkClick|Get-GdArkWindow')
    # 反过来也判：内核既然两路共用，就不许认识任何一端的专有东西(否则 1999 仓库带上它就等于拖着端游的假设跑)
    $rescueLib = Get-GdTestCodeOnly (Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\vision-rescue.ps1') -Raw -Encoding utf8)
    Check '兜底内核不认识 adb，也不认识端游窗口(截图/点击一律向靶子要)' (
        $rescueLib -notmatch 'adb |input tap|screencap|PrintWindow|Send-GdArkClick|arknightsExe')
    Check '内核按 New-GdVisionTarget-<kind> 的命名约定找后端，找不到就说"这棵树上没有这一路"' (
        $rescueLib -match 'New-GdVisionTarget-' -and $rescueLib -match '这棵树上没有')
    Check '模拟器截屏把客户区报成整块屏(通用的千分比换算因此直接给出设备像素)' (
        $fnCap -match 'ClientLeft = 0' -and $fnCap -match 'ClientW = \$w')
    $fnTap = (Get-Command Send-GdMuMuTap).Definition
    Check '点击出口与端游那一路同形状(@{Mode;Sent})，判定层因此不需要认识 adb' (
        $fnTap -match 'Mode = ' -and $fnTap -match 'Sent = ')

    # 接线：这一路此前**压根没有**兜底调用点(后端是死代码)。现在按"进程还在不在"分两类接上。
    Check 'm9a.ps1 挂了兜底调用点，且 -Target mumu、带上实例的 adb 通道' (
        $m9aLib -match "Invoke-GdVisionRescue .*-Target 'mumu'" -and $m9aLib -match '-Conn \$conn')
    Check '兜底用 Get-Command 探路(这份库单独被拿去用时没这函数也只是不兜)' (
        $m9aLib -match 'Get-Command Invoke-GdVisionRescue')
    Check '预算装在哈希表里、只用成员赋值扣(& 调用跑在子作用域，$x-- 扣不动)' (
        $m9aLib -match '\$rescue\.Left = \[int\]\$rescue\.Left - 1' -and $m9aLib -notmatch '\$rescue\.Left--')
    Check '次数上限沿用同一个键 runtime.visionMaxRescue(限的是"一轮里跟模型没完没了的次数")' (
        $m9aLib -match '\[int\]\$S\.runtime\.visionMaxRescue')
    # (这里用 -like 而不是 -match：双引号里的 `\$res` 会被 PowerShell 当成变量展开，正则反而什么都匹配不上 —— 假红过一次)
    Check '停滞/超时(进程还在)兜成了就撤回调款、计时从头算继续等' (
        ($m9aLib -like "*res.Status -in @('stalled', 'timeout')*") -and $m9aLib -match '\$state\.lastLog = Get-Date')
    Check 'blocked/not-started/empty-queue 也问一次(把猜测换成"画面上确实是 XX")' (
        $m9aLib -match "blocked', 'not-started', 'empty-queue" -and $m9aLib -match "'判因'")
    Check '兜清楚的证据是唯一能翻掉"重试没意义"的东西 → 打 $res.Rescued 标记' (
        $m9aLib -match '\$res\.Rescued = \$true')
    Check '这一路的救火账由 m9a.ps1 记(-Game r1999 / -Target mumu)' (
        $m9aLib -match "Add-GdRescueRecord .*-Game 'r1999'")
    Check '兜底次数与最后一问拼进 Detail(判因链不许丢这条线)' (
        $m9aLib -match '视觉兜底 \$\(@\(\$rescue\.Notes\)\.Count\) 次')
    Check '状态名一律没改(收尾与通知按状态名分流，造新名字等于把判因链弄断)' (
        $m9aLib -notmatch "Status = 'rescued'" -and $m9aLib -notmatch "Status = 'vision")
    # 外层重试的分流：画面被兜清楚了，本来"重试也没用"的那几种才值得再跑一次
    Check 'r1999-daily 里 $retryable 认这个标记(not-started/empty-queue/blocked 只有兜清楚才重试)' (
        $dailyLib -match '\$retryable' -and $dailyLib -match '\$r\.Rescued -and \$r\.Status -in @\(''not-started''')
    Check '带标记时日志说得清是"兜清楚了才重试"(不是原来那句"关闭游戏后重试一次")' (
        $dailyLib -match '视觉兜底把挡路的界面点掉了')
} else {
    Check 'lib\vision-mumu.ps1 有 New-GdVisionTarget-mumu(1999 仓库少了它兜底就是死代码)' $false '没找到这个函数'
}

$res = Show-GdTestResult -Tag 'r1999'
if ($NoExit) { return $res }
exit $(if ($res.Fail) { 1 } else { 0 })
