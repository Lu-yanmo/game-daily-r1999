# lib/supervisor.ps1 — 监督 agent：整轮全程旁观的"裁判"，只在判定层**自己不动手**的地方补位
#
# 为什么要有它：现在的自动化是"既当运动员又当裁判"——ark/r1999 的等待循环既是干活的那个，
# 也是唯一在盯进度那个。它一旦以我们没想到的方式挂住(等待逻辑本身死了、日志格式变了读不懂、
# 助手进程人间蒸发)，没有任何别的东西会说话。2026-09-25 那轮 MAA 基建 10 分 41 秒一个字不写、
# 2026-09-24 中秋弹窗 22 分钟空转，都是事后翻日志才知道"当时其实早就该看出来"。
# 监督 agent 就是那个"当时就在场"的第三方：每 60 秒收一次现场证据(助手日志新鲜度 / 进程存活 /
# 完成信号)，规则先判，**只在规则说可疑时**才拍一张画面问视觉模型复核(平时零 token)，
# 结论写进 state\supervision.json + logs\supervision-*.jsonl，面板转达，异常时 webhook 推手机。
#
# 权限边界(2026-09-26 用户拍板"报告 + 有限干预")：
#   * 它**不点鼠标** —— 画面里有弹窗挡路(blocking=true)时它只记录，点窗是主流程视觉兜底的活，
#     两边抢同一个鼠标比谁都袖手更糟；
#   * 唯一的干预 = 重启助手进程本身，而且只有声明了 SupervisorRestart 钩子的那一路可以(现在是方舟：
#     MAA 重启后自动开队列，和主流程起它的方式一模一样，风险=多跑一遍)；
#   * 1999 那一路**不碰模拟器** —— MuMu 冷启动要好几分钟，误判一次的代价远超"多跑一轮"，
#     所以这一路只发通知；
#   * 一轮最多干预 supervisorMaxInterventions 次，用完就只剩报告。
#
# 框架纪律：本文件不认识任何游戏。每一路"有什么可看的"(助手进程名/日志/完成线)由模块清单的
# SupervisorEvidence 钩子给(可选钩子，没给的路线就不监督)，和 Import-GdModules 的
# "文件在才算装了那一路"同一套口径。新增一款游戏 = 清单里多一个钩子，这里一个字不改。
#
# 只读自检纪律(见 test-modules [sv] 块)：证据→状态→动作三段全是纯函数，断言喂字典不喂现场；
# 采集/问模型/重启三个有副作用的函数按命名约定可被同名局部函数钉死(PS 动态作用域)。
#
# 依赖：调用方(入口或 tools\supervise.ps1)先点 lib\common.ps1；模块钩子经 modules.ps1 的
# Invoke-GdModuleHook 调；问模型复用 lib\vision-rescue.ps1 那套靶子/提示词/解析(安全边界不重写)。

# ---------- 开关与取值(语义和 arkVisionRescue 一致：没显式 false 就是开) ----------
function Get-GdSupervisorEnabled {
    param([Parameter(Mandatory)]$S)
    return ($null -eq $S.runtime.supervisor) -or ($S.runtime.supervisor -ne $false)
}
function Get-GdSupervisorIntervalSec {
    param([Parameter(Mandatory)]$S)
    $v = 0
    if ($S.runtime.supervisorIntervalSeconds) { $v = [int]$S.runtime.supervisorIntervalSeconds }
    if ($v -lt 15) { $v = 60 }     # 拍得太快既费日志也没意义(助手日志本身按分钟动)
    return $v
}
function Get-GdSupervisorQuietSec {
    param([Parameter(Mandatory)]$S)
    $m = 30
    if ($S.runtime.supervisorQuietMinutes) { $m = [int]$S.runtime.supervisorQuietMinutes }
    if ($m -lt 5) { $m = 5 }
    return $m * 60
}
function Get-GdSupervisorMaxInterventions {
    param([Parameter(Mandatory)]$S)
    if ($null -eq $S.runtime.supervisorMaxInterventions) { return 1 }
    $v = [int]$S.runtime.supervisorMaxInterventions
    if ($v -lt 0) { $v = 0 }
    return $v
}

