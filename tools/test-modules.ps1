# tools/test-modules.ps1 — 框架层自检：模块清单齐不齐、注册表对不对、框架有没有掺进游戏知识
# 用法: pwsh -NoProfile -File tools\test-modules.ps1
#       只读：不改配置、不启动任何程序、不点鼠标。判定钩子会读 state，所以照样存/还原。
#
# 为什么要单独一份：lib\modules.ps1 是"少写一项不会立刻报错"的那类代码 ——
# 清单里缺一个钩子，dot-source 阶段什么都不知道，要等到夜里计划任务走到那一步才炸，
# 而且炸出来是一句"无法将…识别为 cmdlet"。这套断言就是把那类错误提前到白天。
#
# 这一份会原样进两个导出仓库，所以全部断言都按"这棵树上有哪些模块"来判，
# 绝不写死 ark / r1999：单游戏仓库里只有一款，它也必须全绿。
param([switch]$NoExit)

. (Join-Path $PSScriptRoot 'test-lib.ps1')

# test-lib 只点判定层和助手库；框架和清单要自己点(清单文件由 Import-GdModules 扫出来，别在这儿写死名字)
$modsLib = Join-Path $root 'lib\modules.ps1'
if (-not (Test-Path -LiteralPath $modsLib)) {
    Write-Host "`n  这棵树上没有 lib\modules.ps1 → 框架自检没意义" -ForegroundColor Yellow
    Check 'lib\modules.ps1 在(框架层没被裁掉)' $false
    $res = Show-GdTestResult -Tag 'modules'
    if ($NoExit) { return $res }
    exit 1
}
. $modsLib

Write-Host "`n[m1] 装载与注册表"
$loaded = @()
$loadErr = ''
try { $loaded = @(Import-GdModules -Quiet) } catch { $loadErr = $_.Exception.Message }
Check 'Import-GdModules 全程不抛(清单体检通过)' ($loadErr -eq '') $loadErr
Check '注册表非空' ($loaded.Count -gt 0) "$($loaded.Count) 款：$($loaded -join ' / ')"
# 入口那三行的顺序也在这里演一遍：注册表只存清单，业务库要在顶层 dot 才留得下函数定义。
# 少了这一步，[m4] 调 Resolve 钩子会报"…不会被识别为 cmdlet" —— 那正是钩子半夜炸的样子。
# 断言写法保持通用：从库文件本身扫出它定义的函数名，再问 Get-Command 认不认，不写死任何一款的函数名。
foreach ($libPath in (Get-GdModuleLibPaths)) { . $libPath }
foreach ($m in @(Get-GdModules -AvailableOnly)) {
    $id = [string]$m.Id
    $want = @()
    foreach ($libName in @($m.Libs)) {
        $libFile = Join-Path $root "lib\$libName"
        if (-not (Test-Path -LiteralPath $libFile)) { continue }
        $want += @([regex]::Matches([System.IO.File]::ReadAllText($libFile), '(?m)^\s*function\s+([A-Za-z][\w-]+)') | ForEach-Object { $_.Groups[1].Value })
    }
    $want = @($want | Sort-Object -Unique)
    $gone = @($want | Where-Object { -not (Get-Command $_ -ErrorAction SilentlyContinue) })
    Check "[$id] 顶层点库后清单声明的 $($want.Count) 个函数全在" ($gone.Count -eq 0 -and $want.Count -gt 0) $(if ($gone.Count) { "缺：$($gone -join ' / ')" } else { '' })
}

$modFiles = @(Get-ChildItem -LiteralPath (Join-Path $root 'lib') -Filter '*-module.ps1' | Sort-Object Name)
Check 'lib\*-module.ps1 有几个就注册了几个(没有静默丢失)' ($loaded.Count -eq $modFiles.Count) "文件 $($modFiles.Count) / 注册 $($loaded.Count)"
$expectOrder = @($modFiles | ForEach-Object { ($_.BaseName -split '-module')[0] })
Check '注册顺序 = 文件名顺序(汇总/收尾的先后因此可预期)' ((@($loaded | Sort-Object) -join ',') -eq (@($expectOrder | Sort-Object) -join ',')) "$($loaded -join ' / ')"

$avail = @(Get-GdModules -AvailableOnly)
Check '这棵树上注册的模块全都装了(没有"注册了但助手库不在")' ($avail.Count -eq $loaded.Count) "装了 $($avail.Count) / 注册 $($loaded.Count)"
# 注册表的判据(助手库在不在)必须和催路径的判据(Get-GdRequiredPathKeys 看同一个 lib)对齐，
# 否则会出现"面板里能看到这一路、setup 却从不催它的路径"这种对不上的口子。
$reqKeys = @(Get-GdRequiredPathKeys $root)
$declaredKeys = @()
foreach ($m in $avail) { $declaredKeys += @($m.PathKeys) }
$declaredUniq = @($declaredKeys | Sort-Object -Unique)
Check '已装模块声明的 PathKeys 全在 Get-GdRequiredPathKeys 名单里' ((@($declaredUniq | Where-Object { $reqKeys -notcontains $_ }).Count) -eq 0) "$($declaredUniq -join ' / ') ⊆ $($reqKeys -join ' / ')"
Check 'Get-GdModuleIds 与 Get-GdModules 数一致' ((@(Get-GdModuleIds).Count) -eq $loaded.Count)

# 重复注册：静默覆盖比报错更难查(改了哪一份全看点库顺序)
$dupId = [string]$avail[0].Id
$before = $global:GdModules.Count
[void](Register-GdModule -Manifest $avail[0])
Check "同 Id 重复注册不覆盖、只丢 WARN($dupId)" ($global:GdModules.Count -eq $before) "$before → $($global:GdModules.Count)"
Check '清单没 Id 时注册直接抛' $(try { [void](Register-GdModule -Manifest [pscustomobject]@{ Id = '' }); $false } catch { $true })
Check '取不存在的模块返回 $null(不抛，由调用方决定怎么说)' ($null -eq (Get-GdModule -Id '不存在的模块'))

Write-Host "`n[m2] 每份清单的完备性"
foreach ($m in @(Get-GdModules)) {
    $id = [string]$m.Id
    $bad = @(Assert-GdModuleManifest -Module $m)
    Check "[$id] 清单体检没有意见" ($bad.Count -eq 0) ($bad -join '；')
    foreach ($k in @('Id', 'Name', 'ShortName', 'AssistantLib', 'OrchestrationLib', 'SettingsBlock')) {
        Check "[$id] $k 非空" ([bool][string]$m.$k)
    }
    Check "[$id] PathKeys 至少一项(这一路总要有要催的路径)" (@($m.PathKeys).Count -gt 0) (@($m.PathKeys) -join ',')
    Check "[$id] Libs 至少两项(助手库 + 编排库)" (@($m.Libs).Count -ge 2) (@($m.Libs) -join ' ')
    # 声明了却不在树上的库 = 半夜那句"无法将…识别为 cmdlet"
    $missing = @(Get-GdModuleLibPaths -MissingOnly)
    Check "[$id] 声明的业务库全在 lib\ 下" ($missing.Count -eq 0) ($missing -join ' / ')
    Check "[$id] 助手库与编排库都在清单的 Libs 里" (($m.Libs -contains [string]$m.AssistantLib) -and ($m.Libs -contains [string]$m.OrchestrationLib))
    Check "[$id] RequiresAdmin 与 AdminWhy 搭配套(要提权就得说为什么)" $(-not $m.RequiresAdmin -or [string]$m.AdminWhy) "RequiresAdmin=$($m.RequiresAdmin)"
    if ($m.RequiresAdmin) {
        Check "[$id] 要管理员 → 提权开关名(ElevationFlag)写了" ([bool][string]$m.ElevationFlag) "ElevationFlag=$($m.ElevationFlag)"
    }
    foreach ($h in @('EnvProbes', 'Resolve', 'Preview', 'Run', 'Stop', 'Ack', 'ShopState')) {
        Check "[$id] 必备钩子 $h 是 scriptblock" ($m.Hooks.$h -is [scriptblock])
    }
    foreach ($h in @('StatusRows', 'StatusVersion', 'StatusProcs', 'StatusProcExtra', 'StatusPlan', 'StatusSection', 'SupervisorEvidence', 'SupervisorRestart')) {
        $v = $m.Hooks.$h
        Check "[$id] 可选钩子 $h 要么没写、要么是 scriptblock" ($null -eq $v -or $v -is [scriptblock])
    }
    # status.ps1 那一段是 `if ($m.Hooks.StatusSection -and $m.StatusSectionTitle)` —— 少了标题，钩子永远不跑
    if ($m.Hooks.StatusSection) { Check "[$id] 有 StatusSection 钩子就得给 StatusSectionTitle(否则那段永远不印)" ([bool][string]$m.StatusSectionTitle) }
    Check "[$id] 短名适合印在汇总里(无空格无换行)" ([string]$m.ShortName -match '^\S+$') $m.ShortName
}

