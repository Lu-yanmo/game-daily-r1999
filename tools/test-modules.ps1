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
# 少了这一步，下面那格"顶层点库后清单声明的 N 个函数全在"会红，而清单里任何一句业务调用(StatusPlan 问
# Get-ArkEvent 那一类)会在夜里报"…不会被识别为 cmdlet" —— 那正是钩子半夜炸的样子。
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

Write-Host "`n[m2] 每份清单的完备性(11-07 SC-C 刀⑰ 笔(c)-2 第 2 批 B2：判定面整段换家 check-module-manifests)"
# 换家史注(按在册「判据不删只换家」＋「改弱了的钉要写下它不再量什么」)：这一段原本 40 行、11 枚逐条 Check，
# 判定面(读清单文本＋跨文件集合比对)自本批起整段住 tools\panel-cs\Chain.cs 的 CheckModuleManifests，
# 门自此只问不算。不再量什么，逐条写清(不是丢了，是换家之后这一侧不再重问)：
#   ① 逐枚格名 ⇒ 换成回执里「规则名｜细节」串的数组：覆盖枚数不降(§11.278 契约格逐格去向)，
#      但红时读的是核点名的规则串，不再是一行一行印出来的 Check 格名；
#   ② 「钩子 $h 是 scriptblock」降档为文本档「值以 { 起」⇒ 自此只量声明形状、不量运行时类型；
#      scriptblock 那半由 [m1] 的 Import-GdModules 全程不抛＋[m4] 的 EnvProbes 真调继续承接；
#   ③ 「[$id] 清单体检没有意见」那枚的运行时面不再由门问：少写必需钩子由 lib\modules.ps1 的
#      Assert-GdModuleManifest 在装载期当场 throw(生产闸)，那是 PS 运行时事实，核不许复制一份；
#   ④ 判据来源从「PS 注册表里的对象」换成「清单文件的文本」⇒ 某份清单若在运行时改写自身字段，
#      门自此看不见(已知不量面)。真树两路是纯文本声明，第二把尺 raw\k80b-cross-check.ps1 逐枚对平
#      (Id 序／LibFile／PS 侧 Assert 同拍零意见四格 True)；
#   ⑤ 入参 requiredPathKeys／requiredHooks 仍由 PS 侧唯一事实源现问(Get-GdRequiredPathKeys 与
#      modules.ps1:24 那枚 $script:GdModuleRequiredHooks)，门不抄第二份名册——抄了就是 §11.186 的病灶。
$b2Body = [pscustomobject]@{
    root             = $root
    entry            = (Join-Path $root 'daily-clear.ps1')
    requiredPathKeys = @(Get-GdRequiredPathKeys $root)
    requiredHooks    = @($script:GdModuleRequiredHooks)
}
$b2 = $null
$b2Err = ''
try { $b2 = Invoke-GdCli -Verb 'check-module-manifests' -Body $b2Body } catch { $b2Err = $_.Exception.Message }
Check '[m2] check-module-manifests 交回读数(核缺席／入闸拒跑都算红，不许静默跳过)' ($b2Err -eq '' -and $null -ne $b2) $b2Err
$b2Mods = @()
if ($null -ne $b2) { $b2Mods = @($b2.Modules) }
$b2RegIds = @(Get-GdModuleIds | ForEach-Object { [string]$_ })
Check ('[m2] 核侧看到的清单数 = 注册表数(零行是"没问"不是"没意见"，故两形都要过)') (($b2Mods.Count -gt 0) -and ($b2Mods.Count -eq $b2RegIds.Count)) ('核 ' + $b2Mods.Count + ' / 注册 ' + $b2RegIds.Count)
$b2Ids = @($b2Mods | ForEach-Object { [string]$_.Id } | Sort-Object)
Check '[m2] Id 名册逐字对平(两侧同集合才算承接，防"核读错树"的假绿)' ((@($b2Ids) -join ',') -eq (@($b2RegIds | Sort-Object) -join ',')) ($b2Ids -join ' / ')
$b2Man = @()
foreach ($r in $b2Mods) { foreach ($q in @($r.Problems)) { $b2Man += ([string]$r.Id + ':' + [string]$q) } }
Check ('[m2] 清单侧零意见(原 11 枚逐条 Check 现由核点名，命中 ' + $b2Man.Count + ' 枚)') (($b2Err -eq '') -and ($b2Man.Count -eq 0)) ($b2Man -join ' ‖ ')

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
    # 刀⑰ 笔(c)-2 第 1 批 B1 换家(§11.276)：老形是 ReadAllText 整本 + 逐枚 -match 手拼，
    # 读文件与正则判定两步的家自此住核内 scan-source-patterns，PS 侧只组参与按 Passed 记名。
    # 语义差逐格登记：老形 -match 拿整本字符串比(等价于跨行扫)、核侧逐行比——靶是库名这种不含换行的
    # 字面量，两形逐格等价；大小写由不敏感收到敏感，库名在清单里恒是小写字面量，现树现读无格翻。
    $m3Self = Join-Path $root "lib\$([string]$m.Id)-module.ps1"
    $other = @(Get-GdModules | Where-Object { [string]$_.Id -ne [string]$m.Id } | ForEach-Object { @($_.Libs) } |
        ForEach-Object { [string]$_ } | Where-Object { $_ -notin $shared })
    $leaked = @()
    foreach ($o3 in $other) {
        $m3Scan = Invoke-GdCli -Verb 'scan-source-patterns' -Body ([pscustomobject]@{
            files = @($m3Self); pattern = [regex]::Escape($o3); expect = 'zero' })
        if (-not [bool]$m3Scan.Passed) { $leaked += $o3 }
    }
    Check "[$([string]$m.Id)] 清单没引用别的模块的业务库" ($leaked.Count -eq 0) ($leaked -join ' / ')
}
# 这份自检查原样进两个导出仓库 → 涉及"两路共用"的断言按注册表里有几款判，绝不写死 ark / r1999
# 2026-11-06 刀⑮ 笔(a)：兜底内核的"在两份清单里"改成名册⇔件在场一把尺现读——同在同不在放行，
# 半迁移(名册还引用而件已退役／件还在而名册不共享)当场红。摘件批不必再回头改这格。
$vrOnDisk = Test-Path -LiteralPath (Join-Path $root 'lib\vision-rescue.ps1')
if (@(Get-GdModules).Count -ge 2) {
    $vrInRoster = (@($shared) -contains 'vision-rescue.ps1')
    Check 'vision-rescue 的名册登记与件在场同一个事实(同在同不在，半迁移红)' ($vrInRoster -eq $vrOnDisk) ("名册=$vrInRoster 件=$vrOnDisk")
}
Check 'Get-GdModuleLibPaths 把共享库去重了(入口点两遍会重复定义一堆函数)' (
    (@(Get-GdModuleLibPaths).Count -eq @(Get-GdModuleLibPaths | ForEach-Object { [System.IO.Path]::GetFileName($_) } | Sort-Object -Unique).Count)) `
    ("$(@(Get-GdModuleLibPaths).Count) 个文件 / $(@($shared).Count) 个共享")
# 后端按命名约定接：内核不许认识任何一路，各路的靶子函数得在自己那一份后端里
# 刀⑮ 笔(a)：这枚无条件读改成两臂——件在按老形咬文本(正锚+负锚都有牙)；件不在不许正锚空咬变恒红或恒绿，
# 改咬"编排命令没在别处复活＋名册已不再登记"，缺读数交不出账就红。
$vrText = if ($vrOnDisk) { [System.IO.File]::ReadAllText((Join-Path $root 'lib\vision-rescue.ps1')) } else { '' }
if ($vrOnDisk) {
    $vrKernelTooth = (Get-GdCodeOnlyText -Text $vrText) -notmatch 'New-GdVisionTarget-ark|New-GdVisionTarget-mumu' -and
        $vrText -match 'New-GdVisionTarget-\$\('
} else {
    $vrKernelTooth = (-not (Get-Command Invoke-GdVisionRescue -ErrorAction SilentlyContinue)) -and
        (-not (@($shared) -contains 'vision-rescue.ps1'))
}
Check '兜底内核不认识端游也不认识模拟器(只按 New-GdVisionTarget-<kind> 找；件不在改咬没复活)' $vrKernelTooth "件=$vrOnDisk"
# 自检脚手架点库的名单是另一份硬编码(tools\test-lib.ps1 第 10 行)：清单加了库而它没跟上，
# test-ark / test-r1999 里那些 (Get-Command X).Definition 断言会整段跳过 —— 静默少一层保护，最难发现的那种。
# 只管兜底这三份(test-lib 的名单本来就不含 ark-daily/r1999-daily 这类编排库，别把不该管的也钉上)。
$tlText = [System.IO.File]::ReadAllText((Join-Path $root 'tools\test-lib.ps1'))
# 2026-11-06 刀⑮ 笔(d)：这一段整族量的是"PS 兜底内核与各路 PS 后端的连接约定"，三件本体出仓后它没有对面了——不许让它靠 continue 静默停跑(在册那条"山搬了家而尺跟着埋")，改两臂：件在按老形逐路对账；件不在咬"名册已摘净＋自检脚手架也不点那三份"，两头都交得出数。
if ($vrOnDisk) {
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
} else {
    Check '兜底 PS 后端名册已摘净：没有任何一路的 Libs 再声明 vision-*.ps1(件不在而名册还写着＝半迁移红)' (
        @(Get-GdModules | ForEach-Object { @($_.Libs) } | Where-Object { [string]$_ -like 'vision-*.ps1' }).Count -eq 0)
    # 11-06 刀⑮ 笔(d)-2 复跑红一次照登：上面那格首拍咬的是 $tlText 全文，而 tools\test-lib.ps1:21 那句史注
    # 逐字点名那三份——注释不是引用(在册纪律；同族见 test-r1999 的 Get-GdTestCodeOnly)。代码面那行名单本来就只剩
    # @('decisions', 'maa', 'm9a', 'mumu') 四枚，判据错在取件轴不在被量的件。处方＝先剥注释再判，史注留着不删。
    $tlCode203 = Get-GdCodeOnlyText -Text $tlText
    Check '自检脚手架的代码面名单也不点那三份(注释剥后再判；名单与清单一起摘才算闭合：只删清单＝少装载一路，只删名单＝多一个恒不命中的名字)' (
        $tlCode203 -notmatch "'vision-rescue'" -and $tlCode203 -notmatch "'vision-ark'" -and $tlCode203 -notmatch "'vision-mumu'")
}

# 刀⑳ 笔(a)-3 清单完备性对面格：test-lib 头注那句「名单由 $GdTestDotLibs 交给 [m3] 逐条对账，漏一项当场变红」
# 到今天才有尺量它。老形 [m3] 只钉了反方向(清单不许引用别人的库)＋vision 三枚的负锚，正方向(名单 ⇔ 两份清单
# Libs 并集 ∪ 共用底座 decisions)没有任何尺量过，后果当场可见＝名单里那枚 'mumu'(lib\mumu.ps1 已随刀⑯ 笔(d)-2
# 整件出仓)活到今天：Test-Path 守卫恒不命中⇒既不红也不供数。处方＝剥注释取代码面名单，与现读并集做双向差集。
# 尺不挂在 $vrOnDisk 那条会消失的分路上(在册「山搬了家而尺跟着埋」)；按注册表里有几款分两臂，绝不写死 ark/r1999。
$tlCode303 = Get-GdCodeOnlyText -Text $tlText
$tlList303 = @()
if ($tlCode303 -match '(?m)^\$script:GdTestDotLibs\s*=\s*@\(([^)]*)\)') {
    $tlList303 = @([regex]::Matches($Matches[1], "'([^']+)'") | ForEach-Object { $_.Groups[1].Value })
}
$tlUnion303 = @(Get-GdModules | ForEach-Object { @($_.Libs) } |
    ForEach-Object { [System.IO.Path]::GetFileNameWithoutExtension([string]$_) })
$tlExpect303 = @((@($tlUnion303) + @('decisions')) | Sort-Object -Unique)
$tlExtra303 = @($tlList303 | Where-Object { $tlExpect303 -notcontains $_ })
$tlMissing303 = @($tlExpect303 | Where-Object { $tlList303 -notcontains $_ })
$tlReadout303 = ("名单=[$($tlList303 -join ' ')] 并集加底座=[$($tlExpect303 -join ' ')] 多=[$($tlExtra303 -join ' ')] 少=[$($tlMissing303 -join ' ')]")
if (@(Get-GdModules).Count -ge 2) {
    Check '自检脚手架名单与两份清单 Libs 并集闭合(双向差集：多一名＝恒不命中的死名，少一名＝那一路自检整段静默跳过)' (
        @($tlExtra303).Count -eq 0 -and @($tlMissing303).Count -eq 0 -and @($tlList303).Count -ge 1) $tlReadout303
} else {
    Check '本路清单声明的库都在自检脚手架名单里(单游戏导出仓只量这一路，另一路的库不在这棵树上)' (
        @($tlMissing303).Count -eq 0 -and @($tlList303).Count -ge 1) $tlReadout303
}

Write-Host "`n[m4] 还活着的钩子真调得动(EnvProbes)＋面板取段的形状耦合格"
foreach ($m in @(Get-GdModules -AvailableOnly)) {
    $id = [string]$m.Id
    $rows = $null
    $err = ''
    # 11-03 转接刀：这几格量的一直是 EnvProbes 钩子自己的返回形状，那枚薄壳只转发明再加一条 null→空表；
    # 壳已摘 ⇒ 这里直接点钩子，判语一字不动（生产侧没有 caller，null 那一档只在壳里，摘了就是少一档兜底）。
    # ⚠ 刀⑤(2026-11-04)：上一句里那枚「薄壳」= Invoke-GdModuleHook，它的本体也随本批出仓 ⇒ 这里逐字直点钩子。
    try { $rows = & $m.Hooks.EnvProbes $S } catch { $err = $_.Exception.Message }
    Check "[$id] EnvProbes 调得动" ($err -eq '') $err
    Check "[$id] 环境探测至少一项" (@($rows).Count -gt 0) "$(@($rows).Count) 项"
    $labels = @($rows.Keys)
    Check "[$id] 探测项标签非空" (@($labels | Where-Object { -not [string]$_ }).Count -eq 0) ($labels -join ' / ')
    Check "[$id] 探测结果是有序表(体检里那几行的先后不能变)" ($rows -is [System.Collections.Specialized.OrderedDictionary])
    Check "[$id] 探测项的值都落在 paths.* 上(框架不猜路径)" (@($labels).Count -eq @($rows.Values | Where-Object { $_ -is [string] -or $null -eq $_ }).Count)
}
# 11-04 刀⑤ 作废登记：这一段原来还点了 Resolve(带 -Force，前后各存/还原一次 state)与 Preview 两发真调。
# 对面不在本文件也不在本棵树：判定面＝tools\rustcore\src\main.rs 的 round-resolve 两枚直答臂(§11.225 刀③)，
# 预览面＝同一文件的 round-preview 两枚直答臂(§11.222 刀②)，语义钉由 gdcore --selftest 的 S28 判定族消费证据格
# 与 preview-crash 注入缝供数。PS 那两枚钩子块体随本批出仓 ⇒ 这两格作废，且不是"改弱"：
# 把真调换成"钩子格还在"的形钉等于给壳续命(§11.186 那句"补齐＝给病因续命"的同一课)，所以直接删。
#
# 下面这格是"缝跟着答方走"的兑现，也是 EnvProbes 仍留在 modules.ps1:24 那本名册里的唯一理由：
# 它的对面不是函数调用而是**源文本解析**——面板按一行正则去清单里抠那段 '标签' = 表达式。
# 钉法只钉形状不钉结果(读 Panel.Summary.cs，问它认段的字面量还在不在)：
#   本格红 ＝ 面板换了取段方式或整件退役 ⇒ 那一刻 EnvProbes 才真的没有对面，名册最后一枚自由了；
#   反过来若名册先被删而本格没红 ⇒ 装载期不会炸、面板「环境」段却少几行且谁都不报错——那才是它要拦的那一档。
$panelSrc = ''
try { $panelSrc = [System.IO.File]::ReadAllText((Join-Path $root 'tools\panel-cs\Panel.Summary.cs')) } catch { }
Check '面板仍按「^\s*EnvProbes\s*=\s*\{\s*$」在清单里取环境探测段(EnvProbes 唯一活着的对面)' (
    $panelSrc.Contains('EnvProbes\s*=\s*\{\s*$')) $(if ($panelSrc.Length -eq 0) { 'tools\panel-cs\Panel.Summary.cs 不在场(尺没接到电)' } else { '那句字面量变了' })

