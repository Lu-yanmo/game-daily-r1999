# lib/r1999-decisions.ps1 — 重返未来：1999(MuMu + M9A)侧的取数层 + PS→C# 转发壳
# 依赖 lib/common.ps1 + lib/decisions-common.ps1；只读判定，不启动任何助手程序。
#
# ── 冻结(2026-09-26，PS→C# 批次 1)────────────────────────────────────────────
# Resolve-R1999Plan / Resolve-R1999Instance(扫描部分)的**规则本体**已迁到
# tools\panel-cs\Decisions.cs(verb：decide-r1999 / r1999-instance；契约见 docs\ps-migration-batch0.md)。
# 本文件留下的是：取数层 Get-R1999Event(读 M9A 活动数据、算 DataAge、追后缀) + 两个转发壳。
# 壳里保留的两件事是协议的一部分：
#   * IgnoreShopDone 与 shopDoneStopsEvent 决定"给不给领空图" —— 门留在 PS，规则读值；
#   * Resolve-R1999Instance 的两道前置检查(计划名没配 / M9A 目录不在)留在 PS，
#     那两句人话是"这台机器没配好"，不是判定规则。
# 改文案/改分支请改 C# 那份，判定面的取值断言在 tools\panel-cs\Decisions.SelfTest.cs(随 build.ps1 -Verify 的 --selftest 跑)，不要在这里加规则。⚠ 2026-10-26 甲档第六刀起 `tools\probe-decisions-golden.ps1` 只剩屏保一族 14 档——它 old 侧原本点的是本件在 HEAD 里的逐字副本(两侧同一份代码、没有对面)，取证与判据见 docs\rust-core-plan.md §11.141，别把那座门的"绿"读成"这里验过"。
# ──────────────────────────────────────────────────────────────────────────────

function Resolve-R1999Plan {
    <# 转发壳：版本窗口/主线确认门/领空回落/文案的规则在 C#(decide-r1999) #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)]$Data,
        [Parameter(Mandatory)][long]$Now,
        [switch]$IgnoreShopDone
    )
    $shopDone = @{}
    if (-not $IgnoreShopDone -and (Test-GdShopDoneEnabled -S $S -Game 'r1999')) {
        $shopDone = Get-GdShopDoneMap -Game 'r1999'
    }
    $body = [pscustomobject]@{
        re1999 = [pscustomobject]@{ mainStoryRequiresAck = $S.re1999.mainStoryRequiresAck }
        now = $Now
        data = $Data
        ack = (Get-GdAckMap -Game 'r1999')
        shopDone = $shopDone
        verbAck = (Get-GdEntryVerb -Game r1999 -Verb Ack)
        verbShopReset = (Get-GdEntryVerb -Game r1999 -Verb ShopReset)
    }
    return Invoke-GdCli -Verb 'decide-r1999' -Body $body
}

function Get-R1999Event {
    <# 读 M9A 自带活动数据(会随 M9A 资源更新)后交给 Resolve-R1999Plan(现即转发壳) #>
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
    <# 两道前置检查留在 PS(那是"这台机器没配好"的人话)；按实例名扫文件的重活转发给 C#(r1999-instance) #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)][string]$PlanKey)
    $want = Get-GdMapValue $S.re1999.planToInstance $PlanKey
    if (-not $want) { throw "settings.re1999.planToInstance 里没有 $PlanKey 对应的实例名" }
    $dir = Join-GdPath $S.paths.m9aDir @($S.paths.m9aInstancesDir)
    if (-not (Test-GdPathReady $dir)) {
        throw $(if ($dir) { "M9A 实例目录不存在：$dir" } else { 'M9A 目录还没填(先跑 setup.ps1)，读不到实例列表' })
    }
    return Invoke-GdCli -Verb 'r1999-instance' -Body ([pscustomobject]@{ dir = $dir; want = [string]$want; planKey = $PlanKey })
}
