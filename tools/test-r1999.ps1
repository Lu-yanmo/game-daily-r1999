# tools/test-r1999.ps1 — 重返未来1999 那一路的纯逻辑自检(只读：不改配置、不启动任何程序)
# 用法: pwsh -NoProfile -File tools\test-r1999.ps1 [-History]
#       -History 追加"回放本机某天的真实 M9A 日志"那 7 条(断言的是那次真跑的内容，换机器不适用，故默认不跑)。
# 方舟那一路在 tools\test-ark.ps1；两边都跑就依次直调这两本(各本自己的退出码说话)。
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
# ⚠ 刀⑰ 笔(c)-2(2026-11-07)改判：上面"两臂都在"按历史引文读——回落臂已出仓，第二格自此翻负(回潮即红)，
#   核臂格与九键信封格保持。放在 [t4] return 之前：没装 M9A 也要有证词，否则锚成哑尺。剥注释是必须的
#   (头注也写了 read-r1999-activity，不剥的"正向锚"真身摘后照样绿＝在册「锚不是调用」形；Get-GdTestCodeOnly 此处未定义)。
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

# ---- 刀㉓ 笔(a)(2026-11-08)：[t7] 那八枚行为钉换家核侧，本壳只留形状证词 ----
# 为什么留锚不留行为：Resolve-R1999Plan 是转发壳，行为面在核侧两本已四发 mutation 证牙(对面坐标见 [t7] 头上那段)；
# 但壳的接线(现问一次取齐、领空吃核交回的那份)一旦断，核侧全绿也照样在跑旧形 ⇒ 这一格留在 PS 侧、放在 [t4] 现场门之前(同上面 $fnEv 三格的道理)。
$fnRPlan = [string](Get-Command Resolve-R1999Plan -ErrorAction SilentlyContinue).Definition
$fnRPlan = ($fnRPlan -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' '
Check '㉓(a) 核臂在：Resolve-R1999Plan 现问 compose-decision-inputs 一次取齐(旧三枚读件那次问不再出现在组值半段)' (
    ($fnRPlan.Contains("Invoke-GdCli -Verb 'compose-decision-inputs'")) -and (-not $fnRPlan.Contains('read-decision-state')) -and
    (-not $fnRPlan.Contains('Get-GdAckMap')) -and (-not $fnRPlan.Contains('Get-GdShopDoneMap')) -and (-not $fnRPlan.Contains('Test-GdShopDoneEnabled'))) $(
    if ($fnRPlan.Length -eq 0) { 'Resolve-R1999Plan 不在场(尺没接到电)' } else { 'len=' + $fnRPlan.Length })
Check '㉓(a) 折算在核：组值半段只传可选入键 ignoreShopDone，领空吃核交回的 $ci.ShopDone(「-IgnoreShopDone 可回看原结论」的行为对面＝roundtest 35)' (
    ($fnRPlan.Contains('ignoreShopDone = [bool]$IgnoreShopDone')) -and ($fnRPlan.Contains('$ci.ShopDone')) -and
    (-not $fnRPlan.Contains('FlagEnabled')) -and (-not $fnRPlan.Contains('$ds.ShopDoneGateOpen'))) $('len=' + $fnRPlan.Length)

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
# 诚实登记(刀㉓ 笔(a) 2026-11-08)：这一段原先那八枚行为判定面(未确认→pending／确认后→event／领空→idle／提示语不带
# verb 名／-IgnoreShopDone 可回看／撤销生效／重复撤销判 miss)整族换家核侧，摘前已过四发 mutation 证牙。逐枚对面坐标＝
# Decisions.SelfTest.cs::DecTest r1999 族(那四枚取值与防回归，其中领空那枚本批收紧了 Instance 断言)＋Round.SelfTest.cs
# 第 35) 节(领空折图三枚)与第 18) 节(set-r1999-shop-state 回执格＋本批新增的盘上格：撤销真摘＋重复撤销真不写盘)。
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
    $x1b = Resolve-R1999Plan -S $S -Data $rData -Now $nowMs
    # 11-01 丙形真转接(§11.176 ⑥)：现场覆盖件退役 ⇒ 这一格不再"先盖一枚长名再看它漏不漏"，改钉"现问到什么名都不许渗进提示语"。
    #   它不再量的是旧那格的"覆盖只影响被写的那一款"——写侧没了，那一形从此不可能出现；跨款隔离这句归 [m6] 的现问面自证。
    #   (刀㉓(a)：这一枚留在 PS 是因为它钉的是壳侧现问 Get-GdEntryVerb 的那枚名，核侧的防回归格用的是拼出来的文案，量不到这枚现读。)
    $evName1b = [string](Get-GdEntryVerb -Game 'r1999' -Verb 'Ack')
    Check "现问到的开关名 [$($evName1b)] 不渗进未确认的提示语(出现即红)" (($x1b.Reason -notmatch [regex]::Escape($evName1b)) -and ($x1b.Reason -match '本轮跑长草方案')) $x1b.Reason
} finally {
    Restore-GdTestState
}
Check 'state 文件已还原' (Test-GdTestStateRestored)

