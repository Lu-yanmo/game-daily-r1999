# lib/decisions.ps1 — "今天该跑什么"的门面：把两边判定拼起来，另带一个跨游戏便捷入口 Show-GdDecision
#
# 2026-09-23 拆分：判定本体已经按游戏搬走，本文件只留聚合(所以老代码 . lib\decisions.ps1 照旧可用)。
#   lib\decisions-common.ps1   两个游戏共用的底座：时间换算 + 活动确认门/商店领空标记的读写
#   lib\ark-decisions.ps1      方舟侧：Get-GdStageTable / Test-GdStages / Resolve-ArkPlan / Get-ArkEvent
#   lib\r1999-decisions.ps1    1999 侧：Resolve-R1999Plan / Get-R1999Event / Resolve-R1999Instance
# 只发布一个游戏的仓库时，本文件可以原样复制过去：下面按"文件在不在"决定点不点，
# 单游戏仓库里没有另一侧那个文件，就自动少一侧的函数，不会因为 dot 一个不存在的文件而炸。
# 依赖关系：这三个文件只用 lib\common.ps1 里的函数，而调用方(daily-clear / status / 自检)本来就先把
# common.ps1 点进来了，所以这里不再重复点 —— 重复点会把控制台编码再设一遍，白做工。
foreach ($part in @('decisions-common', 'ark-decisions', 'r1999-decisions')) {
    $partPath = Join-Path $PSScriptRoot "$part.ps1"
    if (Test-Path -LiteralPath $partPath) { . $partPath }
}

function Show-GdDecision {
    <# 一次把两路的判定都拿回来(自检/面板这类跨游戏视角用；日常跑的是各自的 Resolve-*，不走这里) #>
    param([Parameter(Mandatory)]$S)
    $ark = $null
    $r = $null
    $inst = $null
    if (Get-Command Get-ArkEvent -ErrorAction SilentlyContinue) { $ark = Get-ArkEvent -S $S }
    if (Get-Command Get-R1999Event -ErrorAction SilentlyContinue) {
        $r = Get-R1999Event -S $S
        try { $inst = Resolve-R1999Instance -S $S -PlanKey $r.Instance } catch { $r.Reason += "；$($_.Exception.Message)" }
    }
    return [pscustomobject]@{ Ark = $ark; Re1999 = $r; Instance = $inst }
}
