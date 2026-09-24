<#
  tools/panel.ps1 —— 每日清体力的图形状态面板(Windows Forms；不装任何东西，5.1 和 7 都能跑)。

  和菜单的分工：菜单适合"我知道要干哪一件事"，面板适合"瞟一眼今天怎么样"。
  一屏显示：今天会刷什么、两道确认门的状态、定时任务下次几点跑、上一轮结果；
  手边留动作：今天会跑什么(预演) / 开始跑一轮 / 环境体检 / 我打过活动了 / 商店已领空 / 打开日志目录。

  面板只是外壳，判定一律转调现成脚本(daily-clear.ps1 / status.ps1 / run-via-task.ps1)：
    - 不自己算结论，命令行和面板看到的是同一套判定；
    - 参数名运行时探测，所以合并仓库(-Games/-AckArk)和单游戏发布仓库(-Ack)都能用；
    - 开跑要"二次点击"确认，整个面板除了启动失败不会弹任何模态框。

  子进程输出怎么进来的(踩过的坑，别再改回去)：
    不用 Register-ObjectEvent —— 它的 -Body 在另一个作用域里跑，读不到本脚本的 $script: 变量，
    而本仓库开着 Set-StrictMode 1.0，一个未初始化变量就是运行时报错。
    改成：Start-Process 把子进程 stdout/stderr 重定向到 %TEMP% 下两个文件，
    定时器每 800 毫秒按字节偏移增量读(共享读，不锁文件)；
    真正那一轮跑在计划任务里时进度只落在 logs\run-*.log，所以同一个泵也追那个文件。
    按钮回调 / Add_Tick 用的都是本脚本的 ScriptBlock，$script: 变量能正常读到。
    退出码要另开一次句柄才读得到：5.1 的 Start-Process -PassThru 不带活句柄，见 Start-PanelTask 里那段。

  用法：
    双击 panel.cmd                        # 推荐，自动挑可用引擎
    pwsh -File tools\panel.ps1
    powershell -File tools\panel.ps1      # 只有系统自带 PowerShell 的机器
    pwsh -File tools\panel.ps1 -SelfTest       # 不显示窗口：只渲染一次，把状态摘要/帮助行/布局/参数探测打出来，有红项就退出码 1
    pwsh -File tools\panel.ps1 -HeadlessCheck  # 窗口挪到屏幕外，自己起一次预演，验"重定向→定时器泵→忙碌状态"这条链后自动关(零弹窗、零点击)
    game-daily-ark.exe --panel                 # 打包成 exe 后同样能开面板(其它动词见 --help)
