# lib/m9a.ps1 — 配置并驱动 M9A(重返未来：1999 / MuMu + ADB)
#
# ── 冻结(2026-09-26，PS→C# 批次 3)────────────────────────────────────────────
# 本体已迁到 tools\panel-cs\Processes.cs(契约见 docs\ps-migration-batch0.md)：
#   Get-M9ALaunchArgs  → verb m9a-launch-args(一律长参数：短参数 -a 实测无效的老规矩在 C# 注释里)｜⚠ 10-26 第五刀刀②：这枚转发壳已从本文件出仓，映射本身照旧
#   Read-GdM9ALine     → verb parse-m9a-lines(单行走一次；解析顺序敏感，改文案改 C# 那份)
#   Read-M9ALogEvents  → verb parse-m9a-lines(整批文本一次转发，绝不逐行起进程)｜⚠ 10-26 第五刀刀②：这枚采集壳已从本文件出仓，整批取数的家只剩核内 m9a_read_log
#   Resolve-M9AStatus  → verb resolve-m9a-status(所有结论文案/优先级在 C#；
#                        本壳把 $State.lastLog 换算成 quietSeconds 递进去，Found:false 翻译回 $null)
# 所以：改日志解析/结论话术请改 Processes.cs 并跑 panel-cs --proctest 对真值，不要在这里加规则。
# 故意留在 PS 侧的：Get-M9APaths/Get-GdM9ACliEnabled(读配置)、Set-M9AInstance(写 appsettings+回读；⚠ 11-05 转接刀·运行面刀⑨ 该枚已出仓，本名单自此按史注读、不换算)、
# Get-M9AConnection(读实例 JSON)、Get-M9ADailyLog/Register-M9AEvent(状态累计+日志染色)、
# Invoke-M9ARun(起进程与轮询，归批次 5 编排层)。
#   ⚠ 10-25 甲档刀②：上面名单里只有这一枚不成立了——Invoke-M9ARun 整段出仓(原 319-528 共 210 行)，"故意留在 PS 侧"
#   的名单自此只剩前两行那些壳。生产面唯一真做入口 = 核内 m9a_run；逐字副本原钉 probe-m9arun 门，
#   ⚠ 10-26 甲档第十五刀：该门整件退役(等值面搬进 gdprobe --selftest 第 26 节，§11.151)，副本随门出仓。
# ──────────────────────────────────────────────────────────────────────────────
#
# ── 追加冻结(2026-09-27，PS→C# 批次 8 —— 活段第二刀)────────────────────────────
# Invoke-M9ARun 出结论那一拍的**反应段判据梯**(ok 完成整句+挂钟时长 / empty-queue 原句 ERROR /
#     其余档拼进程后缀 → "五档问一次画面"门槛 → 兜底闭包(预算扣数、调用/记账 Tag 同数、
#     兜-账-志次序) → 兜成分流(停滞撤回结论续等 / 判因立 Rescued 标记) → 收尾拼串)
#     已迁到 tools\panel-cs\RunLoops.cs(verb：decide-m9a-run-tick)。口径见 docs\ps-migration-live-caliber.md，
#     剧本对拍 = tools\panel-cs\build.ps1 -Verify 的 --looptest(基线钉 c498da0；原 live 门 10-26 摘件)。
# 本壳留下的正是口径文档划给"活"的部分：while+sleep、tail 轮询、adb 探活、Resolve-M9AStatus 出结论(批 3)、
#     真兜底/真记账、回执喂回+同 tick 重入的迷你循环、把结论回写 $res。
# 旧的 $rescue/$askScreen 与反应段曾在下方逐行冻结 —— ⚠ 10-25 甲档刀② 点名推翻这句的后半：
#     ① 逐行冻结段(原 378-399/483-527)随真身一起出仓，本文件已无该段；
#     ② 那句"test-r1999 按那些字面钉锚(预算扣数写法、'判因'、$state.lastLog、Rescued、收尾拼串)"现读已不实——
#        那九格正向文本锚于刀①-b(本仓 0ac6dcb)改打 tools\panel-cs\RunLoops.cs ＋ tools\rustcore\src\main.rs 两张活桌，
#        现树 lib\m9a.ps1 只剩 test-r1999:480 那一枚**否定式**($m9aLib -notmatch)还在读。
#     ⇒ "别删别改字"因此不再是本段约束：两刀制的第二刀就是"先让门不需要真身，再摘真身"，本笔即第二刀。
# 改判据/话术请改 RunLoops.cs 并复拍对拍门，不要在这里加 if。
# (批 3 头注"Invoke-M9ARun(起进程与轮询，归批次 5 编排层)"到批 8 作废一半(轮询里的判据已走，骨架还在)；
#  ⚠ 10-25 甲档刀② 起作废全部：骨架也出仓了，本文件不再持有那枚轮询。预算与 rescueNotes 的跨拍寿命现只住在
#  核内 state(RunLoops.cs 的 rescueLeft/notes ＋ main.rs 的 m9a_scen)，旧 $stM9a opaque 中继随真身同批出仓。)
# ──────────────────────────────────────────────────────────────────────────────
#
# ── 追加冻结(2026-09-27，PS→C# 批次 9 —— 活段第三批：事件状态机回灌)────────────
# Register-M9AEvent 十三档状态机的本体(tasks 的 @($null) 占位累计、queue-prepared 转数+原文话术、
#     task-fail 累计计数、res 版本差 WARN、connect-fail 只留首条、finish 分流、unknown 只加
#     eventCount 不 touch lastLog)已搬进 tools\panel-cs\RunLoops.cs(verb：apply-m9a-events)。
# 壳留三件事：摘 state 十二键(lastLog 是挂钟量不进信封) → 一次转发 → 回写 State + 按
#     Directives.TouchLastLog 置 $State.lastLog = Get-Date。Invoke-M9ARun 轮询改用批量入口
#     Register-M9AEvents(批 3 省钱点延伸：一轮一个进程，绝不每事件一个)。
#     ⚠ 10-25 甲档刀②：那句里"Invoke-M9ARun 轮询"这个读者已随真身出仓。本枚批量入口现读只剩两处在调——
#     本文件 :169 那发单事件壳转发(表账 ps-map 第六列 1:0 就是它；⚠ 10-26 第五刀刀② 现读：原写 :216 本就漂了两格，摘件后同段再 −49)。门内那枚逐字副本已随 10-26 甲档第十五刀摘门出仓。
# (批 3 头注里"故意留在 PS 侧的 Register-M9AEvent(状态累计+日志染色)"到本批作废：状态累计本体已走，
#  壳只剩转发与染色。旧十三档逐行 # 仍冻在原位(那一段没随刀② 出仓)；⚠ 但原句"test-r1999 的 `$state.lastLog = Get-Date`、
#  各档话术字面锚在冻结行里 substring 照命中，别删别改字"已于刀①-b(本仓 0ac6dcb)作废——正向锚九格改打 RunLoops.cs／main.rs
#  两张活桌，本段不再是任何门的钉家(现读全仓读现树本文件的只剩 test-r1999:480 那枚否定式；probe-m9arun 的 dot-source 已随 10-26 甲档第十五刀摘门消失)。)
# ──────────────────────────────────────────────────────────────────────────────

