# lib/r1999-daily.ps1 — 重返未来：1999 这一路的"日常动作"：状态标记 / 方案判定 / dry-run 预览 / 真正跑一轮
#
# 与 lib\ark-daily.ps1 同一批拆分产物，搬动规则和那里注释里写的完全一致
# (闭包 $S 改成显式参数；结果 return 给入口脚本，不再直接写 $results)。
# 公开命令行(-Ack1999 / -RShopDone / -RPlan / -Force)仍然长在 daily-clear.ps1 里。
#
# 依赖：调用方先点 lib\common.ps1、lib\r1999-decisions.ps1(或聚合的 lib\decisions.ps1)、
#       lib\m9a.ps1、lib\mumu.ps1。本文件不重复点它们，理由见 lib\decisions.ps1 开头。

function Set-GdR1999Ack {
    <# -Ack1999：确认"这一版的主线活动我已经手动打过" → 之后日常才允许跑活动方案 #>
    param([Parameter(Mandatory)]$S)
    $d = Get-R1999Event -S $S -IgnoreShopDone
    if (-not $d.EventKey) { Write-GdLog "1999 当前没有待确认活动：$($d.Reason)" -Level WARN }
    else {
        Set-GdAckKey -Game 'r1999' -Key $d.EventKey
        Write-GdLog "已确认 1999 活动 $($d.EventKey)[$($d.EventTip)]→ 日常将使用活动方案" -Level OK
    }
}

function Set-GdR1999ShopState {
    <# -RShopDone / -RShopReset：标记或撤销"活动商店已领空"。-Done 为真=标记，为假=撤销 #>
    param([Parameter(Mandatory)]$S, [switch]$Done)
    $d = Get-R1999Event -S $S -IgnoreShopDone
    if (-not $d.EventKey) { Write-GdLog "1999 当前没有进行中的活动可标记：$($d.Reason)" -Level WARN }
    elseif ($Done) {
        Set-GdShopDoneKey -Game 'r1999' -Key $d.EventKey
        Write-GdLog "已标记 1999 活动 $($d.EventKey)[$($d.EventTip)] 商店领空 → 之后跑[$($S.re1999.planToInstance.idle)]实例" -Level OK
    }
    elseif (Remove-GdShopDoneKey -Game 'r1999' -Key $d.EventKey) {
        Write-GdLog "已撤销 $($d.EventKey) 的领空标记 → 恢复活动方案" -Level OK
    } else {
        Write-GdLog "$($d.EventKey) 本来就没有领空标记" -Level WARN
    }
}

function Resolve-GdR1999Decision {
    <# 命令行意图 -> 本轮 1999 方案 + M9A 实例(-Plan 手动指定、-Force 跳过确认门)。返回 @{Decision;Instance}
       参数故意叫 -Plan 而不是 -RPlan：本文件只管 1999，带前缀的名字留给合体入口 daily-clear.ps1。 #>
    param([Parameter(Mandatory)]$S, [string]$Plan = 'auto', [switch]$Force)
    $r1999 = Get-R1999Event -S $S
    if ($Force -and $r1999.Mode -eq 'event-pending') {
        $r1999 = [pscustomobject]@{ Mode = 'event'; Instance = 'event'; Version = $r1999.Version; VersionName = $r1999.VersionName; EventKey = $r1999.EventKey; EventTip = $r1999.EventTip; Reason = '(-Force) 跳过 1999 活动确认门，直接用活动方案'; DataAge = $r1999.DataAge }
        Write-GdLog '-Force：1999 本轮忽略活动确认门' -Level WARN
    }
    if ($Plan -ne 'auto') {
        $r1999 = [pscustomobject]@{ Mode = $Plan; Instance = $Plan; Version = 'manual'; VersionName = '命令行指定'; EventKey = $null; EventTip = $null; Reason = "命令行指定 $Plan"; DataAge = $null }
    }
    try { $inst = Resolve-R1999Instance -S $S -PlanKey $r1999.Instance }
    catch { Write-GdLog "1999 实例解析失败：$($_.Exception.Message)" -Level ERROR; throw }
    Write-GdLog "1999 方案：$($r1999.Mode) → 实例[$($inst.Name)]— $($r1999.Reason)"
    return [pscustomobject]@{ Decision = $r1999; Instance = $inst }
}