Write-Host "`n[m5] 选择与参数名"
# 11-03 转接刀：这一段原本量的是 Select-GdModules 那枚 PS 真身(不传 Id ／ -Ids all ／ 只挑一款 ／
# 点了没注册的那一路当场抛 ／ 抛出来那句把树上的名册一起印出来)。真身已摘 ⇒ 这几格作废，且自此 PS 侧
# 不再量「命令行 ids 变成待跑那几路」这一格 —— 对面在 tools\rustcore\src\main.rs 的 select_gd_modules，
# 语义钉由它 --selftest 的 24e 段供数(三档在位树现问＋all／ALL／空串／没传 ids 同一读数)。
# 空壳留着只会让下一批把「门绿着」读成「还在量这一格」。
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

Write-Host "`n[m6] 入口动词与真参数对得上(菜单/面板印的开关名不能是编的；11-07 B2 换家)"
# 换家史注：这一段原本 44 行(现读 daily-clear.ps1 的参数表 + 逐枚开关名正查 + ark×Ack 反向负锚)，
# 判定面自本批起住核内(Chain.cs 入口侧：读 param 块 → 逐枚 FlagFor 对照 → 负锚)。不再量什么：
#   ① 逐枚"[$id] $verb 的开关名是入口真认识的参数"的格名 ⇒ 换成 EntryProblems 里的
#      verb-flag-absent｜id.Verb｜-Flag 串；正查逐枚照做，只是点名由核来；
#   ② "参数表读得到"从「Get-Command 真绑定」降为文本档「读得到行首 param 块且括号配平」⇒
#      自此不再量参数绑定后的规范化名；两形都照样红(entry-param-block／entry-param-table-empty)；
#   ③ ark×Ack 那枚反向负锚的事实源从本文件旧 :306 的硬编码判语进核(AckExitedIds)：
#      门侧自此不持有任何一款的名字，ark 之外那一路的 Ack 正查照旧(核内逐路 FlagFor)；
#   ④ Get-GdEntryVerb 现问注册表那半仍留在 PS 侧(面板/菜单印开关名用的是它的真身)，
#      本段不再拿它当判据；「注册表还能答」由下面 Test-GdMergedEntry 那一格守住(未换家、未摘)。
# 现问现答，不复用 [m2] 那次的读数：中间还隔着 m3~m5，不靠变量存活做判据。
$b6 = $null
$b6Err = ''
try { $b6 = Invoke-GdCli -Verb 'check-module-manifests' -Body $b2Body } catch { $b6Err = $_.Exception.Message }
$b6Entry = @()
if ($null -ne $b6) { $b6Entry = @($b6.EntryProblems) }
Check ('[m6] 入口侧零意见(参数表读得到／逐枚开关名在参数表里／ark×Ack 反向负锚，命中 ' + $b6Entry.Count + ' 枚)') (($b6Err -eq '') -and ($b6Entry.Count -eq 0)) ($b6Entry -join ' ‖ ')
Check '合体判断走注册表(Test-GdMergedEntry 返回布尔)' ($null -ne (Test-GdMergedEntry)) "merged=$(Test-GdMergedEntry)"