Write-Host "`n[t10] M9A 日志事件解析(用真实日志行)"
# 刀㉓ 笔(b) 第二子批(11-08)：这里原先 11 枚单行判定格整族换家核侧＝Processes.SelfTest.cs 的 LineGroup（证牙＝mutation 七发，锚检 OUTER-HITS=0）。
# 刀㉕ 笔(d)：原「解析器本体仍只有一处」格出仓——判据只是 ParseM9aLine 出现 ≥1 次，证不了"只有一处"、且只是源文本在场；
#   行为面由 LineGroup 的 line/ 格与 E2eGroup 真日志链钉着（解析器被删则编不过），证牙＝改 res-current 档名后 --proctest 咬中三格（gdlogs/cut25d/m2-proctest.log）。
$procCsST = [string](Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'panel-cs/Processes.SelfTest.cs'))
$lineCells = ([regex]::Matches($procCsST, 'b\.Chk\("line(?:s)?/')).Count
Check "核侧 LineGroup 的 line/ 格枚数不缩水(现拍 $lineCells 枚，门槛 16)" ($lineCells -ge 16) "=$lineCells"
Check 'settings 里 1999 起跑超时已收紧' ([int]$S.runtime.r1999StartTimeoutMinutes -le 12) "=$($S.runtime.r1999StartTimeoutMinutes) 分钟"

Show-GdCommonTailChecks

Write-Host "`n[t12] M9A 命令行脚本化入口 + 结果判定"
# 刀㉬ 笔(b) 第三子批 第五刀(11-08)：上面五枚启动参数格已换家核侧：Processes.SelfTest.cs 的 args/id(:283)钉整串、args/name(:284)、args/noforce(:285)、args/nocli(:286)；「绝不使用短参数」被整串等值全称负锚吃掂。证牙：raw/gdlogs-mut4 四发全 TEETH(锚唯一、改弱后 --proctest 红行含对应 label、整本还原后 88/88)。ArgsStr 与被摘的 desk 取数壳吃同一枚五键信封→同形同轴。
Check 'settings 里三个 M9A 开关都在' ($null -ne $S.runtime.m9aCliArgs -and $null -ne $S.runtime.m9aInstanceArg -and $null -ne $S.runtime.m9aForceStart)
# 「默认走命令行入口」那格的行为判据随 Get-GdM9ACliEnabled 出仓(11-06 刀⑫ 笔(b))，改指下面 [t16] 段 $rustCore 那张活桌

# 真实日志行(09-23 17:30 用 --autostart 成功提交队列那一段)
# 刀㉓ 笔(b) 第二子批(11-08)：原先两枚（ui-start 与 queue 的 Text）已换家核侧＝line/ui-start 与 line/queue，账同 [t10]。

# 刀㉓ 笔(b) 第三子批 第二刀(11-08)：本节 [t12]/[t13]/[t14] 原先 16 枚「问一句答什么」状态结论格换家核侧＝
#   Processes.SelfTest.cs 的 res/ 族(empty-queue-没收到／ok／ok带尾警／ok-quit／notstart-活／notstart-退／error-失败／
#   还在跑／unknowfmt-退／unknowfmt-活／timeout；整句期望比本侧 -match 强一档)。累计面三枚随后补成同形同轴：
#   RunLoops「connect-start→finish」与「queue-prepared+十枚 task 累计」、Processes「res/empty-queue-零任务自报数」；
#   真日志串接面由 e2e/0923・0925・legacy 三枚接管；launch-args 六枚由第五刀换家(args/id・name・noforce・nocli
#   ＋整串等值吃掉「绝不使用短参数」)。unknown-format 两支(新版文案返回 $null→line/unknown-2；零标志不谎称
#   --autostart 没生效→res/unknowfmt-不谎称autostart)亦已出仓。故 [t13]/[t14]/[t17] 三段整族出仓，
#   本段只剩 settings 现场值两枚；证牙落盘 raw\gdlogs-mut3\、raw/gdlogs-mut8\。