function Get-M9APaths {
    param([Parameter(Mandatory)]$S)
    return [pscustomobject]@{
        Dir        = $S.paths.m9aDir
        Exe        = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aExe))
        AppSettings= (Join-GdPath $S.paths.m9aDir @($S.paths.m9aAppSettings))
        FrameLog   = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aFrameworkLog))
        LogDir     = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aLogDir))
        Name       = [System.IO.Path]::GetFileNameWithoutExtension($S.paths.m9aExe)
    }
}

function Get-GdM9ACliEnabled {
    <# 默认走命令行入口；settings 里显式写 false 才回退裸启动 #>
    param([Parameter(Mandatory)]$S)
    if ($null -eq $S.runtime.m9aCliArgs) { return $true }
    return [bool]$S.runtime.m9aCliArgs
}

# Set-M9AInstance 已删(11-05 转接刀·运行面刀⑨)：全仓 AST 零调用点(k853 正则尺＋k854/k855 名册尺同帧对平)，
# 对面在 tools/panel-cs/Chain.cs::ApplyM9AInstance(verb apply-m9a-instance，Round.SelfTest 第 29 节有格)，
# 表账档位词「已承接」；本文件头注那份"故意留在 PS 侧"的名单自此少一枚，按史注读不换算。
function Get-M9AConnection {
    <# 从实例配置里取 adb 路径 + 序列号，避免把端口写死 #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance)
    $cfg = Read-GdJson -Path $Instance.Path
    $dev = $cfg.AdbDevice
    if (-not $dev -or -not $dev.AdbSerial) { throw "实例[$($Instance.Name)]里没有 AdbDevice.AdbSerial，无法探活" }
    $adb = [string]$dev.AdbPath
    if (-not $adb -or -not (Test-Path -LiteralPath $adb)) { $adb = (Get-GdMuMu -S $S).Adb }
    return [pscustomobject]@{ Adb = $adb; Serial = [string]$dev.AdbSerial; Name = [string]$dev.Name }
}