Write-Host "`n[m7] 注册表被清空时的行为(工程裁坏了要说人话，别少印四段)"
$snap = [ordered]@{}
foreach ($k in @($global:GdModules.Keys)) { $snap[$k] = $global:GdModules[$k] }
$global:GdModules.Clear()
Check '清空后 Get-GdModules 返回空(不抛)' (@(Get-GdModules).Count -eq 0)
# 11-03 转接刀：这一格原本拿清空后的注册表去点 Select-GdModules 的那句 throw，真身已摘 ⇒ 作废；
# 「裁坏了要说人话」那一档从此由核内的在位树现问供数(--selftest 24e 的 root-none 那档交回同一句)。
# 这一段剩下的还是 PS 侧自己的事：注册表清空不炸、判定层的两款兜底、还原后数量对平。
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
# 2026-11-06 刀⑮ 笔(d)：Invoke-GdVisionRaw 的真身随 lib\vision-rescue.ps1 出仓。它管的那四件事(起的是 python + ask.py
# 而不是自己发 HTTP／key 只进子进程的环境变量／key 不进命令行／提示词走请求文件)判据一个字没松，家换成核内
# Live.cs::Live.VisionAsk(表账那行第四列本就写着那一枚，本刀只是把 PS 那一份取走)。形制按在册两臂：件在按老形咬
# Definition 文本；件不在不许正锚空咬变恒红，改咬"PS 那枚没在别处复活＋那四条形制在核侧文本里在场"，缺读数交不出账就红。
$visionRescueOnDisk = Test-Path -LiteralPath (Join-Path $root 'lib\vision-rescue.ps1')
$rawDef = ''
if (Get-Command Invoke-GdVisionRaw -ErrorAction SilentlyContinue) { $rawDef = (Get-Command Invoke-GdVisionRaw).Definition }
Check '兜底内核件与 Invoke-GdVisionRaw 命令同一个事实(同在同不在，半迁移红)' (
    $visionRescueOnDisk -eq [bool](Get-Command Invoke-GdVisionRaw -ErrorAction SilentlyContinue)) "件=$visionRescueOnDisk"
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
} else {
    Check '问模型这一层在 PS 侧不再有第二份实现(Invoke-GdVisionRaw 没在别处复活)' (
        -not (Get-Command Invoke-GdVisionRaw -ErrorAction SilentlyContinue))
$liveAsk = ''
    try { $liveAsk = [System.IO.File]::ReadAllText((Join-Path $root 'tools\panel-cs\Live.cs')) } catch { }
    Check '那四条形制住在核侧(起 python + ask.py／key 只进环境变量／提示词走 --request／命令行上没有 key 的口子)' (
        $liveAsk.Length -gt 0 -and $liveAsk.Contains('ask.py') -and $liveAsk.Contains('DASHSCOPE_API_KEY') -and
        $liveAsk.Contains('--request') -and -not $liveAsk.Contains('--api-key')) ('Live.cs=' + $liveAsk.Length + ' 字符')
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
    Check "[$id] VisionKind 是小写短名(它是喂给核内 resolve-rescue-context 的那一枚 target，大小写或空格都会让认不到)" (
        $kind -match '^[a-z][a-z0-9]*$') $kind
    # 2026-11-06 刀⑮ 笔(d)：下面三格原来咬"PS 后端文件在不在／写没写进 Libs／靶子函数点没点到"，那三件事随三件出仓。
    # 件在按老形跑；件不在改咬"那一路的 PS 后端不许留在树上、靶子命令不许复活"——VisionKind 这一格本身两臂都跑，
    # 因为它量的是清单上的业务知识，不是 PS 件。
    if ($vrOnDisk) {
        # 后端文件名与 kind 绑死：kind 写 mumu 就得有 lib\vision-mumu.ps1；
        # 后半句更要紧 —— 文件在但没写进这一路的 Libs，等于压根没人点它，还是静默不生效。
        $beFile = Join-Path $root "lib\vision-$kind.ps1"
        Check "[$id] 后端文件按约定存在(lib\vision-$kind.ps1)" (Test-Path -LiteralPath $beFile)
        Check "[$id] 后端文件写进了这一路的 Libs" (@($m.Libs) -contains "vision-$kind.ps1") (@($m.Libs) -join ' ')
        $builder = "New-GdVisionTarget-$kind"
        Check "[$id] 顶层点库之后 $builder 真在(命名约定对得上)" ([bool](Get-Command $builder -ErrorAction SilentlyContinue))
    } else {
        Check "[$id] 这一路的 PS 后端已随三件出仓(lib\vision-$kind.ps1 不许留在树上)" (
            -not (Test-Path -LiteralPath (Join-Path $root "lib\vision-$kind.ps1")))
        Check "[$id] 靶子命令没在现树复活(New-GdVisionTarget-$kind 由核内 resolve-rescue-context 顶替)" (
            -not (Get-Command "New-GdVisionTarget-$kind" -ErrorAction SilentlyContinue))
    }
}
# VisionKind 摘了后端不等于摘了消费者：入口那一路仍只问清单(在册那条"业务知识只存一份"的判据换了宿主，没换出处)。
$askSrcT = ''
try { $askSrcT = [System.IO.File]::ReadAllText((Join-Path $root 'tools\vision-ask.ps1')) } catch { }
Check 'VisionKind 仍被入口消费(tools\vision-ask.ps1 只问清单，不自己记 kind 表；读不到那一份就红)' (
    $askSrcT.Length -gt 0 -and $askSrcT.Contains('VisionKind')) ('vision-ask.ps1=' + $askSrcT.Length + ' 字符')
