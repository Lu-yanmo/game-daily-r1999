# lib/decisions-common.ps1 — 两个游戏共用的判定底座：时间换算 + 两道手动门的读写(只读状态，不碰 GUI)
# 方舟侧看 lib/ark-decisions.ps1，1999 侧看 lib/r1999-decisions.ps1；只发布一个游戏时只需要本文件 + 对应那一侧。
#
# 关于下面的 [ValidateSet('ark','r1999')] 都换成了 Assert-GdGameId：
# 本文件的职责是"按游戏 id 读写状态"，id 该由模块清单说了算(加一款游戏不该来改这里)。
# 但本文件也是老消费方(tools\test-*.ps1 / probe-decisions / 只点 common+decisions 的自检)唯一认识的底座，
# 所以注册表没装载时它必须照旧认这两个 id —— 回落分支就是给那些入口留的，别删。

function Get-GdGameIds {
    <# 判定层认得的游戏 id：注册表里有模块就用模块的，否则回落到本仓库一直用的两个。 #>
    if (Get-Command Get-GdModuleIds -ErrorAction SilentlyContinue) {
        $ids = @(Get-GdModuleIds)
        if ($ids.Count) { return $ids }
    }
    return @('ark', 'r1999')
}

function Assert-GdGameId {
    param([Parameter(Mandatory)][string]$Game)
    $ids = @(Get-GdGameIds)
    if ($ids -notcontains $Game) {
        throw "不认识的游戏 id：'$Game'(这棵树上认得：$($ids -join ' / '))"
    }
    return $Game
}

function Get-GdSettingsBlock {
    <# 游戏 id -> settings.json 里的块名。两者不同名(1999 的 id 是 r1999，settings 块叫 re1999)，
        所以这个映射必须由模块清单提供，不能靠字符串拼 —— 以前它就是 Test-GdShopDoneEnabled 里的一句 if。 #>
    param([Parameter(Mandatory)][string]$Game)
    if (Get-Command Get-GdModule -ErrorAction SilentlyContinue) {
        $m = Get-GdModule -Id $Game
        if ($m -and [string]$m.SettingsBlock) { return [string]$m.SettingsBlock }
    }
    return $(if ($Game -eq 'ark') { 'ark' } else { 're1999' })
}

function Get-GdNowMs { [DateTimeOffset]::Now.ToUnixTimeMilliseconds() }