# ---------- 文件位置 ----------
function Get-GdSupervisorStatePath { Join-Path $global:GD.State 'supervision.json' }
function Get-GdSupervisorStopPath  { Join-Path $global:GD.State 'supervisor.stop' }
function Get-GdSupervisorBeatPath {
    # 一天一个 jsonl：一轮几十到几百拍，一行一拍，回看"当时到底在不在场"就翻它
    return Join-Path $global:GD.Logs ("supervision-" + (Get-Date -Format 'yyyyMMdd') + '.jsonl')
}

# ---------- 纯函数：证据 → 状态 ----------
function Get-GdSupervisorState {
    <# 规则心跳，零 token。输入一条拍到的证据字典，输出 done / dead / quiet / busy 四态之一。
       参数名刻意叫 Evidence 不叫 Ev：函数里一旦用了 [Parameter()] 属性就是 advanced function，
       公共参数(-ErrorVariable 等)会跟进来，`-Ev` 正好撞它的缩写别名，调用直接绑参失败(2026-09-26 冒烟实测)。
       判据顺序是认真的：
       * done 排最前 —— 完成线看到了，进程在不在都不重要(MAA 跑完可能自己退)。
       * dead 只在"曾经见过助手活着、现在没了"时才判 —— 一轮开跑时 MAA 还没被主流程拉起来，
         上来就判 dead 是假警报(这条来自只读自检不许要求机器现场状态的同族坑)。
       * quiet 要求**两本日志都**安静超过阈值 —— MAA 单条任务链 10 分钟不写 gui.log 是已知常态
         (2026-09-25 实测 10 分 41 秒)，只看助手日志会天天误报；主流程的 stall 判据(22 分)先动，
         监督的默认阈值(30 分)故意更宽，它兜的是"主流程自己也没醒过来"那种局。 #>
    param(
        [Parameter(Mandatory)]$Evidence,
        [int]$QuietSec = 1800,
        [bool]$EverAlive = $true
    )
    if ($Evidence.Done) { return 'done' }
    if ($EverAlive -and -not $Evidence.AssistantAlive) { return 'dead' }
    $a = [double]$Evidence.AsstAgeSec
    $d = [double]$Evidence.DailyAgeSec
    if ($a -gt $QuietSec -and $d -gt $QuietSec) { return 'quiet' }
    return 'busy'
}

# ---------- 纯函数：状态 → 动作 ----------
function Get-GdSupervisorAction {
    <# 干预决策表。把"要不要动手"从一堆现场 if 里拎成一张可自检的表：
       quiet 要**连续两拍**才算数(一拍可能是日志轮转/时间抖动)；
       画面 blocking=true → 不干预，那是主流程兜底的活场；
       blocking=false(没弹窗却全线静止)且这一路允许重启且还有额度 → restart_assistant；
       其余异常一律 notify，且每种状态一轮只通知一次(由调用方记 alreadyNotified)。 #>
    param(
        [Parameter(Mandatory)][string]$State,
        [int]$QuietStreak = 0,
        $Screen = $null,                 # @{Ok;Blocking;What} 或 $null(还没问/问不成)
        [bool]$CanRestart = $false,
        [int]$Interventions = 0,
        [int]$MaxInterventions = 1,
        [bool]$AlreadyNotified = $false
    )
    if ($State -eq 'done' -or $State -eq 'busy') { return [pscustomobject]@{ Kind = 'none'; Why = '' } }
    if ($State -eq 'dead') {
        if ($AlreadyNotified) { return [pscustomobject]@{ Kind = 'none'; Why = '' } }
        return [pscustomobject]@{ Kind = 'notify'; Why = '助手进程不见了，而这一轮还没宣布完成' }
    }
    # quiet
    if ($QuietStreak -lt 2) { return [pscustomobject]@{ Kind = 'none'; Why = '' } }
    if ($null -ne $Screen -and $Screen.Ok) {
        if ($Screen.Blocking) {
            return [pscustomobject]@{ Kind = 'none'; Why = ('画面被「' + [string]$Screen.What + '」挡着，交给主流程兜底') }
        }
        if ($CanRestart -and $Interventions -lt $MaxInterventions) {
            return [pscustomobject]@{ Kind = 'restart_assistant'; Why = ('连续静止且画面没有挡路的东西(' + [string]$Screen.What + ')，判定层没醒，重启助手') }
        }
        if ($AlreadyNotified) { return [pscustomobject]@{ Kind = 'none'; Why = '' } }
        return [pscustomobject]@{ Kind = 'notify'; Why = '连续静止、画面没有弹窗，但这一路不许自动重启(或额度用完)' }
    }
    if ($AlreadyNotified) { return [pscustomobject]@{ Kind = 'none'; Why = '' } }
    $why = '连续静止，且看不清画面'
    if ($null -ne $Screen -and $Screen.Why) { $why += '(' + [string]$Screen.Why + ')' }
    return [pscustomobject]@{ Kind = 'notify'; Why = $why }
}

