# lib/r1999-daily.ps1 — 重返未来：1999 这一路的"日常动作"：状态标记 / 方案判定 / dry-run 预览 / 真正跑一轮
#
# 与 lib\ark-daily.ps1 同一批拆分产物，搬动规则和那里注释里写的完全一致
# (闭包 $S 改成显式参数；结果 return 给入口脚本，不再直接写 $results)。
# 公开命令行(-Ack1999 / -RShopDone / -RPlan / -Force)仍然长在 daily-clear.ps1 里。
#
# 依赖：调用方先点 lib\common.ps1、lib\r1999-decisions.ps1(或聚合的 lib\decisions.ps1)、
#       lib\m9a.ps1、lib\mumu.ps1。本文件不重复点它们，理由见 lib\decisions.ps1 开头。
#
# ── 冻结(2026-09-27，PS→C# 批次 5)────────────────────────────────────────────
# Resolve-GdR1999Decision 的**构造段规则**(-Force 跳确认门、-Plan≠auto 无条件覆盖、两段文案与键序)
# 已迁到 tools\panel-cs\Orchestration.cs(verb：apply-r1999-daily-intent；契约见 docs\ps-migration-batch0.md)。
# 壳里留的是"活"：Get-R1999Event 取数、Resolve-R1999Instance 实例解析、"1999 方案：…"日志行与返回形状；
# Set-Gd*Ack/ShopState 与 Start-GdR1999Daily 整函数留壳(起进程/切实例归后续批次)。
# 改规则请改 C# 并跑 tools\probe-orch-golden.ps1 对拍，不要在这里加规则。
# ──────────────────────────────────────────────────────────────────────────────
#
# ── 追加冻结(2026-09-27，PS→C# 批次 8)─────────────────────────────────────────
# Start-GdR1999Daily 的 $retryable 分流(五状态跳过清单 + "画面被兜清楚才翻案"的 or 支、
#  retryOnce 合取、if/elseif 链的落穿语义与四句文案)已迁到 tools\panel-cs\Orchestration.cs
#  (verb：decide-daily-retry，-Game r1999)。壳里留的正是"活"：Stop-M9A、sleep 20、重跑 $run9、
#  日志回放。旧判据段在分流处逐行 # 前缀冻结(test-r1999 钉 $retryable 表达式与话术字面锚)。
#  改分流规则/话术请改 Orchestration.cs 并复拍 tools\probe-live-golden.ps1，不要在这里加 if。
# ──────────────────────────────────────────────────────────────────────────────

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
    <# 转发壳(批次 5)：-Force/-Plan 两段构造的规则在 C#(apply-r1999-daily-intent)；
       这里取数(Get-R1999Event)、解析实例(Resolve-R1999Instance)、打日志并把 {Decision;Instance} 交回。
       参数故意叫 -Plan 而不是 -RPlan：本文件只管 1999，带前缀的名字留给合体入口 daily-clear.ps1。 #>
    param([Parameter(Mandatory)]$S, [string]$Plan = 'auto', [switch]$Force)
    $r1999 = Get-R1999Event -S $S
    $d = Invoke-GdCli -Verb 'apply-r1999-daily-intent' -Body ([pscustomobject]@{
        decision = $r1999; plan = $Plan; force = [bool]$Force
    })
    $r1999 = $d.Decision
    foreach ($w in @($d.Warnings)) { Write-GdLog $w -Level WARN }
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
    $selfEngine = (Get-Process -Id $PID).Path   # 起子进程用当前引擎自己(5.1→powershell.exe / 7→pwsh.exe)，两处外挂共用
    <# M9A 自动更新闸(2026-09-30 用户口径「检查到更新自己更就行了」)：必须放在起 M9A 之前 ——
       更新器见 m9a 进程在跑就拒动(且永不硬杀)。整链(备份/下载/覆盖/校验)最坏十几分钟，
       但任何失败只 WARN 不挡当日轮；已是最新时子进程秒退，日常不拖节奏。契约在 tools\m9a-update.ps1 头注。 #>
    try {
        $updOut = @(& $selfEngine -NoProfile -ExecutionPolicy Bypass -File (Join-Path $global:GD.Root 'tools\m9a-update.ps1') -Apply 2>&1)
        $updCode = $LASTEXITCODE
        if ($updCode -eq 0) {
            $updLine = (@($updOut | Where-Object { [string]$_ -match '已是最新|更新完成' }) | Select-Object -Last 1)
            Write-GdLog ('M9A 自动更新闸：' + [string]$updLine)
        } else {
            Write-GdLog ('M9A 自动更新未完成(退出码 ' + $updCode + ')，不挡当日轮：' + ((@($updOut | ForEach-Object { [string]$_ })) -join ' / ')) -Level WARN
        }
    } catch {
        Write-GdLog "M9A 自动更新闸本身抛错，跳过它照常跑：$($_.Exception.Message)" -Level WARN
    }
    Invoke-GdTakeover -S $S -Names @((Get-M9APaths -S $S).Name) -Label 'M9A'
    if (-not (Start-GdMuMu -S $S)) { Write-GdLog 'MuMu 未能确认启动，仍尝试让 M9A 自行处理' -Level WARN }
    $m9aCfg = Set-M9AInstance -S $S -Instance $inst -Apply
    $m9aCfg.Changes | ForEach-Object { Write-GdLog "  M9A：$_" }
    $run9 = { Invoke-M9ARun -S $S -Instance $inst }
    $r = Invoke-GdStep -Name '1999' -Body $run9
    <#
      ===== 批次 8：$retryable 分流改问 decide-daily-retry(Orchestration.cs) =====
      判据(not-started/empty-queue/blocked 只有 Rescued 才翻案)、if/elseif 链的落穿语义
      (retryOnce 关着不重试也照样落告警句)与全部话术都在 C#；本壳只回放日志 + 落地"活"
      (Stop-M9A / sleep 20 / 重跑)。旧的 $retryable 表达式逐行 # 前缀冻结在下方 —— test-r1999
      按那些字面钉锚(块注释不可嵌套，只能这么保)。
    #>
    #     $retryable = ($r.Status -notin @('ok', 'not-started', 'empty-queue', 'unknown-format', 'blocked')) -or
    #                  ($r.Rescued -and $r.Status -in @('not-started', 'empty-queue', 'blocked'))
    $d = Invoke-GdCli -Verb 'decide-daily-retry' -Body ([pscustomobject]@{
        game      = 'r1999'
        status    = [string]$r.Status
        rescued   = [bool]$r.Rescued
        retryOnce = [bool]$S.runtime.retryOnce
    })
    foreach ($l in @($d.Logs)) {
        $lvl = [string]$l.Level
        if ($lvl -and ($lvl -ne 'INFO')) { Write-GdLog ([string]$l.Text) -Level $lvl } else { Write-GdLog ([string]$l.Text) }
    }
    $retryable = [bool]$d.Retryable
    if ($retryable) {
        Stop-M9A -S $S
        Start-Sleep -Seconds 20
        $r = Invoke-GdStep -Name '1999重试' -Body $run9
    }
    <# 批次 8 冻结段：旧 if/elseif 的两句 WARN/ERROR 兜底话术(原文语义见 Orchestration.cs r1999 支)。
       实测 09-25 19:24：客户端在模拟器里下 1.91G 更新包，10 个任务里 8 个「任务失败」，
       重试只是再空转 5 分钟 —— "游戏没在界面上"的形态重试无用，要人去处理。 #>
    #     if ($retryable -and $S.runtime.retryOnce) {
    #         Write-GdLog $(if ($r.Rescued) { '1999 本轮未成功，但视觉兜底把挡路的界面点掉了 → 关闭 M9A 后重试一次' }
    #                       else { '1999 本轮未成功 → 关闭游戏后重试一次' }) -Level WARN
    #         Stop-M9A -S $S
    #         Start-Sleep -Seconds 20
    #         $r = Invoke-GdStep -Name '1999重试' -Body $run9
    #     } elseif ($r.Status -eq 'unknown-format') {
    #         Write-GdLog '1999 日志标志一条都没认出来 → 重试同样看不懂，直接记入本轮告警(见下面的处理建议)' -Level ERROR
    #     } elseif ($r.Status -eq 'blocked') {
    #         Write-GdLog '1999 从头一个任务起就没进游戏 → 是客户端/资源要更新或模拟器状态不对，重试没有意义，直接记入本轮告警' -Level ERROR
    #     } elseif ($r.Status -in @('not-started', 'empty-queue')) {
    #         Write-GdLog '1999 队列没开始/空跑完成 → 重试没有意义，直接记入本轮告警' -Level WARN
    #     }
    Stop-M9A -S $S
    <# 活动打完没打完 —— 让 agent 看画面判断(2026-09-30 用户口径，工具 tools\1999-event-audit.ps1)：
       只在队列真跑完的形态问；工具内部对"没有待确认活动"自带秒退，且自己负责把被 M9A「关闭游戏」
       顺手关掉的虚拟机重新起起来、用完关回去(09-30 18:21 首跑实锤的时序)。
       判定 yes 才会 ack，unknown/no 维持 event-pending 照旧等人。整段最坏七八分钟(起 VM+拉游戏+问一次)，
       任何失败只 WARN —— 审计是加分项，不许把当日轮带崩。 #>
    if ($r.Status -in @('ok', 'empty-queue')) {
        try {
            $audOut = @(& $selfEngine -NoProfile -ExecutionPolicy Bypass -File (Join-Path $global:GD.Root 'tools\1999-event-audit.ps1') -Apply 2>&1)
            $audCode = $LASTEXITCODE
            $audLine = (@($audOut | ForEach-Object { [string]$_ }) | Where-Object { $_ -match '审计结论|无事发生|维持 event-pending' } | Select-Object -Last 1)
            if ($audCode -eq 0) { Write-GdLog ('1999 活动审计：' + [string]$audLine) }
            else { Write-GdLog ('1999 活动审计没审成(退出码 ' + $audCode + ')，维持现状：' + ((@($audOut | ForEach-Object { [string]$_ })) -join ' / ')) -Level WARN }
        } catch {
            Write-GdLog "1999 活动审计本身抛错，跳过它照常收尾：$($_.Exception.Message)" -Level WARN
        }
    }
    return $r
}
