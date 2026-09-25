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
    [switch]$DumpTree,
    [switch]$DumpSummary,
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
# 框架层：面板要的是"这棵树上有几款游戏、各叫什么、要不要管理员"，这三样都在模块清单里。
# 只注册不点业务库 —— 面板自己一行游戏逻辑都不跑，全部动作都是子进程调入口。
$haveModules = $false
if (Test-Path -LiteralPath (Join-Path $root 'lib\modules.ps1')) {
    . (Join-Path $root 'lib\modules.ps1')
    $haveModules = $true
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# ---------- 这个仓库里有哪几路、入口脚本叫什么、参数怎么拼 ----------
$script:games = @()
if ($haveModules) {
    [void](Import-GdModules -Quiet)
    $script:games = @(Get-GdModuleIds -AvailableOnly)
}
if (-not $script:games.Count) {
    # 回落(工程里没有 modules.ps1 的老树/被裁坏)：按助手库在不在探测，和拆分前逐字一致。
    # 正常路径永远走不到这里 —— 面板要用的是清单里的显示名和提权标记，那些只有注册表给得出。
    if (Test-Path -LiteralPath (Join-Path $root 'lib\maa.ps1'))  { $script:games += 'ark' }
    if (Test-Path -LiteralPath (Join-Path $root 'lib\m9a.ps1'))  { $script:games += 'r1999' }
}
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
# "哪一路要管理员"现在从模块清单的 RequiresAdmin 读，所以方舟仓库里是"，方舟那一路要管理员"、
# 1999 仓库里是空串 —— 同一段代码，措辞由这一路自己的声明决定。
$adminNames = @()
if ($haveModules) {
    foreach ($id in $script:games) {
        $m = Get-GdModule -Id $id
        if ($m -and $m.RequiresAdmin) { $adminNames += [string]$m.ShortName }
    }
}
$script:elevNote = ''
if ($adminNames.Count) { $script:elevNote = '，' + ($adminNames -join '/') + '那一路要管理员' }
elseif (-not $haveModules -and ($script:games -contains 'ark')) { $script:elevNote = '，方舟那一路要管理员' }   # 老树回落

# 单路时按钮上写这一路的名字，两路以上写"全部" —— 这个名字来自清单的 ShortName，
# -HeadlessCheck 会拿 $script:idleRunText 比对，所以清单里的短名一改，面板文案跟着改，不会两处不一致。
if ($script:both) { $panelTail = '全部' }
elseif ($script:games.Count -eq 1) {
    if ($haveModules) {
        # 单路仓库的按钮文案 = 这一路自己的短名(方舟 / 1999)，来自清单
        $one = Get-GdModule -Id $script:games[0]
        $panelTail = if ($one -and [string]$one.ShortName) { [string]$one.ShortName } else { [string]$script:games[0] }
    }
    else { $panelTail = if ($script:games[0] -eq 'ark') { '方舟' } else { '1999' } }   # 老树回落
}
else { $panelTail = '全部' }
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
$script:stampLabel  = $null
$script:runLabel    = $null
$script:runBtn      = $null
$script:ackBtn      = $null
$script:logBox      = $null
$script:cardTimes   = $null
$script:cardLast    = $null
$script:cardEnv     = $null
$script:cardGates   = $null
$script:timer       = $null
$script:proc        = $null
$script:page        = 'all'          # 左侧导航当前停在哪儿：'all' 或某个模块 id
$script:navBtns     = @{}            # id -> 导航按钮(刷新时要按这一路的状态改灯)
$script:navStripes  = @{}            # id -> 导航项左边那条 3px 强调条(只有选中那项点亮，比"整块换底色"清楚)
$script:navKind     = @{}            # id -> 这一路此刻该亮什么色(ok/warn/err/dim)，由摘要行的 Kind 得来
$script:pageTitle   = $null          # 头部那行页标题(跟着 $script:page 变)
<# 重定向文件带上本进程的 PID：定时器一上来就会从 0 开始读这两个文件，
   用固定名字的话，双击面板会先把上一次会话(甚至上一轮)的输出原样重播一遍，看着像"刚开跑就有日志"。 #>
$script:outPath     = Join-Path $env:TEMP ('game-daily-panel-stdout-{0}.txt' -f $PID)
$script:errPath     = Join-Path $env:TEMP ('game-daily-panel-error-{0}.txt' -f $PID)
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
<#
  一份读数、两处用：卡片按行着色渲染(-ShowForm)，-SelfTest 打成纯文本比对。
  所以这里只出"行 + 这一行该是什么颜色"(Kind=ok/warn/err/dim)，颜色不算结论、卡片也不下结论 ——
  结论仍然只来自 Get-GdSettings / Get-ScheduledTask / history.csv / 判定库这四样。
  Kind 的判据是"这一行要不要人操心"：红=已经出过问题，黄=还没就绪(未确认/未注册/会弹 UAC)，绿=就绪，灰=纯说明。
#>
function Get-PanelSummaryParts {
    $S = $null
    try { $S = Get-GdSettings -Quiet } catch { }

    $times = @()
    if ($S -and $S.schedule -and $S.schedule.times) { $times = @($S.schedule.times) }
    $prefix = 'GameDaily'
    if ($S -and $S.schedule -and $S.schedule.taskPrefix) { $prefix = [string]$S.schedule.taskPrefix }

    $timeRows = @()
    foreach ($t in $times) {
        $nm = '{0}-{1}' -f $prefix, (([string]$t) -replace ':', '')
        $task = $null
        $info = $null
        try { $task = Get-ScheduledTask -TaskName $nm; $info = $task | Get-ScheduledTaskInfo } catch { }
        if ($null -eq $info) { $timeRows += @{ Text = ('{0} 未注册 → 跑 install-task.ps1' -f $t); Kind = 'warn' }; continue }
        $when = '未排期'
        if ($info.NextRunTime -and $info.NextRunTime.Year -gt 2000) { $when = $info.NextRunTime.ToString('MM-dd HH:mm') }
        # State 挂在 Get-ScheduledTask 那个对象上；运行级别在 Principal.RunLevel(适配器上的 RunLevel 是空的)。
        # 卡片一行只有 272px 宽，所以把"运行级别"压成两个字(免/会弹 UAC)，别写成「最高权限,免 UAC」——那会被省略号吃掉。
        $st = [string]$task.State
        $lvl = [string]$task.Principal.RunLevel
        $kind = 'ok'
        $run = '会弹 UAC'
        if ($lvl -match 'Highest') { $run = '免 UAC' } else { $kind = 'warn' }
        if ($st -notmatch 'Ready|Running') { $kind = 'warn' }
        $timeRows += @{ Text = ('{0} → {1}·{2}·{3}' -f $t, $when, $st, $run); Kind = $kind }
    }
    if (-not $timeRows.Count) { $timeRows = @(@{ Text = '没读到时间点(先跑 setup / 装定时)'; Kind = 'warn' }) }

    <# 每个游戏只留最近一条：以前是"最后两行"，两路一起跑就正好是同一轮的两行，一旦某一路连着跑过两次就看错轮次 #>
    $lastRows = @()
    $hist = Join-Path (Get-GdDir 'State') 'history.csv'
    if (Test-Path -LiteralPath $hist) {
        try {
            $byGame = @{}
            foreach ($r in @(Import-Csv -LiteralPath $hist)) { if ($r.game) { $byGame[[string]$r.game] = $r } }
            foreach ($g in ($byGame.Keys | Sort-Object)) {
                $r = $byGame[$g]
                $dur = [string]$r.duration_sec
                if ($dur) { $dur = ('{0:N0}s' -f [int]$dur) } else { $dur = '-' }
                $st = [string]$r.status
                $kind = 'ok'
                if ($st -match 'stall|timeout|error|fail|not-started|unknown') { $kind = 'err' }
                elseif ($st -notmatch '^(ok|done|完成)') { $kind = 'warn' }
                $time = [string]$r.time
                if ($time -match '\d{4}-\d{2}-\d{2}\s+(\d{2}:\d{2})') { $time = $matches[1] }
                <# 卡片一行只有 272px：mode 和 status 都长的时候会被省略号吃掉，那种情况下宁可丢掉 mode(status 才是要看的) #>
                $txt = ('{0} {1} → {2} {3}' -f $time, $g, $st, $dur)
                $withMode = ('{0} {1} {2}→{3} {4}' -f $time, $g, [string]$r.mode, $st, $dur)
                if ($withMode.Length -le 38) { $txt = $withMode }
                $lastRows += @{ Text = $txt; Kind = $kind; Game = [string]$g }
            }
        } catch { $lastRows = @(@{ Text = ('history.csv 读不动：' + $_.Exception.Message); Kind = 'warn' }) }
    }
    if (-not $lastRows.Count) { $lastRows = @(@{ Text = '这台机器还没跑过'; Kind = 'dim' }) }

    $gateRows = @()
    foreach ($g in $script:games) {
        # 每条门状态都带上 Game(= 模块 id)：左侧导航按这一路过滤卡片时靠它，不再靠"文本前缀像不像"。
        if (-not $haveDecisions) { $gateRows += @{ Text = ($g + ' 判定库不在，门状态未知'); Kind = 'warn'; Game = [string]$g }; continue }
        $ack = @{}; $shop = @{}
        try { $ack = Get-GdAckMap -Game $g } catch { }
        try { $shop = Get-GdShopDoneMap -Game $g } catch { }
        if ($ack.Count) { $gateRows += @{ Text = (($g + ' 活动确认：已确认 ' + (($ack.Keys | Sort-Object) -join ','))); Kind = 'ok'; Game = [string]$g } }
        else { $gateRows += @{ Text = ($g + ' 活动确认：未确认 → 新活动先跳过'); Kind = 'warn'; Game = [string]$g } }
        if ($shop.Count) { $gateRows += @{ Text = (($g + ' 商店领空：已标记 ' + (($shop.Keys | Sort-Object) -join ',') + '(剩余理智转长草)')); Kind = 'ok'; Game = [string]$g } }
        else { $gateRows += @{ Text = ($g + ' 商店领空：未标记'); Kind = 'dim'; Game = [string]$g } }
    }

    $envRows = @()
    $keys = @()
    try { $keys = @(Get-GdRequiredPathKeys $root) } catch { }
    $notReady = @()
    foreach ($k in $keys) {
        $v = $null
        try { $v = $S.paths.$k } catch { }
        if (-not (Test-GdPathReady $v)) { $notReady += $k }
    }
    <# 环境卡片只有 264px 宽：一行只说一件事，键名多了截到 3 个并报总数，否则整行会被省略号吞掉 #>
    if ($notReady.Count) {
        $show = @($notReady)
        $tail = ''
        if ($show.Count -gt 3) { $tail = ('… 共 ' + $show.Count + ' 项'); $show = @($show[0..2]) }
        $envRows += @{ Text = ('配置还差：' + ($show -join ' , ') + $tail + ' → 跑 setup.ps1'); Kind = 'err' }
    }
    elseif ($keys.Count) { $envRows += @{ Text = ('助手路径 ' + $keys.Count + ' 项全部就绪'); Kind = 'ok' } }
    $envRows += @{ Text = ('这一仓：' + ($script:games -join ' + ')); Kind = 'dim' }
    $envRows += @{ Text = ('入口：' + $entryName); Kind = 'dim' }
    <# 版本号只取 Major.Minor：5.1 的完整版本是 5.1.26100.94281，写全会把这一行顶出卡片宽度 #>
    $psv = '{0}.{1}' -f [int]$PSVersionTable.PSVersion.Major, [int]$PSVersionTable.PSVersion.Minor
    $envRows += @{ Text = ('面板 PS ' + $psv + ' · 子进程 ' + [System.IO.Path]::GetFileName($script:engine)); Kind = 'dim' }

    return @{ Times = $timeRows; Last = $lastRows; Gates = $gateRows; Env = $envRows }
}

# 纯文本版：-SelfTest 用，也是"卡片渲染不出来时"的兜底说法
function Get-PanelSummaryText {
    $p = Get-PanelSummaryParts
    $out = @()
    $out += ('这一仓：{0}    引擎：PS {1}    入口：{2}    (子进程用 {3})' -f ($script:games -join ' + '), $PSVersionTable.PSVersion, $entryName, [System.IO.Path]::GetFileName($script:engine))
    $out += '定时：'
    foreach ($r in $p.Times) { $out += ('  ' + [string]$r.Text) }
    $out += '上一轮：'
    foreach ($r in $p.Last) { $out += ('  ' + [string]$r.Text) }
    $out += '两道门：'
    foreach ($r in $p.Gates) { $out += ('  ' + [string]$r.Text) }
    $out += '环境：'
    foreach ($r in $p.Env) { $out += ('  ' + [string]$r.Text) }
    return ($out -join "`r`n")
}

<# 机器可读版：tools\panel-cs\Panel.cs(图形外壳)唯一的读数入口。
   行式协议、'|' 分隔，C# 那边按前 4 个 '|' 切(正文允许再带 '|')：
     HEADER|<仓内游戏,空格分隔>|<入口脚本>|<引擎 exe 文件名>|<引擎绝对路径>|<root>
     GAME|<id>|<显示名>
     TIME|<kind>|<文本>            LAST|<kind>|<game或''>|<文本>
     GATE|<kind>|<game>|<文本>     ENV|<kind>|<文本>
     PROBE|<kind>|<game>|<文本>    (某一路自己的路径探针；分页时环境卡要用，总览页不显示)
     FLAGS|<有Ack参数的参数名或''>|<有ShopDone参数的参数名或''>|<有Games参数则yes否则''>|<run-via-task.ps1在则yes否则''>|<要管理员的显示名,逗号分隔>
     HELP|<标签>|<命令>
   设计约束：这里只转达 Get-PanelSummaryParts 的既有结论，一个新判定都不加 ——
   判定改了只需要改上面一处，PowerShell 面板和 C# 面板就永远说同一句话。 #>
function Dump-PanelSummaryMachine {
    $p = Get-PanelSummaryParts
    Write-Output ('HEADER|' + ($script:games -join ' ') + '|' + $entryName + '|' +
                  [System.IO.Path]::GetFileName($script:engine) + '|' + [string]$script:engine + '|' + $root)
    foreach ($g in $script:games) {
        $shown = [string]$g
        if ($haveModules) {
            $mm = Get-GdModule -Id ([string]$g)
            if ($mm -and [string]$mm.ShortName) { $shown = [string]$mm.ShortName }
        }
        Write-Output ('GAME|' + [string]$g + '|' + $shown)
    }
    foreach ($r in $p.Times)  { Write-Output ('TIME|' + [string]$r.Kind + '|' + [string]$r.Text) }
    foreach ($r in $p.Last)   { Write-Output ('LAST|' + [string]$r.Kind + '|' + [string]$r.Game + '|' + [string]$r.Text) }
    foreach ($r in $p.Gates)  { Write-Output ('GATE|' + [string]$r.Kind + '|' + [string]$r.Game + '|' + [string]$r.Text) }
    foreach ($r in $p.Env)    { Write-Output ('ENV|' + [string]$r.Kind + '|' + [string]$r.Text) }
    foreach ($g in $script:games) {
        foreach ($r in @(Get-PanelModuleEnvRows -Id ([string]$g))) {
            Write-Output ('PROBE|' + [string]$r.Kind + '|' + [string]$g + '|' + [string]$r.Text)
        }
    }
    Write-Output ('FLAGS|' + [string]$flagAck + '|' + [string]$flagShopDone + '|' +
                  $(if ($flagGames) { 'yes' } else { '' }) + '|' +
                  $(if (Test-Path -LiteralPath (Join-Path $root 'run-via-task.ps1')) { 'yes' } else { '' }) + '|' +
                  ($adminNames -join ','))
    foreach ($r in $script:helpRows) { Write-Output ('HELP|' + [string]$r.Label + '|' + [string]$r.Cmd) }
}

# ---------- 分页(左侧导航按模块过滤卡片) ----------
<# "总览"就是重排前那一页：四张卡把这一仓所有行都摆出来。
   选中某一路时只留那一路的行(上一轮 / 两道门按行带的 Game 过滤，环境那一路的探针由模块清单给)，
   定时任务是"按计划任务时间点"的、不分路 → 每一页都原样留着。
   单游戏仓库也走同一套：那边的行本来就全属于这一路，过滤等于没过滤，唯一区别是环境卡多出这一路的探针 ——
   所以"总览 / 这一路"两条导航在哪个仓库里都不是摆设(以前按 $script:both 直接短路，单游戏仓库点了等于没点)。
   过滤后可能一张卡一行都不剩，那会让 -SelfTest 的"卡片不许空"红，也容易看着像面板坏了，
   所以每个会被过滤的槽位都补一行说明。 #>
function Get-PanelPageRows {
    param([Parameter(Mandatory)]$Parts, [AllowEmptyString()][string]$Page = 'all')
    if (-not $Page -or $Page -eq 'all') { return $Parts }
    $keep = { param($rows) @($rows | Where-Object { -not $_.Game -or [string]$_.Game -eq $Page }) }
    $last = @(& $keep $Parts.Last)
    if (-not $last.Count) { $last = @(@{ Text = ('{0} 这一路这台机器还没跑过' -f (Get-PanelPageTitle -Page $Page)); Kind = 'dim' }) }
    $gates = @(& $keep $Parts.Gates)
    if (-not $gates.Count) { $gates = @(@{ Text = '没读到门状态(判定库不在？)'; Kind = 'warn' }) }
    <# 环境卡：先"这一仓的路径就绪没有"，再这一路自己的探针，最后才是面板自己的说明行(入口名/引擎版本)。
       顺序这样排是因为卡片装不下时会从尾部省掉 —— 停在某一路时最不该被省掉的就是那一路的探针。 #>
    $mod = @(Get-PanelModuleEnvRows -Id $Page)
    $head = @(@($Parts.Env) | Where-Object { $_.Kind -ne 'dim' })
    $tail = @(@($Parts.Env) | Where-Object { $_.Kind -eq 'dim' -and [string]$_.Text -notmatch '^这一仓' })
    return @{
        Times = $Parts.Times
        Last  = $last
        Gates = $gates
        Env   = @(@($head) + @($mod) + @($tail))
    }
}

function Get-PanelModuleEnvRows {
    <# 这一路自己那几条路径就绪没有 —— 由模块清单的 EnvProbes 钩子给(和 daily-clear 开跑前探的是同一份清单，
        面板不另写一套路径知识)。注册表没装载/这一路没装时返回空，让上面那行"读不到"兜着。 #>
    param([string]$Id)
    $out = @()
    if (-not (Get-Command Get-GdModule -ErrorAction SilentlyContinue)) { return $out }
    $m = Get-GdModule -Id $Id
    if (-not $m -or -not (Test-GdModuleInstalled -Module $m)) { return $out }
    $S = $null
    try { $S = Get-GdSettings -Quiet } catch { }
    $rows = $null
    try { $rows = Get-GdModuleEnvRows -Module $m -S $S } catch { $rows = $null }
    if ($null -eq $rows) { $out += @{ Text = "$Id 这一路的环境清单读不到"; Kind = 'warn' }; return $out }
    foreach ($label in @($rows.Keys)) {
        $p = [string]$rows[$label]
        if (Test-GdPathReady $p) { $out += @{ Text = ("$label 就绪"); Kind = 'ok' } }
        else { $out += @{ Text = ("$label 没填或不存在 → 跑 setup.ps1"); Kind = 'err' } }
    }
    return $out
}

