# tools/test-r1999.ps1 — 重返未来1999 那一路的纯逻辑自检(只读：不改配置、不启动任何程序)
# 用法: pwsh -NoProfile -File tools\test-r1999.ps1 [-History]
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
Show-GdCommonScreensaverChecks

# ---- B-3②-d 刀①(10-29)静态锚：Get-R1999Event 的取数半段"两臂都在"，钉在 m9aDir 那道现场门之前 ----
# ⚠ 刀⑰ 笔(c)-2(2026-11-07)改判：上面那句"两臂都在"按历史引文读——回落臂本批出仓(与刀⑯ 判死的
#   "回落臂假绿形"同形：try 把核缺席的 throw 吞成 null 再 PS 直读＝把核坏了化装成合法长草读数)，
#   下面第二格自此翻负(回潮即红)，核臂格与九键信封格保持；判据的家与语义见 docs\rust-core-plan.md。
# 为什么放这里：这一格钉的是源码形状，M9A 目录在不在都该有证词(只读自检不许要求机器现场状态)；
# 放在下面 [t4] 那道 return 之后，没装 M9A 的机器上它就静默不跑，等于把锚做成哑尺。
# 剥注释照下面 $fnEv 那两形(-replace 逐字复用；Get-GdTestCodeOnly 到本文件后半才定义、这里调不到。原指的那枚（模拟器端点的函数体变量）已随死臂 19 格由刀⑳ 笔(a)-1 摘掉，故改指在位的那两形)：本文件里 read-r1999-activity 这个词头注也写了，
# 不剥注释的"正向锚"会在真身被摘掉后照样绿——那是本仓登记过的"锚不是调用"形。
$fnEv = [string](Get-Command Get-R1999Event -ErrorAction SilentlyContinue).Definition
$fnEv = ($fnEv -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' '
Check '②-d 刀①核臂在：取数半段打 read-r1999-activity，且只喂 paths 直答(不喂第二份 settings)' (
    $fnEv -match "Invoke-GdCli -Verb 'read-r1999-activity'" -and $fnEv -match 'paths = ' -and ($fnEv -notmatch 'settings = ')) $(
    if ($fnEv.Length -eq 0) { 'Get-R1999Event 不在场(尺没接到电)' } else { "len=$($fnEv.Length)" })
Check '⑰(c)-2 改判负锚：回落臂三针(直读 json／New-TimeSpan 算龄／Test-GdPathReady 判路)都不在 Get-R1999Event 剥注释体里(回潮＝把核坏了化装成合法长草读数的假绿形复活)' (
    $fnEv -notmatch 'Read-GdJson' -and $fnEv -notmatch 'New-TimeSpan' -and $fnEv -notmatch 'Test-GdPathReady') $(
    'len=' + $fnEv.Length)
Check '九键 idle 信封全函数只构造一次(两臂汇流到 $idle；抄第二份就是双源，§11.52 ⑤)' (
    ([regex]::Matches($fnEv, "Mode = 'idle'")).Count -eq 1) $(
    'count=' + ([regex]::Matches($fnEv, "Mode = 'idle'")).Count)

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
    # 10-30 B-3②-d 刀①：body 摘掉 verbAck／verbShopReset 两枚键 ⇒ 判定层不再往提示语里拼开关名；旧那句"照抄就能跑"的分工自此归入口自己的帮助面。
    Check '未确认的提示语不再带 verb 名(反向钉：出现 daily-clear 即红)' (($x1.Reason -notmatch 'daily-clear') -and ($x1.Reason -match '本轮跑长草方案')) $x1.Reason
    $x1b = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
    # 11-01 丙形真转接(§11.176 ⑥)：现场覆盖件退役 ⇒ 这一格不再"先盖一枚长名再看它漏不漏"，改钉"现问到什么名都不许渗进提示语"。
    #   它不再量的是旧那格的"覆盖只影响被写的那一款"——写侧没了，那一形从此不可能出现；跨款隔离这句归 [m6] 的现问面自证。
    $evName1b = [string](Get-GdEntryVerb -Game 'r1999' -Verb 'Ack')
    Check "现问到的开关名 [$($evName1b)] 不渗进未确认的提示语(出现即红)" (($x1b.Reason -notmatch [regex]::Escape($evName1b)) -and ($x1b.Reason -match '本轮跑长草方案')) $x1b.Reason
    # 刀⑯ 笔(d)-1：写件三枚出仓 ⇒ 夹具写盘改指核侧写 verb（真身 tools\panel-cs\Round.Data.cs::R1999Ack／R1999ShopState）。
    #   判据换家照 ark 侧同形：Remove-GdShopDoneKey 的 true/false 换成信封 Action（reset＝摘掉了、miss＝本来就没有且不写盘）。
    Invoke-GdCli -Verb 'set-r1999-ack' -Body ([pscustomobject]@{ eventKey = '9.9-combat'; eventTip = '测试' }) | Out-Null
    $x2 = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
    Check '1999 确认后 → 活动方案' ($x2.Mode -eq 'event' -and $x2.Instance -eq 'event')
    Invoke-GdCli -Verb 'set-r1999-shop-state' -Body ([pscustomobject]@{ eventKey = '9.9-combat'; done = $true }) | Out-Null
    $x3 = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
    Check '1999 商店领空 → 长草实例' ($x3.Mode -eq 'idle' -and $x3.Instance -eq 'idle' -and $x3.ShopDone) $x3.Reason
    Check '领空回落的提示语不再带 verb 名(10-30 B-3②-d 刀① 摘掉 body 两枚键，反向钉：出现即红)' (($x3.Reason -notmatch 'daily-clear') -and ($x3.Reason -match '本轮跑长草方案')) $x3.Reason
    Check '1999 领空后仍可撤销' ((Resolve-R1999Plan -S $S -Data $rData -Now $nowMs -IgnoreShopDone).Mode -eq 'event')
    $rvReset = Invoke-GdCli -Verb 'set-r1999-shop-state' -Body ([pscustomobject]@{ eventKey = '9.9-combat'; done = $false })
    Check '1999 撤销标记生效' ($rvReset.Action -eq 'reset' -and (Resolve-R1999Plan -S $S -Data $rData -Now $nowMs).Mode -eq 'event')
    $rvMiss = Invoke-GdCli -Verb 'set-r1999-shop-state' -Body ([pscustomobject]@{ eventKey = '9.9-combat'; done = $false })
    Check '1999 重复撤销判 miss(不写盘那一形，DropMapKey 语义换家到 Action)' ($rvMiss.Action -eq 'miss')
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
# ⚠ 10-26 甲档第五刀刀① 换桌：下面四格原先真调现树 lib\m9a.ps1 的转发壳 Get-M9ALaunchArgs，而那枚壳生产面零调用
#   (拼装本体在 tools\panel-cs\Processes.cs 的 verb m9a-launch-args)、本批起出仓。四格判据一字未动，只换取法：
#   套件按壳原先递进信封的那五键(cliArgs/mode/id/name/forceStart，顺序也照它)直调 Invoke-GdCli ⇒ 拍的仍是那张桌。
function Get-GdM9ALaunchArgsFromDesk {
    param($Instance, [string]$Mode = 'id', [bool]$ForceStart = $true, [bool]$CliArgs = $true)
    $d = Invoke-GdCli -Verb 'm9a-launch-args' -Body ([pscustomobject]@{
        cliArgs    = [bool]$CliArgs
        mode       = $Mode
        id         = [string]$Instance.Id
        name       = [string]$Instance.Name
        forceStart = [bool]$ForceStart
    })
    return @(foreach ($a in @($d.Args)) { [string]$a })
}
$a1 = @(Get-GdM9ALaunchArgsFromDesk -Instance $inst12 -Mode 'id')
Check '命令行入口用长参数(--instance/--autostart/…)' (($a1 -join ' ') -eq '--instance deadbeef --autostart --quit-after-run --force-start') ($a1 -join ' ')
Check '绝不使用短参数(-a/-i/-q/-f 实测不可靠)' (-not ($a1 -contains '-a') -and -not ($a1 -contains '-i') -and -not ($a1 -contains '-q') -and -not ($a1 -contains '-f'))
$a2 = @(Get-GdM9ALaunchArgsFromDesk -Instance $inst12 -Mode 'name')
Check '--instance 也能按实例中文名传' (($a2 -join ' ') -eq '--instance 日常-长草 --autostart --quit-after-run --force-start') ($a2 -join ' ')
$a3 = @(Get-GdM9ALaunchArgsFromDesk -Instance $inst12 -Mode 'id' -ForceStart $false)
Check '关掉 force-start 时只留三项' (($a3 -join ' ') -eq '--instance deadbeef --autostart --quit-after-run') ($a3 -join ' ')
$a4 = @(Get-GdM9ALaunchArgsFromDesk -Instance $inst12 -CliArgs $false)
Check '关掉命令行入口时不加任何参数(回退老路子)' ($a4.Count -eq 0)
Check 'settings 里三个 M9A 开关都在' ($null -ne $S.runtime.m9aCliArgs -and $null -ne $S.runtime.m9aInstanceArg -and $null -ne $S.runtime.m9aForceStart)
# 「默认走命令行入口」那格的行为判据随 Get-GdM9ACliEnabled 出仓(11-06 刀⑫ 笔(b))，改指下面 [t16] 段 $rustCore 那张活桌

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

# 重试策略跟着"跑一轮"一起搬进了 lib\r1999-daily.ps1，入口脚本只剩参数分发；10-24 刀② 把回退段
# 整段出仓后这三格的桌子跟着换：跳过清单与 rescued 翻案的家在 tools\panel-cs\Orchestration.cs 的
# DailyRetry r1999 支，"失败只 WARN"那句话术的家在 tools\rustcore\src\main.rs 的 updgate 分流，
# lib\r1999-daily.ps1 只剩现场件(executor)。断言内容一个字没改——五枚状态名一枚不许少，改的只是到哪家去问。
# 11-04 转接刀·运行面刀⑦ 笔(c/d) 作废登记＋搬靶：上面那段"lib\r1999-daily.ps1 只剩现场件(executor)"自本批
# 按历史引文读——编排岛整件出仓(310 行／六枚真身)，PS 侧不再有那台驱动档 executor。逐格分档如下，
# 判据一个字没改，改的是到哪家去问(搬靶)与哪家都没有(作废登记)：
#   · 搬靶两格——更新闸那格与审计闸那格：那两发起子进程的活整个在 lib\rust-round.ps1 的 updgate／audit
#     两枚 case 里(m9a-update.ps1 -Apply 与 1999-event-audit.ps1 -Apply，含那句"维持 event-pending"的挑行)，
#     对面逐字在场 ⇒ 靶换成轮档件，语义不丢。
#     ⚠ 刀⑧(2026-11-04) 再搬一回靶：那两枚 case 已从轮档件摘净(臂与名册同批)，起子进程＋挑行＋拼串的直身
#     现在 tools\panel-cs\Gate.Host.cs(verb run-m9a-update／run-1999-event-audit)，"谁先跑"这一条的真身在
#     核内链体的 fx.updgate()／fx.audit() 两处调用点 ⇒ 下面三格的靶逐格换成"$gateCs 抓调用行字面 ＋
#     $rustCore 抓链体话术与先后"。判据一个字没改，改的还是到哪家去问。
#   · 作废三格——ask 白名单七枚／'run' case 不许复活／信封 role 字面 B2d-drive-m9a-daily：那是**驱动档**的数。
#     轮档件现拍的 case 名册刀⑧ 那帧是三枚、刀⑩(2026-11-05) 起是一枚(只剩 round-elevate，
#     round-supervisor-start／-stop 随臂出册；再往前刀⑦ 那帧是五枚，updgate／audit 两枚那批出册)，
#     而两件头注本来就明写「不要把驱动档那十四枚白名单当这枚名册用，那是另一族的数」——硬搬就是把轮档那本当
#     驱动档那本用。run 那枚的真身在核内，家一字未动；驱动档信封自此没有 PS 侧的写信人，核内 --drive-m9a-daily
#     成死出口候选(§11.235 已登记，账记在"名册枚数"那侧、不记在净行数)。
#   · 行序那格**拆成两半**：旧形比的是"更新闸在 Invoke-GdTakeover 之前"。轮档件的代码面现拍没有 Invoke-GdTakeover
#     (接管那一步在核内问序里，PS 侧不再执行它) ⇒ 那一半当场没有桌子，登记为漏钉(要钉就得在核内侧成格，归后续刀)；
#     还能量的一半是两把闸的先后——更新闸在上、审计闸在下，同一台 executor 的 ask 序，这半搬靶成立。
#     ⚠ 刀⑧ 起"同一台 executor 的 ask 序"这半也换了家：两枚闸不再出 ask，先后由核内链体的两处调用点决定
#     ⇒ 这一格的靶从轮档件的文本序换成 $rustCore 里 fx.updgate()／fx.audit() 的文本序(Gate.Host.cs 里那两枚
#     臂的定义序不算判据——定义谁先谁后不决定跑谁先跑)。
$r1999RoundPs = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\rust-round.ps1') -Raw -Encoding utf8
$gateCs = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Gate.Host.cs') -Raw -Encoding utf8
$orchCs = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Orchestration.cs') -Raw -Encoding utf8
$rustCore = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'rustcore\src\main.rs') -Raw -Encoding utf8
# 11-06 刀⑫ 笔(b)：m9a.ps1 那枚三态壳出仓后，「没写就算开」这一档全仓只有一处会答，格就打核内那张活桌(不联网、不起进程)
Check '命令行入口三态判据住在核内 --selftest 第 21 节(壳摘后 PS 侧没有第二份答案；回潮＝两份各答一档)' (
    ($rustCore.Contains('let cli21 = |extra: &str|')) -and ($rustCore.Contains('runtime.m9aCliArgs 的三态塌成一态')))