function Get-M9ADailyLog {
    <# M9A 每天一个日志：logs\log-yyyyMMdd.log(运行中可能跨天，每次轮询重算) #>
    param([Parameter(Mandatory)]$P)
    return Join-Path $P.LogDir "log-$(Get-Date -Format 'yyyyMMdd').log"
}

function Read-GdM9ALine {
    <#
      转发壳(parse-m9a-lines 的单行入口)：一行 → 至多一个事件。
      实测语义/顺序敏感点(如 res-latest 必须排在 noise 前)全部在 Processes.cs 的 ParseM9aLine 注释里。
      返回形状与旧实现一致：$null 或 @{Kind;Text}(Register-M9AEvent 吃 hashtable，别改成对象)。
      注意：轮询路径上的整批转发壳 Read-M9ALogEvents 已于 10-26 第五刀刀② 出仓(那一路的家只剩核内 m9a_read_log)，这里只给别人手动解析单行用。
    #>
    param([Parameter(Mandatory)][string]$Line, [string]$InstanceId = '')
    $d = Invoke-GdCli -Verb 'parse-m9a-lines' -Body ([pscustomobject]@{ text = $Line; instanceId = $InstanceId })
    $evs = @($d.Events)
    if ($evs.Count -eq 0) { return $null }
    return @{ Kind = [string]$evs[0].Kind; Text = [string]$evs[0].Text }
}

function Register-M9AEvents {
    <#
      批次 9：十三档状态机本体在 tools\panel-cs\RunLoops.cs(verb：apply-m9a-events)。
      壳只干三件事：摘 state 十二键(lastLog 是挂钟量，不进信封) → 整批一次转发 →
      回写 State + 按 Directives.TouchLastLog 置 $State.lastLog = Get-Date。
      批量入口给 Invoke-M9ARun 轮询用(一轮一个进程，绝不每事件一个 —— 批 3 省钱点延伸)。
      三个 PS 语义坑(tasks 的 @($null) 占位 / failedTasks 的 $null -eq @() 塌缩 /
      res-latest 的 -ne 不分大小写 / 不认识的 Kind 只加 eventCount 不 touch)全在 C# 逐字复刻。
    #>
    param([Parameter(Mandatory)][hashtable]$State, $Events)
    $evs = @($Events)
    if ($evs.Count -eq 0) { return }
    $st = @{}
    foreach ($k in @('tasks', 'chainStarted', 'chainDone', 'connectPhase', 'failed', 'queueCount',
                     'startRequested', 'eventCount', 'failedTasks', 'resCurrent', 'resLatest', 'connectFail')) {
        if ($State.ContainsKey($k)) { $st[$k] = $State[$k] }
    }
    $d = Invoke-GdCli -Verb 'apply-m9a-events' -Body ([pscustomobject]@{ events = $evs; state = $st })
    foreach ($l in @($d.Logs)) {
        $lvl = [string]$l.Level
        if ($lvl -and ($lvl -ne 'INFO')) { Write-GdLog ([string]$l.Text) -Level $lvl } else { Write-GdLog ([string]$l.Text) }
    }
    foreach ($sp in $d.State.PSObject.Properties) { $State[[string]$sp.Name] = $sp.Value }
    if ($d.Directives.TouchLastLog) { $State.lastLog = Get-Date }
}