function Get-PanelPageTitle {
    param([AllowEmptyString()][string]$Page)
    if (-not $Page -or $Page -eq 'all') { return '总览' }
    if (Get-Command Get-GdModule -ErrorAction SilentlyContinue) {
        $m = Get-GdModule -Id $Page
        if ($m) { return [string]$m.Name }
    }
    return [string]$Page
}

# ---------- 日志区 ----------
<#
  逐行着色：子进程那些行本来就是 Write-GdLog 的 `[时间][LEVEL] 正文` 格式，
  所以颜色不是"我再判一次结论"，而是把日志自己带的 LEVEL 翻译成眼睛能扫的东西。
  时间戳刷成灰、级别标签隐去(颜色已经表达了它)，正文按级别上色；面板自己写的说明行没有级别 → 走 info/dim。
  判据只有这一个正则，别的文字一律不动 —— 别加"看到 失败 就涂红"这类猜测，那会把正文里引用的原文涂错色。
#>
function Get-PanelLogKind {
    param([string]$Text)
    if ($Text -like '[[]stderr]*') { return 'err' }
    if ($Text -match '^\s*[=─]{4,}') { return 'dim' }
    if ($Text -match '^────') { return 'dim' }
    if ($Text -match '\[ERROR\]') { return 'err' }
    if ($Text -match '\[WARN\]') { return 'warn' }
    if ($Text -match '\[OK\]') { return 'ok' }
    if ($Text -match '\[STEP\]') { return 'step' }
    if ($Text -match '^退出码 0(\(|$)') { return 'ok' }
    if ($Text -match '^退出码 [1-9]') { return 'err' }
    return 'info'
}