# 框架与工具层不许硬写某一路的 kind：内核只按命名约定拼函数名，外壳只问清单。
# 判的是"把 kind 当字面量用"的那几种写法(-Target/-Kind/直接调 New-GdVisionTarget-xxx)，
# 不是"文件里出现过 mumu 这四个字母"—— common.ps1 催装对里 Lib = 'm9a.ps1' 那种是库名，另一回事
# (刀⑯ 起 lib\mumu.ps1 整件出仓，举例换现存的库名)。
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

Write-Host "`n[sv] 监督 agent(11-02 刀③ 后只剩轮档静态锚，PS 两件已出仓)"
# 这一段现在只剩源级静态锚(量的是 lib\rust-round.ps1 的接线形状)：不喂字典、不起进程、不碰现场状态。
# 原来那批行为级断言(三段纯函数喂字典判决策表／临时文件真演采集／临时目录落记录再归还 $global:GD)的去向见下面那笔账。
# ---- 刀③ 摘件笔(11-02)的账：这一段原来还有 $supLib／$supLoop 两枚在场格，以及它们门内那批行为级断言 ----
# 在场格先让门红一次(实跑交回「202 通过 / 2 失败」，红的恰是这两格)再按门落账，不是改完就宣称绿。
# 门内那批随墙一起拆，判语分三类登记，免得日后当成"静默消失"：
#   ①配置语义/状态表/动作表/详情文本(共约 40 格)——对面早已搬到 tools\panel-cs\Supervisor.cs，期望串逐字同源，
#     行为级断言的家换成 panel-cs.exe --svtest(刀② 11-02 已把采集/记录两族 12 格接进去，现量 85 格)。
#   ②「入口点 lib\supervisor.ps1」那格——它钉的是 dot-source 接线，入口那行已改成一句注释(不装载)，靶子本身出局。
#   ③纯洁性那两格(监督两件代码面无游戏名)——PS 层没了，尺改钉 Supervisor.cs 自己，落在 --svtest(本格批新增)。
# 为什么这段注释留在 [sv] 段头而不另起一节：下面的静态锚仍活在这一棵树上，日后有人想给 PS 侧"补回"监督断言，
# 先在这里撞见判语，比在 git 里考古便宜。

