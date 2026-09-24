# lib/r1999-decisions.ps1 — 重返未来：1999(MuMu + M9A)侧"今天跑哪个方案"的判定 + 实例解析
# 依赖 lib/common.ps1 + lib/decisions-common.ps1；只读判定，不启动任何程序。

function Resolve-R1999Plan {
    <# 纯判定：M9A 活动数据 + 当前时间 -> 1999 方案(Mode = idle | event | rerun | event-pending) #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)]$Data,
        [Parameter(Mandatory)][long]$Now,
        [switch]$IgnoreShopDone
    )

    $cur = $null
    foreach ($p in $Data.PSObject.Properties) {
        if ($p.Value.start_time -le $Now -and $Now -le $p.Value.end_time) { $cur = $p; break }
    }

    $result = [pscustomobject]@{
        Mode = 'idle'; Version = $null; VersionName = $null; EventKey = $null; EventTip = $null
        Reason = ''; Instance = 'idle'; DataAge = $null; ShopDone = $false
    }

    if (-not $cur) {
        $latest = 0
        foreach ($p in $Data.PSObject.Properties) { if ($p.Value.end_time -gt $latest) { $latest = $p.Value.end_time } }
        $note = if ($latest -lt $Now) { "；活动数据只记录到 $(Format-GdUnixMs $latest 'yyyy-MM-dd')，可能未更新" } else { '' }
        $result.Reason = "不在任何已记录版本的窗口内 → 长草$note"
        return $result
    }

    $result.Version = $cur.Name
    $result.VersionName = [string]$cur.Value.version_name
    $activity = Get-GdMapValue $cur.Value 'activity'
    $combat = Get-GdMapValue $activity 'combat'
    $rerun  = Get-GdMapValue $activity 're-release'

    if ($combat -and $combat.start_time -le $Now -and $Now -le $combat.end_time) {
        $isMain = [string](Get-GdMapValue $combat 'event_type') -match 'MainStory'
        $key = "$($cur.Name)-combat"
        $ack = Get-GdAckMap -Game 'r1999'
        $result.EventKey = $key
        $result.EventTip = $result.VersionName
        if ((-not $IgnoreShopDone) -and (Test-GdShopDoneEnabled -S $S -Game 'r1999') `
                -and ((Get-GdShopDoneMap -Game 'r1999').ContainsKey($key))) {
            $result.ShopDone = $true
            $result.Reason = "版本 $($cur.Name)[$($result.VersionName)]活动商店已标记领空 → 本轮跑长草方案(要恢复：daily-clear.ps1 $(Get-GdEntryVerb -Game r1999 -Verb ShopReset))"
            return $result
        }
        if ($isMain -and $S.re1999.mainStoryRequiresAck -and -not $ack.ContainsKey($key)) {
            $result.Mode = 'event-pending'
            $result.Instance = 'idle'
            $result.Reason = "版本 $($cur.Name)[$($result.VersionName)]主线活动进行中(至 $(Format-GdUnixMs $combat.end_time))，未确认手动通关 → 本轮跑长草方案；确认后运行 daily-clear.ps1 $(Get-GdEntryVerb -Game r1999 -Verb Ack)"
        } else {
            $result.Mode = 'event'
            $result.Instance = 'event'
            $result.Reason = "版本 $($cur.Name)[$($result.VersionName)]活动进行中($(Format-GdUnixMs $combat.start_time) ~ $(Format-GdUnixMs $combat.end_time))"
        }
        return $result
    }
    if ($rerun -and $rerun.start_time -le $Now -and $Now -le $rerun.end_time) {
        $result.Mode = 'rerun'
        $result.Instance = 'rerun'
        $result.EventTip = "$($result.VersionName) 复刻"
        $result.Reason = "版本 $($cur.Name) 复刻进行中($(Format-GdUnixMs $rerun.start_time) ~ $(Format-GdUnixMs $rerun.end_time))"
        return $result
    }
    $result.Reason = "版本 $($cur.Name)[$($result.VersionName)]当前无进行中活动(版本窗口 $(Format-GdUnixMs $cur.Value.start_time) ~ $(Format-GdUnixMs $cur.Value.end_time))→ 长草"
    return $result
}

function Get-R1999Event {
    <# 读 M9A 自带活动数据(会随 M9A 资源更新)后交给 Resolve-R1999Plan #>
    param([Parameter(Mandatory)]$S, [switch]$IgnoreShopDone)

    $now = Get-GdNowMs
    $path = Join-GdPath $S.paths.m9aDir @($S.paths.m9aActivityData)
    # Test-GdPathReady 而不是 Test-Path：没跑 setup 时 $path 是空串，"缺少 ，按长草处理"那种半截话玩家看不懂
    if (-not (Test-GdPathReady $path)) {
        $why = if ($path) { "缺少 $path" } else { 'M9A 目录还没填(先跑 setup.ps1)' }
        return [pscustomobject]@{
            Mode = 'idle'; Version = $null; VersionName = $null; EventKey = $null; EventTip = $null
            Reason = "$why，按长草处理"; Instance = 'idle'; DataAge = $null; ShopDone = $false
        }
    }
    $data = Read-GdJson -Path $path
    $age = [math]::Round((New-TimeSpan -Start (Get-Item -LiteralPath $path).LastWriteTime).TotalDays, 1)

    $a = @{ S = $S; Data = $data; Now = $now }
    if ($IgnoreShopDone) { $a['IgnoreShopDone'] = $true }
    $result = Resolve-R1999Plan @a
    $result.DataAge = $age
    if ($age -gt 0 -and $result.Reason -notmatch '未更新') { $result.Reason += "(活动数据 $age 天前)" }
    return $result
}

function Resolve-R1999Instance {
    <# 按实例名在 M9A config/instances 里找 id #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)][string]$PlanKey)
    $want = Get-GdMapValue $S.re1999.planToInstance $PlanKey
    if (-not $want) { throw "settings.re1999.planToInstance 里没有 $PlanKey 对应的实例名" }
    $dir = Join-GdPath $S.paths.m9aDir @($S.paths.m9aInstancesDir)
    if (-not (Test-GdPathReady $dir)) {
        throw $(if ($dir) { "M9A 实例目录不存在：$dir" } else { 'M9A 目录还没填(先跑 setup.ps1)，读不到实例列表' })
    }
    foreach ($f in @(Get-ChildItem -LiteralPath $dir -Filter '*.json')) {
        $cfg = Read-GdJson -Path $f.FullName
        if ([string]$cfg.InstanceName -eq [string]$want) {
            $tasks = @($cfg.CurrentTasks | ForEach-Object { ($_ -split '<\|\|\>')[-1] })
            return [pscustomobject]@{ Id = $f.BaseName; Name = [string]$cfg.InstanceName; Path = $f.FullName; Tasks = $tasks }
        }
    }
    throw "M9A 中找不到名为[$want]的实例(计划 $PlanKey)"
}