function Add-PanelColoredText {
    param([string]$Text, [System.Drawing.Color]$Color)
    if ([string]::IsNullOrEmpty($Text)) { return }
    $box = $script:logBox
    $box.Select($box.TextLength, 0)
    $box.SelectionColor = $Color
    $box.AppendText($Text)
}

function Append-PanelLog {
    param([string]$Text)
    if (-not $script:logBox) { return }
    if ([string]::IsNullOrEmpty($Text)) { return }
    if ($script:logBox.TextLength -gt 160000) {
        $script:logBox.Select(0, ($script:logBox.TextLength - 100000))
        $script:logBox.SelectedText = '……(更早的已滚出窗口，完整内容在日志目录里)……' + "`r`n"
    }
    $kind = Get-PanelLogKind $Text
    $t = $script:logTheme[$kind]
    <# 面板自己写的说明行(以 '  | ' 开头的是转抄日志、其余是原样说明)；能吃掉"时间戳 + 级别标签"这一段前缀 #>
    $m = [regex]::Match($Text, '^(?<pre>(?:  \| )?(?:(?:\[[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}\]|\[[0-9]{2}:[0-9]{2}:[0-9]{2}\])+)(?:\[[A-Z]+\])? )(?<body>.*)$')
    if ($m.Success) {
        Add-PanelColoredText ([string]$m.Groups['pre'].Value) $script:cLogDim
        Add-PanelColoredText ([string]$m.Groups['body'].Value) $t
    } else {
        Add-PanelColoredText $Text $t
    }
    $script:logBox.AppendText("`r`n")
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
    # 头部那颗状态灯：空闲=灰，正在跑=琥珀(不闪、不弹框，只是让人扫一眼就知道别碰)
    Set-PanelChip -Text $Note -Kind $(if ($Busy) { 'busy' } else { 'idle' })
}

function Get-PanelModuleLight {
    <# 导航那一行左边的小圆点亮什么：这一路"要不要人看一眼"，不是面板下的结论 ——
        全部取自 Get-PanelSummaryParts 已经算好的 Kind(上一轮红=红；门没确认=黄；跑通过=绿；没记录=灰)。 #>
    param([Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)]$Parts)
    $last = @($Parts.Last | Where-Object { $_.Game -and [string]$_.Game -eq $Id })
    foreach ($r in $last) { if ([string]$r.Kind -eq 'err') { return 'err' } }
    foreach ($r in @($Parts.Gates | Where-Object { $_.Game -and [string]$_.Game -eq $Id })) {
        if ([string]$r.Kind -eq 'err') { return 'err' }
        if ([string]$r.Kind -eq 'warn') { return 'warn' }
    }
    if ($last.Count) { return 'ok' }
    return 'dim'
}
function Get-PanelWorstLight {
    <# "总览"那一行该亮什么：取所有已注册模块里最扎眼的那一档(红 > 黄 > 绿 > 灰)。
        这不是新判定 —— 四档本来就来自各卡片行的 Kind，这里只是把同一批颜色折叠一下，好让导航灯不撒谎。 #>
    param([Parameter(Mandatory)]$Kinds)
    foreach ($k in @($Kinds)) { if ([string]$k -eq 'err') { return 'err' } }
    foreach ($k in @($Kinds)) { if ([string]$k -eq 'warn') { return 'warn' } }
    foreach ($k in @($Kinds)) { if ([string]$k -eq 'ok') { return 'ok' } }
    return 'dim'
}
function Set-PanelNavLight {
    param([Parameter(Mandatory)][string]$Id, [string]$Kind = 'dim')
    $b = $script:navBtns[$Id]
    if (-not $b) { return }
    # 认不得的 Kind(键缺失/以后新增的颜色名)一律回落到灰：这里画错只是灯不好看，但 $theme[空] 拿到 $null 会让赋值炸。
    if (@('ok', 'warn', 'err') -notcontains [string]$Kind) { $Kind = 'dim' }
    $on = ([string]$script:page -eq $Id)
    # 灯色说"这一路要不要人看一眼"，实心/空心说"当前停在哪一页" —— 选中项一定实心，否则换页看不出换来换去
    $b.Text = ([string]$b.Tag) + $(if ($on -or $Kind -ne 'dim') { '   ●' } else { '   ○' })
    $b.ForeColor = $(if ($Kind -eq 'dim') { $script:theme['dim'] } else { $script:theme[[string]$Kind] })
    $b.BackColor = $(if ($on) { $script:cNavOn } else { $script:cNav })
    # 左边那条 3px 强调条：选中才亮。整块换底色在深色导航带上太含蓄，光靠底色经常看不出停在哪一条。
    $st = $script:navStripes[$Id]
    if ($st) { $st.BackColor = $(if ($on) { $script:cAccent } else { $script:cNav }) }
}
function Select-PanelPage {
    <# 换页只做两件事：记住这一页，然后让 Refresh-PanelSummary 重画(它自己会重画导航选中态和页标题)。
        不在这里重复画一遍 —— 以前两边都画，改一处漏一处就会出现"点了但高亮没动"。
        不改任何命令行参数、不碰判定 —— 玩家在这一页点「跑一轮」仍然是整仓一起跑(和重排前一致)。 #>
    param([AllowEmptyString()][string]$Page)
    if (-not $Page) { $Page = 'all' }
    $script:page = $Page
    Refresh-PanelSummary
}
function Refresh-PanelSummary {
    $p = Get-PanelSummaryParts
    $script:navKind = @{}
    foreach ($g in $script:games) { $script:navKind[[string]$g] = Get-PanelModuleLight -Id ([string]$g) -Parts $p }
    $script:navKind['all'] = Get-PanelWorstLight -Kinds @(@($script:navKind.Values))
    foreach ($k in @($script:navBtns.Keys)) { Set-PanelNavLight -Id ([string]$k) -Kind ([string]$script:navKind[[string]$k]) }
    if ($script:pageTitle) { $script:pageTitle.Text = Get-PanelPageTitle -Page $script:page }
    $r = Get-PanelPageRows -Parts $p -Page $script:page
    Set-PanelCardLines -Card $script:cardTimes -Rows $r.Times
    Set-PanelCardLines -Card $script:cardLast -Rows $r.Last
    Set-PanelCardLines -Card $script:cardGates -Rows $r.Gates -MaxLines 5
    Set-PanelCardLines -Card $script:cardEnv -Rows $r.Env -MaxLines 6
    $script:stampLabel.Text = '状态刷新于 ' + (Get-Date -Format 'HH:mm:ss')
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
<#
  版式(2026-09-25 重做，之前是"一整块灰底 TextBox + 一排系统默认按钮")：
    头部深色带：标题 + 一句"面板只转达不判定" + 右边状态灯(空闲/正在跑) + 刷新时刻
    四张白卡：定时任务 / 上一轮 / 两道确认门 / 环境 —— 每张卡里的行按"要不要人操心"着色，
             绿=就绪、黄=还没就绪(未注册·未确认·会弹 UAC)、红=已经出过问题(上一轮 stalled/配置没填全)
    动作区分两档：实心蓝=唯一那个会真开跑的；描边白=只读或改状态的；小片=两道门和开目录
    日志区：深色，逐行按 Write-GdLog 自带的级别上色(时间戳刷灰，级别标签隐去——颜色就是它)
  只用 System.Windows.Forms + System.Drawing，不引任何外部依赖，5.1 和 7 同一份代码。
  布局仍然全部写死坐标 + Anchor(不引 TableLayout)：-SelfTest 那条"控件不许互相压住、不许越出父容器"
  的检查要靠 Bounds 判定，绝对坐标才判得准。
#>
function New-GdColor { param([int]$R,[int]$G,[int]$B) return [System.Drawing.Color]::FromArgb($R,$G,$B) }
$script:cLogDim  = New-GdColor 125 133 144
<# 配色(2026-09-25 第三次重排，方舟风深色科幻)：整窗深炭底 + 方舟橙做唯一强调色(对应端游 UI 里
   那圈橙色辉光)，语义色一律往"深底上看得清"那一档走 —— 卡片行是浅色文字，绿/黄/红要亮,不能拿浅底那套深色凑。
   只有 info(正文) 和 dim(说明) 是中性色，其余三色都带语义：改任何一色都会同时改四张卡的读感。 #>