# ---------- 有副作用的三件：采集 / 问画面 / 重启(自检按名字钉桩) ----------
function Collect-GdSupervisorEvidence {
    <# 向模块钩子要"这一路有什么可看的"，然后只看磁盘和进程表，一个字都不写。
       契约(模块清单的 SupervisorEvidence 钩子给)：
         AssistantProc   助手进程名            AssistantLog  助手主日志(新鲜度按它)
         GameProc        被操作对象的进程名(可空，只作记录)
         DonePattern     助手日志新增行里认"这一路完成"的正则
         ProgressPattern 完成信号需要上下文时先认这个(1999 的「启动任务完成」单独出现只等于连上了模拟器)
       返回 @{AssistantAlive;GameAlive;AsstAgeSec;DailyAgeSec;Done}；日志读不动按"很老"处理，绝不抛。
       刻意不回传读到的原文：谁把它整个塞进记录/异常消息，谁就把几 MB 的日志搬进 supervision.json(2026-09-26 冒烟踩过)。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][hashtable]$Spec,
        [Parameter(Mandatory)][hashtable]$Ctx,      # 跨拍状态：asst_offset / task_seen，按路线 id 分格
        [string]$DailyLogPath = '',
        [string]$Game = ''
    )
    $asstLog = [string]$Spec.AssistantLog
    $asstAge = 999999.0
    $newText = ''
    if ($asstLog -and (Test-Path -LiteralPath $asstLog)) {
        try { $asstAge = (New-TimeSpan -Start (Get-Item -LiteralPath $asstLog).LastWriteTime -End (Get-Date)).TotalSeconds } catch { }
        try {
            <# 偏移按"路径+字节数"成对存：M9A 一天换一本 log-yyyyMMdd.log，跨天后昨天的偏移对新文件全是失配；
               而首拍刻意**不扫内容** —— 上一轮留在日志里的完成线不是这一轮的完成线
               (2026-09-26 冒烟实测：首拍 offset=0 会把 7 月那本几 MB 的 gui.log 整个当"新增"读进来，
               既把 jsonl 撑爆，又把 ark 一路秒判成 done)。路径变了/首拍都只对末尾、下个间隔只扫新增。 #>
            $prev = $null
            if ($Ctx.asst_offset.ContainsKey($Game)) { $prev = $Ctx.asst_offset[$Game] }
            if ($prev -and [string]$prev.path -eq $asstLog) {
                $tail = Get-GdFileTail -Path $asstLog -Offset ([long]$prev.offset)
            }
            else {
                $tail = @{ Text = ''; Offset = [long](Get-Item -LiteralPath $asstLog).Length }
            }
            $Ctx.asst_offset[$Game] = @{ path = $asstLog; offset = [long]$tail.Offset }
            $newText = [string]$tail.Text
        } catch { }
    }
    $dailyAge = 0.0
    if ($DailyLogPath -and (Test-Path -LiteralPath $DailyLogPath)) {
        try { $dailyAge = (New-TimeSpan -Start (Get-Item -LiteralPath $DailyLogPath).LastWriteTime -End (Get-Date)).TotalSeconds } catch { }
    }
    $done = $false
    if ($newText -and $Spec.DonePattern) {
        if ($Spec.ProgressPattern) {
            # 先吃进度线再认完成线，顺序不能反：同一拍里两类行可能一起涌进来
            if ($newText -match ([string]$Spec.ProgressPattern)) { $Ctx.task_seen[$Game] = $true }
            if ($Ctx.task_seen.ContainsKey($Game) -and $Ctx.task_seen[$Game] -and ($newText -match ([string]$Spec.DonePattern))) { $done = $true }
        }
        elseif ($newText -match ([string]$Spec.DonePattern)) { $done = $true }
    }
    return [pscustomobject]@{
        AssistantAlive = [bool](Test-GdProcessRunning -Name ([string]$Spec.AssistantProc))
        GameAlive      = $(if ($Spec.GameProc) { [bool](Test-GdProcessRunning -Name ([string]$Spec.GameProc)) } else { $false })
        AsstAgeSec     = $asstAge
        DailyAgeSec    = $dailyAge
        Done           = $done
    }
}

