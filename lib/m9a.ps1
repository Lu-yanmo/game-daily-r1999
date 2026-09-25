# lib/m9a.ps1 — 配置并驱动 M9A(重返未来：1999 / MuMu + ADB)

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

function Get-M9ALaunchArgs {
    <#
      M9A 自带的脚本化入口(帮助文本原文)：
        -i, --instance <instance>   指定启动的实例
        -a, --autostart             自动开始任务
        -f, --force-start           配合 -i：停掉现有任务，按命令行重新连接并开始，不读上次任务状态
        -q, --quit-after-run        配合 -i：任务结束后退出程序，"用于脚本化调度"
      M9A 已在运行时，这条命令会被转发给正在跑的实例(日志里是"转发启动命令…")。

      实测坑(09-23 两轮对比)：**短参数 -a 会被原样记成 key "a"，自动开始判定匹配不上，队列根本不提交；
      长参数 --autostart 才会打出「用户操作：启动任务 / 准备执行任务队列：任务数量=N」**。
      (-i/--instance 两种写法都能被识别，但为了不再踩同一个坑，这里一律用长参数。)
    #>
    param(
        [Parameter(Mandatory)]$Instance,
        [string]$Mode = 'id',
        [bool]$ForceStart = $true,
        [bool]$CliArgs = $true
    )
    if (-not $CliArgs) { return @() }
    $value = if ($Mode -eq 'name') { [string]$Instance.Name } else { [string]$Instance.Id }
    $out = @('--instance', $value, '--autostart', '--quit-after-run')
    if ($ForceStart) { $out += '--force-start' }
    return $out
}