function Register-M9AEvent {
    <#
      单事件转发壳(test-r1999 钉着签名：$State hashtable 原地更新、无返回值)。
      旧十三档本体在下方逐行 # 冻结(PS 块注释不可嵌套)——
      ⚠ 10-25 甲档刀①-b(本仓 0ac6dcb)改口：原句"test 锚 `$state.lastLog = Get-Date` 与各档话术字面在冻结行里
      substring 照命中，别删别改字"已不实，那九格正向锚改打 RunLoops.cs／main.rs 两张活桌，本段不再被任何门逐字比。
    #>
    param([Parameter(Mandatory)][hashtable]$State, [Parameter(Mandatory)][hashtable]$Event)
    Register-M9AEvents -State $State -Events $Event
}

    # ===== 批次 9 冻结段：旧 Register-M9AEvent 十三档(c498da0 原位备查) =====
# function Register-M9AEvent {
#     <# 把一个事件累计进状态包(纯状态更新，方便进程退出后再补读一次) #>
#     param([Parameter(Mandatory)][hashtable]$State, [Parameter(Mandatory)][hashtable]$Event)
#     # 只要读进来一个"认识"的事件就计数：eventCount 一直是 0 = 日志压根看不懂了(见 Resolve-M9AStatus 的 unknown-format)
#     $State.eventCount = [int]$State.eventCount + 1
#     switch ($Event.Kind) {
#         'task' {
#             $State.chainStarted = $true
#             $State.tasks = @($State.tasks) + $Event.Text
#             Write-GdLog "M9A 进行中[$(@($State.tasks).Count)] $($Event.Text)"
#             $State.lastLog = Get-Date
#         }
#         'chain-start' { $State.chainStarted = $true; $State.lastLog = Get-Date }
#         'chain-done'  { $State.chainDone = $true; $State.lastLog = Get-Date }
#         'queue-prepared' {
#             $State.queueCount = [int]$Event.Text
#             if ([int]$Event.Text -gt 0) { $State.chainStarted = $true }
#             Write-GdLog "M9A 已提交任务队列：任务数量=$($Event.Text)"
#             $State.lastLog = Get-Date
#         }
#         'ui-start' { $State.startRequested = $true; $State.lastLog = Get-Date }
#         'connect-start' { $State.connectPhase = 'started'; $State.lastLog = Get-Date }
#         'finish' {
#             <#
#               「启动任务完成」有两种含义，靠上下文区分：
#                 已经看到过任务/队列 → 这就是整条链路跑完(--quit-after-run 模式下唯一的完成信号)
#                 什么都没看到        → 只完成了连接，M9A 自己也没开始任务队列
#             #>
#             if ($State.chainStarted -or (@($State.tasks).Count -gt 0)) { $State.chainDone = $true }
#             else { $State.connectPhase = 'done' }
#             $State.lastLog = Get-Date
#         }
#         'fail' { if (-not $State.failed) { $State.failed = $Event.Text }; $State.lastLog = Get-Date }
#         'task-fail' {
#             <#
#               「任务失败：<中文名>」是 M9A 对单个任务的判失败，一轮里可能连着打好几条。
#               只把第一条塞进 $State.failed 是不够的：真正决定这轮可信度的是
#               「第一个任务(启动游戏)有没有失败」——它失败说明后面全是空转，见 Resolve-M9AStatus 的 blocked。
#             #>
#             <#
#               两个坑叠在这段上，写成 "$ft = if(...){@()}else{...}; $State.x = $ft + $t" 会静默退化成字符串拼接：
#                 1) 空数组经 if/管道赋给变量会塌成 $null，而 $null + 'a' 是**字符串** 'a'(它的 Count 永远是 1)；
#                 2) @($null) 的长度又是 1，键没初始化时不能直接 @(...) 包。
#               两条分支都用"直接赋值 + 数组字面量"，不经过管道。
#             #>
#             $t = [string]$Event.Text
#             if ($null -eq $State.failedTasks) { $State.failedTasks = @($t) }
#             else { $State.failedTasks = @($State.failedTasks) + $t }
#             Write-GdLog "M9A 任务失败：$($Event.Text)(累计 $($State.failedTasks.Count) 个)" -Level WARN
#             if (-not $State.failed) { $State.failed = "任务失败：$($Event.Text)" }
#             $State.lastLog = Get-Date
#         }
#         'res-current' { $State.resCurrent = $Event.Text; $State.lastLog = Get-Date }
#         'res-latest' {
#             $State.resLatest = $Event.Text
#             if ($State.resCurrent -and ($State.resLatest -ne $State.resCurrent)) {
#                 Write-GdLog "M9A 资源/程序有新版：当前 $($State.resCurrent) → 最新 $($State.resLatest)(整包更新，命令行没有更新入口，需在 M9A 界面点『更新』)" -Level WARN
#             }
#             $State.lastLog = Get-Date
#         }
#         'connect-fail' {
#             if (-not $State.connectFail) {
#                 $State.connectFail = $Event.Text
#                 Write-GdLog "M9A 连不上模拟器：$($Event.Text)" -Level WARN
#             }
#             $State.lastLog = Get-Date
#         }
#         'noise' { $State.lastLog = Get-Date }
#     }
# }

