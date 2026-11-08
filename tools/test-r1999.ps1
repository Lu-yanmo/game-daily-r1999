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

# ---- B-3②-d 刀①(10-29)静态锚：Get-R1999Event 的取数半段"两臂都在"，钉在 m9aDir 那道现场门之前 ----…（细节见该帧）
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
# 诚实登记(刀㉓ 笔(a) 2026-11-08)：这一段原先那八枚行为判定面(未确认→pending／确认后→event／领空→idle／提示语不带…（细节见该帧）
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
    $evName1b = [string](Get-GdEntryVerb -Game 'r1999' -Verb 'Ack')
    Check "现问到的开关名 [$($evName1b)] 不渗进未确认的提示语(出现即红)" (($x1b.Reason -notmatch [regex]::Escape($evName1b)) -and ($x1b.Reason -match '本轮跑长草方案')) $x1b.Reason
} finally {
    Restore-GdTestState
}
Check 'state 文件已还原' (Test-GdTestStateRestored)

Write-Host "`n[t10] M9A 日志事件解析(用真实日志行)"
# 刀㉕ 笔(d)：原「解析器本体仍只有一处」格出仓——判据只是 ParseM9aLine 出现 ≥1 次，证不了"只有一处"、且只是源文本在场；…（细节见该帧）
$procCsST = [string](Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'panel-cs/Processes.SelfTest.cs'))
$lineCells = ([regex]::Matches($procCsST, 'b\.Chk\("line(?:s)?/')).Count
Check "核侧 LineGroup 的 line/ 格枚数不缩水(现拍 $lineCells 枚，门槛 16)" ($lineCells -ge 16) "=$lineCells"
Check 'settings 里 1999 起跑超时已收紧' ([int]$S.runtime.r1999StartTimeoutMinutes -le 12) "=$($S.runtime.r1999StartTimeoutMinutes) 分钟"

Show-GdCommonTailChecks

Write-Host "`n[t12] M9A 命令行脚本化入口 + 结果判定"
Check 'settings 里三个 M9A 开关都在' ($null -ne $S.runtime.m9aCliArgs -and $null -ne $S.runtime.m9aInstanceArg -and $null -ne $S.runtime.m9aForceStart)
# 「默认走命令行入口」那格的行为判据随 Get-GdM9ACliEnabled 出仓(11-06 刀⑫ 笔(b))，改指下面 [t16] 段 $rustCore 那张活桌


#   ＋整串等值吃掉「绝不使用短参数」)。unknown-format 两支(新版文案返回 $null→line/unknown-2；零标志不谎称…（细节见该帧）

#   · 搬靶两格(更新闸／审计闸)刀⑧ 再搬：起子进程＋挑行＋拼串在 Gate.Host.cs(verb run-m9a-update／…（细节见该帧）
$r1999RoundPs = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\rust-round.ps1') -Raw -Encoding utf8
$gateCs = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Gate.Host.cs') -Raw -Encoding utf8
$rustCore = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'rustcore\src\main.rs') -Raw -Encoding utf8
# 11-06 刀⑫ 笔(b)：m9a.ps1 那枚三态壳出仓后，「没写就算开」这一档全仓只有一处会答，格就打核内那张活桌(不联网、不起进程)
Check '命令行入口三态判据住在核内 --selftest 第 21 节(壳摘后 PS 侧没有第二份答案；回潮＝两份各答一档)' (
    ($rustCore.Contains('let cli21 = |extra: &str|')) -and ($rustCore.Contains('runtime.m9aCliArgs 的三态塌成一态')))
# M9A 自动更新闸(tools\m9a-update.ps1，2026-09-30 用户「检查到更新自己更就行了」)：这里只做静态锚……（细节见该帧）
Check '1999 起跑段挂了 M9A 自动更新闸(调 tools\m9a-update.ps1 -Apply，失败只 WARN；11-04 笔(c/d) 搬靶到轮档件，刀⑧ 再搬进 Gate.Host.cs)' (
    ($gateCs -match 'GateRun\(raw, @"tools\\m9a-update\.ps1"') -and ($gateCs -match '-Apply') -and ($rustCore -match 'M9A 自动更新未完成'))