function Request-GdSupervisorScreen {
    <# "规则说可疑了，人眼看一眼" —— 复用兜底那套靶子和提示词(安全边界只有一份，t13 盯的还是它)。
       问的 schema 就是兜底的 blocking/what：对监督来说，"有没有东西挡住主流程"正是它想知道的答案。
       任何一步不对都返回 Ok=false + 一句原因，绝不抛 —— 看不清画面不是停下来的理由。 #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Module, [string]$Game = '')
    if (-not (Get-Command Get-GdVisionTarget -ErrorAction SilentlyContinue)) {
        return [pscustomobject]@{ Ok = $false; Why = '这棵树上没有视觉兜底内核' }
    }
    $kind = [string]$Module.VisionKind
    if (-not $kind) { return [pscustomobject]@{ Ok = $false; Why = '这一路没声明兜底后端' } }
    $target = $null
    try { $target = Get-GdVisionTarget -S $S -Kind $kind } catch { return [pscustomobject]@{ Ok = $false; Why = ('找靶子就失败：' + $_.Exception.Message) } }
    if ($null -eq $target) { return [pscustomobject]@{ Ok = $false; Why = '这一路没有兜底后端' } }
    if (-not $target.Enabled) { return [pscustomobject]@{ Ok = $false; Why = [string]$target.Why } }
    if ($target.Alive -and -not (& $target.Alive)) { return [pscustomobject]@{ Ok = $false; Why = '要看的窗口/虚拟机现在不在' } }
    $dir = Get-GdRescueShotDir
    if (-not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir -Force) }
    $shotPath = Join-Path $dir ('supervision-' + (Get-Date -Format 'MMdd-HHmmss') + '-' + $Game + '.png')
    $shot = $null
    try { $shot = & $target.Shot -Path $shotPath } catch { return [pscustomobject]@{ Ok = $false; Why = ('截图失败：' + $_.Exception.Message) } }
    if (-not $shot -or -not $shot.Ok) { return [pscustomobject]@{ Ok = $false; Why = ('截图没成功：' + [string]($shot.Why)) } }
    $raw = $null
    try {
        $raw = Invoke-GdVisionRaw -S $S -ImagePath ([string]$shot.Path) -Scene ([string]$target.Scene) `
                                  -UserText '这是自动化正在跑的一路的当前画面。监督方注意到它很久没有进展，请判断画面上有没有挡住主流程的东西。按规则只回一个 JSON 对象。'
    } catch { return [pscustomobject]@{ Ok = $false; Why = ('问模型失败：' + $_.Exception.Message) } }
    if (-not $raw.Ok) { return [pscustomobject]@{ Ok = $false; Why = ('问不成：' + [string]$raw.Error) } }
    $v = Get-GdVisionVerdictFromText -Text ([string]$raw.Text)
    if ($null -eq $v) { return [pscustomobject]@{ Ok = $false; Why = '模型回了但解析不出 JSON' } }
    return [pscustomobject]@{ Ok = $true; Blocking = [bool]$v.Blocking; What = [string]$v.What }
}

function Restart-GdSupervisorAssistant {
    <# 唯一的干预动作，本体在模块清单的 SupervisorRestart 钩子里(方舟那份=停 MAA 进程再原样拉起，
       MAA 配置里"启动即开队列"，和主流程起它的方式一模一样)。这一路没给钩子就是不许重启。
       钩子按其它钩子同一口径收 $S(Get-MaaPaths 要它)，所以这里也得把 $S 带进去。 #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Module)
    $sb = $Module.Hooks.SupervisorRestart
    if ($null -eq $sb) { return $false }
    try { return [bool](& $sb -S $S) } catch { return $false }
}

# ---------- 记录 ----------
function Write-GdSupervisorBeat {
    <# 一拍两份落点：jsonl 追加(历史流水，回看用) + supervision.json 整体重写(面板/DumpSummary 读的"现在")。
       supervision.json 里的 beats 只留最近 30 拍 —— 它是给"此刻"用的，流水在 jsonl。 #>
    param(
        [Parameter(Mandatory)][hashtable]$Meta,       # pid/owner/started/games，起拍时就定好
        [Parameter(Mandatory)][array]$PerGame,        # 这一拍每路的 @{game;state;asst_age;alive;done;screen;action;why}
        [array]$Actions = @()                          # 到目前为止发生过的干预(整轮累计)
    )
    $beat = [ordered]@{
        ts    = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
        games = $PerGame
    }
    try {
        ($beat | ConvertTo-Json -Compress -Depth 6) | Add-Content -LiteralPath (Get-GdSupervisorBeatPath) -Encoding utf8
    } catch { }
    $beats = @()
    $prev = Read-GdSupervisorSummary
    if ($prev -and $prev.beats) { $beats = @($prev.beats) }
    $beats = @($beats + $beat)
    if ($beats.Count -gt 30) { $beats = @($beats | Select-Object -Last 30) }
    $doc = [ordered]@{
        pid      = $Meta.pid
        owner    = $Meta.owner
        started  = $Meta.started
        games    = $Meta.games
        updated  = $beat.ts
        beats    = $beats
        actions  = $Actions
        stopped  = ''
    }
    try { Write-GdJson -Path (Get-GdSupervisorStatePath) -Object ([pscustomobject]$doc) } catch { }
    # 流水文件按天一个，留最近 7 天
    try {
        Get-ChildItem -LiteralPath $global:GD.Logs -Filter 'supervision-*.jsonl' -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -Skip 7 | Remove-Item -Force -ErrorAction SilentlyContinue
    } catch { }
}

function Read-GdSupervisorSummary {
    <# 给面板/Stop/自检读的"现在"。读不动/没有就返回 $null —— 没监督过不是错误。 #>
    $p = Get-GdSupervisorStatePath
    if (-not (Test-Path -LiteralPath $p)) { return $null }
    try {
        $txt = ''
        $fs = [System.IO.File]::Open($p, 'Open', 'Read', 'ReadWrite')
        try { $txt = (New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8)).ReadToEnd() } finally { $fs.Dispose() }
        if (-not $txt.Trim()) { return $null }
        return ($txt | ConvertFrom-Json)
    } catch { return $null }
}

function Get-GdSupervisorDetailLines {
    <# 把 supervision.json 里**已经算好**的东西念成给人看的展开文本(面板那一行点开来就是它)。
       纯函数、一个字都不判：state/action/screen/why 都是采集与判据那两层写进去的值，
       这里只按固定字段取值、按固定说法拼句子 —— 所以自检可以喂一份假文档把整段话钉死。
       行数刻意收敛：详情进的是 -DumpSummary 的协议行正文(C# 面板的日志区才是落点，卡片不装)，
       最近 MaxBeats 拍 × 每路一行 + 现值头 + 介入记录，够回看"当时在不在场"，再多就没人翻了。 #>
    param($Sv, [int]$MaxBeats = 6)
    $out = @()
    if (-not $Sv) { return $out }
    $out += ('现值：PID ' + [string]$Sv.pid + ' · 盯的路线 ' + [string]$Sv.games +
             ' · 上岗 ' + [string]$Sv.started + ' · 最后一拍 ' + [string]$Sv.updated +
             $(if ($Sv.stopped) { ' · 收工 ' + [string]$Sv.stopped } else { '' }))
    $out += ('流水：logs\supervision-' + (Get-Date -Format 'yyyyMMdd') + '.jsonl(一行一拍，留 7 天)')
    $beats = @($Sv.beats)
    if (-not $beats.Count) {
        # 只说"还没第一拍"，不许就此 return：json 里的 beats 只留最近 30 拍，而 actions 是整轮的账本 ——
        # 拍被挤掉、介入记录还在，提前返回会把"它干预过什么"整个吞掉(2026-09-26 自检抓到的)。
        $out += '这一枚监督进程还没落下第一拍(起拍间隔默认 60 秒)。'
    } else {
        if ($MaxBeats -lt 1) { $MaxBeats = 1 }
        $out += '最近 ' + [Math]::Min($MaxBeats, $beats.Count) + ' 拍(字段：状态 / 助手日志静默 / 进程 / 完成线 / 画面 / 动作)：'
        foreach ($b in @($beats | Select-Object -Last $MaxBeats)) {
            $ts = [string]$b.ts
            if ($ts.Length -ge 19) { $ts = $ts.Substring(11, 8) }
            foreach ($g in @($b.games)) {
                $age = [string]$g.asst_age
                if ($age -eq '-1') { $age = '读不到' } else { $age += '秒' }
                $out += ('  ' + $ts + ' ' + [string]$g.game + '=' + [string]$g.state +
                         ' 助手日志静默 ' + $age +
                         ' 进程 ' + $(if ($g.alive) { '在' } else { '没' }) +
                         ' 完成线 ' + $(if ($g.done) { '见过' } else { '没有' }) +
                         ' 画面 ' + $(if ($g.screen) { [string]$g.screen } else { '没问' }) +
                         ' 动作 ' + $(if ($g.action) { [string]$g.action } else { 'none' }) +
                         $(if ($g.why) { ' 原因 ' + [string]$g.why } else { '' }))
            }
        }
    }
    $acts = @($Sv.actions)
    if ($acts.Count) {
        $out += '介入记录(' + $acts.Count + ' 次)：'
        foreach ($a in $acts) {
            $out += ('  ' + [string]$a.ts + ' ' + [string]$a.game + ' ' + [string]$a.kind +
                     ' ' + $(if ($a.ok) { '成' } else { '没成' }) + ' — ' + [string]$a.why)
        }
    } else {
        $out += '这一轮没有介入过(它唯一的干预是重启助手，重启过会记在这里)。'
    }
    return $out
}

# ---------- 起停(主流程调这两个；子进程本体在 tools\supervise.ps1) ----------
function Start-GdSupervisor {
    <# 起一个隐藏的 PowerShell 子进程当监督者，主流程只登记不陪跑。
       已经是有一枚活着(上一轮硬退出没停干净?)就先说一句再补一个 —— 双监督不会互相抢鼠标，
       但会双份通知，所以这里宁可只留新的：老的写进 jsonl 的记录还在。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][string]$Games
    )
    if (-not (Get-GdSupervisorEnabled -S $S)) { return $false }
    $prev = Read-GdSupervisorSummary
    if ($prev -and $prev.pid) {
        try {
            $old = Get-Process -Id ([int]$prev.pid) -ErrorAction SilentlyContinue
            if ($old) { Write-GdLog "上一枚监督进程(PID $($prev.pid))还活着，先停掉它再起新的" -Level WARN; Stop-Process -Id ([int]$prev.pid) -Force -ErrorAction SilentlyContinue }
        } catch { }
    }
    $script = Join-Path (Get-GdRoot) 'tools\supervise.ps1'
    if (-not (Test-Path -LiteralPath $script)) { Write-GdLog '没有 tools\supervise.ps1，这一轮没人监督' -Level WARN; return $false }
    $exe = (Get-Process -Id $PID).Path
    if (-not $exe) { $exe = (Get-Command powershell.exe).Source }
    $meta = [ordered]@{
        pid     = 0
        owner   = $PID
        started = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
        games   = $Games
        updated = ''
        beats   = @()
        actions = @()
        stopped = ''
    }
    try {
        $p = Start-Process -FilePath $exe `
              -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', $script, '-OwnerPid', "$PID", '-Games', $Games) `
              -WorkingDirectory (Get-GdRoot) -WindowStyle Hidden -PassThru
        $meta.pid = $p.Id
        $p.Dispose()
    } catch {
        Write-GdLog "监督进程没起来：$($_.Exception.Message)(这一轮没人监督，主流程不受影响)" -Level WARN
        return $false
    }
    try { Write-GdJson -Path (Get-GdSupervisorStatePath) -Object ([pscustomobject]$meta) } catch { }
    Write-GdLog "监督 agent 上岗(PID $($meta.pid)，每 $(Get-GdSupervisorIntervalSec -S $S) 秒一拍，只在可疑时才问模型)" -Quiet
    return $true
}