function Set-M9AInstance {
    <# 把 M9A 的"当前实例"切到指定 id(命令行 -i 之外的第二道保险：人工打开 M9A 时也能看到对的实例) #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance, [switch]$Apply)

    $P = Get-M9APaths -S $S
    if (Test-GdProcessRunning -Name $P.Name) {
        if (-not (Get-GdM9ACliEnabled -S $S)) { throw 'M9A 正在运行，请先关闭再改实例' }
        Write-GdLog 'M9A 正在运行：改实例这一步跳过，启动时由命令行 -i/-f 接管(会转发给正在跑的实例)' -Level WARN
        return [pscustomobject]@{ Changes = @('(M9A 在跑，交给命令行 -i 转发)'); Instance = $Instance; Applied = $false }
    }
    if (-not (Test-Path -LiteralPath $P.AppSettings)) { throw "M9A appsettings 不存在：$P.AppSettings" }

    $cfg = Read-GdJson -Path $P.AppSettings
    $cur = [string]$cfg.'Instances.LastActive'
    $changes = @()
    if ($cur -ne $Instance.Id) { $changes += "Instances.LastActive: $cur → $($Instance.Id)($($Instance.Name))" }
    if ([string]$cfg.'NoAutoStart' -ne 'False') { $changes += "NoAutoStart: $($cfg.'NoAutoStart') → False(启动即自动开始)" }

    if ($Apply -and $changes.Count -gt 0) {
        $bak = Backup-GdFile -Path $P.AppSettings
        $cfg | Add-Member -NotePropertyName 'Instances.LastActive' -NotePropertyValue $Instance.Id -Force
        $cfg | Add-Member -NotePropertyName 'Instances.LastActiveName' -NotePropertyValue $Instance.Name -Force
        $cfg | Add-Member -NotePropertyName 'NoAutoStart' -NotePropertyValue 'False' -Force
        Write-GdJson -Path $P.AppSettings -Object $cfg -Depth 10
        $verify = Read-GdJson -Path $P.AppSettings
        if ([string]$verify.'Instances.LastActive' -ne $Instance.Id) { throw "M9A 实例切换回读失败(备份：$(Split-Path -Leaf $bak))" }
        Write-GdLog "M9A 实例已切到[$($Instance.Name)](备份：$(Split-Path -Leaf $bak))" -Level OK
    }
    if ($changes.Count -eq 0) { $changes = @("(已是 $($Instance.Name)，无需改动)") }
    return [pscustomobject]@{ Changes = $changes; Instance = $Instance; Applied = [bool]$Apply }
}

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
      把一行 M9A 日志解析成事件：
        $null / @{Kind=task|queue-prepared|ui-start|chain-start|chain-done|finish|connect-start|fail|task-fail|res-current|res-latest|connect-fail|noise; Text=...}
      实测语义(对照 08-30 手工成功日志 + 09-23 命令行真跑日志)：
        用户操作：启动任务          = 开始命令真的下发了(命令行 --autostart 走的就是这条路)
        准备执行任务队列：任务数量=N = 队列已提交，N 是任务数
        队列任务开始                = 每个任务开跑前都会打一次
        开始任务：<中文名>          = 单个任务开始
        启动任务开始                = 「连接模拟器 + 起 Agent」这一步开始
        启动任务完成 / 任务已全部完成 = 整条链路结束。两种模式形态不同：
          · 手工界面跑：结束时会打「任务已全部完成！」，进程继续留着
          · 命令行带 --quit-after-run：只打「启动任务完成」+「停止前状态：SUCCEEDED」，
            紧接着「应用已关闭」——**根本没有「任务已全部完成！」这一行**(实测 09-23 17:43)
        所以「启动任务完成」必须按上下文判断：前面已经跑过任务 = 链路完成；否则只是连上了模拟器。
        这条判断在 Register-M9AEvent 里做，解析函数只负责给出中性的 'finish' 事件。
    #>
    param([Parameter(Mandatory)][string]$Line, [string]$InstanceId = '')
    if ($InstanceId -and ($Line -match '\[inst=') -and (-not $Line.Contains("/$InstanceId]"))) { return $null }
    if ($Line -match '开始任务：(.+?)\s*$') { return @{ Kind = 'task'; Text = $Matches[1].Trim() } }
    if ($Line -match '准备执行任务队列：任务数量=(\d+)') { return @{ Kind = 'queue-prepared'; Text = $Matches[1] } }
    if ($Line -match '用户操作：启动任务') { return @{ Kind = 'ui-start'; Text = '' } }
    if ($Line -match '队列任务开始') { return @{ Kind = 'chain-start'; Text = '' } }
    if ($Line -match '任务已全部完成') { return @{ Kind = 'chain-done'; Text = '' } }
    if ($Line -match '启动任务完成') { return @{ Kind = 'finish'; Text = '' } }
    if ($Line -match '启动任务开始') { return @{ Kind = 'connect-start'; Text = '' } }
    if ($Line -match '任务运行失败|停止前状态：FAILED') {
        return @{ Kind = 'fail'; Text = (($Line -replace '^.*?op=\w+\]\s*', '') -replace '>>>\s*任务\s*', '').Trim() }
    }
    <#
      下面三条是 2026-09-25 19:24 那轮之后补的，当时 10 个任务里 8 个都在识别失败，
      我们却一路报「M9A 进行中[1..8]」、最后只给出「进程提前退出」——因为这三类行根本没进解析器：
        任务失败：<中文名>        每个识别不出界面的任务都会打一行(旧解析器只认「任务运行失败」，差一个字)
        当前/最新资源版本：vX     M9A 自己报的版本差(实测 当前 v4.7.1 / 最新 v4.10.0)；
                                 「最新资源版本」含「新资源版本」子串，会被下面 noise 那条吞掉，所以必须排在 noise 前面
        初始化控制器失败/连接失败  连不上模拟器的硬信号(实测 19:29：AdbSerial 为空、按指纹匹配到的设备数量=0)
    #>
    if ($Line -match '任务失败：(.+?)\s*$') { return @{ Kind = 'task-fail'; Text = $Matches[1].Trim() } }
    if ($Line -match '当前资源版本：(\S+)') { return @{ Kind = 'res-current'; Text = $Matches[1] } }
    if ($Line -match '最新资源版本：(\S+)') { return @{ Kind = 'res-latest'; Text = $Matches[1] } }
    if ($Line -match '初始化控制器失败|连接模拟器时发生错误|按指纹匹配到的设备数量：0|连接任务已在重试耗尽后结束|\[Record\]\s*连接失败') {
        # 这类行有的带 [op=...]、有的只有 [cfg=Default]，统一把开头连续的 [...] 段剥掉再当原因用
        return @{ Kind = 'connect-fail'; Text = (($Line -replace '^(\[[^\]]*\]\s*)+', '') -replace '^>>>\s*任务\s*', '').Trim() }
    }
    if ($Line -match '截图超时|Agent程序|资源检测|控制器检测|热更新|新资源版本') { return @{ Kind = 'noise'; Text = '' } }
    return $null
}