# ---- 监督族刀①(11-02 接线)静态锚 → 刀⑩(11-05 进核)按新轴翻形：轮档件不再答监督，钉的是"确实不再答" ----
# 为什么这几格不受 [sv] 段那批行为级断言出仓的影响(刀③ 已把门内整段摘净)：它们钉的是 lib\rust-round.ps1 的形状，而那枚
# 文件不属监督族、那两件 PS 件出仓后它照旧在场。当年把钉落在那道 Test-Path 门**之外**，为的就是"摘件那一笔不许把接线的
# 证据一起带走"；门本身出局后这层理由原样成立 —— "把钉做在会被拆的那面墙上"这一课不许反过来教(不该因此把锚也拆了)。
# 为什么用整本读源码而不是 Get-Command .Definition：自检套件没点 lib\rust-round.ps1(test-lib.ps1 只点判定层与
# 助手库)，函数体在这一棵树上拿不到。剥注释两形逐字复用 tools\test-ark.ps1 那批锚 —— 本刀的注释面**就写了**
# start-supervisor／Stop-GdSupervisor 这些词(那是语义边界登记)，不剥注释的正向锚会在臂被摘掉后照样绿。
# 刀③ 改口两处(旧句活的只有判据，指的东西换了)：①"两臂都在"这句作废 —— PS 兜底真身退役后只剩一枚臂，
#   ①那两格量的是"这一枚接上了"；②原来那两格"回落臂在(逐字＝接线前原样)"翻成负锚，它们不再量兜底路是否完好，
#   改量兜底路是否真的没了(留着一枚调不到人的调用点，比没有更坏：它把"核不可达"这一档交给不在场的命令去答)。
# 刀⑩ 再改口(2026-11-05 监督两臂进核)：那"只剩一枚臂"也没了——ask 名册里 round-supervisor-start／-stop 两枚
#   自此不出场，所以"①臂在"那两格**当场没有对面**，按在册规矩不许静默删，逐格写下它们不再量什么并翻成新轴：
#   正向锚不留旧形(旧形量的是"臂接上了"，如今那件事的家在核内)，改钉"码面确实不再答这一族"＋"段 8 触发点换了轴
#   且两侧同源"。核内那两臂真办过事的证据不在这里(归 --selftest §28 的 R-sup-direct)，本段只钉 PS 侧不再有人答。
$rrSrc = ''
try { $rrSrc = [System.IO.File]::ReadAllText((Join-Path $root 'lib\rust-round.ps1')) } catch { }
$rrSrc = ($rrSrc -replace '(?s)<\#.*?\#>', ' ') -replace '(?m)^\s*#.*$', ' '
$coreRsForSv = ''
try { $coreRsForSv = [System.IO.File]::ReadAllText((Join-Path $root 'tools\rustcore\src\main.rs')) } catch { }
Check '⑩臂出册(start)：轮档码面不再问 start-supervisor(监督那一档的家已进核，留臂就是死臂)' (
    -not $rrSrc.Contains("Invoke-GdCli -Verb 'start-supervisor'")) $(
    if ($rrSrc.Length -eq 0) { 'lib\rust-round.ps1 不在场(尺没接到电)' } else { "len=$($rrSrc.Length)" })