function Get-GdCodeOnlyText {
    <# 把注释剥干净，只留代码。两步都要：整块注释(井号+尖括号包起来那种)如果只按行剥，
        块里那些不带井号的中间行会原样留着 —— 而"框架里引用了游戏专有函数"这句话恰恰写在块注释中间。
        注释里别写那两个成对符号本身：写了就是提前把这块注释闭合掉，本函数第一个版本就栽在这。 #>
    param([Parameter(Mandatory)][string]$Text)
    $t = [regex]::Replace($Text, '<\#[\s\S]*?\#>', ' ')
    return ((@($t -split "`n") | ForEach-Object { $_ -replace '#.*$', '' }) -join "`n")
}

Write-Host "`n[m3] 框架纯洁性(业务知识只能待在清单里)"
$fwCode = Get-GdCodeOnlyText -Text ([System.IO.File]::ReadAllText($modsLib))
Check '剥注释这招真的能拿到块注释中间的函数名(自检本身先自检)' ($fwCode -notmatch 'Resolve-GdArkDecision')
foreach ($pat in @('Resolve-GdArk', 'Start-GdArkDaily', 'Stop-MaaAndGame', 'MaaAgentBinary', 'Resolve-GdR1999', 'Start-GdR1999Daily', 'Stop-M9A', 'Stop-GdMuMu', 'Get-R1999Event', 'Show-GdArkPreview')) {
    Check "modules.ps1 的代码里没有 $pat" ($fwCode -notmatch [regex]::Escape($pat))
}
Check 'modules.ps1 不硬编码某一款的提权文案(全走 AdminWhy/AdminHint)' ($fwCode -notmatch '端游要管理员')
Check 'modules.ps1 里没有出现 ark/r1999 的分支字面量(选路由 -Ids 传进来)' ($fwCode -notmatch "'ark'" -and $fwCode -notmatch "'r1999'")
# 反过来：清单里也不该出现另一款游戏的库(导出时会把不存在的路径点进来)。
# 但"两路都写了的那份"不算对方的业务库，那是**共用的基础设施**(lib\vision-rescue.ps1 就是这样：
# 兜底的闸门/解析/点位换算两边一模一样，只有截图和点击各写一份后端)。判据用"至少两份清单都有它"，
# 而不是列一张共享名单 —— 名单会漏更新，漏了就变成"要么放过一份没人点的库，要么把正常写法判成泄漏"。
$sharedLibs = @{}
foreach ($m in @(Get-GdModules)) {
    foreach ($n in @($m.Libs | ForEach-Object { [string]$_ })) {
        if (-not $sharedLibs.ContainsKey($n)) { $sharedLibs[$n] = @{} }
        $sharedLibs[$n][[string]$m.Id] = $true
    }
}
$shared = @($sharedLibs.Keys | Where-Object { @($sharedLibs[$_].Keys).Count -ge 2 })
foreach ($m in @(Get-GdModules)) {
    $txt = [System.IO.File]::ReadAllText((Join-Path $root "lib\$([string]$m.Id)-module.ps1"))
    $other = @(Get-GdModules | Where-Object { [string]$_.Id -ne [string]$m.Id } | ForEach-Object { @($_.Libs) } |
        ForEach-Object { [string]$_ } | Where-Object { $_ -notin $shared })
    $leaked = @($other | Where-Object { $txt -match [regex]::Escape($_) })
    Check "[$([string]$m.Id)] 清单没引用别的模块的业务库" ($leaked.Count -eq 0) ($leaked -join ' / ')
}
# 这份自检查原样进两个导出仓库 → 涉及"两路共用"的断言按注册表里有几款判，绝不写死 ark / r1999
if (@(Get-GdModules).Count -ge 2) {
    Check '两路共用的内核确实出现在两份清单里(vision-rescue.ps1)' (@($shared) -contains 'vision-rescue.ps1') ($shared -join ' / ')
}
Check 'Get-GdModuleLibPaths 把共享库去重了(入口点两遍会重复定义一堆函数)' (
    (@(Get-GdModuleLibPaths).Count -eq @(Get-GdModuleLibPaths | ForEach-Object { [System.IO.Path]::GetFileName($_) } | Sort-Object -Unique).Count)) `
    ("$(@(Get-GdModuleLibPaths).Count) 个文件 / $(@($shared).Count) 个共享")
# 后端按命名约定接：内核不许认识任何一路，各路的靶子函数得在自己那一份后端里
$vrText = [System.IO.File]::ReadAllText((Join-Path $root 'lib\vision-rescue.ps1'))
Check '兜底内核不认识端游也不认识模拟器(只按 New-GdVisionTarget-<kind> 找)' (
    (Get-GdCodeOnlyText -Text $vrText) -notmatch 'New-GdVisionTarget-ark|New-GdVisionTarget-mumu' -and
    $vrText -match 'New-GdVisionTarget-\$\(')
# 自检脚手架点库的名单是另一份硬编码(tools\test-lib.ps1 第 10 行)：清单加了库而它没跟上，
# test-ark / test-r1999 里那些 (Get-Command X).Definition 断言会整段跳过 —— 静默少一层保护，最难发现的那种。
# 只管兜底这三份(test-lib 的名单本来就不含 ark-daily/r1999-daily 这类编排库，别把不该管的也钉上)。
$tlText = [System.IO.File]::ReadAllText((Join-Path $root 'tools\test-lib.ps1'))
foreach ($m in @(Get-GdModules)) {
    $id = [string]$m.Id
    $libs = @($m.Libs | ForEach-Object { [string]$_ })
    if ($libs -notcontains 'vision-rescue.ps1') { continue }
    $mine = @($libs | Where-Object { $_ -like 'vision-*.ps1' -and $_ -ne 'vision-rescue.ps1' })
    Check "[$id] 用了兜底内核就得自带一份后端(否则 Get-GdVisionTarget 永远找不到靶子)" ($mine.Count -eq 1) ($libs -join ' / ')
    foreach ($bf in $mine) {
        $btPath = Join-Path $root "lib\$bf"
        Check "[$id] 清单声明的后端 $bf 在这棵树上" (Test-Path -LiteralPath $btPath)
        if (-not (Test-Path -LiteralPath $btPath)) { continue }
        $bt = [System.IO.File]::ReadAllText($btPath)
        $kind = [regex]::Match($bt, '(?m)^function New-GdVisionTarget-([a-z0-9]+)').Groups[1].Value
        Check "[$id] 后端 $bf 交出一个 New-GdVisionTarget-<kind>" ($kind -ne '') "kind=[$kind]"
        Check "[$id] 靶子带齐契约字段(内核只认这几个名字)" (
            $bt -match 'Shot\s*=' -and $bt -match 'Click\s*=' -and $bt -match 'Alive\s*=' -and
            $bt -match 'Enabled\s*=' -and $bt -match 'NeedsAdmin\s*=' -and $bt -match 'Scene\s*=')
        # 命名约定真接得上：内核按同一个 kind 找得到靶子(建靶子只造闭包，不碰窗口、不碰 adb、不联网)
        $t = $null
        try { $t = Get-GdVisionTarget -S $S -Kind $kind } catch { $t = $null }
        Check "[$id] Get-GdVisionTarget -Kind $kind 拿得到靶子" ($t -and [string]$t.Kind -eq $kind) $(
            if ($t) { "开关 Enabled=$([bool]$t.Enabled) 要提权=$([bool]$t.NeedsAdmin)" } else { '没拿到' })
        Check "[$id] 后端 $bf 只写在 $id 一路的清单里(另一路的仓库不会带上它)" (
            @(Get-GdModules | Where-Object { [string]$_.Id -ne $id -and (@($_.Libs | ForEach-Object { [string]$_ }) -contains $bf) }).Count -eq 0)
        Check "[$id] 自检脚手架也点了 $bf(那两路自检的结构断言才不会整段跳过)" (
            $tlText -match ("'" + [regex]::Escape([System.IO.Path]::GetFileNameWithoutExtension($bf)) + "'"))
    }
}

Write-Host "`n[m4] 只读钩子真调得动(EnvProbes / Resolve / Preview)"
foreach ($m in @(Get-GdModules -AvailableOnly)) {
    $id = [string]$m.Id
    $rows = $null
    $err = ''
    try { $rows = Get-GdModuleEnvRows -Module $m -S $S } catch { $err = $_.Exception.Message }
    Check "[$id] EnvProbes 调得动" ($err -eq '') $err
    Check "[$id] 环境探测至少一项" (@($rows).Count -gt 0) "$(@($rows).Count) 项"
    $labels = @($rows.Keys)
    Check "[$id] 探测项标签非空" (@($labels | Where-Object { -not [string]$_ }).Count -eq 0) ($labels -join ' / ')
    Check "[$id] 探测结果是有序表(体检里那几行的先后不能变)" ($rows -is [System.Collections.Specialized.OrderedDictionary])
    Check "[$id] 探测项的值都落在 paths.* 上(框架不猜路径)" (@($labels).Count -eq @($rows.Values | Where-Object { $_ -is [string] -or $null -eq $_ }).Count)

    $dec = $null
    $err = ''
    Save-GdTestState @("$id-event-ack", "$id-shop-done")
    try {
        $dec = Invoke-GdModuleHook -Module $m -Hook Resolve -Params @{ S = $S; Opt = @{ Force = $true } }
    } catch { $err = $_.Exception.Message } finally { Restore-GdTestState }
    Check "[$id] Resolve 调得动(-Force 也不抛)" ($err -eq '') $err
    Check "[$id] 判定结果带 Mode(汇总/历史那一行要用)" ([bool][string]$dec.Mode) "Mode=$($dec.Mode)"
    if ($dec) {
        $pvErr = ''
        try {
            $sb = $m.Hooks.Preview
            & $sb -S $S -Decision $dec 6>&1 | Out-Null
        } catch { $pvErr = $_.Exception.Message }
        Check "[$id] Preview 调得动(dry-run 就靠它)" ($pvErr -eq '') $pvErr
    }
}