function Read-M9ALogEvents {
    <# 从上次读到的字节偏移继续读 M9A 日志，返回 @{LogPath;Events}。Offsets/PreOffsets 是同一个 hashtable，原地更新 #>
    param(
        [Parameter(Mandatory)]$P,
        [Parameter(Mandatory)]$Instance,
        [Parameter(Mandatory)][hashtable]$Offsets,
        [Parameter(Mandatory)][hashtable]$PreOffsets
    )
    $logPath = Get-M9ADailyLog -P $P
    $events = @()
    if (-not (Test-GdPathReady $logPath)) { return @{ LogPath = $logPath; Events = $events } }
    if (-not $Offsets.ContainsKey($logPath)) {
        $Offsets[$logPath] = if ($PreOffsets.ContainsKey($logPath)) { $PreOffsets[$logPath] } else { 0 }
    }
    $tail = Get-GdFileTail -Path $logPath -Offset ([long]$Offsets[$logPath])
    $Offsets[$logPath] = $tail.Offset
    foreach ($line in (@($tail.Text -split "`r?`n") | Where-Object { $_.Trim() })) {
        $ev = Read-GdM9ALine -Line $line -InstanceId $Instance.Id
        if ($ev) { $events += $ev }
    }
    return @{ LogPath = $logPath; Events = $events }
}

function Register-M9AEvent {
    <# 把一个事件累计进状态包(纯状态更新，方便进程退出后再补读一次) #>
    param([Parameter(Mandatory)][hashtable]$State, [Parameter(Mandatory)][hashtable]$Event)
    # 只要读进来一个"认识"的事件就计数：eventCount 一直是 0 = 日志压根看不懂了(见 Resolve-M9AStatus 的 unknown-format)
    $State.eventCount = [int]$State.eventCount + 1
    switch ($Event.Kind) {
        'task' {
            $State.chainStarted = $true
            $State.tasks = @($State.tasks) + $Event.Text
            Write-GdLog "M9A 进行中[$(@($State.tasks).Count)] $($Event.Text)"
            $State.lastLog = Get-Date
        }
        'chain-start' { $State.chainStarted = $true; $State.lastLog = Get-Date }
        'chain-done'  { $State.chainDone = $true; $State.lastLog = Get-Date }
        'queue-prepared' {
            $State.queueCount = [int]$Event.Text
            if ([int]$Event.Text -gt 0) { $State.chainStarted = $true }
            Write-GdLog "M9A 已提交任务队列：任务数量=$($Event.Text)"
            $State.lastLog = Get-Date
        }
        'ui-start' { $State.startRequested = $true; $State.lastLog = Get-Date }
        'connect-start' { $State.connectPhase = 'started'; $State.lastLog = Get-Date }
        'finish' {
            <#
              「启动任务完成」有两种含义，靠上下文区分：
                已经看到过任务/队列 → 这就是整条链路跑完(--quit-after-run 模式下唯一的完成信号)
                什么都没看到        → 只完成了连接，M9A 自己也没开始任务队列
            #>
            if ($State.chainStarted -or (@($State.tasks).Count -gt 0)) { $State.chainDone = $true }
            else { $State.connectPhase = 'done' }
            $State.lastLog = Get-Date
        }
        'fail' { if (-not $State.failed) { $State.failed = $Event.Text }; $State.lastLog = Get-Date }
        'task-fail' {
            <#
              「任务失败：<中文名>」是 M9A 对单个任务的判失败，一轮里可能连着打好几条。
              只把第一条塞进 $State.failed 是不够的：真正决定这轮可信度的是
              「第一个任务(启动游戏)有没有失败」——它失败说明后面全是空转，见 Resolve-M9AStatus 的 blocked。
            #>
            <#
              两个坑叠在这段上，写成 "$ft = if(...){@()}else{...}; $State.x = $ft + $t" 会静默退化成字符串拼接：
                1) 空数组经 if/管道赋给变量会塌成 $null，而 $null + 'a' 是**字符串** 'a'(它的 Count 永远是 1)；
                2) @($null) 的长度又是 1，键没初始化时不能直接 @(...) 包。
              两条分支都用"直接赋值 + 数组字面量"，不经过管道。
            #>
            $t = [string]$Event.Text
            if ($null -eq $State.failedTasks) { $State.failedTasks = @($t) }
            else { $State.failedTasks = @($State.failedTasks) + $t }
            Write-GdLog "M9A 任务失败：$($Event.Text)(累计 $($State.failedTasks.Count) 个)" -Level WARN
            if (-not $State.failed) { $State.failed = "任务失败：$($Event.Text)" }
            $State.lastLog = Get-Date
        }
        'res-current' { $State.resCurrent = $Event.Text; $State.lastLog = Get-Date }
        'res-latest' {
            $State.resLatest = $Event.Text
            if ($State.resCurrent -and ($State.resLatest -ne $State.resCurrent)) {
                Write-GdLog "M9A 资源/程序有新版：当前 $($State.resCurrent) → 最新 $($State.resLatest)(整包更新，命令行没有更新入口，需在 M9A 界面点『更新』)" -Level WARN
            }
            $State.lastLog = Get-Date
        }
        'connect-fail' {
            if (-not $State.connectFail) {
                $State.connectFail = $Event.Text
                Write-GdLog "M9A 连不上模拟器：$($Event.Text)" -Level WARN
            }
            $State.lastLog = Get-Date
        }
        'noise' { $State.lastLog = Get-Date }
    }
}

