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
              lastLog = (Get-Date); queueCount = $null; startRequested = $false; eventCount = 0 }
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
Check '1999 重试会跳过 not-started/empty-queue/unknown-format' ($r1999Daily -match "Status -notin @\('ok', 'not-started', 'empty-queue', 'unknown-format'\)")

$res = Show-GdTestResult -Tag 'r1999'
if ($NoExit) { return $res }
exit $(if ($res.Fail) { 1 } else { 0 })