$script:theme = @{
    ok     = New-GdColor  74 222 128
    warn   = New-GdColor 251 191  36
    err    = New-GdColor 248 113 113
    info   = New-GdColor 226 232 240
    step   = New-GdColor 255 184  77
    dim    = New-GdColor 148 163 184     # 深底上的"纯说明"行：比正文暗一档就够，再浅就和 ok 分不开了
    logOk   = New-GdColor 126 231 135
    logWarn = New-GdColor 227 179  65
    logErr  = New-GdColor 255 123 114
    logStep = New-GdColor 255 195  96
    logInfo = New-GdColor 201 209 217
    logDim  = New-GdColor 125 133 144
}
$script:cFace    = New-GdColor  34  33  38      # 卡片：从冷蓝灰压成暖炭，配橙色强调才像端游那套深灰+橙
$script:cCardTitle = New-GdColor 200 190 178    # 卡片小标题(比上一版亮一档：8.5pt 的字再灰就只剩"一片浅灰")
$script:cWindow  = New-GdColor  22  22  26      # 窗口底
$script:cNav     = New-GdColor  15  15  18      # 左侧导航带(比窗口底再深一档，靠描边分不出来)
$script:cNavSub  = New-GdColor 150 141 130
$script:cLine    = New-GdColor  66  60  54      # 卡片描边
$script:cAccent  = New-GdColor 240 130  32      # 主动作：方舟橙(端游按钮/辉光那一档)，深炭底上就是唯一光源
$script:cAccentHi= New-GdColor 255 158  64
$script:cAccentLo= New-GdColor 202 104  18
$script:cHover   = New-GdColor  46  43  47      # 次级按钮悬停
$script:cDown    = New-GdColor  58  53  55
$script:cNavOn   = New-GdColor  56  41  27      # 导航项选中底(橙压暗：和主动作同族，但别盖过按钮)
$script:cLogBack = New-GdColor  12  12  15
<# 卡片里的行几何：起始 y / 行高 / 行距。Set-PanelCardLines 按这三个算"这张卡装得下几行"，
   所以改这里就等于改了四张卡的容量 —— 别再回到"高度写死、行数另写一处"的老路。
   三个数是配着 132 的卡高定的：32 + 4*20 + 18 = 130 ≤ 132，正好 5 行；
   行距从 18 放到 20 是为了让 8.5pt 的省略行不至于和上一条粘住，代价就是不能再多一行。 #>
$script:rowY0    = 32
$script:rowH     = 18
$script:rowPitch = 20
<# 日志区是深底，卡片那套绿/黄/红在上面太暗 → 同一套语义换一组亮色，别拿卡片色去涂日志 #>
$script:logTheme = @{
    ok   = $script:theme['logOk']
    warn = $script:theme['logWarn']
    err  = $script:theme['logErr']
    step = $script:theme['logStep']
    info = $script:theme['logInfo']
    dim  = $script:theme['logDim']
}

# 字体：中文界面字 + 日志等宽字。这台机器上没有 YaHei UI 也不会报错，GDI+ 自己退回默认字体
$script:fontUI      = New-Object System.Drawing.Font('Microsoft YaHei UI', 9.5)
$script:fontTitle   = New-Object System.Drawing.Font('Microsoft YaHei UI', 13.5, [System.Drawing.FontStyle]::Bold)
$script:fontSection = New-Object System.Drawing.Font('Microsoft YaHei UI', 8.5, [System.Drawing.FontStyle]::Bold)
$script:fontSmall   = New-Object System.Drawing.Font('Microsoft YaHei UI', 8.5)
$script:fontLog     = New-Object System.Drawing.Font('Consolas', 9.5)

function New-PanelLabel {
    <# 定宽标签一律关 AutoSize、开 AutoEllipsis(这样 -SelfTest 能按 Bounds 判布局)。
       但有个坑(2026-09-25 截图才发现，四张卡的小标题全都"没渲染出来")：
       AutoEllipsis 的 Label 只要 Height < Font.Height，GDI 就一个字都不画 —— 不是截半，是整行消失。
       这台机器上 YaHei UI 8.5 的 Font.Height=16、9.5 的=17，所以 15px 高的行是隐形的。
       -SelfTest 里那条"矮到装不下字体"的检查就是钉这个的。 #>
    param($Parent, [string]$Text, [int]$X, [int]$Y, [int]$W = 0, [int]$H = 0,
          $Color = $null, $Font = $null, [string]$Align = 'Left', [string]$Anchor = '')
    $l = New-Object System.Windows.Forms.Label
    $l.Text = $Text
    $l.Location = New-Object System.Drawing.Point($X, $Y)
    if ($W -gt 0 -and $H -gt 0) {
        $l.AutoSize = $false
        $l.Size = New-Object System.Drawing.Size($W, $H)
        $l.AutoEllipsis = $true
    } else { $l.AutoSize = $true }
    if ($Color) { $l.ForeColor = $Color }
    if ($Font)  { $l.Font = $Font }
    if ($Align -eq 'Right') { $l.TextAlign = 'TopRight' }
    if ($Anchor) { $l.Anchor = $Anchor }
    $null = $Parent.Controls.Add($l)
    return $l
}

function Set-PanelRound {
    <# 圆角只给"尺寸固定"的东西用(按钮、状态灯)：卡片是横向锚定的，改了宽度圆角就错位 #>
    param($Ctl, [int]$Radius = 7)
    $w = [int]$Ctl.Width; $h = [int]$Ctl.Height; $d = $Radius * 2
    if ($w -le $d -or $h -le $d) { $Ctl.Region = $null; return }
    $gp = New-Object System.Drawing.Drawing2D.GraphicsPath
    $null = $gp.AddArc(0, 0, $d, $d, 180, 90)
    $null = $gp.AddArc($w - $d, 0, $d, $d, 270, 90)
    $null = $gp.AddArc($w - $d, $h - $d, $d, $d, 0, 90)
    $null = $gp.AddArc(0, $h - $d, $d, $d, 90, 90)
    $null = $gp.CloseFigure()
    $old = $Ctl.Region
    $Ctl.Region = New-Object System.Drawing.Region($gp)
    if ($old) { try { $old.Dispose() } catch { } }
    try { $gp.Dispose() } catch { }
}