function Resolve-M9AStatus {
    <#
      根据已收集的事件判定结果；返回 $null = 还在跑。顺序讲究：
        0) 第一个任务(启动游戏)就失败，或开始过的任务全失败 = blocked
           —— 界面根本没进游戏，后面每条「开始任务」都是空转，重试也没用(实测 09-25 19:24：客户端正在下 1.91G 更新包)
        1) 有完成信号且确实跑过任务，或「任务数已达队列声明的数量 + 进程已按 -q 退出」 = ok
           (--quit-after-run 会让 M9A 在最后一个任务后立刻关闭，日志常来不及写完成行就被读到退出)
        2) 有"任务已全部完成"但一条"开始任务"都没有 = empty-queue(队列没提交；实测 08-30 长草/09-23 16:39 都是这个形态)
        3) 一条认识的事件都没解析出来却已经到点/进程没了 = unknown-format(日志文案或实例 id 变了，别按本轮结果下结论)
        4) 进程已消失(且经过两轮确认)             = error
        5) 超时 / 到点没开始 / 日志停滞           = timeout / not-started / stalled
    #>
    param(
        [Parameter(Mandatory)][hashtable]$State,
        [Parameter(Mandatory)][double]$Elapsed,
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)]$Instance,
        [bool]$ProcessGone = $false,
        [string]$LogName = ''
    )
    $tasks = @($State.tasks)
    $progress = $(if ($tasks.Count) { "已到[$($tasks[-1])]" }
                  elseif ($State.chainStarted) { '队列已开始但还没有任务' }
                  elseif ($State.connectPhase -eq 'done') { '只完成了连接(队列任务从未开始)' }
                  elseif ($State.connectPhase -eq 'started') { '正在连接模拟器' }
                  else { '尚未开始任何任务' })

    # 队列声明过任务数量、实际任务数已达标、且进程已经退出 → 视为按 -q 正常收尾
    $allQueuedDone = ($null -ne $State.queueCount) -and ([int]$State.queueCount -gt 0) -and ($tasks.Count -ge [int]$State.queueCount)
    $seenNothing = ([int]$State.eventCount -eq 0)
    # hashtable 里没这个键时取出来是 $null，而 @($null) 的长度是 1；空数组从 if 里赋值又会塌成 $null —— 用 @(foreach) 拿真正的 0 长度数组
    $failedTasks = @(foreach ($ft in $State.failedTasks) { $ft })
    <#
      两条"可行动"提示，拼到所有非 ok 结论后面。09-25 那轮就是吃了没这两条的亏：
      客户端在模拟器里下 1.91G 更新包、M9A 自己报了版本差，我们却只说「进程提前退出」然后原样重试一遍。
    #>
    $updateHint = ''
    if ($State.resLatest -and $State.resCurrent -and ([string]$State.resLatest -ne [string]$State.resCurrent)) {
        $updateHint = "；M9A 报资源/程序落后(当前 $($State.resCurrent) → 最新 $($State.resLatest))——这是整包更新，命令行没有更新入口，得在 M9A 界面点『更新』"
    }
    $connectHint = ''
    if ($State.connectFail) { $connectHint = "；M9A 连不上模拟器($($State.connectFail))——多半是 MuMu 虚拟机没起来或 adb 设备指纹变了" }
    $firstTaskFailed = ($failedTasks.Count -gt 0) -and ($tasks.Count -gt 0) -and ($failedTasks -contains $tasks[0])
    <# 一条"开始任务"都没有时不算"全失败"——那种形态该走 empty-queue/not-started，别报"开始过的 0 个任务全部识别失败" #>
    $allStartedFailed = ($tasks.Count -gt 0) -and ($failedTasks.Count -gt 0) -and ($failedTasks.Count -ge $tasks.Count)
    if ($ProcessGone -and ($firstTaskFailed -or $allStartedFailed)) {
        $why = if ($firstTaskFailed) { "第一个任务[$($tasks[0])]就失败，后面 $($tasks.Count - 1) 个都是在没进游戏的情况下空转" }
               else { "开始过的 $($tasks.Count) 个任务全部识别失败" }
        return @{ Status = 'blocked'; Detail = "M9A 有 $($failedTasks.Count) 个任务报『任务失败』($why)。先看模拟器：客户端多半停在更新/下载界面或已退回桌面，不是判定库的问题${updateHint}${connectHint}；失败任务：$(($failedTasks | Select-Object -First 5) -join '、')；日志：$LogName"; Tasks = $tasks; FailedTasks = $failedTasks }
    }
    if (($State.chainDone -and $tasks.Count -and (-not $firstTaskFailed) -and (-not $allStartedFailed)) -or ($ProcessGone -and $allQueuedDone -and (-not $State.failed))) {
        $detail = "实例[$($Instance.Name)] 完成 $($tasks.Count) 个任务"
        if ($null -ne $State.queueCount) { $detail += "/队列声明 $($State.queueCount) 个" }
        if (-not $State.chainDone) { $detail += '(M9A 已按 --quit-after-run 退出，未见完成行)' }
        if ($State.failed) { $detail += "(收尾阶段有告警：$($State.failed))" }
        if ($failedTasks.Count) { $detail += "(其中 $($failedTasks.Count) 个任务报过『任务失败』：$(($failedTasks | Select-Object -First 3) -join '、'))" }
        return @{ Status = 'ok'; Detail = $detail; Tasks = $tasks }
    }
    if ($State.chainDone -and ($tasks.Count -eq 0)) {
        <#
          「队列声明 10 个却一个任务没开始」不能说成"队列为空"——实测 09-25 19:29 那种情况是
          控制器初始化压根没过(AdbSerial 为空)，M9A 报的 10 个是它勾选的任务数，不是空队列。
        #>
        $why = if ($State.connectFail) { "M9A 连模拟器这一步就失败了(声明了 $($State.queueCount) 个任务，一个都没开始)" }
               elseif ($null -ne $State.queueCount) { "M9A 自己报的任务数量=$($State.queueCount)(声明了却没执行，通常是界面没进游戏)" }
               elseif ($State.startRequested) { '开始命令下发了但没有任何任务(任务列表可能全被取消勾选)' }
               else { 'M9A 从未收到"用户操作：启动任务" → 命令行 --autostart 没生效(注意：短参数 -a 实测无效，必须用长参数)' }
        return @{ Status = 'empty-queue'; Detail = "M9A 连接完成后直接报『任务已全部完成』，但一条『开始任务：』都没有(用时 $([math]::Round($Elapsed / 60, 1)) 分钟) → $why${updateHint}${connectHint}；实例[$($Instance.Name)]"; Tasks = $tasks }
    }
    $unknownDetail = "日志里一条认识的标志都没解析出来(实例[$($Instance.Name)] id=$($Instance.Id)，日志：$LogName) → 多半是 M9A 更新后改了日志文案，或实例重新导入后 id 变了(行内 [inst=名字/id] 过滤把整批行排除了)，也可能是 M9A 压根没往这个文件里写(进程卡在启动/日志目录改了)。这轮结果不可信，别按它判定体力是否清完：先跑 tools\probe-m9a-history.ps1 看实例 id、再用 tools\replay-m9a-log.ps1 对着这份日志复现，然后更新 lib\m9a.ps1 的 Read-GdM9ALine"
    if ($ProcessGone) {
        if ($seenNothing) {
            return @{ Status = 'unknown-format'; Detail = "M9A 已退出，但$unknownDetail"; Tasks = $tasks }
        }
        if ($State.failed) { return @{ Status = 'error'; Detail = "M9A 已退出，链路失败：$($State.failed)($progress)${updateHint}${connectHint}"; Tasks = $tasks } }
        if ($tasks.Count -eq 0) {
            <# 进程都没了却一个任务没跑：多半是 --autostart 没生效 + --quit-after-run 直接退出，等下去也没有 → 快速失败且重试无意义 #>
            return @{ Status = 'not-started'; Detail = "M9A 连接后就退出了，一个任务都没跑($progress) → 命令行 --autostart 没生效(短参数 -a 实测无效)，或资源/程序有新版本要更新${updateHint}${connectHint}；日志：$LogName"; Tasks = $tasks }
        }
        return @{ Status = 'error'; Detail = "M9A 进程提前退出($progress，队列声明 $(if ($null -ne $State.queueCount) { $State.queueCount } else { '未知' }) 个)${updateHint}${connectHint}"; Tasks = $tasks }
    }
    if ($seenNothing -and ($Elapsed -gt ([int]$S.runtime.r1999StartTimeoutMinutes * 60))) {
        <# 进程还在、但超过「该开始了」的时间点却一条标志都没读到：不套 not-started/stalled 的猜测，直接说看不懂日志 #>
        return @{ Status = 'unknown-format'; Detail = "启动已 $([math]::Round($Elapsed / 60, 1)) 分钟，$unknownDetail"; Tasks = $tasks }
    }
    if ($Elapsed -gt ([int]$S.runtime.r1999TimeoutMinutes * 60)) {
        return @{ Status = 'timeout'; Detail = "超过 $([int]$S.runtime.r1999TimeoutMinutes) 分钟未跑完($progress)"; Tasks = $tasks }
    }
    if (-not $State.chainStarted -and ($tasks.Count -eq 0) -and $Elapsed -gt ([int]$S.runtime.r1999StartTimeoutMinutes * 60)) {
        $hint = if ($State.connectPhase -eq 'done') {
            'M9A 连上了模拟器却没开始任务队列 → 命令行 --autostart 没生效(短参数 -a 实测无效)，或资源/程序有新版本要更新(日志里有"还请更新"这类提示)，或界面被弹窗挡住'
        } else { 'M9A 连模拟器这一步都没走完' }
        return @{ Status = 'not-started'; Detail = "$([int]$S.runtime.r1999StartTimeoutMinutes) 分钟内没跑任何任务($progress)。$hint${updateHint}${connectHint}，日志：$LogName"; Tasks = $tasks }
    }
    if (((Get-Date) - $State.lastLog).TotalSeconds -gt ([int]$S.runtime.stallTimeoutMinutes * 60)) {
        return @{ Status = 'stalled'; Detail = "M9A 日志 $([int](((Get-Date) - $State.lastLog).TotalSeconds / 60)) 分钟无输出($progress)，可能卡在某个界面"; Tasks = $tasks }
    }
    return $null
}