$orchRetry = [regex]::Match($orchCs, '(?s)bool baseRetry(?<win>.*?)retryable = true').Groups['win'].Value
Check '1999 重试会跳过 not-started/empty-queue/unknown-format/blocked' (($orchRetry -match '"ok"') -and ($orchRetry -match '"not-started"') -and ($orchRetry -match '"empty-queue"') -and ($orchRetry -match '"unknown-format"') -and ($orchRetry -match '"blocked"'))
# M9A 自动更新闸(tools\m9a-update.ps1，2026-09-30 用户「检查到更新自己更就行了」)：这里只做静态锚，
# 不联网、不起子进程 —— 实测链路当晚 12:40 人工 -Apply 走过一次(v4.10.0→v4.11.0)，测试不许依赖网络。
Check '1999 起跑段挂了 M9A 自动更新闸(调 tools\m9a-update.ps1 -Apply，失败只 WARN；11-04 笔(c/d) 搬靶到轮档件，刀⑧ 再搬进 Gate.Host.cs)' (
    ($gateCs -match 'GateRun\(raw, @"tools\\m9a-update\.ps1"') -and ($gateCs -match '-Apply') -and ($rustCore -match 'M9A 自动更新未完成'))
Check '两把闸的先后还在：更新闸排在审计闸之前(刀⑧ 起靶＝核内链体两处调用点的文本序；Gate.Host.cs 里那两枚臂的定义序不决定跑谁先跑，不当判据)' (
    $rustCore.IndexOf('fx.updgate()') -ge 0 -and $rustCore.IndexOf('fx.audit()') -ge 0 -and
    $rustCore.IndexOf('fx.updgate()') -lt $rustCore.IndexOf('fx.audit()'))
