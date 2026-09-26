# lib/m9a.ps1 — 配置并驱动 M9A(重返未来：1999 / MuMu + ADB)
#
# ── 冻结(2026-09-26，PS→C# 批次 3)────────────────────────────────────────────
# 本体已迁到 tools\panel-cs\Processes.cs(契约见 docs\ps-migration-batch0.md)：
#   Get-M9ALaunchArgs  → verb m9a-launch-args(一律长参数：短参数 -a 实测无效的老规矩在 C# 注释里)
#   Read-GdM9ALine     → verb parse-m9a-lines(单行走一次；解析顺序敏感，改文案改 C# 那份)
#   Read-M9ALogEvents  → verb parse-m9a-lines(整批文本一次转发，绝不逐行起进程)
#   Resolve-M9AStatus  → verb resolve-m9a-status(所有结论文案/优先级在 C#；
#                        本壳把 $State.lastLog 换算成 quietSeconds 递进去，Found:false 翻译回 $null)
# 所以：改日志解析/结论话术请改 Processes.cs 并跑 tools\probe-process-golden.ps1 对拍，不要在这里加规则。
# 故意留在 PS 侧的：Get-M9APaths/Get-GdM9ACliEnabled(读配置)、Set-M9AInstance(写 appsettings+回读)、
# Get-M9AConnection(读实例 JSON)、Get-M9ADailyLog/Register-M9AEvent(状态累计+日志染色)、
# Invoke-M9ARun(起进程与轮询，归批次 5 编排层)。
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