# 重试策略跟着"跑一轮"搬进 lib\r1999-daily.ps1，10-24 刀② 回退段出仓后判据与话术的家＝Orchestration.cs；
# 11-04 刀⑦ 笔(c/d) 作废登记＋搬靶：编排岛整件出仓(310 行／六枚真身)，PS 侧不再有驱动档 executor。分档如下：
#   · 搬靶两格(更新闸／审计闸)刀⑧ 再搬：起子进程＋挑行＋拼串在 Gate.Host.cs(verb run-m9a-update／
#     run-1999-event-audit)，先后由核内链体 fx.updgate()／fx.audit() 两处调用点决定——判据未改，改的是到哪家问。
#   · 作废三格(ask 白名单七枚／'run' case 不许复活／信封 role 字面 B2d-drive-m9a-daily)＝**驱动档**的数；
#     轮档件是另一族的名册，两件头注明写"别把驱动档那十四枚白名单当这枚名册"。'run' 真身在核内，
#     --drive-m9a-daily 成死出口候选(§11.235 已登记，账记名册侧、不记净行数)。
#   · 行序那格拆两半："更新闸在 Invoke-GdTakeover 之前"半当场没有桌子(登记为漏钉，归核内侧后续刀)；
#     还能量的一半＝$rustCore 里 fx.updgate() 在 fx.audit() 之前(Gate.Host.cs 两臂定义序不算判据)。
$r1999RoundPs = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\rust-round.ps1') -Raw -Encoding utf8
$gateCs = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Gate.Host.cs') -Raw -Encoding utf8
$rustCore = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'rustcore\src\main.rs') -Raw -Encoding utf8
# 11-06 刀⑫ 笔(b)：m9a.ps1 那枚三态壳出仓后，「没写就算开」这一档全仓只有一处会答，格就打核内那张活桌(不联网、不起进程)
Check '命令行入口三态判据住在核内 --selftest 第 21 节(壳摘后 PS 侧没有第二份答案；回潮＝两份各答一档)' (
    ($rustCore.Contains('let cli21 = |extra: &str|')) -and ($rustCore.Contains('runtime.m9aCliArgs 的三态塌成一态')))
# 刀㉕ 笔(d)(11-08)：原「1999 重试会跳过 not-started/empty-queue/unknown-format/blocked」是 Orchestration.cs 的源窗口 -match 格，整枚出仓——
#   行为面由 Orchestration.SelfTest.cs 的 RetryGroup 11 枚格同形同轴钉死(整句等值，比 -match 强一档)；证牙＝摘掉 baseRetry 的 empty-queue 后 --orchtest 咬中三格（gdlogs/cut25d/m1-orchtest.log）。
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
# 11-04 刀⑦ 笔(b) 改指：审计的 ack 不再叫编排岛那枚壳，改问核(Round.Data.cs::R1999Ack)。正向半抓"改指没落地"、
# 反向半抓"话术抄回 PS"，且正向半必须剥注释后按调用行字面抓(第一版只抓裸 verb 名，mutation 当场自伤：
# 头注那句指路把锚喂饱，套件照样全绿——§「锚不是调用」)。
Check '审计的 ack 那一步问的是核，且件内不复写确认话术(11-04 刀⑦ 笔(b) 改指)' (
    ($r1999AuditCode -match "Invoke-GdCli -Verb 'set-r1999-ack'") -and ($r1999Audit -notmatch '已确认 1999 活动'))
# 2026-10-27 第十六刀从 probe-m9adrive 等值门搬来的三格(ask 白名单七枚／'run' case 不许复活／role 字面)已随
# 编排岛出仓作废登记(量的是驱动档 executor 的源码形状；'run' 真身在核内)。不新立"轮档名册"正格——那本账在
# 核内 S28_ROSTER 与 Get-GdRoundOuterActs 两处已逐枚点名，PS 再抄＝双账；下面负锚的名册由 foreach 供数，注释不写数。
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
# 转接刀·运行面刀⑫ 负锚(形状同上面刀⑨ 那格，共用 $cut9M9APs 原文)：针只取'function 名 '那形(裸串/
# 散文串会在史注里自撞成恒红)，名册由 foreach 供数、注释不写数。⚠ 这些针按原文逐字命中(含注释面)⇒自此
# 不许出现在本件或 m9a.ps1 的散文里；要引先把针换成只活在本体里的形，别留假警。
$cut12M9ABad = @()
foreach ($cut12Needle in @('function Stop-M9A ', 'switch ($Event.Kind)', 'function Get-M9APaths ', 'function Get-GdM9ACliEnabled ', 'function Get-M9ADailyLog ')) {
    if ($cut9M9APs.Contains($cut12Needle)) { $cut12M9ABad += $cut12Needle }
}
Check '刀⑫ 笔(a)(b)(c) 出仓的 Stop-M9A 壳、冻结段体特征串与三枚读配置壳的定义行都不在 m9a.ps1(verb stop-m9a 与 paths／三态各只住一处；回潮＝史注之外再多一具身体)' (
    $cut12M9ABad.Count -eq 0) ($cut12M9ABad -join ' | ')