function Set-PanelBorder {
    <# 白底容器(卡片 / 动作区 / 日志区)贴在略深的窗口底上几乎没有边界感 → 各自画一圈 1px 描边。
       只画方角、不切圆角：切圆要设 Region，而带 Top,Right 锚定的容器一改窗口宽度 Region 就过期，
       在 Paint 里改 Region 又会自触发重绘 —— 得不偿失。 #>
    param($Ctl)
    if (-not $Ctl) { return }
    $Ctl.Add_Paint({
        param($sender, $pe)
        $pen = New-Object System.Drawing.Pen($script:cLine)
        try { $pe.Graphics.DrawRectangle($pen, 0, 0, ($sender.Width - 1), ($sender.Height - 1)) }
        finally { $pen.Dispose() }
    })
}

function Get-PanelDescendants {
    <# 面板现在是"卡片/动作区/日志区各是一个 Panel，控件装在它们里面"，
       所以凡是"把所有控件过一遍"的地方(按钮清单、布局自检)都得递归，不能再只看 form.Controls #>
    param($Parent)
    $out = @()
    foreach ($c in $Parent.Controls) {
        $out += $c
        foreach ($d in (Get-PanelDescendants $c)) { $out += $d }
    }
    return $out
}

function New-PanelCard {
    param([int]$X, [int]$Y, [int]$W, [int]$H, [string]$Title, [string]$Anchor = 'Top,Left')
    $p = New-Object System.Windows.Forms.Panel
    $p.Location = New-Object System.Drawing.Point($X, $Y)
    $p.Size = New-Object System.Drawing.Size($W, $H)
    $p.BackColor = $script:cFace
    $p.Anchor = $Anchor
    Set-PanelBorder $p
    $null = $script:form.Controls.Add($p)
    # 标题前那一小块强调色：卡片标题只有 8.5pt，深底上光靠字色分不出"这是标题"，给个固定的视觉锚点
    $sq = New-Object System.Windows.Forms.Panel
    $sq.Location = New-Object System.Drawing.Point(14, 13)
    $sq.Size = New-Object System.Drawing.Size(3, 13)
    $sq.BackColor = $script:cAccent
    $sq.Anchor = 'Top,Left'
    $sq.Tag = 'accent'
    $null = $p.Controls.Add($sq)
    $t = New-PanelLabel -Parent $p -Text $Title.ToUpper() -X 24 -Y 10 -W ($W - 38) -H 17 `
                        -Color $script:cCardTitle -Font $script:fontSection
    $t.Tag = 'title'
    $t.Anchor = 'Top,Left,Right'
    return $p
}

function Set-PanelCardLines {
    <# 卡片里的行：一条一个 Label(定宽、超出省略号)，这样才能逐行着色。
       行与行只按 y 排、宽度都等于卡片内宽 → 结构上不可能互相压住(-SelfTest 会继续钉这条)

       行数上限不写死在调用方：由 $script:rowY0 / $script:rowH / $script:rowPitch 和卡片高度算
       (第 i 行底 = rowY0 + rowH + i*rowPitch，不能超出高度)，所以 $MaxLines 只是"最多说到第几条"。
       为什么必须算：分页后"环境"这一路会多出模块自己的探针行，写死 6 就会把最后一行画到卡片外面，
       而 -SelfTest 的"越出容器"是唯一能发现它的自动化闸门 —— 让它自己少画一行、把余下条数报进省略行。 #>
    param($Card, [object[]]$Rows, [int]$MaxLines = 4)
    if (-not $Card) { return }
    foreach ($c in @($Card.Controls)) {
        if ($c.Tag -eq 'line') { $Card.Controls.Remove($c); try { $c.Dispose() } catch { } }
    }
    $rows = @($Rows)
    $w = [int]$Card.ClientSize.Width - 28
    $y0 = [int]$script:rowY0; $rh = [int]$script:rowH; $pitch = [int]$script:rowPitch
    $ch = [int]$Card.ClientSize.Height                                   # PowerShell 只有 [int] 方括号转换，别写 C# 式 (int)
    $cap = [int][Math]::Floor(($ch - $y0 - $rh) / $pitch) + 1            # 装得下的最多行数
    if ($cap -lt 1) { $cap = 1 }
    $n = $rows.Count
    if ($n -gt $MaxLines) { $n = $MaxLines }
    if ($n -gt $cap) { $n = $cap }
    if ($rows.Count -gt $n -and $n -ge $cap) { $n = $n - 1 }   # 末尾还要留一行给"…另有 N 条"
    if ($n -lt 0) { $n = 0 }
    $y = $y0
    for ($i = 0; $i -lt $n; $i++) {
        $r = $rows[$i]
        $kind = 'info'
        if ($r.Kind) { $kind = [string]$r.Kind }
        $l = New-PanelLabel -Parent $Card -Text ([string]$r.Text) -X 14 -Y $y -W $w -H $rh -Color $script:theme[$kind]
        $l.Tag = 'line'
        $l.Anchor = 'Top,Left,Right'
        $y += $pitch
    }
    if ($rows.Count -gt $n) {
        $l = New-PanelLabel -Parent $Card -Text ('…另有 ' + ($rows.Count - $n) + ' 条，命令行跑 status.ps1 看全部') `
                            -X 14 -Y $y -W $w -H $rh -Color $script:theme['dim']
        $l.Tag = 'line'
        $l.Anchor = 'Top,Left,Right'
    }
}

$script:form = New-Object System.Windows.Forms.Form
$script:form.Text = '每日清体力 · 状态面板'
$script:form.StartPosition = 'CenterScreen'
$script:form.ClientSize = New-Object System.Drawing.Size(1140, 730)
$script:form.BackColor = $script:cWindow
$script:form.Font = $script:fontUI
$script:form.MinimumSize = New-Object System.Drawing.Size(($script:form.Width), ($script:form.Height - 90))
<# 窗体图标 + 品牌区头像 = tools\assets\icon.png(阿米娅 Q 版头像)。Bitmap→Icon 走 GetHicon：
   FromHandle 只是包一层、不复制像素，所以 $script: 里必须同时扣住 Bitmap 和 Icon 两个引用，
   也别自己 DestroyIcon(表单还在用这个句柄)——面板是短命进程，退出时系统统一回收。
   资产没导出/解码失败就维持默认图标、标题顶回头像位：面板是给人看状态的，
   不该因一张图缺失而起不来，也不该留个 52px 的空洞，整段静默降级。 #>
$script:iconOk = $false
try {
    $iconPng = Join-Path $PSScriptRoot 'assets\icon.png'
    if (Test-Path -LiteralPath $iconPng) {
        $script:iconBmp = New-Object System.Drawing.Bitmap $iconPng
        $script:formIcon = [System.Drawing.Icon]::FromHandle($script:iconBmp.GetHicon())
        $script:form.Icon = $script:formIcon
        $script:iconOk = $true
    }
} catch { }

<# ---- 版式：左侧导航带 + 右侧内容区(参考 MAA) ----
   内容区整体右移 $railW，宽度 CW 维持重排前的 916 —— 卡片内部坐标(标题 14/行 14)因此一个字都不用改。
   导航带里的"这一路"项由模块注册表生成：几款游戏就几条，单游戏仓库只有一条(那时也没有分页可言)。 #>
$script:railW = 200
$script:x0    = 212                       # 内容区左边界 = 导航带 + 12 留白
$script:cw    = 916                       # 内容区宽度(= 重排前的窗口内容宽)
$script:fontPage = New-Object System.Drawing.Font('Microsoft YaHei UI', 12, [System.Drawing.FontStyle]::Bold)

$rail = New-Object System.Windows.Forms.Panel
$rail.Location = New-Object System.Drawing.Point(0, 0)
$rail.Size = New-Object System.Drawing.Size($script:railW, 730)
$rail.BackColor = $script:cNav
$rail.Anchor = 'Top,Left,Bottom'
$null = $script:form.Controls.Add($rail)
<# 品牌区头像：和窗体图标同一张 assets\icon.png，Zoom 等比缩放。图标段加载失败($script:iconOk=false)时
   标题/副标顶回 X 20/21，头像框整个隐藏——不留"文字凭空右移 52px"的空洞。 #>