Check '更新器本体在位且退出码契约写在头注' ((Test-Path -LiteralPath (Join-Path $PSScriptRoot 'm9a-update.ps1')) -and ((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'm9a-update.ps1') -Raw -Encoding utf8) -match '0 = 已是最新'))
# 活动打完没打完的 agent 判定(tools\1999-event-audit.ps1，2026-09-30 用户「让agent判断」)：同样只做静态锚，
# 不碰模拟器不联网 —— 真链路 09-30 用 -Image 旁路对着一张方舟截图实问过(模型如实答 unknown，保守纪律成立)。
$r1999Audit = Get-Content -LiteralPath (Join-Path $PSScriptRoot '1999-event-audit.ps1') -Raw -Encoding utf8
# 11-06 刀⑮ 笔(c) 随动：剥注释尺上移到本格之前——审计工具的截屏已改问核 mumu-screen-shot，
# 正向锚自此抓调用行字面。凡"正向锚＋同件注释也写了这个词"的格从一开始就得剥注释
# (本格旧版只抓裸 Save-GdMuMuShot 时，头注那句改口说明会把锚喂饱——§11.170 那笔账的同一形制)。
$r1999AuditCode = ($r1999Audit -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' '
Check '1999 收尾段挂了活动审计(队列跑完才问、失败只 WARN 不挡轮；11-04 笔(c/d) 搬靶到轮档件，刀⑧ 再搬进 Gate.Host.cs)' (
    ($gateCs -match 'GateRun\(raw, @"tools\\1999-event-audit\.ps1"') -and ($gateCs -match '审计结论\|无事发生\|维持 event-pending') -and
    ($rustCore -match '1999 活动审计没审成'))
Check '审计工具的护栏语义在案：只有画面证据明确的 yes 才 ack，no/unknown 照旧等人' (($r1999Audit -match '只有画面上出现明确的"全部完成"证据才许答 yes') -and ($r1999Audit -match 'no/unknown 维持 pending'))
Check '审计工具只问不点：点击通道一个字都不许出现，截屏只走核的 mumu-screen-shot 调用行(刀⑮ 笔(c) 起)' (($r1999Audit -notmatch 'input tap') -and ($r1999Audit -notmatch 'Invoke-GdVisionRescue') -and ($r1999AuditCode -match "Invoke-GdCli -Verb 'mumu-screen-shot'"))
# 18:21/19:10/19:20 三次实锤定下的现场自理三件：VM 自己起自己关(谁拉起谁关)、落点拿到后先显式 connect
# (adb server 列表为空时 -s 序列号直接 device not found)、序列化拿不到时有界轮询而不是当场判死。
Check '审计工具自己保虚拟机在场：起 VM、只有我们起的才关回去(对称还原；刀⑯ 起 verb 名——Start-GdMuMu/Stop-GdMuMu 真身随 mumu.ps1 出仓)' (($r1999Audit -match "Verb 'start-mumu'") -and ($r1999Audit -match "Verb 'stop-mumu'") -and ($r1999Audit -match 'weBootedVm'))
Check '审计工具拿到落点先 adb-connect 再走 -s(实锤 09-30 19:20 空列表 device not found；刀⑯ 起 verb 名)' ($r1999Audit -match "Verb 'adb-connect'")
# 11-04 刀⑦ 笔(b) 定档改口：审计工具的 ack 那一步不再叫编排岛里那枚壳，改问核(对面 tools\panel-cs\Round.Data.cs::R1999Ack)。
# 两半都是钉：正向半抓"改指没落地"，反向半(锚翻负)抓"有人把那两句人话又抄回 PS 侧"——话术自此只有一处真身，PS 侧一个字都不许留。
# 正向半必须**剥注释后按调用行字面**抓(m9a-update 两形照上面 $fnEv 那套，§「锚不是调用」那笔账)：
# 本格第一版图省事只抓裸 verb 名，mutation 现拍当场自伤——把调用行整个换回旧壳，套件照样全绿，
# 因为头注里那句指路的话把锚喂饱了。凡"正向锚＋同件注释也写了这个词"的格，从一开始就得剥注释。
Check '审计的 ack 那一步问的是核，且件内不复写确认话术(11-04 刀⑦ 笔(b) 改指)' (
    ($r1999AuditCode -match "Invoke-GdCli -Verb 'set-r1999-ack'") -and ($r1999Audit -notmatch '已确认 1999 活动'))
# 2026-10-27 第十六刀：probe-m9adrive 等值门整件退役，门里 C-ps-whitelist 那格咬现树 executor 的三个字面，
# 判据原样落到这里(与 [t16] 既有静态锚同形状)。三条都是从门里逐字搬的口径：白名单七枚、'run' case 不许复活、role 字面。
# ↑ 11-04 转接刀·运行面刀⑦ 笔(c/d) 作废登记：上面这三格(ask 白名单七枚／'run' case 不许复活／role 字面
# B2d-drive-m9a-daily)随编排岛整件出仓——它们量的是驱动档那台 PS executor 的源码形状，岛没了当场没有桌子。
# 逐枚写下它们不再量什么，不静默删：白名单那本是**驱动档**的数(轮档件是另一族的名册，见上面分档那句)；
# 'run' 的真身在核内，家没变；role 字面随信封一起出仓，核内 --drive-m9a-daily 成死出口候选。
# ⚠ 这里**不新立**一枚"轮档名册"的正格：那一族的账在核内 S28_ROSTER 与 PS 侧 Get-GdRoundOuterActs 两处已逐枚点名
# (刀⑧ 起两侧同为三枚)，PS 侧再抄一遍就是本主线一直在删的那本双账(三处各写一遍)。
# 要立只立"一处现问"那种形，射程归后续刀，不在本批。
# 同一批留一枚「退役不回潮」负锚，形状照 test-ark 那两形(拉起族三枚真身／编排岛真身)：岛上那批真身
# (枚数由下面 foreach 的名册供数，注释不写数——写死的枚数会被下一次扩名册变成陈旧账；含 11-04 笔(b) 出仓的 Set-GdR1999Ack)若被人搬回 PS 侧，最可能落的两件就是轮档件与清单件。
$r1999ModulePs = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\r1999-module.ps1') -Raw -Encoding utf8
$r1999IslandBad = @()
foreach ($r1999IslandFn in @('Set-GdR1999ShopState', 'Resolve-GdR1999Decision', 'Show-GdR1999Preview', 'Start-GdR1999Daily', 'Get-GdM9ADriveExe', 'Invoke-GdR1999DailyRust', 'Set-GdR1999Ack')) {
    if ($r1999RoundPs.Contains('function ' + $r1999IslandFn + ' ') -or $r1999ModulePs.Contains('function ' + $r1999IslandFn + ' ')) { $r1999IslandBad += $r1999IslandFn }
}
Check '编排岛真身(含笔(b) 那枚 ack 壳)的定义不在轮档件与清单件里(名册逐枚现扫；回潮＝核内一份＋PS 一份那本双账复活)' ($r1999IslandBad.Count -eq 0) ($r1999IslandBad -join ',')
# 转接刀·运行面刀⑨ 的「退役不回潮」负锚（形状照 test-ark 那两格）：Set-M9AInstance（写 M9A appsettings 的
# plan/currentVersion 再回读那 31 行）整件出仓，对面在 tools\panel-cs\Chain.cs::ApplyM9AInstance
# （verb apply-m9a-instance，Round.SelfTest 第 29 节钉着）。这一格只防"有人把定义搬回 PS 侧"＝写手两份那本双账复活。
$cut9M9APs = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\m9a.ps1') -Raw -Encoding utf8
Check '刀⑨ 那枚 Set-M9AInstance 的定义不在 m9a.ps1(写实例配置只住核内 Chain.cs 一处；回潮＝PS 一份＋核内一份那本双账复活)' (
    -not $cut9M9APs.Contains('function Set-M9AInstance '))
# 转接刀·运行面刀⑫ 的「退役不回潮」负锚（形状照上面 刀⑨ 那格，同一本 $cut9M9APs 现读的原文）：笔(a) 那批发
# 的是批次 9 冻结段那 71 行逐行 # 的旧本体与 Stop-M9A 那 8 行 PS 壳，笔(b)(c) 又添三枚读配置壳的定义行。
# 针一律不取件名裸串或「批次 9 冻结段」那种散文串——史注里就写着它名，把散文当命中会造出一格恒红；
# 走的针只活在被摘本体里('function 名 ' 那形)，名册由下面 foreach 供数(注释不写枚数——写死的数会被下一次
# 扩名册变成陈旧账)。⚠ 这些针按原文逐字命中(含注释面)，所以它们自此不许出现在本件或 m9a.ps1 的散文里，
#   将来谁要在史注里引那几串，先把针换成只活在本体里的形，别把红格留成假警。
$cut12M9ABad = @()
foreach ($cut12Needle in @('function Stop-M9A ', 'switch ($Event.Kind)', 'function Get-M9APaths ', 'function Get-GdM9ACliEnabled ', 'function Get-M9ADailyLog ')) {
    if ($cut9M9APs.Contains($cut12Needle)) { $cut12M9ABad += $cut12Needle }
}
Check '刀⑫ 笔(a)(b)(c) 出仓的 Stop-M9A 壳、冻结段体特征串与三枚读配置壳的定义行都不在 m9a.ps1(verb stop-m9a 与 paths／三态各只住一处；回潮＝史注之外再多一具身体)' (
    $cut12M9ABad.Count -eq 0) ($cut12M9ABad -join ' | ')

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
# 刀⑮ 笔(a) 曾把这份后端件的读改成「不依赖件在场」＋给下面那枚护栏立现读格；刀⑳ 笔(a)-1 起件已整枚退役，
# 那份读（$mumuLib）随之零消费者、同批摘掉。护栏照旧两条腿都在：件与 New-GdVisionTarget-mumu 必须同一个事实
# (同在同不在)，半迁移(命令还在但件被挪走)当场红。
$mumuLibPath = Join-Path $PSScriptRoot '..\lib\vision-mumu.ps1'
$mumuLibOnDisk = Test-Path -LiteralPath $mumuLibPath
Check '模拟器后端件与 New-GdVisionTarget-mumu 命令同一个事实(同在同不在，半迁移红)' (
    $mumuLibOnDisk -eq [bool](Get-Command New-GdVisionTarget-mumu -ErrorAction SilentlyContinue)) "件=$mumuLibOnDisk"
$m9aLib = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\m9a.ps1') -Raw -Encoding utf8
# (原来这里还读一份 lib\r1999-daily.ps1 给下面那两格外层重试锚用；10-24 刀② 后那两格打的是 Orchestration.cs，
#  读面在 [t16] 段的 $orchCs/$orchRetry 已经取好，这里不再重复读同一份文件)
# ⚠ 10-25 甲档刀①-b：下面那九格正向文本锚也从 lib\m9a.ps1 搬走了，$m9aLib 只剩最后那格**否定式**在用
#   ("状态名一律没改"——现树里不许出现新状态名，这一形与真身在不在场无关，摘件的刀② 之后仍要留在现树身上)
# 2026-11-06 刀⑮ 笔(d)：本节原来一枚护栏罩着整段，其中大半格量的根本不是 lib\vision-mumu.ps1，
# 而是 RunLoops.cs／main.rs／vision-ask.ps1／lib\vision-rescue.ps1／Gate.Host.cs —— 让它们跟着这枚件一起被摘
# 就是"山搬了家而尺跟着埋"(在册那条空串 -match 假绿的同一形)。自此那些格提出护栏无条件跑，
# 护栏只留"靶子真调才跑得动"那几格；提出与留下的格数由装配件那把尺逐档现读打印，注释不预写枚数。
# 件不在不许正锚空咬，改咬"那三枚没复活＋端点三源金样与两枚出口住在核侧"。
    # "注释里写着不该有的东西"和"代码里真的那么干了"是两回事：Get-Command 的 Definition 连着
# 注释一起给，所以"不许写死端口/不许走 exec-out"这类**否定式**断言必须先把注释剥掉再判 ——
# 否则文件里多写一句解释就把自检弄红(2026-09-26 第一次跑就是这两条假红)。
function Get-GdTestCodeOnly {
    param([string]$Text)
    return (($Text -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' ')
}
# 反过来也判：内核既然两路共用，就不许认识任何一端的专有东西(否则 1999 仓库带上它就等于拖着端游的假设跑)
# 2026-11-08 刀㉑ 笔(c)：那枚「不认识 adb／端游窗口」的格连同三行取数一并摘除——它咬的守卫件
#   lib\vision-rescue.ps1 已整枚出仓且名册零登记，取数恒空串 ⇒ -notmatch 恒真，是一枚哑合取(在册「空串假绿」那一形)。
#   语义不埋：承接面是现树活判据两枚——本文件下面那枚「编排没复活」的负锚，与 tools\test-modules.ps1:168-170
#   那枚「名册不登记 + 命令不复活」的兜底内核格；件若回来，那一族要连臂一起写回来，不许只把文件放回(同帧 modules:160 半迁移红会当场咬)。
#   门账自此换代 test-r1999 170→169(摘的是恒真格，覆盖率不掉：它本来就一次都没判过任何东西)。
# 2026-10-28 甲档第二十刀刀③：话术锚「这棵树上没有」的代码面唯一出处在被摘的旧 :325(Invoke-GdVisionRescue
# 的闸门段)——那一截编排进核后，内核侧只留"按命名约定找靶子、找不到回 $null"这一件事，说不说那句话归调用方。
# 活着的对面有两份：PS 薄壳 tools\vision-ask.ps1:55、核内 C# tools\panel-cs\Live.cs:809(由 --livetest 盯)。
# 命名约定那一半没搬家，本刀一个字没动它：这是它本来就有的分工，不是判据放宽。
# 2026-11-06 刀⑮ 笔(c)：那份里的 PS 薄壳那一半也搬进核了——薄壳不再自己判"这棵树上有没有这一路"，改问
# resolve-rescue-context，话术由核内 No(data, why) 经 $ctx.Skip 转达(Live.cs:809 与 --livetest 那格
# Live.SelfTest.cs:132 逐字钉同一句话)。所以下面这格从"两份各咬一次"改成"核内正锚 + 薄壳负锚"：
# 正锚证明话术还在被说，负锚证明它确实只剩一处出处而不是搬家没搬净——搬家非增删，不许让旧形静默失守。
$askCode = Get-GdTestCodeOnly (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'vision-ask.ps1') -Raw -Encoding utf8)
Check '"这棵树上没有这一路"那句话只由核内说一次，PS 薄壳的代码面不再自己拼这句(刀⑮ 笔(c) 起；头注里的说明句不算)' (
    ((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Live.cs') -Raw -Encoding utf8) -match '这棵树上没有') -and
    ($askCode -notmatch '这棵树上没有') -and
    ($askCode -match 'resolve-rescue-context'))
# 2026-11-08 刀㉑ 笔(a) 死臂随批摘：上面的守卫件 lib\vision-rescue.ps1 已整枚出仓且名册零登记 ⇒ 真臂恒不执行，
# 摘真臂、else 支升为唯一形(它本来就在承接)；这不触"摘而不搬"红线——负锚量的"编排没复活"就是现树的活判据。
$rescueConvTooth = (-not (Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue))
Check '内核按 New-GdVisionTarget-<kind> 的命名约定拼函数名找后端，找不到就回 $null(刀㉑ 起此格唯一形＝编排没复活)' $rescueConvTooth "复活=$([bool](Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue))"
# 接线：这一路此前**压根没有**兜底调用点(后端是死代码)。现在按"进程还在不在"分两类接上。
# ⚠ 10-25 甲档刀①-b 换桌：下面九格原先咬的是 lib\m9a.ps1 的 Invoke-M9ARun 整段循环，而那一刀要把它摘掉。
#   新桌子两张，都是活码：判据/话术/扣数/分流 = tools\panel-cs\RunLoops.cs 的 decide-m9a-run-tick，
#   调用点/记账/靶子名与 adb 通道 = tools\rustcore\src\main.rs 的执行侧(of_m9a／do_rescue／do_record／合体轮)。
#   断言内容一个字没改，改的只是到哪家去问(与上面 [t16] 那三格 10-24 刀② 同一处置)。
#   唯一一处**形变**写在第 3 格：旧那半句"`$rescue.Left--` 不许出现"是 PowerShell 闭包子作用域特有的坑
#   (& 调用里扣不动)，C# 侧恰恰用 `rescueLeft--` 且跨拍状态由 verb 自己中继 ⇒ 那一形在新桌上不成立，
#   换成钉"扣的那一数必须写回跨拍状态"。另补一格"尺接没接电"：窗口一枚取不到就当场红，
#   免得桌子搬了家而九格靠空串 -match 假绿(在册纪律：现场那一路要补正例自检格)。
$runLoopsCs = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\RunLoops.cs') -Raw -Encoding utf8
$m9aLadder = [regex]::Match($runLoopsCs, '(?s)void ResumeRecord\(\).*?Eq\(status, "empty-queue"\);').Value
$m9aAsk = [regex]::Match($runLoopsCs, '(?s)if \(InAskSet\(\) && rescueLeft > 0 && rescueAvail\).*?phase = "ask-rescue"').Value
$m9aEmit = [regex]::Match($runLoopsCs, '(?s)"Directives", Gd\.Out\(.*?if \(hasDuration\)').Value
$rsRound = [regex]::Match($rustCore, '(?s)let mut m9a_scen = m9a_scen_core.*?m9a_scen\.grab_available = false').Value
$rsEnv9 = [regex]::Match($rustCore, '(?s)fn of_m9a.*?serial: sc\.serial\.clone\(\)').Value
$rsRescue = [regex]::Match($rustCore, '(?s)fn do_rescue.*?\*rescue_left = rl;').Value
$rsTick = [regex]::Match($rustCore, '(?s)let rescue_avail = h\.rescue_available\(\).*?st_m9a = jv_get\(&d, "State"\);').Value
$rsM9aRt = [regex]::Match($rustCore, '(?s)let rt = jv_get\(v, "runtime"\);\n    // mode 缺键.*?sc\.stall_min = jv_int\(&jv_get\(&rt, "stallTimeoutMinutes"\), 0\);').Value
$rsClock = [regex]::Match($rustCore, '(?s)if jv_truthy\(&jv_get\(&jv_get\(&d, "Directives"\), "ResetLastLog"\)\).*?last_log = h\.clock\(\);').Value
$rsRescued = [regex]::Match($rustCore, '(?s)if jv_truthy\(&jv_get\(&d, "Rescued"\)\).*?res\.rescued = true;').Value
$rsRecord = [regex]::Match($rustCore, '(?s)fn do_record.*?jv_s\("record"\)').Value
Check '兜底这九格的桌子都取到了窗口(尺接没接电，空串 -match 会假绿)' (
    $m9aLadder.Length -gt 0 -and $m9aAsk.Length -gt 0 -and $m9aEmit.Length -gt 0 -and $rsRound.Length -gt 0 -and $rsEnv9.Length -gt 0 -and $rsRescue.Length -gt 0 -and $rsTick.Length -gt 0 -and $rsM9aRt.Length -gt 0 -and $rsClock.Length -gt 0 -and $rsRescued.Length -gt 0 -and $rsRecord.Length -gt 0) $(
    'len: ladder=' + $m9aLadder.Length + ' ask=' + $m9aAsk.Length + ' emit=' + $m9aEmit.Length + ' round=' + $rsRound.Length + ' env9=' + $rsEnv9.Length + ' rescue=' + $rsRescue.Length + ' tick=' + $rsTick.Length + ' rt=' + $rsM9aRt.Length + ' clock=' + $rsClock.Length + ' rescued=' + $rsRescued.Length + ' record=' + $rsRecord.Length)
Check '兜底调用点认 mumu 这枚靶子，且带上实例的 adb 通道(桌子＝main.rs 的 of_m9a 与合体轮)' (
    $rsEnv9 -match 'target: sc\.target\.clone\(\)' -and $rsEnv9 -match 'adb: sc\.adb\.clone\(\)' -and $rsRound -match 'm9a_scen\.target = "mumu"')
Check '兜底不是硬调：先问可用性，树上没有就只是不兜、不炸(旧那格问的是 Get-Command 探路)' (
    $m9aAsk -match 'rescueAvail' -and $rsRescue -match 'h\.rescue_available\(\)' -and $rsTick -match 'let rescue_avail = h\.rescue_available\(\)')
Check '预算扣的那一数必须写回跨拍状态(一轮里真扣得动，扣完就不再问)' (
    $m9aAsk -match 'rescueLeft--' -and $rsRescue -match 'rl -= 1' -and $rsRescue -match '\*rescue_left = rl;')
Check '次数上限沿用同一个键 runtime.visionMaxRescue(限的是"一轮里跟模型没完没了的次数")' (
    $rsM9aRt -match 'jv_get\(&rt, "visionMaxRescue"\)' -and $rsTick -match '"rescueMaxRaw"')
Check '停滞/超时(进程还在)兜成了就撤回调款、计时从头算继续等' (
    $m9aLadder -match 'if \(Alive\(\)\)' -and $m9aLadder -match 'resetLastLog = true;' -and $rsClock -match 'last_log = h\.clock\(\);')
Check 'blocked/not-started/empty-queue 也问一次(把猜测换成"画面上确实是 XX")' (
    $m9aLadder -match 'Eq\(status, "blocked"\)' -and $m9aLadder -match 'Eq\(status, "not-started"\)' -and $m9aLadder -match 'Eq\(status, "empty-queue"\)' -and $m9aLadder -match '"判因"')
Check '兜清楚的证据是唯一能翻掉"重试没意义"的东西 → 打 Rescued 标记并交给核认下' (
    $m9aLadder -match 'rescued = true;' -and $m9aEmit -match 'd\["Rescued"\] = rescued;' -and $rsRescued -match 'res\.rescued = true;')
Check '这一路的救火账记在核里，game 归 r1999、target 归 mumu(do_record→add_rescue_record)' (
    $rsRecord -match 'add_rescue_record' -and $rsEnv9 -match 'game: sc\.game\.clone\(\)' -and $rsRound -match 'm9a_scen\.game = "r1999"')
Check '兜底次数与最后一问拼进 Detail(判因链不许丢这条线)' (
    $m9aLadder -match '；视觉兜底 ' -and $m9aLadder -match '次，最后一次：' -and $m9aLadder -match '\[停滞已解除，继续等待\]')
Check '状态名一律没改(收尾与通知按状态名分流，造新名字等于把判因链弄断)' (
    $m9aLib -notmatch "Status = 'rescued'" -and $m9aLib -notmatch "Status = 'vision")
# 外层重试的分流：画面被兜清楚了，本来"重试也没用"的那几种才值得再跑一次。
# 10-24 刀② 后这段判据住在 Orchestration.cs DailyRetry 的 r1999 支($orchRetry 那枚窗口在 [t16] 段就取好了)，
# 话术逐字由它交出、executor 只回放 —— 桌子换了，钉的还是同一件事：rescued 只翻那三种状态。
Check '重试分流认这个标记(not-started/empty-queue/blocked 只有兜清楚才翻案)' (
    $orchRetry -match 'baseRetry \|\| \(rescued &&' -and $orchRetry -match '"not-started"' -and $orchRetry -match '"empty-queue"' -and $orchRetry -match '"blocked"')
Check '带标记时日志说得清是"兜清楚了才重试"(不是原来那句"关闭游戏后重试一次")' (
    $orchCs -match '视觉兜底把挡路的界面点掉了')
# 刀⑳ 笔(a)-1：这里原来是一根 if / else 两臂的门，腹内 19 枚正向格打的是模拟器后端件在场时的那套 PS 靶
# （New-GdVisionTarget-mumu／Get-GdMuMuAdbEndpoint／Save-GdMuMuShot／Send-GdMuMuTap 的字段与实现形状）。
# 该件已随刀⑯ 笔(d)-2 整枚退役出仓 ⇒ Test-Path 恒假 ⇒ 腹内从那一刻起一枚都不执行（不记失败的死臂，不是覆盖）。
# 下面那 3 枚核侧对面格由「只在件不在时跑」升为无条件跑——件已不在，判据本来就该一直说话。
Check '模拟器后端那三枚没在现树复活(算端点／拍屏／点一下)' (
    -not (Get-Command Get-GdMuMuAdbEndpoint -ErrorAction SilentlyContinue) -and
    -not (Get-Command Save-GdMuMuShot -ErrorAction SilentlyContinue) -and
    -not (Get-Command Send-GdMuMuTap -ErrorAction SilentlyContinue))
$gateHostT18 = ''
try { $gateHostT18 = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Gate.Host.cs') -Raw -Encoding utf8 } catch { }
$cliT18 = ''
try { $cliT18 = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Cli.cs') -Raw -Encoding utf8 } catch { }
Check '端点那三条规矩住在核侧(Gate.Host.cs：MuMuManager adb -v 只抠 ip:port／JSON 分字段兜底／拿不到先说跑 setup.ps1)' (
    $gateHostT18.Length -gt 0 -and $gateHostT18.Contains('adb_host') -and $gateHostT18.Contains('adb_port') -and
    $gateHostT18.Contains('setup.ps1') -and $gateHostT18.Contains('MuMuManager')) ('Gate.Host.cs=' + $gateHostT18.Length + ' 字符')
Check '模拟器拍屏与点一下两枚出口仍在核内分派链上(mumu-screen-shot／mumu-screen-click，靶子名走 resolve-rescue-context)' (
    $cliT18.Length -gt 0 -and $cliT18.Contains('mumu-screen-shot') -and $cliT18.Contains('mumu-screen-click') -and
    $cliT18.Contains('resolve-rescue-context')) ('Cli.cs=' + $cliT18.Length + ' 字符')

# 刀⑱ 笔(a)「退役不回潮」负锚：Get-M9AConnection 整枚出仓(b5 扫描尺现拍代码面 LIVE=0)，
# 对面 verb read-m9a-connection。针只取 'function 名 ' 那形，不吃散文。追加在收尾之前＝不动本件既有行号锚。
Check '刀⑱ 那枚 Get-M9AConnection 的定义不在 m9a.ps1(探活只住核内 Chain.cs::ReadM9AConnection 一处；回潮＝PS 一份＋核内一份那本双账复活)' (
    -not $cut9M9APs.Contains('function Get-M9AConnection '))

$res = Show-GdTestResult -Tag 'r1999'
if ($NoExit) { return $res }
exit $(if ($res.Fail) { 1 } else { 0 })