function Convert-GdUtc8ToMs {
    param($Text)
    if (-not $Text) { return $null }
    $s = ([string]$Text) -replace '\s+', ' '
    $s = $s.Replace('/', '-').Trim()
    $dt = [datetime]::MinValue
    $fmts = @('yyyy-MM-dd HH:mm:ss', 'yyyy-M-d H:mm:ss', 'yyyy-MM-dd', 'yyyy-M-d')
    $ok = [datetime]::TryParseExact($s, [string[]]$fmts, [System.Globalization.CultureInfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::None, [ref]$dt)
    if (-not $ok) { $ok = [datetime]::TryParse($s, [ref]$dt) }
    if (-not $ok) { return $null }
    # $dt 是 UTC+8 墙上时间：先按 UTC 解释，再回退 8 小时得到真实 UTC
    $asUtc = [DateTime]::SpecifyKind($dt, [DateTimeKind]::Utc)
    $dto = [DateTimeOffset]::new($asUtc, [TimeSpan]::FromHours(0))
    return $dto.ToUnixTimeMilliseconds() - 8 * 3600 * 1000
}

function Get-GdAckMap {
    param([Parameter(Mandatory)][string]$Game)
    # 游戏 id 会被拼进状态文件名("$Game-event-ack")，所以认不得的 id 必须当场挡住，
    # 不能让它悄悄写出一个谁都不会再读的 json。两臂共这道闸，回落臂那句原样留在下面。
    Assert-GdGameId -Game $Game | Out-Null
    # ── B-3②-d 刀①(2026-11-01 §11.189 定档)：ack 表那半段读改指核 ──
    # 核臂＝verb read-decision-state(真身 tools\panel-cs\Round.Data.cs::DecisionState)，want 里只点自己那一枚：
    #   没要的表核不去碰(不多读那两本，也不替调用方多取)。键不分大小写两侧同一把尺——C# 侧那本表用
    #   OrdinalIgnoreCase 建，交回来的对象在这里经 Convert-GdMapToHashtable 落成 PS 的 @{}，两边都不分大小写。
    # 回落臂＝下面那段，逐字是改指前的原样。
    # 语义边界(两臂必须同判语的那一句)：账本坏了两侧都拦、谁也不许咽。C# 那本表没有 try，坏 json 当场抛、
    #   信封非 0 由唯一转发通道 throw 上来，这一发的 catch 只把它交给回落臂——而回落臂读同一枚文件会抛同一句话，
    #   所以"坏了就拦下这一轮"不因为多了一条核臂就变软。反过来核不在场(exe 没编／那枚 exe 还不认这 verb／
    #   settings 缺席时核读不到它自己不消费的 ShopDoneGateOpen)也只是回落到 PS，读数一字不差。
    #   这里不取 ShopDoneGateOpen：那把尺仍归 Test-GdShopDoneEnabled(表账「不许动」那一档)。
    $core = $null
    try {
        $core = Invoke-GdCli -Verb 'read-decision-state' -Body ([pscustomobject]@{ game = $Game; want = @('ack') })
    } catch { $core = $null }
    if ($null -ne $core -and $null -ne $core.PSObject.Properties['Ack'] -and $null -ne $core.Ack) {
        return (Convert-GdMapToHashtable $core.Ack)
    }
    $obj = Get-GdStateObject -Name "$Game-event-ack" -Default $null
    if ($null -eq $obj) { return @{} }
    return (Convert-GdMapToHashtable (Get-GdMapValue $obj 'acked'))
}

function Set-GdAckKey {
    param([Parameter(Mandatory)][string]$Game, [Parameter(Mandatory)][string]$Key)
    $map = Get-GdAckMap -Game $Game
    $map[$Key] = (Get-Date -Format 'yyyy-MM-dd HH:mm')
    Set-GdStateObject -Name "$Game-event-ack" -Value ([pscustomobject]@{ acked = $map })
}

# --- "活动商店已领空"标记(手动门：领完商店后打个招呼，之后该活动不再刷体力，回落长草计划) ---
function Get-GdShopDoneMap {
    param([Parameter(Mandatory)][string]$Game)
    Assert-GdGameId -Game $Game | Out-Null
    # ── B-3②-d 刀①(2026-11-01 §11.189 定档)：领空表那半段读改指核，与上面 ack 那枚同一把出口 ──
    # 核臂＝read-decision-state 的 shopDone 那一格(want 只点这一枚)；回落臂＝下面那段，逐字是改指前原样。
    # 语义边界同 Get-GdAckMap 那一句：坏了两侧都拦(核那本表也没 try)，核不在场就整段回落到 PS，读数不变；
    #   "领空表给不给调用方"仍归这里(核只把原表和读数摆上桌，这一发只取表)。撤销那一步的 ContainsKey
    #   按不分大小写判(Remove-GdShopDoneKey 与 tools\test-ark.ps1 的[撤销标记生效]都吃这把尺)——两臂同一把。
    $core = $null
    try {
        $core = Invoke-GdCli -Verb 'read-decision-state' -Body ([pscustomobject]@{ game = $Game; want = @('shopDone') })
    } catch { $core = $null }
    if ($null -ne $core -and $null -ne $core.PSObject.Properties['ShopDone'] -and $null -ne $core.ShopDone) {
        return (Convert-GdMapToHashtable $core.ShopDone)
    }
    $obj = Get-GdStateObject -Name "$Game-shop-done" -Default $null
    if ($null -eq $obj) { return @{} }
    return (Convert-GdMapToHashtable (Get-GdMapValue $obj 'done'))
}

function Set-GdShopDoneKey {
    param([Parameter(Mandatory)][string]$Game, [Parameter(Mandatory)][string]$Key)
    $map = Get-GdShopDoneMap -Game $Game
    $map[$Key] = (Get-Date -Format 'yyyy-MM-dd HH:mm')
    Set-GdStateObject -Name "$Game-shop-done" -Value ([pscustomobject]@{ done = $map })
}

function Remove-GdShopDoneKey {
    param([Parameter(Mandatory)][string]$Game, [Parameter(Mandatory)][string]$Key)
    $map = Get-GdShopDoneMap -Game $Game
    if (-not $map.ContainsKey($Key)) { return $false }
    $map.Remove($Key)
    Set-GdStateObject -Name "$Game-shop-done" -Value ([pscustomobject]@{ done = $map })
    return $true
}

function Test-GdShopDoneEnabled {
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)][string]$Game)
    Assert-GdGameId -Game $Game | Out-Null
    # 块名从模块清单要(id 'r1999' 对应 settings 块 're1999'，两者不同名，拼不出来)
    $block = Get-GdSettingsBlock -Game $Game
    $flag = $S.$block.shopDoneStopsEvent
    return ($flag -ne $false)   # 缺省即开启，显式写 false 才关掉
}