Check '⑩臂出册(stop)：轮档码面不再问 stop-supervisor(同上)' (
    -not $rrSrc.Contains("Invoke-GdCli -Verb 'stop-supervisor'"))
Check '⑩名册只剩提权那一枚：外发名册里不再有 round-supervisor-* 的动作字面(一次删两侧，名册与臂同批)' (
    -not $rrSrc.Contains('round-supervisor-start') -and -not $rrSrc.Contains('round-supervisor-stop'))
Check '③回落臂摘净(start)：轮档码面不再有 Start-GdSupervisor 那枚 PS 真身(原"回落臂在"翻负，它不再量兜底路完好)' (
    -not $rrSrc.Contains('Start-GdSupervisor'))
Check '③回落臂摘净(stop)：轮档码面不再有 Stop-GdSupervisor 那枚 PS 真身(同上)' (
    -not $rrSrc.Contains('Stop-GdSupervisor'))
Check '⑩回执键形随臂出册：Available/Err 那枚拼串在轮档码面归零(键形的事实源改由核内 §28 R-sup-direct 钉)' (
    ([regex]::Matches($rrSrc, 'Available = \[bool\]\$core\.Available; Err = \[string\]\$core\.Err')).Count -eq 0) $(
    'hits=' + ([regex]::Matches($rrSrc, 'Available = \[bool\]\$core\.Available')).Count)