function Get-M9ALaunchArgs {
    <#
      转发壳：参数拼装本体在 Processes.cs(m9a-launch-args)。
      老规矩留在 C# 注释里：一律长参数 —— 短参数 -a 会被 M9A 记成 key "a"、自动开始判定匹配不上(09-23 实测)。
    #>
    param(
        [Parameter(Mandatory)]$Instance,
        [string]$Mode = 'id',
        [bool]$ForceStart = $true,
        [bool]$CliArgs = $true
    )
    $d = Invoke-GdCli -Verb 'm9a-launch-args' -Body ([pscustomobject]@{
        cliArgs    = [bool]$CliArgs
        mode       = $Mode
        id         = [string]$Instance.Id
        name       = [string]$Instance.Name
        forceStart = [bool]$ForceStart
    })
    return @(foreach ($a in @($d.Args)) { [string]$a })
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
      转发壳(parse-m9a-lines 的单行入口)：一行 → 至多一个事件。
      实测语义/顺序敏感点(如 res-latest 必须排在 noise 前)全部在 Processes.cs 的 ParseM9aLine 注释里。
      返回形状与旧实现一致：$null 或 @{Kind;Text}(Register-M9AEvent 吃 hashtable，别改成对象)。
      注意：轮询路径上的正式取数走 Read-M9ALogEvents 的整批转发，这里只给别人手动解析单行用。
    #>
    param([Parameter(Mandatory)][string]$Line, [string]$InstanceId = '')
    $d = Invoke-GdCli -Verb 'parse-m9a-lines' -Body ([pscustomobject]@{ text = $Line; instanceId = $InstanceId })
    $evs = @($d.Events)
    if ($evs.Count -eq 0) { return $null }
    return @{ Kind = [string]$evs[0].Kind; Text = [string]$evs[0].Text }
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
    <#
      整批一次转发(批次 3 的省钱点)：拆行/空行过滤/逐行解析都在 parse-m9a-lines 本体里，
      轮询一次只起一次进程 —— 别改回 foreach 行内调 Read-GdM9ALine，那是每行一个进程。
      本轮没有新字节就干脆不起进程(和旧实现在这一点上同形)。
    #>
    if ($tail.Text.Trim()) {
        $d = Invoke-GdCli -Verb 'parse-m9a-lines' -Body ([pscustomobject]@{ text = $tail.Text; instanceId = $Instance.Id })
        foreach ($e in @($d.Events)) { $events += @{ Kind = [string]$e.Kind; Text = [string]$e.Text } }
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
    <#
      视觉兜底的预算与"这一轮已经问过几次"。次数和端游那一路共用同一个键(runtime.visionMaxRescue)，
      因为限的是"一轮里跟模型没完没了的次数"，跟哪一路无关。
      状态装在一个 hashtable 里、只用点号改它的成员：$askScreen 用 & 调用时跑在**子作用域**，
      在里面写 `$left--` 只会造出一个子作用域的局部变量，预算根本扣不动；
      而 `$rescue.Left = ...` 改的是同一个哈希对象的成员，外面看得到 —— 所以预算和账都放这儿，
      不必为这点事上 GetNewClosure(它会把闭包里的变量绑成另一套 session state，读起来更费劲)。
      $rescue.Notes 记每一次的结论，最后拼进 Detail —— 判因层不许丢这条线。
    #>
    $rescue = @{ Left = [int]$S.runtime.visionMaxRescue; Notes = @() }
    if ([int]$rescue.Left -le 0) { $rescue.Left = 2 }
    $askScreen = {
        param([string]$Why, [string]$Ctx = '')
        if (-not (Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue)) { return $null }
        if ([int]$rescue.Left -le 0) { return $null }
        $rescue.Left = [int]$rescue.Left - 1
        $rc = $null
        try { $rc = Invoke-GdVisionRescue -S $S -Tag "m9a$($rescue.Notes.Count + 1)" -Target 'mumu' -Context $Ctx -Conn $conn }
        catch { $rc = [pscustomobject]@{ Tried = $false; Done = $false; Detail = "兜底抛异常：$($_.Exception.Message)" } }
        $rescue.Notes = @($rescue.Notes) + "[$Why] $($rc.Detail)"
        Add-GdRescueRecord -S $S -Game 'r1999' -Result $rc -Tag "m9a$($rescue.Notes.Count)" -Target 'mumu'
        Write-GdLog "视觉兜底($Why)：$($rc.Detail)" $(if ($rc.Done) { 'OK' } else { 'WARN' })
        return [pscustomobject]@{ Tried = [bool]$rc.Tried; Done = [bool]$rc.Done }
    }

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
            <#
              卡住的四种形态都值得一问画面，但"问完怎么办"分两类，差别就在进程还在不在：
                stalled / timeout(还在跑)  → 兜成了就把这一轮的结论撤回去继续等，M9A 自己会往下走;
                blocked / not-started / empty-queue(进程已经没了或在空转) → 点掉弹窗救不了已经退出的进程，
                  但**这一问仍然有价值**：它把"多半停在更新/下载界面"这种猜测换成"画面上确实是 XX 弹窗"，
                  而且点掉之后外层那一次重试才可能真的跑进去。所以只问一次、只把结果写进 Detail。
              状态名一律不改：daily-clear 的收尾与通知按状态名分流，改成新名字等于把判因链弄断。
            #>
            $alive = ($res.Status -in @('stalled', 'timeout'))
            if ($res.Status -in @('stalled', 'timeout', 'blocked', 'not-started', 'empty-queue')) {
                $whyKind = if ($alive) { '停滞' } else { '判因' }
                $r = & $askScreen $whyKind ([string]$res.Detail)
                if ($r -and $r.Done) {
                    if ($alive) {
                        # 挡路的东西拿掉了：停滞计时从头算，给 M9A 机会自己往下走(与 lib\maa.ps1 同一处理)
                        $state.lastLog = Get-Date
                        $rescue.Notes = @($rescue.Notes) + '[停滞已解除，继续等待]'
                        continue
                    }
                    <# 进程都没了，点掉弹窗救不了这一轮，但**外层那一次重试就有了意义**：
                       blocked/not-started 本来是"重试也没用，直接记告警"，画面被兜清楚之后重试一次是划算的。
                       这个标记就是给 r1999-daily 的那道分流看的(见 Start-GdR1999Daily 里的 $retryable)。 #>
                    $res.Rescued = $true
                }
            }
            if (@($rescue.Notes).Count) {
                $res.Detail += "；视觉兜底 $(@($rescue.Notes).Count) 次，最后一次：$(@($rescue.Notes)[-1])"
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