Write-Host "`n[m5] 选择与参数名"
Check 'Select-GdModules 不传 Id = 装了的全跑' ((@(Select-GdModules).Count) -eq @(Get-GdModules -AvailableOnly).Count)
Check "Select-GdModules -Ids all 等价于全跑" ((@(Select-GdModules -Ids @('all')).Count) -eq @(Get-GdModules -AvailableOnly).Count)
$firstId = [string](@(Get-GdModuleIds -AvailableOnly)[0])
Check "Select-GdModules -Ids $firstId 只挑一款" ((@(Select-GdModules -Ids @($firstId)).Count) -eq 1)
$pickErr = ''
try { [void](Select-GdModules -Ids @('__no_such__')) } catch { $pickErr = $_.Exception.Message }
Check '挑了不存在的模块当场抛(不静默少跑一款)' ($pickErr -ne '')
Check '抛出来那句会告诉这棵树上有什么(照抄就能改对)' ($pickErr -match $firstId) $pickErr
foreach ($id in @(Get-GdModuleIds)) {
    Check "[$id] 过 Get-GdGameId 校验(注册表与判定层的口径一致)" $(try { $null = Assert-GdGameId -Game $id; $true } catch { $false })
}
Check 'Get-GdGameIds 就是注册表里的 Id' ((@(Get-GdGameIds) -join ',') -eq (@(Get-GdModuleIds) -join ',')) ((@(Get-GdGameIds)) -join ' / ')
Check '认不出来的游戏名会被判定层挡下' $(try { Assert-GdGameId -Game '__no_such__'; $false } catch { $true })
# 每款游戏 settings 里那块名字不一定等于 Id(re1999 vs r1999)，取块必须走 Get-GdSettingsBlock
foreach ($m in @(Get-GdModules)) {
    $blk = [string]$m.SettingsBlock
    $got = Get-GdSettingsBlock -Game ([string]$m.Id)
    Check "[$([string]$m.Id)] Get-GdSettingsBlock 取到的是清单写的那块" ($got -eq $blk) "$blk"
    Check "[$([string]$m.Id)] settings 模板里有 $($blk) 这一块" ($null -ne (Read-GdJson -Path (Join-Path $root 'config\settings.example.json')).$blk)
}

Write-Host "`n[m6] 入口动词与真参数对得上(菜单/面板印的开关名不能是编的)"
# 11-01 丙形真转接(§11.176 ⑥)：这里不再点同步件——下面每一枚开关名都由 Get-GdEntryVerb 现问本进程的注册表得到。
# 参数表来自仓库根目录那个 daily-clear.ps1：它是这一棵树上真正的入口，动词必须被它认识。
# 读不到(工程被裁得没入口)就直接判失败 —— 静默跳过正是要防的那种"绿着但没人检查"。
$dailyTable = @{}
$dailyPath = Join-Path $root 'daily-clear.ps1'
try {
    $cmd = Get-Command $dailyPath -ErrorAction Stop
    foreach ($k in $cmd.Parameters.Keys) { $dailyTable[[string]$k] = $true }
} catch { Write-Host "  读不到 $dailyPath 的参数表：$($_.Exception.Message)" -ForegroundColor Red }
Check 'daily-clear.ps1 的参数表读得到' ($dailyTable.Count -gt 0) "$($dailyTable.Count) 个参数"
foreach ($m in @(Get-GdModules)) {
    $id = [string]$m.Id
    # 默认三门每路都查；此外只查该模块在 Merged 里自己声明过的动词 ——
    # ark 声明了 Farm='-FarmArk' 会被查，r1999 没有 Farm 这道门，硬塞给它就是假红。
    $verbs = @('Ack', 'ShopDone', 'ShopReset')
    if ($m.EntryVerbs -and $m.EntryVerbs.Merged) {
        # Merged 多半是 hashtable：它的 PSObject.Properties 会把 .NET 成员(Count/IsReadOnly…)也吐出来，
        # 拿键枚举才对；写成对象(PSCustomObject)的那种清单则只有属性名。两种都兼容。
        $mv = $m.EntryVerbs.Merged
        $declared = @()
        if ($mv -is [System.Collections.IDictionary]) { $declared = @($mv.Keys | ForEach-Object { [string]$_ }) }
        else { $declared = @($mv.PSObject.Properties.Name) }
        foreach ($p in $declared) {
            if ($p -in @('Ack', 'ShopDone', 'ShopReset', 'Farm') -and $p -notin $verbs) { $verbs += $p }
        }
    }
    foreach ($verb in $verbs) {
        if ($id -eq 'ark' -and $verb -eq 'Ack') {
            # 2026-10-12 第 1 刀：方舟的活动确认门从入口退场了(-AckArk 不再被 daily-clear.ps1 认识，
            # 活动图改每轮机器自证)。但 EntryVerbs.Merged 的 Ack 键与 Hooks.Ack 那一格都还在——
            # 前者是 Rust 链体那枚 entryverb("ark","Ack") 的 ask 名册位(返回值弃读，删键=形状变要升契约号)，
            # 后者是 lib\modules.ps1 的七枚必需钩子名册。所以这一路反过来钉：入口**必须不认识** -AckArk。
            # 谁哪天把参数加回来，这格就红，提醒他要么真装回人肉门、要么改这里。
            $arkAckNote = '名册位与钩子仍在，值弃读'
            if ($dailyTable.ContainsKey('AckArk')) { $arkAckNote = 'daily-clear.ps1 又认识 AckArk 了' }
            Check '[ark] Ack 已退场：入口不再认识 -AckArk(活动图改每轮自证)' (-not $dailyTable.ContainsKey('AckArk')) $arkAckNote
            continue
        }
        $flag = Get-GdEntryVerb -Game $id -Verb $verb        # 函数返回的是带前导连字符的完整开关名(判定日志直接拼进提示语)
        Check "[$id] $verb 的开关名 $flag 是入口真认识的参数" ($dailyTable.ContainsKey($flag.TrimStart('-')))
    }
}
Check '合体判断走注册表(Test-GdMergedEntry 返回布尔)' ($null -ne (Test-GdMergedEntry)) "merged=$(Test-GdMergedEntry)"