function Show-GdGateState {
    <# status.ps1 的[6]段：把某一款的两道手动门当前状态打出来。放在这里是因为只有这一层拿得到两张表。
       追加(2026-10-12 第 1 刀)：方舟的第一道门(活动要不要自动刷)已换成"每轮机器自证"——
       这里就改报那本账(state\ark-event-probe.json)的最新一条；1999 那一路照旧报 ack 表(它没有自证通道)。 #>
    param([Parameter(Mandatory)][string]$Game)
    if ($Game -eq 'ark') {
        Write-Host "  ark: 活动图每轮自证(拿最后一关当探针，进得去就按活动图刷)"
        $pb = $null
        try {
            if (Get-Command Get-GdArkEventProbe -ErrorAction SilentlyContinue) { $pb = Get-GdArkEventProbe }
        } catch { $pb = $null }
        if ($pb) {
            $txt = if ($pb.Entered) { '进得去 → 下一轮照常刷活动图' } else { '进不去 → 判为还没打通，当日改刷长草' }
            Write-Host "  ark自证($($pb.Date)) $($pb.EventKey)/$($pb.Stage)：$txt" -ForegroundColor $(if ($pb.Entered) { 'Green' } else { 'Yellow' })
        } else {
            Write-Host "  ark自证: 还没有结论(下一轮即以活动最后一关为探针)"
        }
    } else {
        $m = Get-GdAckMap -Game $Game
        if ($m.Count -eq 0) { Write-Host "  ${Game}: 无已确认活动(新活动会先跳过清体力)" }
        else { foreach ($k in $m.Keys) { Write-Host "  ${Game}已确认: $k → $($m[$k])" } }
    }
    $d = Get-GdShopDoneMap -Game $Game
    if ($d.Count) { foreach ($k in $d.Keys) { Write-Host "  ${Game}商店已领空: $k → $($d[$k])(剩余理智转长草)" -ForegroundColor Yellow } }
}