function Show-GdR1999Preview {
    <# -DryRun：预览将切换的 M9A 实例、ADB 序列号和 MuMu 虚拟机状态，不启动任何程序 #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance)
    $prev = Set-M9AInstance -S $S -Instance $Instance
    $prev.Changes | ForEach-Object { Write-GdLog "  M9A：$_" }
    $conn = Get-M9AConnection -S $S -Instance $Instance
    Write-GdLog "  ADB：$($conn.Serial)($($conn.Name))"
    $st = Get-GdMuMuState -S $S
    Write-GdLog "  MuMu 虚拟机 $($S.mumu.vmIndex)：$(if ($st.AndroidStarted) { '运行中' } else { '未运行(将自动启动)' })"
}

function Start-GdR1999Daily {
    <# 真正跑一轮 1999：起 MuMu → 切实例 → 跑 M9A → 该重试就重试 → 关掉 M9A。返回 @{Status;Detail;Duration} #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance)
    $inst = $Instance
    Write-GdLog '--- 重返未来：1999(MuMu + M9A)---' -Level STEP
    Invoke-GdTakeover -S $S -Names @((Get-M9APaths -S $S).Name) -Label 'M9A'
    if (-not (Start-GdMuMu -S $S)) { Write-GdLog 'MuMu 未能确认启动，仍尝试让 M9A 自行处理' -Level WARN }
    $m9aCfg = Set-M9AInstance -S $S -Instance $inst -Apply
    $m9aCfg.Changes | ForEach-Object { Write-GdLog "  M9A：$_" }
    $run9 = { Invoke-M9ARun -S $S -Instance $inst }
    $r = Invoke-GdStep -Name '1999' -Body $run9
    <#
      原来这批状态一律"不重试"，理由成立：--autostart 没生效 / 队列没提交 / 第一个任务就失败，
      原样再跑一遍当然还是同一个结果。唯一能把这条规则翻过来的证据是**画面真的被兜清楚了**
      ($r.Rescued = 视觉兜底点掉了一个挡路的弹窗/界面)，那时重跑一次才有意义。
      unknown-format 永远不在其列：日志都读不懂，重试只是再看不懂一遍。
    #>
    $retryable = ($r.Status -notin @('ok', 'not-started', 'empty-queue', 'unknown-format', 'blocked')) -or
                 ($r.Rescued -and $r.Status -in @('not-started', 'empty-queue', 'blocked'))
    if ($retryable -and $S.runtime.retryOnce) {
        Write-GdLog $(if ($r.Rescued) { '1999 本轮未成功，但视觉兜底把挡路的界面点掉了 → 关闭 M9A 后重试一次' }
                      else { '1999 本轮未成功 → 关闭游戏后重试一次' }) -Level WARN
        Stop-M9A -S $S
        Start-Sleep -Seconds 20
        $r = Invoke-GdStep -Name '1999重试' -Body $run9
    } elseif ($r.Status -eq 'unknown-format') {
        Write-GdLog '1999 日志标志一条都没认出来 → 重试同样看不懂，直接记入本轮告警(见下面的处理建议)' -Level ERROR
    } elseif ($r.Status -eq 'blocked') {
        <# 实测 09-25 19:24：客户端在模拟器里下 1.91G 更新包，10 个任务里 8 个「任务失败」，
           重试只是再空转 5 分钟。这种"游戏没在界面上"的形态重试无用，要人去处理。 #>
        Write-GdLog '1999 从头一个任务起就没进游戏 → 是客户端/资源要更新或模拟器状态不对，重试没有意义，直接记入本轮告警' -Level ERROR
    } elseif ($r.Status -in @('not-started', 'empty-queue')) {
        Write-GdLog '1999 队列没开始/空跑完成 → 重试没有意义，直接记入本轮告警' -Level WARN
    }
    Stop-M9A -S $S
    return $r
}