$script:avatar = New-Object System.Windows.Forms.PictureBox
$script:avatar.Location = New-Object System.Drawing.Point(20, 16)
$script:avatar.Size = New-Object System.Drawing.Size(44, 44)
$script:avatar.SizeMode = 'Zoom'
$script:avatar.Anchor = 'Top,Left'
$script:avatar.Tag = 'accent'
$null = $rail.Controls.Add($script:avatar)
$brandTitle = New-PanelLabel -Parent $rail -Text '每日清体力' -X 72 -Y 18 -Color ([System.Drawing.Color]::White) -Font $script:fontTitle
$brandSub    = New-PanelLabel -Parent $rail -Text '只转达 · 不判定' -X 73 -Y 50 -Color $script:cNavSub -Font $script:fontSmall
if ($script:iconOk) {
    $script:avatar.Image = $script:iconBmp
    $script:avatar.Visible = $true
} else {
    $script:avatar.Visible = $false
    $brandTitle.Location = New-Object System.Drawing.Point(20, 18)
    $brandSub.Location   = New-Object System.Drawing.Point(21, 50)
}
# 品牌区和导航之间画一条分隔线 + 一个"导航"小标：不然左侧带从上到下是一整块同色，眼睛找不到分组
$sep = New-Object System.Windows.Forms.Panel
$sep.Location = New-Object System.Drawing.Point(20, 76)
$sep.Size = New-Object System.Drawing.Size(($script:railW - 40), 1)
$sep.BackColor = $script:cLine
$sep.Anchor = 'Top,Left,Right'
$sep.Tag = 'accent'
$null = $rail.Controls.Add($sep)
$null = New-PanelLabel -Parent $rail -Text '导 航' -X 21 -Y 86 -Color $script:cNavSub -Font $script:fontSection
$script:railNavY = 112                    # 导航项从这里往下排(建按钮时用它)

# ---- 头部：当前页 / 一句话定位 / 状态灯 / 刷新时刻 ----
$hdr = New-Object System.Windows.Forms.Panel
$hdr.Location = New-Object System.Drawing.Point($script:x0, 12)
$hdr.Size = New-Object System.Drawing.Size($script:cw, 64)
$hdr.BackColor = $script:cNav
$hdr.Anchor = 'Top,Left,Right'
$null = $script:form.Controls.Add($hdr)
$script:pageTitle = New-PanelLabel -Parent $hdr -Text (Get-PanelPageTitle -Page $script:page) -X 20 -Y 12 -Color ([System.Drawing.Color]::White) -Font $script:fontPage
$null = New-PanelLabel -Parent $hdr -Text '结论与命令行跑同一套脚本得到的一样 · 要点鼠标请先看底部日志' `
                       -X 21 -Y 42 -Color $script:cNavSub -Font $script:fontSmall
$script:runLabel = New-PanelLabel -Parent $hdr -Text '空闲' -X 792 -Y 12 -W 106 -H 26 `
                                  -Color $script:theme['dim'] -Font $script:fontSection -Align 'Right'
$script:runLabel.TextAlign = 'MiddleCenter'
$script:runLabel.BackColor = $script:cHover
$script:runLabel.Anchor = 'Top,Right'
Set-PanelRound -Ctl $script:runLabel -Radius 12
$script:stampLabel = New-PanelLabel -Parent $hdr -Text '状态刷新于 -' -X 606 -Y 43 -W 292 -H 17 `
                                    -Color $script:cNavSub -Font $script:fontSmall -Align 'Right'
$script:stampLabel.Anchor = 'Top,Right'

function Set-PanelChip {
    param([string]$Text, [string]$Kind = 'idle')
    if (-not $script:runLabel) { return }
    $script:runLabel.Text = $Text
    if ($Kind -eq 'busy') {
        $script:runLabel.BackColor = New-GdColor  58  46  16      # 深底上的"正在跑"：黄字配暗黄底，不用系统高亮色
        $script:runLabel.ForeColor = $script:theme['warn']
    } else {
        $script:runLabel.BackColor = $script:cHover
        $script:runLabel.ForeColor = $script:theme['dim']
    }
    Set-PanelRound -Ctl $script:runLabel -Radius 12
}

# ---- 四张状态卡 ----
# 132 高 = 标题(10+17) + 最多 5 行(32 起每行 20，最后一行底 130) + 留白 2。
# 每张卡实际画几行由 Set-PanelCardLines 按容器高度算(见那里的注释)，$MaxLines 只是想说到第几条 —— 加行不必再改高度。
$script:cardTimes = New-PanelCard $script:x0         86 300 132 '定时任务' 'Top,Left'
$script:cardLast  = New-PanelCard ($script:x0 + 312) 86 300 132 '上一轮'   'Top,Left'
$script:cardEnv   = New-PanelCard ($script:x0 + 624) 86 292 132 '环境'     'Top,Right'
$script:cardGates = New-PanelCard $script:x0        226 $script:cw 132 '两道确认门(新活动要先你手动打过 / 商店领空后不再刷活动)' 'Top,Left,Right'

# ---- 动作区：主动作只有一个，其余分两档 ----
$act = New-Object System.Windows.Forms.Panel
$act.Location = New-Object System.Drawing.Point($script:x0, 366)
$act.Size = New-Object System.Drawing.Size($script:cw, 104)
$act.BackColor = $script:cFace
$act.Anchor = 'Top,Left,Right'
Set-PanelBorder $act
$null = $script:form.Controls.Add($act)

function New-PanelButton {
    param($Parent, [string]$Text, [int]$X, [int]$Y, [int]$W, [int]$H = 42, [string]$Kind = 'secondary')
    $b = New-Object System.Windows.Forms.Button
    $b.Text = $Text
    $b.Location = New-Object System.Drawing.Point($X, $Y)
    $b.Size = New-Object System.Drawing.Size($W, $H)
    $b.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
    $b.UseVisualStyleBackColor = $false
    $b.Cursor = [System.Windows.Forms.Cursors]::Hand
    $b.Font = $script:fontUI
    switch ($Kind) {
        'primary' {
            $b.BackColor = $script:cAccent
            $b.ForeColor = [System.Drawing.Color]::White
            $b.FlatAppearance.BorderSize = 0
            $b.FlatAppearance.MouseOverBackColor = $script:cAccentHi
            $b.FlatAppearance.MouseDownBackColor = $script:cAccentLo
            $b.Font = New-Object System.Drawing.Font('Microsoft YaHei UI', 10.5, [System.Drawing.FontStyle]::Bold)
        }
        'nav' {
            # 左侧导航项：平时和导航带同色(看着像文字)，悬停/选中才浮出底色 —— MAA 就是这么处理次级导航的
            $b.BackColor = $script:cNav
            $b.ForeColor = $script:theme['info']
            $b.Font = $script:fontUI
            $b.TextAlign = 'MiddleLeft'
            $b.Padding = New-Object System.Windows.Forms.Padding(12, 0, 0, 0)   # 左边让给 3px 强调条 + 留白
            $b.FlatAppearance.BorderSize = 0
            $b.FlatAppearance.MouseOverBackColor = $script:cHover
            $b.FlatAppearance.MouseDownBackColor = $script:cDown
        }
        'chip' {
            $b.BackColor = $script:cFace
            $b.ForeColor = $script:theme['info']
            $b.Font = $script:fontSmall
            $b.FlatAppearance.BorderColor = $script:cLine
            $b.FlatAppearance.BorderSize = 1
            $b.FlatAppearance.MouseOverBackColor = $script:cHover
            $b.FlatAppearance.MouseDownBackColor = $script:cDown
        }
        default {
            $b.BackColor = $script:cFace
            $b.ForeColor = $script:theme['info']
            $b.FlatAppearance.BorderColor = $script:cLine
            $b.FlatAppearance.BorderSize = 1
            $b.FlatAppearance.MouseOverBackColor = $script:cHover
            $b.FlatAppearance.MouseDownBackColor = $script:cDown
        }
    }
    $null = $Parent.Controls.Add($b)
    Set-PanelRound -Ctl $b -Radius 8
    return $b
}

# ---- 左侧导航项：总览 + 注册表里有几款游戏就几条(单游戏仓库里只有"总览 + 那一款") ----
<# 条目文字取自模块清单的 ShortName，页 id 存在 AccessibleName 里给点击回调用 ——
   不在这里写死 ark / r1999：加一款游戏只需要往 lib\ 里放清单，面板自己长出来。
   注册表没装载成(缺 lib\modules.ps1)时退回 $script:games 那串 id，导航仍然可用，只是名字是 id。 #>
$navItems = @(@{ Id = 'all'; Label = '总览' })
foreach ($g in $script:games) {
    $id = [string]$g
    $shown = $id
    if (Get-Command Get-GdModule -ErrorAction SilentlyContinue) {
        $mm = Get-GdModule -Id $id
        if ($mm -and [string]$mm.ShortName) { $shown = [string]$mm.ShortName }
    }
    $navItems += @{ Id = $id; Label = $shown }
}
for ($i = 0; $i -lt $navItems.Count; $i++) {
    $it = $navItems[$i]
    $nb = New-PanelButton -Parent $rail -Text ([string]$it.Label) -X 12 -Y ($script:railNavY + $i * 40) -W ($script:railW - 24) -H 34 -Kind 'nav'
    $nb.Tag = [string]$it.Label
    $nb.AccessibleName = [string]$it.Id
    # 强调条做成按钮的孩子：跟着按钮走(选中态只改一条颜色)，也不用自己维护"第几项在哪像素"
    $st = New-Object System.Windows.Forms.Panel
    $st.Location = New-Object System.Drawing.Point(0, 8)
    $st.Size = New-Object System.Drawing.Size(3, 18)
    $st.BackColor = $script:cNav
    $st.Tag = 'navstripe'
    $null = $nb.Controls.Add($st)
    $script:navStripes[[string]$it.Id] = $st
    # 页 id 用 sender 取(不用 $this)：$this 只在"事件脚本块"这一种上下文里有定义，写成 param($s,$e) 才是显式的，
    # 也才让 -SelfTest 能拿 Invoke-PanelClick 把这条点击链真演一遍(见下面的导航断言)。
    $nb.Add_Click({ param($s, $e) Select-PanelPage -Page ([string]$s.AccessibleName) })
    $script:navBtns[[string]$it.Id] = $nb     # 'all' 也要存：不存就没人给它重画选中底色，离开总览后它还留着高亮
}
$navHintY = $script:railNavY + $navItems.Count * 40 + 10
$null = New-PanelLabel -Parent $rail -Text '导航只是换个角度看同一批' -X 21 -Y $navHintY -W ($script:railW - 30) -H 17 -Color $script:cNavSub -Font $script:fontSmall
$null = New-PanelLabel -Parent $rail -Text '读数，不改变要跑什么' -X 21 -Y ($navHintY + 17) -W ($script:railW - 30) -H 17 -Color $script:cNavSub -Font $script:fontSmall
<# 导航带底下这两行：一是把"这一台跑在什么上面"说清楚(截图给人看问题时很有用)，
   二是让左侧带不至于一半是空的。锚在底部，所以窗口拉高它也跟着贴着。
   文字要短：这两行宽只有 railW-30，写成整句会被省略号吃掉半截(截图里实测过)。 #>
$null = New-PanelLabel -Parent $rail -Text ('面板 PS ' + ('{0}.{1}' -f [int]$PSVersionTable.PSVersion.Major, [int]$PSVersionTable.PSVersion.Minor) + `
                                               ' · ' + [System.IO.Path]::GetFileName($script:engine)) `
                                               -X 21 -Y 684 -W ($script:railW - 30) -H 17 -Color $script:cNavSub -Font $script:fontSmall -Anchor 'Bottom,Left,Right'
$null = New-PanelLabel -Parent $rail -Text ('这一仓 ' + ($script:games -join ' + ')) `
                                               -X 21 -Y 705 -W ($script:railW - 30) -H 17 -Color $script:cNavSub -Font $script:fontSmall -Anchor 'Bottom,Left,Right'