Write-Host "`n[m7] 注册表被清空时的行为(工程裁坏了要说人话，别少印四段)"
$snap = [ordered]@{}
foreach ($k in @($global:GdModules.Keys)) { $snap[$k] = $global:GdModules[$k] }
$global:GdModules.Clear()
Check '清空后 Get-GdModules 返回空(不抛)' (@(Get-GdModules).Count -eq 0)
Check '清空后 Select-GdModules 抛"没有可加载的游戏模块"' $(try { [void](Select-GdModules); $false } catch { $_.Exception.Message -match '没有可加载' })
Check '清空后 Get-GdGameIds 落到老的两款兜底' ((@(Get-GdGameIds) -join ',') -match 'ark')
foreach ($k in @($snap.Keys)) { $global:GdModules[$k] = $snap[$k] }
Check '还原后注册表数量不变' ($global:GdModules.Count -eq $snap.Count) "$($snap.Count) → $($global:GdModules.Count)"

Write-Host "`n[m8] 本机专属值不许出现在源码/文案里(导出前那道闸，前移到这里)"
# 判据和"发出去之前扫导出目录"用的是同一个函数(lib\common.ps1)，所以这里先红一次，就不用等
# publish-split 跑到最后一步才发现某份自检样本里粘了本机路径 —— 那已经是晚一整轮的代价(2026-09-25 两轮连踩)。
# 值全部运行时从本机 settings.json / M9A 实例目录里取：这段自含零条本机字面量，不会自己命中自己。
# 新机器上没跑过 setup 也照样有得扫(env.USERPROFILE 那一条拦的是"源码里写死家目录")。
# 红的时候只报"哪个文件 <- 哪一项"，值本身不进日志(日志是要进诊断包给人看的)。
$m8Vals = @(Get-GdLocalOnlyValues -LocalSettings (Join-Path $root 'config\settings.json'))
Check "本机专属值取到 $($m8Vals.Count) 项(有的扫)" ($m8Vals.Count -gt 0)
$m8Hits = @(Test-GdExportLeaks -Dir $root -Values $m8Vals)
Check '源码 / README / 自检样本里一项本机专属值都没有' ($m8Hits.Count -eq 0) (@($m8Hits | ForEach-Object { $_ }) -join ' ｜ ')

Write-Host "`n[m9] 问模型那一层(tools\vision\ask.py + PowerShell 侧的桥)"
# 为什么值得单独一段：这一层的规矩是"key 只走环境变量、Python 只发请求、点位判断留在 PowerShell"。
# 三条都是**改了不会立刻坏**的那种 —— 哪天有人图省事把 key 拼进命令行，日常照样跑通，
# 直到日志/诊断进程列表里带出 key 才出事。所以规矩在这里钉成断言，不靠人记得住。
$askPy = Join-Path $root 'tools\vision\ask.py'
Check 'tools\vision\ask.py 在(它是内核的一部分，两路仓库都要带上)' (Test-Path -LiteralPath $askPy)
$pySrc = ''
if (Test-Path -LiteralPath $askPy) {
    $pyBytes = [System.IO.File]::ReadAllBytes($askPy)
    Check 'ask.py 不带 BOM(python 会把 BOM 当语法错误)' (
        $pyBytes.Length -lt 3 -or -not ($pyBytes[0] -eq 0xEF -and $pyBytes[1] -eq 0xBB -and $pyBytes[2] -eq 0xBF))
    $pySrc = [System.Text.Encoding]::UTF8.GetString($pyBytes)
    Check 'ask.py 用 LF(和 .gitattributes 给文本定的规矩一致)' (-not ($pySrc -match "`r"))
    Check 'ask.py 只用标准库(不需要 pip：embeddable 包里就这些)' (
        $pySrc -match '(?m)^import urllib\.request' -and
        -not ($pySrc -match '(?m)^\s*(from|import) (requests|httpx|openai|dashscope|PIL|numpy)'))
    Check 'ask.py 里的 key 只从环境变量读，没有 --api-key 这种命令行口子' (
        $pySrc -match 'os\.environ' -and -not ($pySrc -match '--api[-_]key'))
    Check 'ask.py 里没有长得像 key 的字面量' (-not ($pySrc -match 'sk-[A-Za-z0-9]{12,}'))
    Check 'ask.py 自己带离线自检(--selftest)' ($pySrc -match 'def run_selftest')
    Check 'ask.py 只有一处真的发请求(urlopen 不被自检绕过)' (([regex]::Matches($pySrc, 'urllib\.request\.urlopen')).Count -eq 1)
}
$rawDef = ''
if (Get-Command Invoke-GdVisionRaw -ErrorAction SilentlyContinue) { $rawDef = (Get-Command Invoke-GdVisionRaw).Definition }
Check 'PowerShell 侧有 Invoke-GdVisionRaw(问模型的唯一出口，自检能整体替换它)' ($rawDef.Length -gt 0)
if ($rawDef) {
    # 注释里点名了 Invoke-RestMethod(解释"为什么换成 Python")，所以判"有没有自己发 HTTP"要先把注释剥掉
    $rawCode = Get-GdCodeOnlyText -Text $rawDef
    # 这条判的是"PowerShell 别再自己发 HTTP"。那两个 cmdlet 的名字要拆开拼：
    # tools\check-ps51.ps1 有一条"文件里出现 Invoke-RestMethod 就得开 TLS 1.2"的启发式，
    # 连着写会让它把这个自检文件当成要走网络请求的代码(拼起来 = 说清楚"这里不该有它"，又不误报)。
    $httpVerbs = @('Invoke-Rest' + 'Method', 'Invoke-Web' + 'Request') -join '|'
    Check '桥起的是 python + ask.py，不是自己发 HTTP' (
        $rawCode -match 'Get-GdPythonExe' -and $rawCode -match 'ask\.py' -and -not ($rawCode -match $httpVerbs))
    Check 'key 只进子进程的环境变量' ($rawDef -match '-Env\s+@\{\s*DASHSCOPE_API_KEY')
    Check 'key 不出现在命令行参数里(进程列表和日志都会把它带出去)' (-not ($rawDef -match '-Arguments[^\r\n]*\$key'))
    Check '提示词走请求文件不走命令行(长文本带引号换行，塞命令行必然出乱子)' ($rawDef -match "'--request'")
}
$gateDef = ''
if (Get-Command Get-GdPythonExe -ErrorAction SilentlyContinue) { $gateDef = (Get-Command Get-GdPythonExe).Definition }
Check '找 Python 会跳过 WindowsApps 那个商店占位程序' ($gateDef -match 'WindowsApps')
Check '找到的解释器要真 import 得动 ask.py 要用的那几个库' ($gateDef -match 'Test-GdPythonUsable')
# 2026-10-28 甲档第二十刀刀③：这一格原来现读 Invoke-GdVisionRescue 的定义文本比两个 IndexOf，那枚壳
# 随兜底编排六枚一起出仓了(它不带 -ErrorAction SilentlyContinue，留着就是整脚本当场崩而不是单条红)。
# "问不成就别白拍一张"这条闸序判据没丢，它现在住在两处：核内 --selftest 第 19 节 rescue-ok 的投影序
# (resolve-rescue-context 必须排在 ark-window-shot 之前，第 30 节 ark 27 档金样同尺)，
# 以及 C# 出 skip 话术那侧的 --livetest。"搬过去的尺还在不在"那三枚无条件静态锚落 tools\test-ark.ps1 的 t11。
# 真跑一遍 ask.py 的离线自检。这台机器上没 Python 就如实报"没跑"，不算红 ——
# 缺不缺解释器是机器现场状态，不该把只读自检弄红(见项目记忆那条"只读自检不许要求机器现场状态")。
$pyExe = ''
if (Get-Command Get-GdPythonExe -ErrorAction SilentlyContinue) { $pyExe = Get-GdPythonExe }
if ($pyExe -and (Test-Path -LiteralPath $askPy)) {
    $st = Invoke-GdChild -File $pyExe -Arguments @($askPy, '--selftest') -TimeoutSeconds 60
    $sj = $null
    try { $sj = ("$($st.Out)").Trim() | ConvertFrom-Json } catch { }
    Check 'ask.py --selftest 全过(离线：不联网、不碰窗口)' ($st.Code -eq 0 -and $sj -and $sj.ok) $(
        if ($sj) { ($sj.failures -join ' / ') } else { "退出码 $($st.Code) / $($st.Err)" })
} else {
    Check 'ask.py --selftest 全过(这台机器没有可用的 Python → 没跑)' $true '跳过：跑 setup.ps1 -WithPython 之后这一条才会真验'
}

