# lib/decisions-common.ps1 — 两个游戏共用的判定底座：时间换算 + 两道手动门的读写(只读状态，不碰 GUI)
# 方舟侧看 lib/ark-decisions.ps1，1999 侧看 lib/r1999-decisions.ps1；只发布一个游戏时只需要本文件 + 对应那一侧。

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
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game)
    $obj = Get-GdStateObject -Name "$Game-event-ack" -Default $null
    if ($null -eq $obj) { return @{} }
    return (Convert-GdMapToHashtable (Get-GdMapValue $obj 'acked'))
}

function Set-GdAckKey {
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game, [Parameter(Mandatory)][string]$Key)
    $map = Get-GdAckMap -Game $Game
    $map[$Key] = (Get-Date -Format 'yyyy-MM-dd HH:mm')
    Set-GdStateObject -Name "$Game-event-ack" -Value ([pscustomobject]@{ acked = $map })
}

# --- "活动商店已领空"标记(手动门：领完商店后打个招呼，之后该活动不再刷体力，回落长草计划) ---
function Get-GdShopDoneMap {
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game)
    $obj = Get-GdStateObject -Name "$Game-shop-done" -Default $null
    if ($null -eq $obj) { return @{} }
    return (Convert-GdMapToHashtable (Get-GdMapValue $obj 'done'))
}

function Set-GdShopDoneKey {
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game, [Parameter(Mandatory)][string]$Key)
    $map = Get-GdShopDoneMap -Game $Game
    $map[$Key] = (Get-Date -Format 'yyyy-MM-dd HH:mm')
    Set-GdStateObject -Name "$Game-shop-done" -Value ([pscustomobject]@{ done = $map })
}

function Remove-GdShopDoneKey {
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game, [Parameter(Mandatory)][string]$Key)
    $map = Get-GdShopDoneMap -Game $Game
    if (-not $map.ContainsKey($Key)) { return $false }
    $map.Remove($Key)
    Set-GdStateObject -Name "$Game-shop-done" -Value ([pscustomobject]@{ done = $map })
    return $true
}

function Test-GdShopDoneEnabled {
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game)
    $flag = if ($Game -eq 'ark') { $S.ark.shopDoneStopsEvent } else { $S.re1999.shopDoneStopsEvent }
    return ($flag -ne $false)   # 缺省即开启，显式写 false 才关掉
}

function Show-GdGateState {
    <# status.ps1 的[6]段：把某一款的两道手动门当前状态打出来。放在这里是因为只有这一层拿得到两张表 #>
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game)
    $m = Get-GdAckMap -Game $Game
    if ($m.Count -eq 0) { Write-Host "  ${Game}: 无已确认活动(新活动会先跳过清体力)" }
    else { foreach ($k in $m.Keys) { Write-Host "  ${Game}已确认: $k → $($m[$k])" } }
    $d = Get-GdShopDoneMap -Game $Game
    if ($d.Count) { foreach ($k in $d.Keys) { Write-Host "  ${Game}商店已领空: $k → $($d[$k])(剩余理智转长草)" -ForegroundColor Yellow } }
}

# ================= 入口参数名(只给提示语用) =================
# 判定层会告诉用户"下一步敲什么"，但参数名是入口自己定的：单游戏仓库的入口用干净短名(-Ack / -ShopReset)，
# 本地合体工程的入口两款混在一起、必须带前缀(-AckArk / -RShopDone)。
# 所以提示语里不许写死参数名 —— 默认给短名(导出的仓库一个字都不用改)，合体工程的入口点完 lib 之后
# 调一次 Sync-GdEntryVerbs(它按"这棵树有没有两路的编排文件"决定要不要换成长名)。
# tools\menu.ps1 那个方向(菜单去探测入口有哪些参数)是同一件事的另一半。
function Get-GdEntryVerbs {
    # 懒初始化：lib\common.ps1 开着 StrictMode 1.0，直接读未声明的 $script:GdEntryVerbs 会当场报错；
    # 也不用在文件顶层赋初值 —— 那样同一个作用域被重复 dot-source 时会把入口已经覆盖过的表冲掉。
    if (-not (Get-Variable -Name GdEntryVerbs -Scope Script -ErrorAction SilentlyContinue)) {
        $short = @{ Ack = '-Ack'; ShopDone = '-ShopDone'; ShopReset = '-ShopReset' }
        $script:GdEntryVerbs = @{ ark = $short.Clone(); r1999 = $short.Clone() }
    }
    return $script:GdEntryVerbs
}
function Get-GdEntryVerb {
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game,
          [Parameter(Mandatory)][ValidateSet('Ack', 'ShopDone', 'ShopReset')][string]$Verb)
    return (Get-GdEntryVerbs)[$Game][$Verb]
}
function Set-GdEntryVerb {
    param([Parameter(Mandatory)][ValidateSet('ark', 'r1999')][string]$Game,
          [Parameter(Mandatory)][ValidateSet('Ack', 'ShopDone', 'ShopReset')][string]$Verb,
          [Parameter(Mandatory)][string]$Name)
    (Get-GdEntryVerbs)[$Game][$Verb] = $Name
}
function Test-GdMergedEntry {
    <# 这棵树是不是"一个入口管两款"的合体工程：判据用两路的编排文件在不在，
        跟 Get-GdRequiredPathKeys"按那路的助手库在不在决定该催哪些路径"是同一招 —— 导出仓库里不猜配置。 #>
    $lib = Join-Path $global:GD.Root 'lib'
    return ((Test-Path -LiteralPath (Join-Path $lib 'ark-daily.ps1')) -and (Test-Path -LiteralPath (Join-Path $lib 'r1999-daily.ps1')))
}
function Sync-GdEntryVerbs {
    <# 每个"会打出下一步提示"的入口(daily-clear / status / probe-decisions / panel)点完库后调一次：
        合体工程里把提示语换成本仓库入口真有的长参数名，单游戏仓库保持默认短名(一个字都不用改)。
        为什么要收成一个函数、而不是各入口抄一遍六行映射：抄漏的地方就会出现
        "status.ps1 让你敲 -Ack，可合体入口只有 -AckArk"这种情况(2026-09-24 就是这个样子)。 #>
    if (-not (Test-GdMergedEntry)) { return }
    foreach ($one in @(
            @{ Game = 'ark'; Verb = 'Ack'; Name = '-AckArk' },
            @{ Game = 'ark'; Verb = 'ShopDone'; Name = '-ArkShopDone' },
            @{ Game = 'ark'; Verb = 'ShopReset'; Name = '-ArkShopReset' },
            @{ Game = 'r1999'; Verb = 'Ack'; Name = '-Ack1999' },
            @{ Game = 'r1999'; Verb = 'ShopDone'; Name = '-RShopDone' },
            @{ Game = 'r1999'; Verb = 'ShopReset'; Name = '-RShopReset' }
        )) { Set-GdEntryVerb -Game $one.Game -Verb $one.Verb -Name $one.Name }
}
