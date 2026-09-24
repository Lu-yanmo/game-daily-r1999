# tools/replay-m9a-log.ps1 — 拿真实 M9A 日志按时间窗口回放整条判定链路(不启动任何程序)
#
#   pwsh -File tools\replay-m9a-log.ps1                                  # 今天的日志 + 整份回放 + 实例自动认
#   pwsh -File tools\replay-m9a-log.ps1 -From "17:44:10" -To "17:49:00"  # 只给时刻 → 按日志那天算
#   pwsh -File tools\replay-m9a-log.ps1 -Log "<m9aDir>\logs\log-20260830.log" -From "2026-08-30 12:18:40" -InstanceId 1a2b3c4d
#
# 用途：改了 lib\m9a.ps1 的判定之后，用「已经发生过的那一轮」的真实字节流验证结论，
#       不用再花十几分钟重跑游戏。ProcessGone 用窗口结束后模拟(真跑里 --quit-after-run 就是会退)。
# 实例 id 不用你写死：M9A 每行日志都带 [inst=实例名/id]，默认取窗口里出现最多的那个 id，
#       并在开头把它打出来；认错了(比如同一窗口里两个实例都动过)再用 -InstanceId 指名。
param(
    [string]$Root = (Split-Path -Parent $PSScriptRoot),
    [string]$Log = '',          # 不传就用 settings.json 里 m9aDir 下今天的日志
    [string]$InstanceId = '',   # 留空 = 从日志里自动认
    [string]$InstanceName = '',
    [string]$From = '',         # 留空 = 从日志第一条带时间戳的行开始
    [string]$To = '',           # 留空 = 到最后一条
    [int]$PollSeconds = 10,
    [switch]$Quiet
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path $Root).Path
. (Join-Path $Root 'lib\common.ps1')
. (Join-Path $Root 'lib\m9a.ps1')
$S = Get-GdSettings -Quiet
if (-not $Log) { $Log = Get-M9ADailyLog -P (Get-M9APaths -S $S) }
if (-not (Test-Path -LiteralPath $Log)) { throw "日志不存在：$Log" }

function Convert-GdReplayTime {
    <# 只写时刻("17:44:10")时按这份日志自己的日期算 —— 回放昨天的日志不用改日期，也不用怕它锚到"今天"上。 #>
    param([string]$Text, [datetime]$Anchor)
    if (-not $Text) { return $null }
    $d = [datetime]$Text
    if ($Text -match '(\d{4})-(\d{2})-(\d{2})') { return $d }
    return [datetime]::new($Anchor.Year, $Anchor.Month, $Anchor.Day, $d.Hour, $d.Minute, $d.Second)
}

# 逐行带时间戳解析；无时间戳的续行归到上一行
$rows = @()
$lastTs = $null
foreach ($line in (Get-Content -LiteralPath $Log -Encoding UTF8)) {
    if ($line -match '^\[(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)') { $lastTs = [datetime]::ParseExact($Matches[1], 'yyyy-MM-dd HH:mm:ss', $null) }
    if ($lastTs) { $rows += [pscustomobject]@{ Ts = $lastTs; Line = $line } }
}
if (-not $rows.Count) { throw "$Log 里一条带时间戳的行都没有 —— M9A 日志格式变了？(先看 tools\probe-m9a-history.ps1)" }
$lo = Convert-GdReplayTime -Text $From -Anchor $rows[0].Ts
$hi = Convert-GdReplayTime -Text $To -Anchor $rows[-1].Ts
if (-not $lo) { $lo = $rows[0].Ts }
if (-not $hi) { $hi = $rows[-1].Ts }
$sel = @($rows | Where-Object { $_.Ts -ge $lo -and $_.Ts -le $hi })
if (-not $sel.Count) { throw "窗口内没有日志行($lo ~ $hi，日志本身 $($rows[0].Ts) ~ $($rows[-1].Ts))" }