#>
[CmdletBinding()]
param(
    [switch]$SelfTest,
    [switch]$HeadlessCheck,
    [switch]$SkipSummary
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $root

. (Join-Path $root 'lib\common.ps1')          # 顺带把控制台编码统一到 UTF-8
$haveDecisions = $false
if (Test-Path -LiteralPath (Join-Path $root 'lib\decisions.ps1')) {
    . (Join-Path $root 'lib\decisions.ps1')    # 只为了读两道门的当前状态，不做判定
    $haveDecisions = $true
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# ---------- 这个仓库里有哪几路、入口脚本叫什么、参数怎么拼 ----------
$script:games = @()
if (Test-Path -LiteralPath (Join-Path $root 'lib\maa.ps1'))  { $script:games += 'ark' }
if (Test-Path -LiteralPath (Join-Path $root 'lib\m9a.ps1'))  { $script:games += 'r1999' }
$script:both = ($script:games.Count -gt 1)

$entryName = ''
foreach ($cand in @('daily-clear.ps1', 'ark-daily.ps1', 'r1999-daily.ps1')) {
    if (Test-Path -LiteralPath (Join-Path $root $cand)) { $entryName = $cand; break }
}
if (-not $entryName) { throw "找不到入口脚本(daily-clear.ps1 / ark-daily.ps1 / r1999-daily.ps1)，面板没法干活。" }
$script:entryPath = Join-Path $root $entryName

$script:dailyParams = @{}
try {
    foreach ($p in (Get-Command $script:entryPath).Parameters.Values) {
        if ($p -and $p.Name) { $script:dailyParams[$p.Name.ToLowerInvariant()] = $true }
    }
} catch { }
function Get-PanelFlag {
    <# 候选参数名里挑这个仓库入口真正有的那个；都没有返回空串，调用方自己降级 #>
    param([string[]]$Candidates)
    foreach ($c in $Candidates) { if ($script:dailyParams.ContainsKey($c.ToLowerInvariant())) { return $c } }
    return ''
}
$flagAck      = Get-PanelFlag @('AckArk', 'Ack')
$flagShopDone = Get-PanelFlag @('ArkShopDone', 'ShopDone')
$flagGames    = Get-PanelFlag @('Games')

# 按钮和提示语里不许出现"只有这台机器/这一份合体工程才成立"的说法：这份文件会原样进两个导出仓库。
$script:elevNote = if ($script:games -contains 'ark') { '，方舟那一路要管理员' } else { '' }

$panelTail = if ($script:both) { '全部' } elseif ($script:games -contains 'ark') { '方舟' } else { '1999' }
$script:idleRunText = '开始跑一轮(' + $panelTail + ')'

# ---------- 子进程用哪个引擎：面板跑在哪个引擎上就用哪个(5.1 的机器不会凭空长出 pwsh) ----------
$script:engine = $null
try { $script:engine = [string](Get-Process -Id $PID).Path } catch { $script:engine = $null }
if (-not $script:engine -and $PSVersionTable.PSVersion.Major -ge 6) {
    try { $script:engine = (Get-Command pwsh.exe -ErrorAction Stop).Source } catch { $script:engine = $null }
}
if (-not $script:engine) { $script:engine = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe' }
if ($script:engine -like '*\WindowsApps\*') {
    <# 商店版(WindowsApps)的 pwsh 不能直接 RunAs，个别机器上连 Start-Process 都会被拦一口；
       有安装版就换成安装版，没有就照旧(跑 dry-run / 触发计划任务都不需要提权)。 #>
    $local = Get-GdLocalPwsh
    if ($local) { $script:engine = $local }
}

# ---------- 状态(StrictMode 1.0：一律先声明再读) ----------
$script:form        = $null
$script:summaryBox  = $null
$script:stampLabel  = $null
$script:runLabel    = $null
$script:runBtn      = $null
$script:ackBtn      = $null
$script:logBox      = $null
$script:timer       = $null
$script:proc        = $null
$script:outPath     = Join-Path $env:TEMP 'game-daily-panel-stdout.txt'
$script:errPath     = Join-Path $env:TEMP 'game-daily-panel-error.txt'
$script:outPos      = 0
$script:errPos      = 0
$script:busy        = $false
$script:confirmAt   = $null
$script:ackAt       = $null
$script:tailPath    = $null
$script:tailPos     = 0
$script:tailSince   = $null
$script:hlDeadline  = $null
$script:hlResult    = $null
$script:hlStats     = @{}
$script:hlTail      = @()

# ---------- 状态摘要(纯读，不判定) ----------
function Get-PanelSummaryText {
    $S = $null
    try { $S = Get-GdSettings -Quiet } catch { }

    $times = @()
    if ($S -and $S.schedule -and $S.schedule.times) { $times = @($S.schedule.times) }
    $prefix = 'GameDaily'
    if ($S -and $S.schedule -and $S.schedule.taskPrefix) { $prefix = [string]$S.schedule.taskPrefix }

    $nextTxt = @()
    foreach ($t in $times) {
        $nm = '{0}-{1}' -f $prefix, (([string]$t) -replace ':', '')
        $task = $null
        $info = $null
        try { $task = Get-ScheduledTask -TaskName $nm; $info = $task | Get-ScheduledTaskInfo } catch { }
        if ($null -eq $info) { $nextTxt += ('{0} 未注册' -f $t); continue }
        $when = '未排期'
        if ($info.NextRunTime -and $info.NextRunTime.Year -gt 2000) { $when = $info.NextRunTime.ToString('MM-dd HH:mm') }
        # State 挂在 Get-ScheduledTask 那个对象上；运行级别在 Principal.RunLevel(适配器上的 RunLevel 是空的)
        $st = [string]$task.State
        $lvl = [string]$task.Principal.RunLevel
        if ($lvl -match 'Highest') { $st += ' 最高权限,免 UAC' } else { $st += ' 普通权限,方舟会弹 UAC' }
        $nextTxt += ('{0} → {1} [{2}]' -f $t, $when, $st)
    }
    if ($nextTxt.Count -eq 0) { $nextTxt = @('没读到时间点(先跑 setup / 装定时)') }

    $lastRun = '这台机器还没跑过'
    $hist = Join-Path (Get-GdDir 'State') 'history.csv'
    if (Test-Path -LiteralPath $hist) {
        try {
            $rows = @(Import-Csv -LiteralPath $hist | Select-Object -Last 2)
            $parsed = @()
            foreach ($r in $rows) {
                $dur = [string]$r.duration_sec
                if ($dur) { $dur = ('{0:N0}s' -f [int]$dur) } else { $dur = '-' }
                $parsed += ('{0} {1}={2} {3}' -f [string]$r.time, [string]$r.game, [string]$r.status, $dur)
            }
            if ($parsed.Count) { $lastRun = $parsed -join '   |   ' }
        } catch { $lastRun = ('history.csv 读不动：' + $_.Exception.Message) }
    }

    $gateTxt = @()
    foreach ($g in $script:games) {
        $ackTxt = '判定库不在，门状态未知'
        $shopTxt = ''
        if ($haveDecisions) {
            $ack = @{}; $shop = @{}
            try { $ack = Get-GdAckMap -Game $g } catch { }
            try { $shop = Get-GdShopDoneMap -Game $g } catch { }
            $ackTxt = '未确认(新活动先跳过)'
            if ($ack.Count) { $ackTxt = '已确认 ' + (($ack.Keys | Sort-Object) -join ',') }
            $shopTxt = '   商店领空: 未标记'
            if ($shop.Count) { $shopTxt = ('   商店领空: 已标记 {0}(剩余理智转长草)' -f (($shop.Keys | Sort-Object) -join ',')) }
        }
        $gateTxt += ('  {0} 活动确认: {1}{2}' -f $g, $ackTxt, $shopTxt)
    }

    $out = @()
    $out += ('这一仓：{0}    引擎：PS {1}    入口：{2}    (子进程用 {3})' -f ($script:games -join ' + '), $PSVersionTable.PSVersion, $entryName, [System.IO.Path]::GetFileName($script:engine))
    $out += ('定时：' + ($nextTxt -join '  |  '))
    $out += ('上一轮：' + $lastRun)
    foreach ($g in $gateTxt) { $out += $g }
    return ($out -join "`r`n")
}

# ---------- 日志区 ----------
function Append-PanelLog {
    param([string]$Text)
    if (-not $script:logBox) { return }
    if ([string]::IsNullOrEmpty($Text)) { return }
    if ($script:logBox.TextLength -gt 160000) {
        $script:logBox.Select(0, ($script:logBox.TextLength - 100000))
        $script:logBox.SelectedText = '……(更早的已滚出窗口，完整内容在日志目录里)……' + "`r`n"
    }
    $script:logBox.AppendText($Text + "`r`n")
    $script:logBox.ScrollToCaret()
}

function Read-PanelNewText {
    <# 从 Pos 起增量读一个可能被别人写着的文本文件；只认整行，半行下次再读 #>
    param([string]$Path, [int]$Pos)
    $res = @{ Lines = @(); Pos = $Pos }
    if (-not $Path) { return $res }
    if (-not (Test-Path -LiteralPath $Path)) { return $res }
    try {
        $fs = [System.IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
        try {
            $len = [long]$fs.Length
            if ($Pos -gt $len) { $Pos = 0 }          # 被重写/截断
            $avail = [int]($len - $Pos)
            if ($avail -le 0) { return $res }
            $null = $fs.Seek($Pos, 'Begin')
            $buf = New-Object byte[] $avail
            $got = $fs.Read($buf, 0, $avail)
            $text = [System.Text.Encoding]::UTF8.GetString($buf, 0, $got)
            $lastNl = $text.LastIndexOf("`n")
            if ($lastNl -lt 0) { return $res }       # 一行都还没写完
            $consumed = $text.Substring(0, ($lastNl + 1))
            $res.Pos = $Pos + [System.Text.Encoding]::UTF8.GetByteCount($consumed)
            $arr = @($consumed -split "`n")
            if ($arr.Count -gt 0 -and $arr[$arr.Count - 1] -eq '') { $arr = @($arr[0..($arr.Count - 2)]) }
            $out = @()
            foreach ($l in $arr) { $out += ($l -replace "`r$", '') }
            $res.Lines = $out
        } finally { $fs.Close() }
    } catch { }
    return $res
}

function Resolve-PanelNewLog {
    <# 一轮刚起来时，比面板启动时刻新的那个 run-*.log 才是它自己的日志 #>
    $dir = Get-GdDir 'Logs'
    if (-not (Test-Path -LiteralPath $dir)) { return $null }
    try {
        $f = @(Get-ChildItem -LiteralPath $dir -Filter 'run-*.log' | Where-Object { $_.LastWriteTime -ge $script:tailSince } | Sort-Object LastWriteTime -Descending | Select-Object -First 1)
        if ($f.Count) { return $f[0].FullName }
    } catch { }
    return $null
}

function Pump-PanelOutput {
    <# 唯一的输出泵：子进程 stdout/stderr + (可选)当轮 run-*.log #>
    $o = Read-PanelNewText -Path $script:outPath -Pos $script:outPos
    $script:outPos = [int]$o.Pos
    foreach ($l in $o.Lines) { Append-PanelLog $l }
    $e = Read-PanelNewText -Path $script:errPath -Pos $script:errPos
    $script:errPos = [int]$e.Pos
    foreach ($l in $e.Lines) { Append-PanelLog ('[stderr] ' + $l) }
    if (-not $script:tailSince) { return }
    if (-not $script:tailPath) {
        $p = Resolve-PanelNewLog
        if ($p) {
            $script:tailPath = $p
            $script:tailPos = 0
            Append-PanelLog ('  跟随日志：' + $p)
        }
    }
    if (-not $script:tailPath) { return }
    $t = Read-PanelNewText -Path $script:tailPath -Pos $script:tailPos
    $script:tailPos = [int]$t.Pos
    foreach ($l in $t.Lines) { Append-PanelLog ('  | ' + $l) }
}

function Set-PanelBusy {
    param([bool]$Busy, [string]$Note = '')
    $script:busy = $Busy
    if ($Busy) {
        $script:runBtn.Text = '正在跑…(点这个按钮停跟随)'
        if (-not $Note) { $Note = '正在跑' }
    } else {
        $script:runBtn.Text = $script:idleRunText
        if (-not $Note) { $Note = '空闲' }
        $script:confirmAt = $null
    }
    $script:runLabel.Text = $Note
}

function Complete-PanelTask {
    Pump-PanelOutput
    $code = $null
    if ($script:proc) { try { $code = $script:proc.ExitCode } catch { $code = $null } }
    if ($null -ne $code) {
        Append-PanelLog ('退出码 ' + [string]$code + '(0=正常；「无可刷活动关卡 / 无进行中活动」也是正常结论)')
    } else {
        Append-PanelLog '子进程已结束(没取到退出码)。'
    }
    if ($script:proc) { try { $script:proc.Dispose() } catch { } }
    $script:proc = $null
    $script:tailSince = $null
    $script:tailPath = $null
    Set-PanelBusy $false
    Refresh-PanelSummary
}

function Stop-PanelChild {
    if ($script:proc) {
        try { if (-not $script:proc.HasExited) { $script:proc.Kill() } } catch { }
        try { $script:proc.Dispose() } catch { }
        $script:proc = $null
    }
    $script:tailSince = $null
    $script:tailPath = $null
    Set-PanelBusy $false
}

function Start-PanelTask {
    param(
        [Parameter(Mandatory)][string]$Script,
        [string[]]$TaskArgs = @(),
        [string]$Title = '',
        [switch]$TailLog
    )
    if ($script:busy -and $script:proc -and -not $script:proc.HasExited) {
        Append-PanelLog '上一个动作还在跑：等它结束，或点"开始跑一轮"那个按钮停掉跟随。'
        return
    }
    if ($script:busy) { Complete-PanelTask }
    if (-not (Test-Path -LiteralPath $Script)) { Append-PanelLog ('找不到要跑的脚本：' + $Script); return }

    $argStr = '-NoProfile -ExecutionPolicy Bypass -File "{0}"' -f $Script
    if (@($TaskArgs).Count) { $argStr += ' ' + ($TaskArgs -join ' ') }
    $label = $Title
    if (-not $label) { $label = [System.IO.Path]::GetFileName($Script) }
    Append-PanelLog ('──── ' + (Get-Date -Format 'HH:mm:ss') + '  ' + $label + ' ────')
    Append-PanelLog ('  ' + [System.IO.Path]::GetFileName($script:engine) + ' ' + $argStr)

    $script:outPos = 0
    $script:errPos = 0
    $script:tailPath = $null
    $script:tailPos = 0
    $script:tailSince = $null
    if ($TailLog) { $script:tailSince = Get-Date }

    $sp = @{
        FilePath               = $script:engine
        ArgumentList           = $argStr
        WorkingDirectory       = $root
        NoNewWindow            = $true
        PassThru               = $true
        RedirectStandardOutput = $script:outPath
        RedirectStandardError  = $script:errPath
    }
    try {
        $p = Start-Process @sp
        <#
          5.1 的坑：Start-Process -PassThru 返回的那个对象不带句柄，进程一退 ExitCode 就是空
          (7 没这个问题)。所以趁它还活着按 PID 自己再开一次句柄 —— 句柄在我们手里，
          进程结束后 .ExitCode 才读得到。抢不过竞态(已经退完了)就退回原对象，宁可少一行退出码也不崩。
        #>
        try {
            $np = [System.Diagnostics.Process]::GetProcessById($p.Id)
            $null = $np.Handle
            $script:proc = $np
        } catch { $script:proc = $p }
        Set-PanelBusy $true
    } catch {
        Append-PanelLog ('启动失败：' + $_.Exception.Message)
        $script:proc = $null
        Set-PanelBusy $false
    }
}

function Refresh-PanelSummary {
    $script:summaryBox.Text = Get-PanelSummaryText
    $script:stampLabel.Text = '状态刷新于 ' + (Get-Date -Format 'HH:mm:ss')
}

function Get-PanelHeadlessStats {
    <# 关窗之前把结论抓下来：Form 一 Close，控件里的文本跟着释放，事后再读只能读到空 #>
    $txt = ''
    $lines = @()
    try { $txt = [string]$script:logBox.Text; $lines = @($script:logBox.Lines) } catch { }
    return @{
        Lines    = $lines.Count
        # 三棵树的开场横幅措辞不同(合体=「每日清体力日常开始」，单游戏仓库=「方舟日常开始」/「1999 日常开始」)，
        # 但都是 Write-GdLog 的 "=== …日常开始(tag=…)" —— 认这个共同后缀，别把合体那一份写死。
        Child    = [bool]($txt -match '日常开始\(tag=')
        Done     = [bool]($txt -match '\[dry-run\] 完成')
        ExitLine = [bool]($txt -match '退出码 \d+')
        ErrLines = @($lines | Where-Object { $_ -like '[[]stderr]*' }).Count
        Idle     = [bool]((-not $script:busy) -and ($script:runBtn.Text -eq $script:idleRunText))
    }
}

# ---------- 控件 ----------
$script:form = New-Object System.Windows.Forms.Form
$script:form.Text = '每日清体力 · 状态面板'
$script:form.StartPosition = 'CenterScreen'
$script:form.ClientSize = New-Object System.Drawing.Size(900, 600)
$fontUI  = New-Object System.Drawing.Font('Microsoft YaHei UI', 9.5)
$fontLog = New-Object System.Drawing.Font('Consolas', 9)
$script:form.Font = $fontUI

$script:summaryBox = New-Object System.Windows.Forms.TextBox
$script:summaryBox.Multiline = $true
$script:summaryBox.ReadOnly = $true
$script:summaryBox.ScrollBars = 'Vertical'
$script:summaryBox.BackColor = [System.Drawing.SystemColors]::Control
$script:summaryBox.Location = New-Object System.Drawing.Point(12, 12)
$script:summaryBox.Size = New-Object System.Drawing.Size(876, 104)
$script:summaryBox.Anchor = 'Top,Left,Right'
$script:form.Controls.Add($script:summaryBox)

$script:stampLabel = New-Object System.Windows.Forms.Label
$script:stampLabel.AutoSize = $true
$script:stampLabel.Location = New-Object System.Drawing.Point(14, 120)
$script:stampLabel.Text = '状态刷新于 -'
$script:form.Controls.Add($script:stampLabel)

$script:runLabel = New-Object System.Windows.Forms.Label
$script:runLabel.AutoSize = $true
$script:runLabel.Location = New-Object System.Drawing.Point(760, 120)
$script:runLabel.Text = '空闲'
$script:runLabel.Anchor = 'Top,Right'
$script:form.Controls.Add($script:runLabel)

function New-PanelButton {
    param([string]$Text, [int]$X, [int]$Y, [int]$W, [int]$H = 32)
    $b = New-Object System.Windows.Forms.Button
    $b.Text = $Text
    $b.Location = New-Object System.Drawing.Point($X, $Y)
    $b.Size = New-Object System.Drawing.Size($W, $H)
    $null = $script:form.Controls.Add($b)
    return $b
}
$w = 148
$gap = 8
$y1 = 146
$btnDry       = New-PanelButton '今天会跑什么'        12 $y1 $w
$script:runBtn = New-PanelButton $script:idleRunText  (12 + $w + $gap) $y1 ($w + 60)
$btnStatus    = New-PanelButton '环境体检'            (12 + 2 * $w + $gap + 60) $y1 $w
$btnRefresh   = New-PanelButton '刷新状态'            (12 + 3 * $w + 2 * $gap + 60) $y1 $w
$y2 = $y1 + 40
$script:ackBtn = New-PanelButton '我打过活动了'       12 $y2 ($w + 60)
$btnShop = New-PanelButton '商店已领空'     (12 + $w + 60 + $gap) $y2 ($w + 60)
$btnLog  = New-PanelButton '打开日志目录'   (12 + 2 * $w + 120 + 2 * $gap) $y2 ($w + 30)
$btnHelp = New-PanelButton '命令行怎么跑'   (12 + 3 * $w + 210 + 3 * $gap) $y2 ($w + 30)

$script:logBox = New-Object System.Windows.Forms.RichTextBox
$script:logBox.ReadOnly = $true
$script:logBox.Font = $fontLog
$script:logBox.BackColor = [System.Drawing.Color]::FromArgb(24, 24, 24)
$script:logBox.ForeColor = [System.Drawing.Color]::Gainsboro
$script:logBox.Location = New-Object System.Drawing.Point(12, 222)
$script:logBox.Size = New-Object System.Drawing.Size(876, 366)
$script:logBox.Anchor = 'Top,Left,Right,Bottom'
$script:form.Controls.Add($script:logBox)

$script:timer = New-Object System.Windows.Forms.Timer
$script:timer.Interval = 800
$script:timer.Add_Tick({
    Pump-PanelOutput
    if ($script:busy -and $script:proc -and $script:proc.HasExited) { Complete-PanelTask }
})

# ---------- 按钮行为(全部转调现成脚本) ----------
$btnDry.Add_Click({
    $a = @('-DryRun', '-Tag', 'manual')
    if ($flagGames -and $script:both) { $a = @('-Games', 'all') + $a }
    Start-PanelTask -Script $script:entryPath -TaskArgs $a -Title '预演今天会跑什么(不启动任何程序)'
})

$script:runBtn.Add_Click({
    if ($script:busy -and $script:proc -and -not $script:proc.HasExited) {
        Stop-PanelChild
        Append-PanelLog '面板不再跟随这一轮。若这一轮是走计划任务起的，本体不受影响，到点还会自己跑完并收尾。'
        return
    }
    if ($script:busy) { Complete-PanelTask }
    $now = Get-Date
    if ($script:confirmAt -and ($now - $script:confirmAt).TotalSeconds -lt 12) {
        $script:confirmAt = $null
        $script:runBtn.Text = $script:idleRunText
        if (Test-Path -LiteralPath (Join-Path $root 'run-via-task.ps1')) {
            $a = @()
            if ($flagGames -and $script:both) { $a = @('-Games', 'all') }
            Start-PanelTask -Script (Join-Path $root 'run-via-task.ps1') -TaskArgs $a -Title '跑一轮(走计划任务，免 UAC)' -TailLog
        } else {
            $a = @('-Tag', 'manual')
            if ($flagGames -and $script:both) { $a = @('-Games', 'all') + $a }
            Start-PanelTask -Script $script:entryPath -TaskArgs $a -Title ('跑一轮(本机直跑' + $script:elevNote + ')') -TailLog
        }
        return
    }
    $script:confirmAt = $now
    $script:runBtn.Text = '再点一次就真的开跑(12 秒内)'
    Append-PanelLog '再点一次这个按钮就开跑。跑完会自动关掉助手、端游和模拟器；中途想停就再点它。'
})

$btnStatus.Add_Click({ Start-PanelTask -Script (Join-Path $root 'status.ps1') -Title '环境体检(只读)' })

$btnRefresh.Add_Click({ Refresh-PanelSummary; Append-PanelLog '已刷新上面的状态。' })

$script:ackBtn.Add_Click({
    if (-not $flagAck) { Append-PanelLog '这个仓库的入口脚本没有"活动确认"参数，这个按钮不做事。'; return }
    $now = Get-Date
    if ($script:ackAt -and ($now - $script:ackAt).TotalSeconds -lt 12) {
        $script:ackAt = $null
        $script:ackBtn.Text = '我打过活动了'
        Start-PanelTask -Script $script:entryPath -TaskArgs @('-' + $flagAck, '-DryRun') -Title '标记本轮活动已打过(不再刷活动图)'
        return
    }
    $script:ackAt = $now
    $script:ackBtn.Text = '再点一次=确认我已打过活动'
    Append-PanelLog '作用：版本更新后你先手动打活动，然后点它，之后到点就只刷日常、不碰活动图。再点一次确认。'
})

$btnShop.Add_Click({
    if (-not $flagShopDone) { Append-PanelLog '这个仓库的入口脚本没有"商店领空"参数，这个按钮不做事。'; return }
    Start-PanelTask -Script $script:entryPath -TaskArgs @('-' + $flagShopDone, '-DryRun') -Title '标记活动商店已领空(剩余理智转长草)'
})

$btnLog.Add_Click({
    try { $null = Start-Process explorer.exe -ArgumentList (Get-GdDir 'Logs') } catch { Append-PanelLog ('打不开日志目录：' + $_.Exception.Message) }
})

$btnHelp.Add_Click({ foreach ($l in (Get-PanelHelpText)) { Append-PanelLog $l } })

# ---------- "命令行怎么跑"：只报这一棵树里真的有的文件(按钮和 -SelfTest 共用这一份) ----------
$script:helpRows = @()
function Add-PanelHelpRow {
    <# Need 里点名的文件有一个不在，这一行就不显示 —— 面板要原样进两个单游戏导出仓库，
       那边没有 start.cmd / tools\menu.ps1，照着提示敲等于去撞一个不存在的文件。 #>
    param([string]$Label, [string]$Cmd, [string[]]$Need = @())
    foreach ($n in $Need) { if (-not (Test-Path -LiteralPath (Join-Path $root $n))) { return } }
    $script:helpRows += @{ Label = $Label; Cmd = $Cmd }
}
function Get-PanelHelpText {
    $out = @('这些按钮对应的命令行(在 ' + $root + ' 下执行，powershell.exe 换成 pwsh 也一样能跑)：')
    foreach ($r in $script:helpRows) { $out += ('  ' + (Set-PanelLabelWidth $r.Label 16) + $r.Cmd) }
    $out += '完整参数说明看 README.md。'
    return $out
}
function Set-PanelLabelWidth {
    <# 日志区是 Consolas：中文占两列。-f 的宽度按字符数算，'跑一轮' 和 '我打过活动了' 会排不齐，
       所以按显示宽度补空格(只是好看，不影响这条命令能不能敲)。 #>
    param([string]$Text, [int]$Width)
    $cols = 0
    foreach ($ch in [char[]]$Text) { $cols += $(if ([int]$ch -ge 0x2E80) { 2 } else { 1 }) }
    if ($cols -ge $Width) { return $Text + ' ' }
    return $Text + (' ' * ($Width - $cols))
}
function Get-PanelHelpRowMisses {
    <# 帮助行里点名的 .ps1/.cmd 必须真在这棵树里。上面按 Need 滤过一遍，这一条查的是"写行的时候漏了 Need" ——
       只有 -SelfTest 会在导出目录里被跑到，所以它能挡住"发布仓库里教玩家敲一个不存在的命令"。 #>
    $miss = @()
    foreach ($r in $script:helpRows) {
        foreach ($m in [regex]::Matches([string]$r.Cmd, '[A-Za-z0-9_\-\\.]+\.(?:ps1|cmd)')) {
            $name = $m.Value.TrimStart('\', '.')
            $hit = (Test-Path -LiteralPath (Join-Path $root $name)) -or
                   (Test-Path -LiteralPath (Join-Path $root (Join-Path 'tools' $name)))
            if (-not $hit) { $miss += ($r.Label + ' → ' + $name) }
        }
    }
    return $miss
}

$timesTxt = 'HH:mm,HH:mm'
try {
    $cfgS = Get-GdSettings -Quiet
    if ($cfgS.schedule -and $cfgS.schedule.times) { $timesTxt = (@($cfgS.schedule.times) -join ',') }
} catch { }
Add-PanelHelpRow '今天会跑什么' ".\$entryName -DryRun" @($entryName)
Add-PanelHelpRow '跑一轮'      '.\run-via-task.ps1'    @('run-via-task.ps1')
if ($script:both) { Add-PanelHelpRow '只跑一路' ".\$entryName -Games ark   (或 r1999)" @($entryName) }
Add-PanelHelpRow '环境体检'    '.\status.ps1'          @('status.ps1')
Add-PanelHelpRow '首次设置'    '.\setup.ps1 -DryRun'   @('setup.ps1')
Add-PanelHelpRow '装定时'      '.\install-task.ps1'    @('install-task.ps1')
Add-PanelHelpRow '改时间点'    ".\tools\reschedule-daily.ps1 -Times $timesTxt" @('tools\reschedule-daily.ps1')
if ($flagAck)      { Add-PanelHelpRow '我打过活动了' ('.\' + $entryName + ' -' + $flagAck + ' -DryRun') @($entryName) }
if ($flagShopDone) { Add-PanelHelpRow '商店领空'     ('.\' + $entryName + ' -' + $flagShopDone + ' -DryRun') @($entryName) }
Add-PanelHelpRow '图形面板'    '.\panel.cmd   (或 exe 加 --panel)' @('panel.cmd')
Add-PanelHelpRow '命令行菜单'  '.\start.cmd   (双击=打开这个菜单)' @('start.cmd', 'tools\menu.ps1')

Refresh-PanelSummary
Append-PanelLog '提示：面板只转达，不替你判定 —— 上面的结论和命令行里跑同一套脚本得到的一样。'
Append-PanelLog ('日志目录：' + (Get-GdDir 'Logs'))
Append-PanelLog '想跑就点"开始跑一轮"，它会让你再点一次确认；不想动窗口就放着，到点计划任务自己跑。'

# ---------- 两种不显示给人看的路径 ----------
if ($SelfTest) {
    <# 自动化验证用：不进消息循环、不显示窗口，只把渲染结果打到控制台 #>
    Write-Host '---- panel 自检(不显示窗口) ----'
    Write-Host (Get-PanelSummaryText)
    $btns = @()
    foreach ($ct in $script:form.Controls) { if ($ct -is [System.Windows.Forms.Button]) { $btns += [string]$ct.Text } }
    Write-Host ('控件 ' + @($script:form.Controls).Count + ' 个   按钮: ' + ($btns -join ' / '))
    Write-Host ('子进程引擎: ' + $script:engine)
    Write-Host ('参数探测: Ack=' + $flagAck + '  ShopDone=' + $flagShopDone + '  Games=' + $flagGames + '  入口=' + $entryName + '  判定库=' + $haveDecisions)
    Write-Host ('帮助行 ' + @($script:helpRows).Count + ' 条(按钮「命令行怎么跑」会打这些)：')
    foreach ($l in (Get-PanelHelpText)) { Write-Host ('  ' + $l) }
    $miss = @(Get-PanelHelpRowMisses)
    Write-Host ('帮助行里点名的文件都在: ' + [string]($miss.Count -eq 0) + $(if ($miss.Count) { '  → ' + ($miss -join ' , ') } else { '' }))
    Pump-PanelOutput
    Write-Host ('日志区行数: ' + $script:logBox.Lines.Count)
    Write-Host ('输出泵可用: ' + [bool](Get-Command Read-PanelNewText -ErrorAction SilentlyContinue))
    <# 布局自检：没人会用自动化去肉眼看窗口，所以把"按钮互相压住/超出客户区"这种问题变成可判定的检查 #>
    $cl = $script:form.ClientSize
    $bad = @()
    $cts = @($script:form.Controls)
    for ($i = 0; $i -lt $cts.Count; $i++) {
        $a = $cts[$i].Bounds
        if ($a.X -lt 0 -or $a.Y -lt 0 -or ($a.X + $a.Width) -gt $cl.Width -or ($a.Y + $a.Height) -gt $cl.Height) {
            $bad += ('越出客户区: ' + [string]$cts[$i].GetType().Name + ' ' + [string]$a)
        }
        for ($j = $i + 1; $j -lt $cts.Count; $j++) {
            $b = $cts[$j].Bounds
            if ($a.X -lt ($b.X + $b.Width) -and $b.X -lt ($a.X + $a.Width) -and
                $a.Y -lt ($b.Y + $b.Height) -and $b.Y -lt ($a.Y + $a.Height)) {
                $bad += ('互相压住: ' + [string]$a + ' × ' + [string]$b)
            }
        }
    }
    if ($bad.Count) { foreach ($x in $bad) { Write-Host ('  布局问题 ' + $x) } }
    Write-Host ('布局自检: ' + $(if ($bad.Count) { '有问题(见上)' } else { 'OK，控件不重叠且都在客户区内' }))
    $script:form.Dispose()
    if ($bad.Count -or $miss.Count) { Write-Host 'panel -SelfTest 有红项。' -ForegroundColor Red; exit 1 }
    exit 0
}

if ($HeadlessCheck) {
    <#
      自动化验证用：窗口挪到屏幕外(不闪、不占任务栏)，自己起一次"预演"(dry-run，不启动任何游戏/助手)，
      等子进程退出后把结论打到控制台再关 —— 验的是"重定向 + 定时器泵 + 忙碌状态机"这条链，全程零弹窗、零人工点击。
    #>
    $script:form.StartPosition = 'Manual'
    $script:form.Location = New-Object System.Drawing.Point(-16000, -16000)
    $script:form.ShowInTaskbar = $false
    $script:hlDeadline = (Get-Date).AddSeconds(150)
    $script:form.Add_Shown({
        Append-PanelLog '[headless] 自动验证：起一次预演(dry-run，不会启动任何游戏/助手)'
        $a = @('-DryRun', '-Tag', 'manual')
        if ($flagGames -and $script:both) { $a = @('-Games', 'all') + $a }
        Start-PanelTask -Script $script:entryPath -TaskArgs $a -Title 'headless 预演'
    })
    $check = New-Object System.Windows.Forms.Timer
    $check.Interval = 1000
    $check.Add_Tick({
        Pump-PanelOutput                       # 这一条链本身就是被测对象：泵一次，把子进程输出吸进日志区
        $txt = $script:logBox.Text
        $ended = ($txt -match '退出码 ') -or ($txt -match '没取到退出码')
        if ((-not $script:busy) -and ($txt -match '\[dry-run\] 完成') -and $ended) {
            $script:hlStats = Get-PanelHeadlessStats
            $script:hlResult = 'PASS'
            $script:form.Close()
            return
        }
        if ((Get-Date) -gt $script:hlDeadline) {
            Append-PanelLog '[headless] 超时，停止跟随。'
            $script:hlStats = Get-PanelHeadlessStats
            $script:hlTail = @($script:logBox.Lines | Select-Object -Last 20)
            $script:hlResult = 'TIMEOUT'
            Stop-PanelChild
            $script:form.Close()
        }
    })
    $check.Start()
    $script:timer.Start()
    $null = [System.Windows.Forms.Application]::Run($script:form)
    $check.Stop()
    $script:timer.Stop()
    Stop-PanelChild

    <# 窗口一关，控件里的文本就跟着释放了 —— 结论必须在关窗前抓下来，这里只负责打印 #>
    Write-Host '---- panel headless 验证(子进程输出 -> 重定向文件 -> 定时器泵) ----'
    if ($script:hlStats.Count -eq 0) { Write-Host '没抓到任何统计：定时器一次都没进回调，这条链直接判失败。'; exit 1 }
    Write-Host ('日志区行数：  ' + [string]$script:hlStats.Lines)
    Write-Host ('看到子进程行：' + [string]$script:hlStats.Child)
    Write-Host ('看到结论行：  ' + [string]$script:hlStats.Done)
    Write-Host ('看到退出码：  ' + [string]$script:hlStats.ExitLine)
    Write-Host ('stderr 行数： ' + [string]$script:hlStats.ErrLines)
    Write-Host ('忙碌已复位：  ' + [string]$script:hlStats.Idle)
    Write-Host ('结论：        ' + $script:hlResult)
    if ($script:hlResult -ne 'PASS') {
        Write-Host '末 20 行日志：'
        foreach ($l in $script:hlTail) { Write-Host ('  ' + $l) }
        exit 1
    }
    if (-not $script:hlStats.Child) { Write-Host 'PASS 但没看到子进程输出：泵没生效，判失败。'; exit 1 }
    exit 0
}

$script:timer.Start()
$null = [System.Windows.Forms.Application]::Run($script:form)
$script:timer.Stop()
Stop-PanelChild
Write-Host '面板已关闭。定时任务不受影响，到点照样自己跑。'
exit 0