function Invoke-M9ARun {
    <#
      启动 M9A 跑一个实例，并以 M9A 自己的日志为准判定结果(见 Resolve-M9AStatus)。
      默认走命令行脚本化入口：m9a.exe -i <实例> -a -q -f(settings.runtime.m9aCliArgs)。
      同时用 adb 的 1999 进程状态做交叉验证。
    #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)]$Instance,
        [switch]$DryRun
    )
    $P = Get-M9APaths -S $S
    $conn = Get-M9AConnection -S $S -Instance $Instance
    $pkg = [string]$S.re1999.package
    $cli = Get-GdM9ACliEnabled -S $S
    $launchArgs = Get-M9ALaunchArgs -Instance $Instance `
        -Mode ([string]$S.runtime.m9aInstanceArg) `
        -ForceStart ([bool]$S.runtime.m9aForceStart) `
        -CliArgs $cli

    if ($DryRun) {
        $shown = if ($launchArgs.Count) { ($launchArgs -join ' ') } else { '(不带参数：老路子，靠 M9A 自己的自动开始)' }
        Write-GdLog "[dry-run] 将启动 $($P.Exe) $shown，ADB $($conn.Serial)，以 logs\log-*.log 判定完成" -Level STEP
        return @{ Status = 'dryrun'; Detail = ''; Args = $launchArgs }
    }
    if ($cli -and (Test-GdProcessRunning -Name $P.Name)) {
        Write-GdLog 'M9A 已在运行：命令行会被转发给正在跑的实例，按 -i/-f 重新开始任务' -Level WARN
    }
    if (-not $cli -and (Test-GdProcessRunning -Name $P.Name)) { throw 'M9A 已在运行，无法保证按新实例开始(请先关闭，或把 runtime.m9aCliArgs 打开)' }

    Write-GdLog "ADB 连接 $($conn.Serial)" -Level STEP
    if (-not (Connect-GdAdb -S $S -AdbPath $conn.Adb -Serial $conn.Serial)) {
        Write-GdLog 'adb devices 未列出该序列号，仍继续(M9A 会自行重连)' -Level WARN
    }

    $started = Get-Date
    # 先记录今天日志现有长度：只认启动之后的新增内容，避免把上次运行的"完成"读成本次的
    $preOffsets = @{}
    $today = Get-M9ADailyLog -P $P
    if (Test-Path -LiteralPath $today) { $preOffsets[$today] = (Get-Item -LiteralPath $today).Length }
    Write-GdLog "启动 M9A：$($P.Exe) $(if ($launchArgs.Count) { $launchArgs -join ' ' })" -Level STEP
    if ($launchArgs.Count) {
        Start-Process -FilePath $P.Exe -ArgumentList $launchArgs -WorkingDirectory $P.Dir | Out-Null
    } else {
        Start-Process -FilePath $P.Exe -WorkingDirectory $P.Dir | Out-Null
    }
    if (-not (Wait-GdCondition -Test { Test-GdProcessRunning -Name $P.Name } -TimeoutSeconds 120 -PollSeconds 3 -What 'M9A 进程启动')) {
        return @{ Status = 'error'; Detail = 'M9A 未能启动(进程未出现)' }
    }

    $offsets = @{}          # 日志文件 -> 已读字节偏移(跨天时新文件从 0 开始)
    $state = @{
        tasks = @(); chainStarted = $false; chainDone = $false
        connectPhase = 'none'; failed = $null; lastLog = (Get-Date)
        queueCount = $null; startRequested = $false; eventCount = 0
        failedTasks = @(); resCurrent = $null; resLatest = $null; connectFail = $null
    }
    $seenGame = $false
    $gone = 0               # 进程连续消失几轮才算真退出(自更新重启会闪一下)

    while ($true) {
        Start-Sleep -Seconds 10
        $read = Read-M9ALogEvents -P $P -Instance $Instance -Offsets $offsets -PreOffsets $preOffsets
        $logPath = $read.LogPath
        foreach ($ev in @($read.Events)) { Register-M9AEvent -State $state -Event $ev }
        if (Get-Gd1999Pid -AdbPath $conn.Adb -Serial $conn.Serial -Package $pkg) { $seenGame = $true }

        $elapsed = ((Get-Date) - $started).TotalSeconds
        if (-not (Test-GdProcessRunning -Name $P.Name)) { $gone++ } else { $gone = 0 }
        $res = Resolve-M9AStatus -State $state -Elapsed $elapsed -S $S -Instance $Instance `
                -ProcessGone ($gone -ge 2) -LogName (Split-Path -Leaf $logPath)
        if ($res) {
            if ($res.Status -eq 'ok') {
                Write-GdLog "M9A 链路完成，共 $(@($res.Tasks).Count) 个任务，用时 $([math]::Round($elapsed / 60, 1)) 分钟" -Level OK
                $res.Duration = $elapsed
            } elseif ($res.Status -eq 'empty-queue') {
                Write-GdLog $res.Detail -Level ERROR
            } else {
                $res.Detail += $(if ($seenGame) { '(游戏进程出现过)' } else { '(游戏进程从未出现)' })
            }
            return $res
        }
    }
}

function Stop-M9A {
    param([Parameter(Mandatory)]$S)
    $P = Get-M9APaths -S $S
    if ($S.runtime.quitAssistantAfter) {
        $n = Stop-GdProcessTree -Name $P.Name
        if ($n) { Write-GdLog "已关闭 M9A($n 个进程)" }
    }
}