Check '两把闸的先后还在：更新闸排在审计闸之前(刀⑧ 起靶＝核内链体两处调用点的文本序；Gate.Host.cs 里那两枚臂的定义序不决定跑谁先跑，不当判据)' (
    $rustCore.IndexOf('fx.updgate()') -ge 0 -and $rustCore.IndexOf('fx.audit()') -ge 0 -and
    $rustCore.IndexOf('fx.updgate()') -lt $rustCore.IndexOf('fx.audit()'))
Check '更新器本体在位且退出码契约写在头注' ((Test-Path -LiteralPath (Join-Path $PSScriptRoot 'm9a-update.ps1')) -and ((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'm9a-update.ps1') -Raw -Encoding utf8) -match '0 = 已是最新'))
# 活动打完没打完的 agent 判定(tools\1999-event-audit.ps1，2026-09-30 用户「让agent判断」)：同样只做静态锚……（细节见该帧）
$r1999Audit = Get-Content -LiteralPath (Join-Path $PSScriptRoot '1999-event-audit.ps1') -Raw -Encoding utf8
# 11-06 刀⑮ 笔(c) 随动：剥注释尺上移到本格之前——审计工具的截屏已改问核 mumu-screen-shot……（细节见该帧）
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
# 11-04 刀⑦ 笔(b) 改指：审计的 ack 不再叫编排岛那枚壳，改问核(Round.Data.cs::R1999Ack)。正向半抓"改指没落地"……（细节见该帧）
Check '审计的 ack 那一步问的是核，且件内不复写确认话术(11-04 刀⑦ 笔(b) 改指)' (
    ($r1999AuditCode -match "Invoke-GdCli -Verb 'set-r1999-ack'") -and ($r1999Audit -notmatch '已确认 1999 活动'))
# 2026-10-27 第十六刀从 probe-m9adrive 等值门搬来的三格(ask 白名单七枚／'run' case 不许复活／role 字面)已随…（细节见该帧）
$r1999ModulePs = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\r1999-module.ps1') -Raw -Encoding utf8
$r1999IslandBad = @()
foreach ($r1999IslandFn in @('Set-GdR1999ShopState', 'Resolve-GdR1999Decision', 'Show-GdR1999Preview', 'Start-GdR1999Daily', 'Get-GdM9ADriveExe', 'Invoke-GdR1999DailyRust', 'Set-GdR1999Ack')) {
    if ($r1999RoundPs.Contains('function ' + $r1999IslandFn + ' ') -or $r1999ModulePs.Contains('function ' + $r1999IslandFn + ' ')) { $r1999IslandBad += $r1999IslandFn }
}
Check '编排岛真身(含笔(b) 那枚 ack 壳)的定义不在轮档件与清单件里(名册逐枚现扫；回潮＝核内一份＋PS 一份那本双账复活)' ($r1999IslandBad.Count -eq 0) ($r1999IslandBad -join ',')
# 转接刀·运行面刀⑨ 的「退役不回潮」负锚（形状照 test-ark 那两格）：Set-M9AInstance（写 M9A appsettings 的…（细节见该帧）
$cut9M9APs = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\m9a.ps1') -Raw -Encoding utf8
Check '刀⑨ 那枚 Set-M9AInstance 的定义不在 m9a.ps1(写实例配置只住核内 Chain.cs 一处；回潮＝PS 一份＋核内一份那本双账复活)' (
    -not $cut9M9APs.Contains('function Set-M9AInstance '))
