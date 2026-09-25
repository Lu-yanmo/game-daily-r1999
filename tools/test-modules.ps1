# tools/test-modules.ps1 — 框架层自检：模块清单齐不齐、注册表对不对、框架有没有掺进游戏知识
# 用法: pwsh -NoProfile -File tools\test-modules.ps1        (powershell.exe 5.1 同样能跑)
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
    foreach ($h in @('StatusRows', 'StatusVersion', 'StatusProcs', 'StatusProcExtra', 'StatusPlan', 'StatusSection')) {
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
# 反过来：清单里也不该出现另一款游戏的库(导出时会把不存在的路径点进来)
foreach ($m in @(Get-GdModules)) {
    $txt = [System.IO.File]::ReadAllText((Join-Path $root "lib\$([string]$m.Id)-module.ps1"))
    $other = @(Get-GdModules | Where-Object { [string]$_.Id -ne [string]$m.Id } | ForEach-Object { @($_.Libs) })
    $leaked = @($other | Where-Object { $txt -match [regex]::Escape($_) })
    Check "[$([string]$m.Id)] 清单没引用别的模块的业务库" ($leaked.Count -eq 0) ($leaked -join ' / ')
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
Sync-GdEntryVerbs
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
    foreach ($verb in @('Ack', 'ShopDone', 'ShopReset')) {
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

$res = Show-GdTestResult -Tag 'modules'
if ($NoExit) { return $res }
exit $(if ($res.Fail) { 1 } else { 0 })