$script:runBtn = New-PanelButton -Parent $act -Text $script:idleRunText -X 16 -Y 16 -W 214 -H 44 -Kind 'primary'
$btnDry     = New-PanelButton -Parent $act -Text '今天会跑什么'  -X 238 -Y 16 -W 150
$btnStatus  = New-PanelButton -Parent $act -Text '环境体检'      -X 396 -Y 16 -W 150
$btnRefresh = New-PanelButton -Parent $act -Text '刷新状态'      -X 554 -Y 16 -W 120
$null = New-PanelLabel -Parent $act -Text '开跑前会让你再点一次确认(12 秒内)' -X 690 -Y 28 -W 214 -H 20 `
                        -Color $script:theme['dim'] -Font $script:fontSmall -Align 'Right'
$script:ackBtn = New-PanelButton -Parent $act -Text '我打过活动了' -X 16  -Y 68 -W 170 -H 30 -Kind 'chip'
$btnShop  = New-PanelButton -Parent $act -Text '商店已领空' -X 194 -Y 68 -W 166 -H 30 -Kind 'chip'
$btnLog   = New-PanelButton -Parent $act -Text '打开日志目录' -X 368 -Y 68 -W 150 -H 30 -Kind 'chip'
$btnHelp  = New-PanelButton -Parent $act -Text '命令行怎么跑' -X 526 -Y 68 -W 150 -H 30 -Kind 'chip'

# ---- 日志区(深色) ----
$logCard = New-Object System.Windows.Forms.Panel
$logCard.Location = New-Object System.Drawing.Point($script:x0, 478)
$logCard.Size = New-Object System.Drawing.Size($script:cw, 240)
$logCard.BackColor = $script:cFace
$logCard.Anchor = 'Top,Left,Right,Bottom'
Set-PanelBorder $logCard
$null = $script:form.Controls.Add($logCard)
$null = New-PanelLabel -Parent $logCard -Text '运行日志' -X 14 -Y 10 -W 200 -H 17 `
                       -Color $script:cCardTitle -Font $script:fontSection