# [t17](09-25 19:24 复盘段)整段出仓：四枚行为格与八行真日志由 Processes.SelfTest.cs 的 e2e/0925(两枚失败
#   累计＋版本差＋结论 blocked)与 e2e/legacy(缺 failedTasks 键仍按 1 算)接管，证牙＝raw/gdlogs-mut8/。

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
#  刀㉕ 笔(d) 起那两格已随 RetryGroup 换家核侧自检，本文件不再读 Orchestration.cs——重试分流的判据与话术只有核内一处)
# ⚠ 10-25 甲档刀①-b：下面那九格正向文本锚也从 lib\m9a.ps1 搬走了，$m9aLib 只剩最后那格**否定式**在用
#   ("状态名一律没改"——现树里不许出现新状态名，这一形与真身在不在场无关，摘件的刀② 之后仍要留在现树身上)
# 2026-11-06 刀⑮ 笔(d)：本节原一枚护栏罩整段，但大半格量的是 RunLoops.cs／main.rs／vision-ask.ps1／
# Gate.Host.cs——跟着件一起摘就是"山搬了家而尺跟着埋"；自此那些格无条件跑，护栏只留"靶子真调才跑得动"几格
# (枚数由装配件尺现读，注释不预写)；件不在不许正锚空咬，改咬"那三枚没复活＋端点三源金样与两枚出口住在核侧"。
    # "注释里写着不该有的东西"和"代码里真的那么干了"是两回事：Get-Command 的 Definition 连着
