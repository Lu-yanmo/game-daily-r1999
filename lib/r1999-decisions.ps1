# lib/r1999-decisions.ps1 — 重返未来：1999(MuMu + M9A)侧的取数层 + PS→C# 转发壳
# 依赖 lib/common.ps1 + lib/decisions-common.ps1；只读判定，不启动任何助手程序。
#
# ── 冻结(2026-09-26，PS→C# 批次 1)────────────────────────────────────────────
# Resolve-R1999Plan / Resolve-R1999Instance(扫描部分)的**规则本体**已迁到
# tools\panel-cs\Decisions.cs(verb：decide-r1999 / r1999-instance；契约见 docs\ps-migration-batch0.md)。
# 本文件留下的是：取数层 Get-R1999Event(读 M9A 活动数据、算 DataAge、追后缀) + 两个转发壳。
#   ⚠ 10-29 B-3②-d 刀① 改口：那句"读 M9A 活动数据、算 DataAge"的**取数半段**自此改指核(verb read-r1999-activity，
#   真身 tools\panel-cs\Round.Data.cs:412)，PS 这一段降为**回落臂**；"追后缀"那句(§11.52 ⑤ 定的"活动数据 N 天前")仍留这里。
#   ⚠ 刀⑰ 笔(c)-2(2026-11-07)改判：上面"降为回落臂"那半句自此只作历史所指——回落臂整块出仓(核缺席⇒throw⇒拒跑，
#   那两句人话的对面逐字在核侧 Round.Data.cs::R1999Activity)，本文件自此只留核臂一条路。
# 壳里保留的两件事是协议的一部分：
#   * IgnoreShopDone 与 shopDoneStopsEvent 决定"给不给领空图" —— 门留在 PS，规则读值；
#   * Resolve-R1999Instance 的两道前置检查(计划名没配 / M9A 目录不在)留在 PS，
#     那两句人话是"这台机器没配好"，不是判定规则。
# 改文案/改分支请改 C# 那份，判定面的取值断言在 tools\panel-cs\Decisions.SelfTest.cs(随 build.ps1 -Verify 的 --dectest 跑)，不要在这里加规则。⚠ 10-26 甲档第六刀摘掉那 45 档没有对面的场景；第八刀把屏保 14 档逐字搬进该自检并整件退役 `tools\probe-decisions-golden.ps1`(取证见 docs\rust-core-plan.md §11.141／§11.144)——那座门自此不存在，判定面只认 `--dectest`。
# ──────────────────────────────────────────────────────────────────────────────

function Resolve-R1999Plan {
    <# 转发壳：版本窗口/主线确认门/领空回落/文案的规则在 C#(decide-r1999)。
       ⚠ 刀⑯ 笔(d)-1(2026-11-06)改口：旧组值 Test-GdShopDoneEnabled＋Get-GdShopDoneMap＋Get-GdAckMap 本批出仓，
       改指核 read-decision-state **一次**取齐；"给不给领空图"的折算仍在本壳(吃核交回的 ShopDoneGateOpen——
       真身 Round.FlagEnabled：缺省即开启、显式写 false 才关)，IgnoreShopDone 语义不变。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)]$Data,
        [Parameter(Mandatory)][long]$Now,
        [switch]$IgnoreShopDone
    )
    $ds = Invoke-GdCli -Verb 'read-decision-state' -Body ([pscustomobject]@{
        game = 'r1999'
        settings = [pscustomobject]@{ re1999 = $S.re1999 }
    })
    $shopDone = @{}
    if (-not $IgnoreShopDone -and $ds.ShopDoneGateOpen) { $shopDone = $ds.ShopDone }
    $body = [pscustomobject]@{
        re1999 = [pscustomobject]@{ mainStoryRequiresAck = $S.re1999.mainStoryRequiresAck }
        now = $Now
        data = $Data
        ack = $ds.Ack
        shopDone = $shopDone
    }
    return Invoke-GdCli -Verb 'decide-r1999' -Body $body
}

function Get-R1999Event {
    <# 读 M9A 自带活动数据(会随 M9A 资源更新)后交给 Resolve-R1999Plan(现即转发壳) #>
    param([Parameter(Mandatory)]$S, [switch]$IgnoreShopDone)

    $now = Get-GdNowMs
    # ── B-3②-d 刀①(2026-10-29)：下面这段"拼路径→判 ready→读活动 json→算 DataAge(天)"改指核 ──
    # 核臂＝verb read-r1999-activity(真身 tools\panel-cs\Round.Data.cs::R1999Activity，自检靶 Round.SelfTest.cs:983-1017)。
    # `paths` 走**直答**：壳手里已经有 $S.paths，喂给核就不让核去现读第二份 settings(双源)。
    # ⚠ 刀⑰ 笔(c)-2(2026-11-07)改判：旧"回落臂＝else 那一整块"(try 把 throw 吞成 null 再 PS 直读)本批出仓——
    # 它与刀⑯ 判死的"回落臂假绿形"同形(核缺席被化装成合法长草读数)。自此核缺席⇒Invoke-GdCli throw⇒这一路拒跑；
    # 那两句人话(空串"还没填"/"缺少 $path")的对面逐字在核侧 Round.Data.cs(:440 现拍)，"不喂这个键怎么回落"自此由核答。
    # 九键 idle 信封仍只在这里造(判定结论，§11.52 ⑤)，核交回的 Reason 汇入 $idle，全函数只构造一次。
    $idle = $null
    $core = Invoke-GdCli -Verb 'read-r1999-activity' -Body ([pscustomobject]@{
        paths = [pscustomobject]@{ m9aDir = $S.paths.m9aDir; m9aActivityData = $S.paths.m9aActivityData }
    })
    if (-not [bool]$core.Ready) { $idle = [string]$core.Reason }
    else {
        $data = $core.Data
        $age = [double]$core.DataAgeDays
    }
    if ($null -ne $idle) {
        return [pscustomobject]@{
            Mode = 'idle'; Version = $null; VersionName = $null; EventKey = $null; EventTip = $null
            Reason = $idle; Instance = 'idle'; DataAge = $null; ShopDone = $false
        }
    }

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