# ================= 入口参数名(只给提示语用) =================
# 判定层会告诉用户"下一步敲什么"，但参数名是入口自己定的：单游戏仓库的入口用干净短名(-Ack / -ShopReset)，
# 本地合体工程的入口两款混在一起、必须带前缀(-AckArk / -RShopDone)。
# 所以提示语里不许写死参数名 —— 短名是回落那一支(导出的仓库一个字都不用改)，长名由该路模块清单自己声明。
# tools\menu.ps1 那个方向(菜单去探测入口有哪些参数)是同一件事的另一半。
# ⚠ 11-01 丙形真转接(§11.176 ⑥ 落码)：这一族原来不是"现问"，是"入口点完库调一次同步件、把长名盖进
#   进程内的一张表"。那张表和模块清单自己声明的 EntryVerbs.Merged 是两份名册(不同源)，而它盖没盖取决于
#   这个进程先跑过哪个入口 —— 漏调用的就是"提示语让你敲短名、可本仓库入口只认长名"那一形(2026-09-24 犯过)，
#   调全了也只是把第二份名册按清单再抄一遍。现在 Get-GdEntryVerb 每次现问注册表，与 Rust 核
#   tools\rustcore\src\main.rs 的 round_entry_verb 同一把尺；同步件与那张表的读写侧(现场覆盖件、取表件)
#   连同那张写死的回落表一起退役。
#   代价照登：注册表不在场的进程(只点 common＋decisions 的老消费方)现在只能看到短名 —— 那是回落那一支，
#   tools\test-lib.ps1 [t1c] 的判语随之改成钉这一支；"清单在场⇒长名"那一支由 tools\test-modules.ps1 [m6]
#   钉住(它装载得起来，并且拿入口自己的 param() 做对照，两把尺不同轴，所以不把装载面搬到 test-lib 去)。
function Get-GdEntryVerb {
    <# 现问，不落表：先判这棵树上把几路当一款游戏在管(合体才换长名)，是就现问该路清单自己声明的
        EntryVerbs.Merged；问不到(注册表没装载／这一路不在这棵树上／这一路没声明这枚 verb)就回落干净短名。
        判据与 tools\rustcore\src\main.rs 的 round_entry_verb 逐条对齐：合体闸按"编排文件在位"数，
        取长名前再核该路自己那两把在位尺，名册里没这枚 verb 才落到 '-' + verb。 #>
    param([Parameter(Mandatory)][string]$Game,
          [Parameter(Mandatory)][ValidateSet('Ack', 'ShopDone', 'ShopReset', 'Farm')][string]$Verb)
    if ((Test-GdMergedEntry) -and (Get-Command Get-GdModules -ErrorAction SilentlyContinue)) {
        foreach ($m in @(Get-GdModules)) {
            if (([string]$m.Id) -ne $Game) { continue }
            if (-not (Test-GdModuleInstalled -Module $m -Key 'OrchestrationLib')) { continue }
            $v = $m.EntryVerbs.Merged
            if (-not $v) { continue }
            $name = [string]$v.$Verb
            if ($name) { return $name }
        }
    }
    # 回落：合体闸没过、注册表不在场、或这一路没声明这枚 verb —— 短名按被问的 verb 现拼，不许写死任何一枚。
    return '-' + $Verb
}
function Test-GdMergedEntry {
    <# 这棵树是不是"一个入口管两款"的合体工程：判据用两路的编排文件在不在，
        跟 Get-GdRequiredPathKeys"按那路的助手库在不在决定该催哪些路径"是同一招 —— 导出仓库里不猜配置。
        注册表装着时按模块清单数(加一款游戏不需要来改这里)；没装载时退回按两个文件名探测，
        因为 status.ps1 / 自检这些老消费方本来就只点 common+decisions，不点 modules.ps1。 #>
    if (Get-Command Get-GdModules -ErrorAction SilentlyContinue) {
        $all = @(Get-GdModules)
        if ($all.Count) {
            $n = @($all | Where-Object { Test-GdModuleInstalled -Module $_ -Key 'OrchestrationLib' }).Count
            return ($n -gt 1)
        }
    }
    $lib = Join-Path $global:GD.Root 'lib'
    return ((Test-Path -LiteralPath (Join-Path $lib 'ark-daily.ps1')) -and (Test-Path -LiteralPath (Join-Path $lib 'r1999-daily.ps1')))
}