Write-Host "`n[m10] 兜底后端与清单对账(kind 是清单上的字段，不是外壳里的一张表)"
# 为什么单独一段：2026-09-26 把兜底后端泛化成两套(端游 ark / 模拟器 mumu)之后，"这一路该找哪个后端"
# 变成了一条**业务知识** —— 写在外壳里就是框架污染，写错了也不会报错，只会"兜底静默不生效"(从来不问模型)。
# 所以这条知识只在清单的 VisionKind 上存一份，这里逐路对账。
foreach ($m in @(Get-GdModules -AvailableOnly)) {
    $id = [string]$m.Id
    $kind = if ($m.PSObject.Properties.Name -contains 'VisionKind') { [string]$m.VisionKind } else { '' }
    Check "[$id] 清单声明了 VisionKind(外壳不许自己记 ark/mumu 对照表)" ($kind.Length -gt 0) "kind=[$kind]"
    Check "[$id] VisionKind 是小写短名(内核拼的是 New-GdVisionTarget-<kind>，大小写或空格都会让它找不到)" (
        $kind -match '^[a-z][a-z0-9]*$') $kind
    # 后端文件名与 kind 绑死：kind 写 mumu 就得有 lib\vision-mumu.ps1；
    # 后半句更要紧 —— 文件在但没写进这一路的 Libs，等于压根没人点它，还是静默不生效。
    $beFile = Join-Path $root "lib\vision-$kind.ps1"
    Check "[$id] 后端文件按约定存在(lib\vision-$kind.ps1)" (Test-Path -LiteralPath $beFile)
    Check "[$id] 后端文件写进了这一路的 Libs" (@($m.Libs) -contains "vision-$kind.ps1") (@($m.Libs) -join ' ')
    $builder = "New-GdVisionTarget-$kind"
    Check "[$id] 顶层点库之后 $builder 真在(命名约定对得上)" ([bool](Get-Command $builder -ErrorAction SilentlyContinue))
}
# 框架与工具层不许硬写某一路的 kind：内核只按命名约定拼函数名，外壳只问清单。
# 判的是"把 kind 当字面量用"的那几种写法(-Target/-Kind/直接调 New-GdVisionTarget-xxx)，
# 不是"文件里出现过 mumu 这四个字母"—— status.ps1 点 lib\mumu.ps1 那种是库名，另一回事。
foreach ($f in @('lib\vision-rescue.ps1', 'lib\modules.ps1', 'tools\vision-ask.ps1', 'daily-clear.ps1', 'status.ps1', 'panel.ps1')) {
    $p = Join-Path $root "tools\$f"
    if (-not (Test-Path -LiteralPath $p)) { $p = Join-Path $root $f }
    if (-not (Test-Path -LiteralPath $p)) { continue }
    $code = Get-GdCodeOnlyText -Text ([System.IO.File]::ReadAllText($p))
    $hard = $code -match "-Target\s+'(ark|mumu)'" -or $code -match "-Kind\s+'(ark|mumu)'" -or
            $code -match 'New-GdVisionTarget-(ark|mumu)\b'
    Check "$(Split-Path -Leaf $p) 没有把兜底 kind 当字面量硬写(-Target/-Kind/New-GdVisionTarget-xxx)" (-not $hard)
}
# 三份模板都得有那个领奖白名单键：导出闸门(Test-GdTemplateCoversCode)要等到打发布包才查得到，
# 而"代码读了 settings 里没有的键"这件事在本地就该红。
foreach ($tpl in @('config\settings.example.json', 'publish\src\ark\config\settings.example.json', 'publish\src\r1999\config\settings.example.json')) {
    $p = Join-Path $root $tpl
    if (-not (Test-Path -LiteralPath $p)) {
        # publish\src\ 那两份**只有合体工程里有**：导出仓库整个 publish\ 都不发(见 SPLIT-PLAN)，
        # 拿它判红等于让 publish-split 的验收链永远过不去 —— 不在这棵树就跳过并说一句。
        # 而这一路自己的 config\settings.example.json 不在仍然是真缺(它是这棵树唯一的模板)，照旧判红。
        if ($tpl -like 'publish\*') { Write-Host "  (这棵树没有 $tpl —— 导出仓库里不发 publish\，跳过)"; continue }
        Check "$tpl 在(模板缺了就是安装即崩)" $false; continue
    }
    $txt = [System.IO.File]::ReadAllText($p)
    Check "$tpl 带 visionAllowReward=false(默认硬禁领奖)" ($txt -match '"visionAllowReward"\s*:\s*false')
}

Write-Host "`n[sv] 监督 agent(lib\supervisor.ps1 + tools\supervise.ps1)"
# 全部离线：三段纯函数喂字典判决策表，采集用临时文件真演(首拍不扫旧内容/追加才算新增/跨天换路径)，
# 记录写进临时目录后原样归还 $global:GD —— 不要求机器现场状态，也不碰任何进程。
$supLib = Join-Path $root 'lib\supervisor.ps1'
$supLoop = Join-Path $root 'tools\supervise.ps1'
Check 'lib\supervisor.ps1 在(它是内核文件，两仓都要有)' (Test-Path -LiteralPath $supLib)
Check 'tools\supervise.ps1 在(心跳子进程本体)' (Test-Path -LiteralPath $supLoop)