Check '⑩语义边界换了家：轮档码面不再喂 runtime 四键的 body，整枚 settings 更不许往下传(四键的现读归 C# 侧)' (
    -not $rrSrc.Contains('settings = [pscustomobject]@{ runtime = [pscustomobject]@{') -and
    -not [regex]::IsMatch($rrSrc, 'settings\s*=\s*\$S\b'))
Check '⑩顺序锚换轴：段 8 睡眠屏蔽改由侧信道回调补位，且仍"只设一次＋按 $esHeld 撤位"(旧 ask 臂末尾那形不许复活)' (
    $rrSrc.Contains('$esBox.Value = Start-GdRoundHostNoSleep -S $S') -and
    $rrSrc.Contains('Stop-GdRoundHostNoSleep -WasSet $esHeld') -and
    -not $rrSrc.Contains('if (-not $esHeld) {'))
Check '⑩触发串两侧同源：轮档件那根针与核内段 8 那句人话是同一段字面(改一侧的措辞当场红，屏蔽不许静默失效)' (
    $rrSrc.Contains(".Contains('已临时禁止系统睡眠')") -and $coreRsForSv.Contains('"已临时禁止系统睡眠')) $(
    'rr=' + $rrSrc.Contains(".Contains('已临时禁止系统睡眠')") + ' core=' + $coreRsForSv.Contains('"已临时禁止系统睡眠'))