# 注释一起给，所以"不许写死端口/不许走 exec-out"这类**否定式**断言必须先把注释剥掉再判 ——
# 否则文件里多写一句解释就把自检弄红(2026-09-26 第一次跑就是这两条假红)。
function Get-GdTestCodeOnly {
    param([string]$Text)
    return (($Text -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' ')
}
# 内核两路共用，不许认识任何一端的专有东西。2026-11-08 刀㉑ 笔(c)：原「不认识 adb／端游窗口」格连同三行取数
#   一并摘除(守卫件 lib\vision-rescue.ps1 已出仓⇒取数恒空串⇒-notmatch 恒真＝在册「空串假绿」哑合取)；
#   承接面＝下面「编排没复活」负锚＋test-modules.ps1:168-170 那枚兜底内核格(件回来要连臂一起写回)。
# 2026-10-28 刀③ 起话术锚「这棵树上没有」只剩调用方说；2026-11-06 刀⑮ 笔(c) 薄壳那半也搬进核(改问
#   resolve-rescue-context，话术由 Live.cs:809 说、Live.SelfTest.cs:132 逐字钉)⇒下面这格＝核内正锚+薄壳负锚，
#   正锚证话术还在被说，负锚证只剩一处出处(搬家非增删)。
$askCode = Get-GdTestCodeOnly (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'vision-ask.ps1') -Raw -Encoding utf8)
Check '"这棵树上没有这一路"那句话只由核内说一次，PS 薄壳的代码面不再自己拼这句(刀⑮ 笔(c) 起；头注里的说明句不算)' (
    ((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Live.cs') -Raw -Encoding utf8) -match '这棵树上没有') -and
    ($askCode -notmatch '这棵树上没有') -and
    ($askCode -match 'resolve-rescue-context'))
# 2026-11-08 刀㉑ 笔(a) 死臂随批摘：上面的守卫件 lib\vision-rescue.ps1 已整枚出仓且名册零登记 ⇒ 真臂恒不执行，
# 摘真臂、else 支升为唯一形(它本来就在承接)；这不触"摘而不搬"红线——负锚量的"编排没复活"就是现树的活判据。
$rescueConvTooth = (-not (Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue))
Check '内核按 New-GdVisionTarget-<kind> 的命名约定拼函数名找后端，找不到就回 $null(刀㉑ 起此格唯一形＝编排没复活)' $rescueConvTooth "复活=$([bool](Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue))"
# 接线：这一路按"进程还在不在"分两类接上。10-25 刀①-b 换桌：判据/话术/扣数/分流＝RunLoops.cs 的
#   decide-m9a-run-tick，调用点/记账/靶子名与 adb 通道＝main.rs 执行侧(of_m9a／do_rescue／do_record／合体轮)。
# 刀㉕ 笔(c)(11-08)：六枚源文本格(先问可用性／预算写回跨拍／停滞兜成撤回调款／三种状态也问一次／Rescued 标记／
#   Detail 拼兜底账)＋外层重试两枚 Orchestration.cs 源文本格整批出仓；对面＝RunLoops.SelfTest.cs「树上没函数→
#   不问直接收」／「blocked 兜成：Rescued=true、状态名不改、Detail 带进程后缀+兜底账」／「静默超限有预算→stall1」
#   与 stall 解除话术／三档各一枚「也问一次」，以及 Orchestration.SelfTest.cs「blocked+Rescued→翻案重试，话术换
#   『视觉兜底把挡路的界面点掉了』」(整句等值)。账见 §11.316 与 raw\gdlogs\cut25c\。留下四枚接线面(靶子名/adb 通道、
#   visionMaxRescue 键、救火账 game/target、状态名负锚)——对面无同轴行为格，按「摘格不越轴」不跟着批走。
$rsRound = [regex]::Match($rustCore, '(?s)let mut m9a_scen = m9a_scen_core.*?m9a_scen\.grab_available = false').Value
$rsEnv9 = [regex]::Match($rustCore, '(?s)fn of_m9a.*?serial: sc\.serial\.clone\(\)').Value
$rsTick = [regex]::Match($rustCore, '(?s)let rescue_avail = h\.rescue_available\(\).*?st_m9a = jv_get\(&d, "State"\);').Value
$rsM9aRt = [regex]::Match($rustCore, '(?s)let rt = jv_get\(v, "runtime"\);\n    // mode 缺键.*?sc\.stall_min = jv_int\(&jv_get\(&rt, "stallTimeoutMinutes"\), 0\);').Value
$rsRecord = [regex]::Match($rustCore, '(?s)fn do_record.*?jv_s\("record"\)').Value
Check '兜底这几格的桌子都取到了窗口(尺接没接电，空串 -match 会假绿)' (
    $rsRound.Length -gt 0 -and $rsEnv9.Length -gt 0 -and $rsTick.Length -gt 0 -and $rsM9aRt.Length -gt 0 -and $rsRecord.Length -gt 0) $(
    'len: round=' + $rsRound.Length + ' env9=' + $rsEnv9.Length + ' tick=' + $rsTick.Length + ' rt=' + $rsM9aRt.Length + ' record=' + $rsRecord.Length)
Check '兜底调用点认 mumu 这枚靶子，且带上实例的 adb 通道(桌子＝main.rs 的 of_m9a 与合体轮)' (
    $rsEnv9 -match 'target: sc\.target\.clone\(\)' -and $rsEnv9 -match 'adb: sc\.adb\.clone\(\)' -and $rsRound -match 'm9a_scen\.target = "mumu"')
Check '次数上限沿用同一个键 runtime.visionMaxRescue(限的是"一轮里跟模型没完没了的次数")' (
    $rsM9aRt -match 'jv_get\(&rt, "visionMaxRescue"\)' -and $rsTick -match '"rescueMaxRaw"')
Check '这一路的救火账记在核里，game 归 r1999、target 归 mumu(do_record→add_rescue_record)' (
    $rsRecord -match 'add_rescue_record' -and $rsEnv9 -match 'game: sc\.game\.clone\(\)' -and $rsRound -match 'm9a_scen\.game = "r1999"')
Check '状态名一律没改(收尾与通知按状态名分流，造新名字等于把判因链弄断)' (
    $m9aLib -notmatch "Status = 'rescued'" -and $m9aLib -notmatch "Status = 'vision")
# 外层重试分流(刀㉕ 笔(c) 出仓)：判据与话术由 Orchestration.SelfTest.cs 整句等值钉死，本段不再用 -match
#   咬 Orchestration.cs。刀⑳ 笔(a)-1：原 if/else 门腹内 19 枚打模拟器后端件在场形状的格随件退役整段不执行
#   (死臂不是覆盖)⇒下面三枚核侧对面格升为无条件跑。
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