# ---- 监督族刀①(11-02 接线)静态锚：轮档那两臂的"两臂都在" ----
# 为什么落在 [sv] 段里、却钉在那道 Test-Path 门**之外**：这一格钉的是 lib\rust-round.ps1 的形状，而那枚文件不属
# 监督族、刀③ 摘件后它照旧在场；写进下面那个 if (supLib 与 supLoop 都在) 里，摘件那一笔会把这格一起带走，
# 接线的证据反而随靶件消失 —— 那是"把钉做在会被拆的那面墙上"。
# 为什么用整本读源码而不是 Get-Command .Definition：自检套件没点 lib\rust-round.ps1(test-lib.ps1 只点判定层与
# 助手库)，函数体在这一棵树上拿不到。剥注释两形逐字复用 tools\test-ark.ps1 那批锚 —— 本刀的注释面**就写了**
# start-supervisor／Stop-GdSupervisor 这些词(那是语义边界登记)，不剥注释的正向锚会在臂被摘掉后照样绿。
$rrSrc = ''
try { $rrSrc = [System.IO.File]::ReadAllText((Join-Path $root 'lib\rust-round.ps1')) } catch { }
$rrSrc = ($rrSrc -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' '
Check '①核臂在(start)：round-supervisor-start 现问宿主批的 start-supervisor' (
    $rrSrc.Contains("Invoke-GdCli -Verb 'start-supervisor'")) $(
    if ($rrSrc.Length -eq 0) { 'lib\rust-round.ps1 不在场(尺没接到电)' } else { "len=$($rrSrc.Length)" })
Check '①核臂在(stop)：round-supervisor-stop 现问同一批的 stop-supervisor' (
    $rrSrc.Contains("Invoke-GdCli -Verb 'stop-supervisor'"))
Check '①回落臂在(start)：核不可用时仍走 Get-Command 守卫 + PS 真身(逐字＝接线前原样)' (
    $rrSrc.Contains('Get-Command Start-GdSupervisor -ErrorAction SilentlyContinue') -and
    $rrSrc.Contains('[void](Start-GdSupervisor -S $S -Games ([string]$prm.games))'))
Check '①回落臂在(stop)：核不可用时仍走 Stop-GdSupervisor(逐字＝接线前原样)' (
    $rrSrc.Contains('Get-Command Stop-GdSupervisor -ErrorAction SilentlyContinue') -and
    $rrSrc.Contains('[void](Stop-GdSupervisor)'))
Check '①两臂同形：start／stop 两臂都按 Available/Err 两枚键交回执(与 Supervisor.SelfTest.cs 钉的核侧键名同形)' (
    ([regex]::Matches($rrSrc, 'Available = \[bool\]\$core\.Available; Err = \[string\]\$core\.Err')).Count -eq 2) $(
    'hits=' + ([regex]::Matches($rrSrc, 'Available = \[bool\]\$core\.Available')).Count)
Check '①语义边界：信封里只喂四枚 runtime 原始值，整枚 settings 不往下传(「取键逐枚点名」那把尺也管这条通道)' (
    $rrSrc.Contains('settings = [pscustomobject]@{ runtime = [pscustomobject]@{') -and
    -not [regex]::IsMatch($rrSrc, 'settings\s*=\s*\$S\b'))
Check '①顺序锚在：监督起来 → 设睡眠屏蔽 那一发仍在(start 臂末尾)，接线不许把顺序挪' (
    $rrSrc.Contains('if (-not $esHeld) { $esHeld = Start-GdRoundHostNoSleep -S $S }'))

if ((Test-Path -LiteralPath $supLib) -and (Test-Path -LiteralPath $supLoop)) {
    . $supLib
    $svEntry = ''
    try { $svEntry = [System.IO.File]::ReadAllText((Join-Path $root 'daily-clear.ps1')) } catch { }
    Check '入口点 lib\supervisor.ps1(不然 Invoke-GdDailyRound 的守卫等于没接)' ($svEntry -match [regex]::Escape('lib\supervisor.ps1'))

    # ---- 配置语义(和 arkVisionRescue 同口径：没显式 false 就是开) ----
    function New-GdSvS { param($V) [pscustomobject]@{ runtime = [pscustomobject]$V } }
    Check 'supervisor 缺省=开'        (Get-GdSupervisorEnabled -S (New-GdSvS @{}))
    Check 'supervisor=false 才关'     (-not (Get-GdSupervisorEnabled -S (New-GdSvS @{ supervisor = $false })))
    Check 'supervisor=true 是开'      (Get-GdSupervisorEnabled -S (New-GdSvS @{ supervisor = $true }))
    Check '间隔缺省 60s'              ((Get-GdSupervisorIntervalSec -S (New-GdSvS @{})) -eq 60)
    Check '间隔<15s 归 60(拍太快没意义)' ((Get-GdSupervisorIntervalSec -S (New-GdSvS @{ supervisorIntervalSeconds = 5 })) -eq 60)
    Check '间隔 90s 原样'             ((Get-GdSupervisorIntervalSec -S (New-GdSvS @{ supervisorIntervalSeconds = 90 })) -eq 90)
    Check '静止阈值缺省 30 分钟'      ((Get-GdSupervisorQuietSec -S (New-GdSvS @{})) -eq 1800)
    Check '静止阈值下限 5 分钟'       ((Get-GdSupervisorQuietSec -S (New-GdSvS @{ supervisorQuietMinutes = 1 })) -eq 300)
    Check '干预额度缺省 1'            ((Get-GdSupervisorMaxInterventions -S (New-GdSvS @{})) -eq 1)
    Check '干预额度可为 0(纯报告模式)' ((Get-GdSupervisorMaxInterventions -S (New-GdSvS @{ supervisorMaxInterventions = 0 })) -eq 0)

    # ---- 状态判定表 ----
    function SvEv { param($D = $false, $A = $true, $AA = 10, $DA = 10)
        [pscustomobject]@{ Done = $D; AssistantAlive = $A; AsstAgeSec = $AA; DailyAgeSec = $DA } }
    Check '完成线压死一切：进程没了也判 done' ((Get-GdSupervisorState -Evidence (SvEv $true $false) -EverAlive $true) -eq 'done')
    Check '曾活着+现在没了+没完成 = dead'      ((Get-GdSupervisorState -Evidence (SvEv $false $false) -EverAlive $true) -eq 'dead')
    Check '助手从没活着过 ≠ dead(开轮太早)'    ((Get-GdSupervisorState -Evidence (SvEv $false $false 9999 10) -EverAlive $false) -eq 'busy')
    Check '两本日志都超阈值才 quiet'          ((Get-GdSupervisorState -Evidence (SvEv $false $true 9999 9999) -QuietSec 1800) -eq 'quiet')
    Check '只有助手日志老 = 不算静止'          ((Get-GdSupervisorState -Evidence (SvEv $false $true 9999 10) -QuietSec 1800) -eq 'busy')
    Check '只有主日志老 = 不算静止'            ((Get-GdSupervisorState -Evidence (SvEv $false $true 10 9999) -QuietSec 1800) -eq 'busy')
    Check '刚好踩在阈值上不算静止(严格大于)'    ((Get-GdSupervisorState -Evidence (SvEv $false $true 1800 1800) -QuietSec 1800) -eq 'busy')

    # ---- 动作决策表 ----
    $scrBlocking = [pscustomobject]@{ Ok = $true; Blocking = $true; What = '活动公告' }
    $scrClear    = [pscustomobject]@{ Ok = $true; Blocking = $false; What = '主界面' }
    $scrFailed   = [pscustomobject]@{ Ok = $false; Why = '截图没成功' }
    Check 'busy 不动'      ((Get-GdSupervisorAction -State 'busy').Kind -eq 'none')
    Check 'done 不动'      ((Get-GdSupervisorAction -State 'done').Kind -eq 'none')
    Check 'dead 通知一次'  ((Get-GdSupervisorAction -State 'dead').Kind -eq 'notify')
    Check 'dead 通知过就闭嘴' ((Get-GdSupervisorAction -State 'dead' -AlreadyNotified $true).Kind -eq 'none')
    Check '静止一拍不动手(可能是抖动)' ((Get-GdSupervisorAction -State 'quiet' -QuietStreak 1 -Screen $scrClear -CanRestart $true).Kind -eq 'none')
    Check 'blocking 交给主流程兜底，不干预' ((Get-GdSupervisorAction -State 'quiet' -QuietStreak 2 -Screen $scrBlocking -CanRestart $true).Kind -eq 'none')
    Check '静止+画面干净+许重启 = restart_assistant' ((Get-GdSupervisorAction -State 'quiet' -QuietStreak 2 -Screen $scrClear -CanRestart $true -Interventions 0 -MaxInterventions 1).Kind -eq 'restart_assistant')
    Check '额度用完退回 notify' ((Get-GdSupervisorAction -State 'quiet' -QuietStreak 2 -Screen $scrClear -CanRestart $true -Interventions 1 -MaxInterventions 1).Kind -eq 'notify')
    Check '这一路不许重启 → 只 notify' ((Get-GdSupervisorAction -State 'quiet' -QuietStreak 2 -Screen $scrClear -CanRestart $false).Kind -eq 'notify')
    Check '看不清画面只 notify(绝不瞎重启)' ((Get-GdSupervisorAction -State 'quiet' -QuietStreak 2 -Screen $scrFailed -CanRestart $true).Kind -eq 'notify')
    Check 'notify 说过不再重复轰炸' ((Get-GdSupervisorAction -State 'quiet' -QuietStreak 3 -Screen $scrFailed -AlreadyNotified $true).Kind -eq 'none')

    # ---- 采集：临时文件真演(首拍对齐 / 追加才算 / 带上下文的完成线 / 跨天换路径) ----
    $svTmp = Join-Path $env:TEMP ('gd-sv-' + $PID)
    [void](New-Item -ItemType Directory -Path $svTmp -Force)
    try {
        $logA = Join-Path $svTmp 'a.log'
        '上一轮留下的 任务已全部完成 旧行' | Set-Content -LiteralPath $logA -Encoding utf8
        function Test-GdProcessRunning { param([string]$Name) return $true }   # 钉桩：进程表一律按"助手活着"答
        $ctx = @{ asst_offset = @{}; task_seen = @{} }
        $spec = @{ AssistantProc = '假助手'; AssistantLog = $logA; DonePattern = '任务已全部完成' }
        $ev1 = Collect-GdSupervisorEvidence -S $S -Spec $spec -Ctx $ctx -Game 't1'
        Check '首拍只对末尾，不把旧完成线当这一轮的' (-not $ev1.Done)
        '这一轮 任务已全部完成' | Add-Content -LiteralPath $logA -Encoding utf8
        $ev2 = Collect-GdSupervisorEvidence -S $S -Spec $spec -Ctx $ctx -Game 't1'
        Check '拍与拍之间新增的行才算数' ([bool]$ev2.Done)
        $logB = Join-Path $svTmp 'b.log'
        'x' | Set-Content -LiteralPath $logB -Encoding utf8
        $specB = @{ AssistantProc = '假助手'; AssistantLog = $logB
                    DonePattern = '启动任务完成'; ProgressPattern = '开始任务：' }
        [void](Collect-GdSupervisorEvidence -S $S -Spec $specB -Ctx $ctx -Game 't2')
        '启动任务完成' | Add-Content -LiteralPath $logB -Encoding utf8
        Check '没见过进度线，「启动任务完成」单独出现不算完成' (-not (Collect-GdSupervisorEvidence -S $S -Spec $specB -Ctx $ctx -Game 't2').Done)
        ('开始任务：日常', '启动任务完成') -join "`r`n" | Add-Content -LiteralPath $logB -Encoding utf8
        Check '进度线补上后，完成线算数' ([bool](Collect-GdSupervisorEvidence -S $S -Spec $specB -Ctx $ctx -Game 't2').Done)
        '任务已全部完成' | Set-Content -LiteralPath (Join-Path $svTmp 'c.log') -Encoding utf8
        $specC = @{ AssistantProc = '假助手'; AssistantLog = (Join-Path $svTmp 'c.log'); DonePattern = '任务已全部完成' }
        Check '日志换路径(跨天)后首拍同样不扫旧内容' (-not (Collect-GdSupervisorEvidence -S $S -Spec $specC -Ctx $ctx -Game 't2').Done)

        # ---- 记录：写进临时目录再归还，读方(FileShare)拿到的必须是同一份 ----
        $oldState = $global:GD.State; $oldLogs = $global:GD.Logs
        try {
            $global:GD.State = Join-Path $svTmp 'state'
            $global:GD.Logs = Join-Path $svTmp 'logs'
            [void](New-Item -ItemType Directory -Path $global:GD.State -Force)
            [void](New-Item -ItemType Directory -Path $global:GD.Logs -Force)
            $meta = @{ pid = 4242; owner = 1; started = '2026-09-26 00:00:00'; games = 'all' }
            $row = @([pscustomobject]@{ game = 't1'; state = 'busy'; alive = $true; asst_age = 5; done = $false; screen = ''; action = 'none'; why = '' })
            Write-GdSupervisorBeat -Meta $meta -PerGame $row -Actions @()
            $doc = Read-GdSupervisorSummary
            Check '一拍落两个文件(jsonl 流水 + json 现值)' ((Test-Path -LiteralPath (Get-GdSupervisorBeatPath)) -and ($null -ne $doc) -and ([int]$doc.pid -eq 4242))
            Check 'beat 内容原样读回' ((@(@($doc.beats)[0].games)[0].game) -eq 't1')
            for ($i = 0; $i -lt 32; $i++) { Write-GdSupervisorBeat -Meta $meta -PerGame $row -Actions @() }
            Check 'json 里只留最近 30 拍(它是"此刻"，流水在 jsonl)' ((@((Read-GdSupervisorSummary).beats).Count) -le 30)
            $jl = @(Get-Content -LiteralPath (Get-GdSupervisorBeatPath)).Count
            Check 'jsonl 一行一拍、不截断(33 拍)' ($jl -eq 33) "实际 $jl"
        } finally { $global:GD.State = $oldState; $global:GD.Logs = $oldLogs }
    } finally { Remove-Item -LiteralPath $svTmp -Recurse -Force -ErrorAction SilentlyContinue }

    # ---- 详情文本(SV 行的正文)：喂假文档逐字段转达，一点现场都不许依赖 ----
    # 这段是钉 2026-09-26 那次"C# 主控台看不到监督现场"的：外壳只会按 '|' 切字段、按段显示，
    # 所以"给人看的话"必须在这儿(能被喂假数据)验完，而不是等真跑一轮有 supervision.json 才第一次见面。
    $fakeBeat = [pscustomobject]@{ ts = '2026-09-26 13:05:00'; games = @(
        [pscustomobject]@{ game = 'ark'; state = 'quiet'; alive = $true; asst_age = 1810; done = $false; screen = 'clear'; action = 'notify'; why = '连续静止，额度用完' },
        [pscustomobject]@{ game = 'r1999'; state = 'dead'; alive = $false; asst_age = -1; done = $true; screen = $null; action = 'none'; why = '' }) }
    $fakeDoc = [pscustomobject]@{ pid = 4321; owner = 1; started = '2026-09-26 13:00:00'; games = 'ark+r1999';
        updated = '2026-09-26 13:05:00'; stopped = ''; beats = @($fakeBeat); actions = @() }
    $dl = @(Get-GdSupervisorDetailLines -Sv $fakeDoc)
    Check '详情头一句把 PID/路线/上岗/最后一拍念全' (@($dl | Where-Object { $_ -match '4321' -and $_ -match 'ark\+r1999' -and $_ -match '13:00:00' -and $_ -match '13:05:00' }).Count -eq 1) ($dl[0])
    Check '没收工的文档不许出现「收工」二字' (-not (@($dl) -join ' ').Contains('收工'))
    Check '一拍两路 → 展开成两行(谁都没被合并)' (@($dl | Where-Object { $_ -match '^\s+13:05:00 ' }).Count -eq 2)
    Check 'asst_age=-1 说成"读不到"，不是"-1秒"' (@($dl | Where-Object { $_ -match '助手日志静默 读不到' }).Count -eq 1)
    Check '画面没问过说"没问"(空串不许直接印出来)' (@($dl | Where-Object { $_ -match '画面 没问' }).Count -eq 1)
    Check '没动作说 none，有动作把动作名原样念出来' ((@($dl | Where-Object { $_ -match '动作 none' }).Count -eq 1) -and (@($dl | Where-Object { $_ -match '动作 notify' }).Count -eq 1))
    Check 'why 为空的那一路不缀"原因 "(不制造空句子)' (-not (@($dl | Where-Object { $_ -match 'r1999=dead.*原因\s*$' }).Count))
    Check '这一轮没介入过 → 明说没有，而不是留一行空的「介入记录()」' (@($dl | Where-Object { $_ -match '这一轮没有介入过' }).Count -eq 1)
    $docStopped = $fakeDoc; $docStopped.stopped = '2026-09-26 13:20:00'
    $docStopped.actions = @([pscustomobject]@{ ts = '13:06:00'; game = 'ark'; kind = 'restart_assistant'; ok = $false; why = '重启没成' })
    $dl2 = @(Get-GdSupervisorDetailLines -Sv $docStopped)
    Check '收工时间只在真收工时出现' (@($dl2 | Where-Object { $_ -match '收工 2026-09-26 13:20:00' }).Count -eq 1)
    Check '介入记录把次数/成没成都念出来' `
          ((@($dl2 | Where-Object { $_ -match '介入记录\(1 次\)：$' }).Count -eq 1) -and (@($dl2 | Where-Object { $_ -match 'restart_assistant 没成 — 重启没成' }).Count -eq 1)) `
          ($dl2[-2] + ' | ' + $dl2[-1])
    Check '喂 $null(这台机器从没监督过)返回空数组而不是抛' (@(Get-GdSupervisorDetailLines -Sv $null).Count -eq 0)
    # 拍被 30 拍上限挤掉、介入记录还在：这两件事必须能同时说。用 Where 数行，不押第几行(位置会随措辞变)。
    $noBeat = @(Get-GdSupervisorDetailLines -Sv ([pscustomobject]@{ pid = 1; games = 'all'; started = 'x'; updated = ''; beats = @();
        actions = @([pscustomobject]@{ ts = '13:06:00'; game = 'ark'; kind = 'restart_assistant'; ok = $true; why = '已重启' }) }))
    Check '只有现值没有拍时说的是"还没落下第一拍"' (@($noBeat | Where-Object { $_ -match '还没落下第一拍' }).Count -eq 1) ($noBeat -join ' / ')
    Check '没有拍也不许顺手吞掉介入记录(节拍被 30 拍上限挤掉时账本还在)' (@($noBeat | Where-Object { $_ -match 'restart_assistant 成' }).Count -eq 1)
    Check "详情文本自己不带'|'(协议分隔符只由 -DumpSummary 那层转义成 '¦')" ((@($dl + $dl2) -join "`n").IndexOf('|') -lt 0)

    # ---- 纯洁性：监督层也不许认识具体游戏(和 m3 同一把尺) ----
    foreach ($f in @('lib\supervisor.ps1', 'tools\supervise.ps1')) {
        $code = Get-GdCodeOnlyText -Text ([System.IO.File]::ReadAllText((Join-Path $root $f)))
        Check "$f 的代码里没有游戏名/助手名字面量" ($code -notmatch "'ark'" -and $code -notmatch "'r1999'" -and $code -notmatch 'Maa' -and $code -notmatch 'MuMu' -and $code -notmatch '端游')
    }
    # settings 模板覆盖：supervisor 四键在每份 example 里都要有(publish-split 的键覆盖检查兜运行时引用，这里先钉静态)
    foreach ($tpl in @('config\settings.example.json', 'publish\src\ark\config\settings.example.json', 'publish\src\r1999\config\settings.example.json')) {
        $tp = Join-Path $root $tpl
        if (-not (Test-Path -LiteralPath $tp)) { continue }
        $tt = [System.IO.File]::ReadAllText($tp)
        $miss = @('supervisor', 'supervisorIntervalSeconds', 'supervisorQuietMinutes', 'supervisorMaxInterventions') |
                Where-Object { $tt -notmatch ('"' + [regex]::Escape($_) + '"') }
        Check "$tpl 覆盖监督的 4 个 runtime 键" ($miss.Count -eq 0) ($miss -join ' ')
    }
}

Write-Host "`n[m12] 版本号：唯一来源 + 三个读数都接上(2026-09-26 起)"
# 为什么钉这一条：v2026.09.26 那次发布时，版本只活在 git tag 上 —— 代码/exe/面板里都没有读数，
# 玩家报"我这儿不对"的时候第一句"你用的哪版"就答不上来。改成仓库根一个 VERSION 之后，
# 真正的风险变成"有人图省事在第二个地方也写一份" —— 那种错不会当场坏，只会哪天改版忘了它，
# 于是 exe 印一个版本、面板印另一个，两边各自都"绿"，谁也不觉得少了什么。所以下面既钉"来源只有一个"，也钉"三个读数都从它取"。
$verFile = Join-Path $root 'VERSION'
Check '仓库根 VERSION 在(它是唯一的版本来源)' (Test-Path -LiteralPath $verFile)
$verNow = ''
if (Test-Path -LiteralPath $verFile) { $verNow = Get-GdVersion }
Check "VERSION 的形状是 x.y.z(读到 '$verNow')" ($verNow -match '^\d+\.\d+\.\d+$')
Check 'Get-GdVersion 读的就是仓库根那份(不是硬编码回来的)' ((Get-GdVersion -File $verFile) -eq $verNow)
# 坏值一律退成 0.0.0：只读的体检/面板不许因为一个文件写坏就抛(喂临时假文件，不要求机器现场状态)
$verTmp = Join-Path $env:TEMP ('gd-ver-' + $PID)
[void](New-Item -ItemType Directory -Path $verTmp -Force)
try {
    'v1.2.3' | Set-Content -LiteralPath (Join-Path $verTmp 'v.txt') -Encoding ascii
    '1.2'    | Set-Content -LiteralPath (Join-Path $verTmp 'short.txt') -Encoding ascii
    ''       | Set-Content -LiteralPath (Join-Path $verTmp 'empty.txt') -Encoding ascii
    $bad = @((Get-GdVersion -File (Join-Path $verTmp 'v.txt')), (Get-GdVersion -File (Join-Path $verTmp 'short.txt')),
             (Get-GdVersion -File (Join-Path $verTmp 'empty.txt')), (Get-GdVersion -File (Join-Path $verTmp 'nope.txt')))
    Check '坏值(带 v / 段数不对 / 空 / 文件不在)全退成 0.0.0，且不抛' (@($bad | Where-Object { $_ -ne '0.0.0' }).Count -eq 0) ($bad -join ' ')
} finally { Remove-Item -LiteralPath $verTmp -Recurse -Force -ErrorAction SilentlyContinue }
# 第二处写死：拿同一个扫描函数扫工作副本，命中只允许是 VERSION 自己
if ($verNow -and $verNow -ne '0.0.0') {
    $verHits = @(Test-GdExportLeaks -Dir $root -Values @([pscustomobject]@{ Label = '版本号'; Value = $verNow }) |
        Where-Object { $_ -notmatch '^VERSION <-' })
    Check "全仓只有 VERSION 自己写着 '$verNow'(没有第二处写死版本号)" ($verHits.Count -eq 0) (@($verHits) -join ' ｜ ')
}
# 三个读数各自接上来源(源级钉法：改了来源却忘了下游，这里红，而不是等玩家截图里两个版本打架)。
# 注意"这一棵树上没有那个文件"是常态：publish-split 只带脚本与面板源码，tools\build-exe.ps1、
# publish\exe\GDLauncher.cs、tools\publish-split.ps1 三样都不进导出树(打包是本地那侧的事)，
# 所以每一句都先探在不在 —— 不在就 PASS 并把"跳过了"写在明细里，绝不用 ReadAllText 硬读(那会在导出验收链里抛)。
function Read-GdSrcIfPresent {
    param([string]$Rel)
    $p = Join-Path $root $Rel
    if (-not (Test-Path -LiteralPath $p)) { return $null }
    return [System.IO.File]::ReadAllText($p)
}
$stText = Read-GdSrcIfPresent 'status.ps1'
Check 'status.ps1 的「版本」行走 Get-GdVersion' ([string]::IsNullOrEmpty($stText) -or ($stText -match 'Get-GdVersion' -and $stText -match "'版本'")) `
    $(if ($stText) { '' } else { '跳过：这一棵树没有 status.ps1' })
$pData = Read-GdSrcIfPresent 'tools\panel-cs\Panel.Data.cs'
$pMain = Read-GdSrcIfPresent 'tools\panel-cs\Panel.cs'
Check 'C# 面板读仓库根 VERSION 当标题，且自己不含版本号字面量' (
    ([string]::IsNullOrEmpty($pData) -or [string]::IsNullOrEmpty($pMain)) -or
    (($pData -match '"VERSION"') -and ($pMain -match '状态面板 · v" \+ version') -and
     -not ($pData -match 'Version\("\d' -or $pMain -match '· v\d'))) `
    $(if ($pData -and $pMain) { '' } else { '跳过：这一棵树没有 panel-cs 源码' })
$launcher = Read-GdSrcIfPresent 'publish\exe\GDLauncher.cs'
Check 'exe 有 --version，读的是释放出来的 app\VERSION(不是常量)' (
    ([string]::IsNullOrEmpty($launcher)) -or
    (($launcher -match '"--version"') -and ($launcher -match 'Path\.Combine\(appDir, "VERSION"\)'))) `
    $(if ($launcher) { '' } else { '跳过：exe 外壳只在本地那棵树里，不进导出仓库' })
# 顺手钉一件本轮真踩到的事：往 GDLauncher.cs 里插方法时，把下一个方法的签名挤到了注释同一行 ——
# 签名整行被 // 吃掉，csc 从下一行起一路 CS1519，而这份文件只有 build-exe 才会编它(自检套件平时碰不到)。
# 这里只做形状检查(一行里"注释 + 一个方法签名")，不做语义分析，误报空间小。
if ($launcher) {
    $swallowed = @()
    foreach ($ln in ($launcher -split "`n")) {
        $t = $ln.Trim()
        if ($t.StartsWith('//') -and $t -match '\b(private|public|internal|protected)\s+(static\s+)?[\w\[\]<>,\s]+\s+\w+\s*\(') { $swallowed += $t.Substring(0, 40) }
    }
    Check 'GDLauncher.cs 里没有"方法签名被并进注释行"(那只有 build-exe 编得到，自检平时碰不到)' ($swallowed.Count -eq 0) ($swallowed -join ' ｜ ')
}
$beText = Read-GdSrcIfPresent 'tools\build-exe.ps1'
Check 'build-exe 拒绝形状不对的 VERSION，并把它盖进 exe 的文件属性' (
    ([string]::IsNullOrEmpty($beText)) -or
    (($beText -match "'VERSION'") -and ($beText -match 'AssemblyFileVersion') -and ($beText -match '不打包'))) `
    $(if ($beText) { '' } else { '跳过：打包脚本不进导出仓库' })
$psText = Read-GdSrcIfPresent 'tools\publish-split.ps1'
Check 'VERSION 进导出归属表的共用内核(两仓都带)' (
    ([string]::IsNullOrEmpty($psText)) -or ($psText -match "'VERSION',")) `
    $(if ($psText) { '' } else { '跳过：导出脚本本身不进导出仓库' })

$res = Show-GdTestResult -Tag 'modules'
if ($NoExit) { return $res }
exit $(if ($res.Fail) { 1 } else { 0 })