function Stop-GdSupervisor {
    <# 收尾：写停拍标记 → 等它自己走完这一拍(它每 5 秒瞄一眼标记，最多再等一拍+余量) → 还不退才停进程。
       先停监督再撤活体标记(modules.ps1 的 finally 里就这个顺序)：它读 running.json 找主日志，
       标记先撤了它会瞬间"找不到被监督的那一轮"，留下一拍假记录。 #>
    param([int]$GraceSeconds = 75)
    $prev = Read-GdSupervisorSummary
    if (-not $prev -or -not $prev.pid) { return $false }
    $stopFile = Get-GdSupervisorStopPath
    try { (Get-Date -Format 'yyyy-MM-dd HH:mm:ss') | Set-Content -LiteralPath $stopFile -Encoding utf8 } catch { }
    $deadline = (Get-Date).AddSeconds($GraceSeconds)
    while ((Get-Date) -lt $deadline) {
        $proc = $null
        try { $proc = Get-Process -Id ([int]$prev.pid) -ErrorAction SilentlyContinue } catch { }
        if (-not $proc) { break }
        Start-Sleep -Seconds 1
    }
    try {
        $proc = Get-Process -Id ([int]$prev.pid) -ErrorAction SilentlyContinue
        if ($proc) { $proc.Kill(); Start-Sleep -Milliseconds 300 }
    } catch { }
    Remove-Item -LiteralPath $stopFile -Force -ErrorAction SilentlyContinue
    # 把"已收工"写回最终读数，面板念得出"几点下班"而不是"进程没了(崩?)"
    try {
        $txt = ''
        $fs = [System.IO.File]::Open((Get-GdSupervisorStatePath), 'Open', 'Read', 'ReadWrite')
        try { $txt = (New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8)).ReadToEnd() } finally { $fs.Dispose() }
        if ($txt) {
            $doc = $txt | ConvertFrom-Json
            $doc.stopped = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
            $doc | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Get-GdSupervisorStatePath) -Encoding utf8
        }
    } catch { }
    return $true
}