# settings 模板覆盖：supervisor 四键在每份 example 里都要有(publish-split 的键覆盖检查兜运行时引用，这里先钉静态)
# 为什么这几格一直在那道 Test-Path 门之外(11-02 刀②，那道门刀③ 已连墙拆掉)：这三格量的是三本 settings.example.json，
# 跟 lib\supervisor.ps1／tools\supervise.ps1 在不在场没有半点关系；写在门内，摘件那一笔会把「配置面还覆盖着监督四键」
# 这条证据一起带走 —— 与监督族刀① 把轮档锚落在门外的理由同一条(钉不许做在会被拆掉的那面墙上)，如今这条已兑现。
foreach ($tpl in @('config\settings.example.json', 'publish\src\ark\config\settings.example.json', 'publish\src\r1999\config\settings.example.json')) {
    $tp = Join-Path $root $tpl
    if (-not (Test-Path -LiteralPath $tp)) { continue }
    $tt = [System.IO.File]::ReadAllText($tp)
    $miss = @('supervisor', 'supervisorIntervalSeconds', 'supervisorQuietMinutes', 'supervisorMaxInterventions') |
            Where-Object { $tt -notmatch ('"' + [regex]::Escape($_) + '"') }
    Check "$tpl 覆盖监督的 4 个 runtime 键" ($miss.Count -eq 0) ($miss -join ' ')
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