$null = New-PanelLabel -Parent $logCard -Text '开跑后会同时跟随 logs\run-*.log' -X 586 -Y 10 -W 316 -H 17 `
                       -Color $script:theme['dim'] -Font $script:fontSmall -Align 'Right'
$script:logBox = New-Object System.Windows.Forms.RichTextBox
$script:logBox.ReadOnly = $true
$script:logBox.Font = $script:fontLog
$script:logBox.BackColor = $script:cLogBack
$script:logBox.ForeColor = $script:theme['logInfo']
$script:logBox.BorderStyle = [System.Windows.Forms.BorderStyle]::None
$script:logBox.Location = New-Object System.Drawing.Point(14, 30)
$script:logBox.Size = New-Object System.Drawing.Size(888, 196)
$script:logBox.Anchor = 'Top,Left,Right,Bottom'
$null = $logCard.Controls.Add($script:logBox)

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
if ($script:both) {
    # 两款时正好等于原来那句"-Games ark   (或 r1999)"；三款以上把后面的并列进括号里，别套两层括号
    $rest = $script:games[1..($script:games.Count - 1)] -join ' / '
    Add-PanelHelpRow '只跑一路' ('.\' + $entryName + ' -Games ' + $script:games[0] + '   (或 ' + $rest + ')') @($entryName)
}
Add-PanelHelpRow '环境体检'    '.\status.ps1'          @('status.ps1')
Add-PanelHelpRow '首次设置'    '.\setup.ps1 -DryRun'   @('setup.ps1')
Add-PanelHelpRow '装定时'      '.\install-task.ps1'    @('install-task.ps1')
Add-PanelHelpRow '改时间点'    ".\tools\reschedule-daily.ps1 -Times $timesTxt" @('tools\reschedule-daily.ps1')
if ($flagAck)      { Add-PanelHelpRow '我打过活动了' ('.\' + $entryName + ' -' + $flagAck + ' -DryRun') @($entryName) }
if ($flagShopDone) { Add-PanelHelpRow '商店领空'     ('.\' + $entryName + ' -' + $flagShopDone + ' -DryRun') @($entryName) }
Add-PanelHelpRow '图形面板'    '.\panel.cmd   (或 exe 加 --panel)' @('panel.cmd')
Add-PanelHelpRow '命令行菜单'  '.\start.cmd   (双击=打开这个菜单)' @('start.cmd', 'tools\menu.ps1')

if ($DumpSummary) {
    # C# 面板(tools\panel-cs\Panel.cs)的读数路径：只打上面那套 '|' 协议行，不建任何控件、不进消息循环。
    Dump-PanelSummaryMachine
    exit 0
}

Refresh-PanelSummary
Append-PanelLog '提示：面板只转达，不替你判定 —— 上面的结论和命令行里跑同一套脚本得到的一样。'
Append-PanelLog ('日志目录：' + (Get-GdDir 'Logs'))
Append-PanelLog '想跑就点"开始跑一轮"，它会让你再点一次确认；不想动窗口就放着，到点计划任务自己跑。'

# ---------- 两种不显示给人看的路径 ----------
if ($DumpTree) {
    <# 版式调试用(不弹窗)：把控件树连坐标一起打出来。
       "标题没显示出来"这类问题靠截图看不如直接看 Bounds/Tag 来得快。 #>
    Write-Host '---- 控件树(调试用) ----'
    $script:dump = @()
    $walk = {
        param($p, $indent)
        foreach ($c in $p.Controls) {
            $script:dump += ('{0}{1} "{2}" bounds={3} tag={4} fore={5} font={6}' -f $indent, $c.GetType().Name,
                      ([string]$c.Text).Substring(0, [Math]::Min(46, ([string]$c.Text).Length)), [string]$c.Bounds,
                      [string]$c.Tag, [string]$c.ForeColor, [string]$c.Font.Size)
            if (@($c.Controls).Count) { & $walk $c ($indent + '  ') }
        }
    }
    & $walk $script:form ''
    foreach ($l in $script:dump) { Write-Host $l }
    Write-Host ('form ClientSize=' + [string]$script:form.ClientSize)
    $script:form.Dispose()
    exit 0
}

function Invoke-PanelClick {
    <# 在"窗口从没显示过"的情况下把某个控件的 Click 真发一次。
        为什么不用 PerformClick：它内部要求控件可见(父窗体没 Show 过 → Visible=false)，于是不管怎么点都是空转，
        自检会因此"永远看不见点击链坏掉"。OnClick 是 Control 的受保护方法，反射调它才会真的抬事件、跑 Add_Click 那个脚本块。 #>
    param([Parameter(Mandatory)]$Ctl)
    $m = $Ctl.GetType().GetMethod('OnClick', [System.Reflection.BindingFlags]'Instance, NonPublic')
    if (-not $m) { throw '拿不到 OnClick，点击链验不了' }
    $null = $m.Invoke($Ctl, @([System.EventArgs]::Empty))
}

function Get-PanelLayoutIssues {
    <# 布局自检：没人会用自动化去肉眼看窗口，所以把"控件互相压住 / 越出父容器 / 卡片空了"变成可判定的检查。
       按容器逐个查(卡片里的行不能压住、不能越出卡片)，返回红项文字列表；空数组就是这一页没问题。
       为什么它现在必须能按页调用：左侧导航换页会重新生成卡片里的行 —— 分页后的行比总览页更长更多，
       只在默认那一页查就等于"改了新页却没人验过新页"。 #>
    param([string]$Tag = '')
    $bad = @()
    $all = @(Get-PanelDescendants $script:form)
    $containers = @($script:form)
    foreach ($d in $all) { if (@($d.Controls).Count -gt 0) { $containers += $d } }
    foreach ($ct2 in $containers) {
        $cl = $ct2.ClientSize
        $kids = @($ct2.Controls)
        $where = [string]$ct2.GetType().Name
        if ($ct2 -ne $script:form) {
            $tt = @($kids | Where-Object { $_.Tag -eq 'title' } | Select-Object -First 1)
            if ($tt.Count) { $where = $where + '(' + [string]$tt[0].Text + ')' }
        }
        for ($i = 0; $i -lt $kids.Count; $i++) {
            $a = $kids[$i].Bounds
            if ($a.X -lt 0 -or $a.Y -lt 0 -or ($a.X + $a.Width) -gt $cl.Width -or ($a.Y + $a.Height) -gt $cl.Height) {
                $bad += ($Tag + '越出容器 ' + $where + ': ' + [string]$kids[$i].GetType().Name + ' ' + [string]$a)
            }
            for ($j = $i + 1; $j -lt $kids.Count; $j++) {
                $b = $kids[$j].Bounds
                if ($a.X -lt ($b.X + $b.Width) -and $b.X -lt ($a.X + $a.Width) -and
                    $a.Y -lt ($b.Y + $b.Height) -and $b.Y -lt ($a.Y + $a.Height)) {
                    $bad += ($Tag + '互相压住(' + $where + '): ' + [string]$a + ' × ' + [string]$b)
                }
            }
        }
    }
    $empty = @()
    foreach ($pair in @(@('定时任务', $script:cardTimes), @('上一轮', $script:cardLast), @('两道门', $script:cardGates), @('环境', $script:cardEnv))) {
        if (@(@($pair[1].Controls) | Where-Object { $_.Tag -eq 'line' }).Count -eq 0) { $empty += [string]$pair[0] }
    }
    if ($empty.Count) { $bad += ($Tag + '这些卡片一行都没渲染出来：' + ($empty -join ' , ')) }
    return $bad
}

if ($SelfTest) {
    <# 自动化验证用：不进消息循环、不显示窗口，只把渲染结果打到控制台。
       这一条会在两份导出仓库里各跑一遍(验收链第 6 步)，所以它是"改版式有没有把人看到的东西弄丢"的唯一自动化闸门：
       按钮清单要递归(按钮现在装在动作区里)、布局要按容器逐个查(卡片里的行不能压住、不能越出卡片)。 #>
    Write-Host '---- panel 自检(不显示窗口) ----'
    Write-Host (Get-PanelSummaryText)
    $all = @(Get-PanelDescendants $script:form)
    $btns = @()
    foreach ($ct in $all) { if ($ct -is [System.Windows.Forms.Button]) { $btns += [string]$ct.Text } }
    Write-Host ('控件 ' + $all.Count + ' 个(含卡片内)   按钮: ' + ($btns -join ' / '))
    Write-Host ('子进程引擎: ' + $script:engine)
    Write-Host ('参数探测: Ack=' + $flagAck + '  ShopDone=' + $flagShopDone + '  Games=' + $flagGames + '  入口=' + $entryName + '  判定库=' + $haveDecisions)
    Write-Host ('帮助行 ' + @($script:helpRows).Count + ' 条(按钮「命令行怎么跑」会打这些)：')
    foreach ($l in (Get-PanelHelpText)) { Write-Host ('  ' + $l) }
    $miss = @(Get-PanelHelpRowMisses)
    Write-Host ('帮助行里点名的文件都在: ' + [string]($miss.Count -eq 0) + $(if ($miss.Count) { '  → ' + ($miss -join ' , ') } else { '' }))
    Pump-PanelOutput
    Write-Host ('日志区行数: ' + $script:logBox.Lines.Count)
    Write-Host ('输出泵可用: ' + [bool](Get-Command Read-PanelNewText -ErrorAction SilentlyContinue))
    <# 布局自检：每一页都换过去查一遍(切页会重建卡片里的行)。
       导航项本身也在这里钉：注册表里多一路就该多一条，漏了就是"面板上看不见这一路"——
       那正是加模块时最容易忘记回来改面板的地方。 #>
    $bad = @(Get-PanelLayoutIssues -Tag '总览页 ')
    <# 换页必须经"点导航按钮"这条路，而不是直接调 Select-PanelPage：
        前者是玩家真正做的事(事件处理 → 换页 → 重画)，后者只验到函数本身 ——
        事件脚本块里那个 sender 取错了(以前踩过的 $this 坑)只有 Invoke-PanelClick 能发现。 #>
    foreach ($g in $script:games) {
        $nb = $script:navBtns[[string]$g]
        if (-not $nb) { $bad += ('导航里没有这一路：' + [string]$g); continue }
        Invoke-PanelClick $nb
        if ([string]$script:page -ne [string]$g) {
            $bad += ('点了 ' + [string]$g + ' 这一路，页面还停在 ' + [string]$script:page + '(点击链没走通)')
        }
        $want = Get-PanelPageTitle -Page ([string]$g)
        if ([string]$script:pageTitle.Text -ne $want) { $bad += ('点了 ' + [string]$g + ' 但页标题没跟着换(应是 ' + $want + '，实际 ' + [string]$script:pageTitle.Text + ')') }
        $bad += @(Get-PanelLayoutIssues -Tag ($want + '页 '))
    }
    if ($script:navBtns['all']) { Invoke-PanelClick $script:navBtns['all'] } else { Select-PanelPage -Page 'all' }
    if ([string]$script:page -ne 'all') { $bad += ('回到总览失败：停在 ' + [string]$script:page) }
    $navMissing = @()
    foreach ($g in $script:games) { if (-not $script:navBtns[[string]$g]) { $navMissing += [string]$g } }
    if ($navMissing.Count) { $bad += ('导航里缺这些模块：' + ($navMissing -join ' , ')) }
    if (-not $script:navBtns['all']) { $bad += '导航里没有"总览"这一条' }
    Write-Host ('导航项: ' + (($script:navBtns.Keys | Sort-Object) -join ' / ') + '   页: ' + (Get-PanelPageTitle -Page $script:page))
    $all = @(Get-PanelDescendants $script:form)   # 切过一轮页，卡片里的行已经换过一批，后面量宽度要用新的
    <# 文字放不放得下：Label 开了 AutoEllipsis，超宽只会安静地变成省略号，肉眼以外没人发现(以前就发生过)。
       这里按"量的宽度 > 控件宽度"报出来 —— 只报，不计红项：别的机器缺中文字体时回退字宽不一样，
       拿它当硬闸门会把导出验收链误伤成"面板坏了"。 #>
    $wide = @()
    $short = @()
    try {
        foreach ($d in $all) {
            if (-not ($d -is [System.Windows.Forms.Label])) { continue }
            if ($d.Height -lt [int]$d.Font.Height) {
                $short += ('高 ' + $d.Height + ' < 字体 ' + [int]$d.Font.Height + '  → ' + [string]$d.Text)
            }
            if ($d.Tag -ne 'line') { continue }
            $w = [System.Windows.Forms.TextRenderer]::MeasureText([string]$d.Text, $d.Font).Width
            if ($w -gt $d.Width) { $wide += ('{0}px>{1}px  {2}' -f $w, $d.Width, [string]$d.Text) }
        }
    } catch { $wide = @('(量不了：' + $_.Exception.Message + ')') }
    foreach ($x in $short) { $bad += ('这一行会被整个画不出来(AutoEllipsis + 高度小于字体行高)：' + $x) }
    Write-Host ('会被省略号吃掉的行数: ' + $wide.Count)
    foreach ($x in $wide) { Write-Host ('  偏长 ' + $x) }
    Write-Host ('重定向文件(带 PID，避免开机重播上一轮): ' + [System.IO.Path]::GetFileName($script:outPath) + ' / ' + [System.IO.Path]::GetFileName($script:errPath))
    if ($bad.Count) { foreach ($x in $bad) { Write-Host ('  布局问题 ' + $x) } }
    Write-Host ('布局自检: ' + $(if ($bad.Count) { '有问题(见上)' } else { 'OK，每一页都不重叠、都在各自容器内、四张卡片都有内容' }))
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