# 转接刀·运行面刀⑫ 负锚(形状同上面刀⑨ 那格，共用 $cut9M9APs 原文)：针只取'function 名 '那形(裸串/…（细节见该帧）
$cut12M9ABad = @()
foreach ($cut12Needle in @('function Stop-M9A ', 'switch ($Event.Kind)', 'function Get-M9APaths ', 'function Get-GdM9ACliEnabled ', 'function Get-M9ADailyLog ')) {
    if ($cut9M9APs.Contains($cut12Needle)) { $cut12M9ABad += $cut12Needle }
}
Check '刀⑫ 笔(a)(b)(c) 出仓的 Stop-M9A 壳、冻结段体特征串与三枚读配置壳的定义行都不在 m9a.ps1(verb stop-m9a 与 paths／三态各只住一处；回潮＝史注之外再多一具身体)' (
    $cut12M9ABad.Count -eq 0) ($cut12M9ABad -join ' | ')


Write-Host "`n[t18] 1999 这一路也接上视觉兜底(2026-09-26 加强 AI 干预；全程不联网、不起 adb、不点任何东西)"
# 为什么 1999 也要接：M9A 和 MAA 得的是同一种病 —— 模板集里没有的界面它看不见。
$mumuLibPath = Join-Path $PSScriptRoot '..\lib\vision-mumu.ps1'
$mumuLibOnDisk = Test-Path -LiteralPath $mumuLibPath
Check '模拟器后端件与 New-GdVisionTarget-mumu 命令同一个事实(同在同不在，半迁移红)' (
    $mumuLibOnDisk -eq [bool](Get-Command New-GdVisionTarget-mumu -ErrorAction SilentlyContinue)) "件=$mumuLibOnDisk"
$m9aLib = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\lib\m9a.ps1') -Raw -Encoding utf8
# ⚠ 10-25 甲档刀①-b：下面那九格正向文本锚也从 lib\m9a.ps1 搬走了，$m9aLib 只剩最后那格**否定式**在用…（细节见该帧）
function Get-GdTestCodeOnly {
    param([string]$Text)
    return (($Text -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' ')
}
# 内核两路共用，不许认识任何一端的专有东西。2026-11-08 刀㉑ 笔(c)：原「不认识 adb／端游窗口」格连同三行取数…（细节见该帧）
$askCode = Get-GdTestCodeOnly (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'vision-ask.ps1') -Raw -Encoding utf8)
Check '"这棵树上没有这一路"那句话只由核内说一次，PS 薄壳的代码面不再自己拼这句(刀⑮ 笔(c) 起；头注里的说明句不算)' (
    ((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'panel-cs\Live.cs') -Raw -Encoding utf8) -match '这棵树上没有') -and
    ($askCode -notmatch '这棵树上没有') -and
    ($askCode -match 'resolve-rescue-context'))
# 2026-11-08 刀㉑ 笔(a) 死臂随批摘：上面的守卫件 lib\vision-rescue.ps1 已整枚出仓且名册零登记 ⇒ 真臂恒不执行，
# 摘真臂、else 支升为唯一形(它本来就在承接)；这不触"摘而不搬"红线——负锚量的"编排没复活"就是现树的活判据。
$rescueConvTooth = (-not (Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue))
Check '内核按 New-GdVisionTarget-<kind> 的命名约定拼函数名找后端，找不到就回 $null(刀㉑ 起此格唯一形＝编排没复活)' $rescueConvTooth "复活=$([bool](Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue))"
# 接线：这一路按"进程还在不在"分两类接上。10-25 刀①-b 换桌：判据/话术/扣数/分流＝RunLoops.cs 的…（细节见该帧）
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
# 外层重试分流(刀㉕ 笔(c) 出仓)：判据与话术由 Orchestration.SelfTest.cs 整句等值钉死，本段不再用 -match…（细节见该帧）
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

# 刀⑱ 笔(a)「退役不回潮」负锚：Get-M9AConnection 整枚出仓(b5 扫描尺现拍代码面 LIVE=0)……（细节见该帧）
Check '刀⑱ 那枚 Get-M9AConnection 的定义不在 m9a.ps1(探活只住核内 Chain.cs::ReadM9AConnection 一处；回潮＝PS 一份＋核内一份那本双账复活)' (
    -not $cut9M9APs.Contains('function Get-M9AConnection '))

$res = Show-GdTestResult -Tag 'r1999'
if ($NoExit) { return $res }
exit $(if ($res.Fail) { 1 } else { 0 })