# 实例自动识别：数 [inst=名字/id] 出现的行数，取最多的那个
$byInst = @{}
foreach ($row in $sel) {
    if ($row.Line -match '\[inst=([^/\]]+)/([0-9A-Za-z]+)\]') {
        $k = "$($Matches[2])`t$($Matches[1])"
        if (-not $byInst.ContainsKey($k)) { $byInst[$k] = 0 }
        $byInst[$k] = [int]$byInst[$k] + 1
    }
}
if (-not $InstanceId) {
    if ($byInst.Count) {
        $best = @($byInst.GetEnumerator() | Sort-Object -Property Value -Descending)[0]
        $parts = @($best.Key -split "`t")
        $InstanceId = [string]$parts[0]
        if (-not $InstanceName) { $InstanceName = [string]$parts[1] }
        if ($byInst.Count -gt 1) {
            Write-Host "窗口里有 $($byInst.Count) 个实例留过日志，取行数最多的：$InstanceName/$InstanceId —— 要换就显式给 -InstanceId" -ForegroundColor Yellow
        }
    } else {
        Write-Host '窗口里没有任何 [inst=名字/id] 标记，本次不按实例过滤(所有实例的行都会喂进判定)' -ForegroundColor Yellow
    }
}
$Instance = [pscustomobject]@{ Id = $InstanceId; Name = $InstanceName; Path = (Join-Path $Root 'config\instances\fake.json') }

$t0 = $sel[0].Ts
Write-Host "回放窗口：$($sel.Count) 行，$($sel[0].Ts.ToString('MM-dd HH:mm:ss')) → $($sel[-1].Ts.ToString('HH:mm:ss'))，实例[$($Instance.Name)/$($Instance.Id)]，日志 $(Split-Path -Leaf $Log)" -ForegroundColor Cyan

$state = @{ tasks = @(); chainStarted = $false; chainDone = $false; connectPhase = 'none'; failed = $null
            lastLog = $t0; queueCount = $null; startRequested = $false; eventCount = 0 }
$verdicts = 0
foreach ($row in $sel) {
    $ev = Read-GdM9ALine -Line $row.Line -InstanceId $InstanceId
    if (-not $ev) { continue }
    if (-not $Quiet) { Write-Host ("  {0}  <{1,-14}> {2}" -f $row.Ts.ToString('HH:mm:ss'), $ev.Kind, $ev.Text) -ForegroundColor DarkGray }
    Register-M9AEvent -State $state -Event $ev

    # 模拟真跑的轮询节奏：每 PollSeconds 秒判定一次
    $elapsed = ($row.Ts - $t0).TotalSeconds
    if (([math]::Floor($elapsed / $PollSeconds)) -gt ([math]::Floor(($elapsed - 0.001) / $PollSeconds))) {
        $res = Resolve-M9AStatus -State $state -Elapsed $elapsed -S $S -Instance $Instance -LogName (Split-Path -Leaf $Log)
        if ($res) {
            $verdicts++
            Write-Host ("→ {0} 判定 {1}：{2}" -f $row.Ts.ToString('HH:mm:ss'), $res.Status, $res.Detail) -ForegroundColor Yellow
        }
    }
}
# 窗口结束：--quit-after-run 的真跑里 M9A 随后就退出了
$final = Resolve-M9AStatus -State $state -Elapsed (($sel[-1].Ts - $t0).TotalSeconds + 12) -S $S -Instance $Instance `
        -ProcessGone $true -LogName (Split-Path -Leaf $Log)
Write-Host ''
Write-Host ("队列声明={0} 任务数={1} chainStarted={2} chainDone={3} 连接阶段={4}" -f `
    $(if ($null -ne $state.queueCount) { $state.queueCount } else { '无' }),
    @($state.tasks).Count, $state.chainStarted, $state.chainDone, $state.connectPhase)
Write-Host ("最后一个任务：{0}" -f $(if (@($state.tasks).Count) { @($state.tasks)[-1] } else { '(无)' }))
if ($final) {
    $color = if ($final.Status -eq 'ok') { 'Green' } else { 'Red' }
    Write-Host "最终判定：$($final.Status) — $($final.Detail)" -ForegroundColor $color
    exit $(if ($final.Status -eq 'ok') { 0 } else { 1 })
}
Write-Host '最终判定：(无结论，按脚本逻辑还在跑)' -ForegroundColor Yellow
exit 2