function Resolve-M9AStatus {
    <#
      转发壳(resolve-m9a-status)：八种 Status 的优先级与全部结论文案都在 Processes.cs 的 M9aStatus 本体里
      (blocked→ok→empty-queue→unknown-format→error→timeout→not-started→stalled，注释跟着代码走)。
      壳只干三件事：
        1) $State 拷一份、摘掉 lastLog —— DateTime 不进信封，C# 侧也不看挂钟；
        2) 把 ((Get-Date) - $State.lastLog).TotalSeconds 算成 quietSeconds 递进去(停滞判定与"几分钟"文案吃它)；
        3) 拆信封：Found:false = 还在跑 → 回 $null(与批次 2 探测壳同一协议)；blocked 才带 FailedTasks 键。
    #>
    param(
        [Parameter(Mandatory)][hashtable]$State,
        [Parameter(Mandatory)][double]$Elapsed,
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)]$Instance,
        [bool]$ProcessGone = $false,
        [string]$LogName = ''
    )
    $st = @{}
    foreach ($k in $State.Keys) { if ($k -ne 'lastLog') { $st[$k] = $State[$k] } }
    $d = Invoke-GdCli -Verb 'resolve-m9a-status' -Body ([pscustomobject]@{
        state               = $st
        elapsed             = $Elapsed
        quietSeconds        = ((Get-Date) - $State.lastLog).TotalSeconds
        processGone         = [bool]$ProcessGone
        logName             = $LogName
        instanceName        = [string]$Instance.Name
        instanceId          = [string]$Instance.Id
        startTimeoutMinutes = [int]$S.runtime.r1999StartTimeoutMinutes
        timeoutMinutes      = [int]$S.runtime.r1999TimeoutMinutes
        stallTimeoutMinutes = [int]$S.runtime.stallTimeoutMinutes
    })
    if (-not $d.Found) { return $null }
    $r = @{ Status = [string]$d.Status; Detail = [string]$d.Detail; Tasks = @($d.Tasks) }
    if ($null -ne $d.PSObject.Properties['FailedTasks']) { $r.FailedTasks = @($d.FailedTasks) }
    return $r
}

# 10-25 甲档刀② 摘件：Invoke-M9ARun 的 PS 真身(原 319-528 共 210 行)整段出仓。
#   生产面本来就没经过它——唯一的真做入口是核内 m9a_run(tools\rustcore\src\main.rs，走 --m9a-run／--daily-round)，
#   反应段判据与话术的家在 tools\panel-cs\RunLoops.cs 的 decide-m9a-run-tick；当时那句"两枚转发壳在本文件上方原位保留、
#   只转发不判据"已被 10-26 甲档第五刀刀② 推翻：Get-M9ALaunchArgs(原 72-91)与 Read-M9ALogEvents(原 156-182)生产面零调用，随真身同批出仓，逐字副本住进两扇 golden 门的 child。
#   逐字副本原钉在 probe-m9arun 门(两扇 golden 门都已退役：events 门甲档第十四刀、m9arun 门第十五刀 10-26 摘件，
#   28 档等值面搬进 gdprobe --selftest 第 26 节，§11.151)；test-r1999 [t18] 那九格文本锚已于刀①-b 搬去两张活桌，不再咬这里。

function Stop-M9A {
    param([Parameter(Mandatory)]$S)
    $P = Get-M9APaths -S $S
    if ($S.runtime.quitAssistantAfter) {
        $n = Stop-GdProcessTree -Name $P.Name
        if ($n) { Write-GdLog "已关闭 M9A($n 个进程)" }
    }
}
