# lib/modules.ps1 — 框架层：游戏模块的注册表、装载器，和"跑一轮"的公共流程
#
# 为什么要这一层：在它有之前，"今天跑哪几路、每路要探哪几条路径、要不要管理员、跑完调谁收尾、
# 汇总里先写谁"这些问题，答案是写死在 daily-clear.ps1 的 ValidateSet / foreach (@('ark','r1999')) /
# Test-Path lib\maa.ps1 里的，同一套流程还被合体入口和两份单游戏薄壳各抄一遍。
# 现在：模块只声明"我叫什么、要哪几条路径、每一步调哪个函数"，框架只认这份声明。
# 加一款游戏 = 往 lib\ 里放一份 <id>-module.ps1，入口和面板都不用再改。
#
# 装载方式沿用本仓库一贯的口径 —— 按文件在不在决定有没有那一路，不猜配置：
# Import-GdModules 扫 lib\*-module.ps1，按文件名排序逐个点进来，各自 Register-GdModule。
# 单游戏导出仓库里只有那一份模块文件，于是注册表天然只有一款，入口一个字都不用改。
#
# 依赖：本文件只用 lib\common.ps1 里的零件(Write-GdLog / Get-GdSettings / Add-GdHistory / Send-GdNotice …)，
# 所以调用方必须已经点过 common.ps1；反过来本文件不点任何游戏专有库(那正是模块清单的事)。

# 注册表：[id] -> 清单对象。用全局是因为这些库都是 dot-source 进入口脚本作用域的，
# 各入口自己那份 $script: 变量互相看不见；和 $global:GD / $global:GdStageTable 同一个路子。
if (-not (Get-Variable -Name GdModules -Scope Global -ErrorAction SilentlyContinue)) {
    $global:GdModules = [ordered]@{}
}

# 一个模块清单必须齐这些东西才算能用。少哪一项就报哪一项 —— 将来加模块的人只会看这一处。
$script:GdModuleRequiredKeys = @('Id', 'Name', 'ShortName', 'AssistantLib', 'OrchestrationLib', 'RequiresAdmin', 'ElevationFlag', 'AdminWhy', 'AdminHint', 'AdminOkNote', 'SettingsBlock', 'PathKeys', 'EntryVerbs', 'Libs', 'Hooks')
$script:GdModuleRequiredHooks = @('EnvProbes', 'Resolve', 'Preview', 'Run', 'Stop', 'Ack', 'ShopState')

function Register-GdModule {
    <# 模块清单入库。同一棵树里同一个 Id 注册两次 = 有两份 <id>-module.ps1(或者入口多点了一次)，
        直接把后一份丢掉并说清楚：静默覆盖会让"我改的到底是哪一份"变成排查噩梦。 #>
    param([Parameter(Mandatory)]$Manifest)
    $id = [string]$Manifest.Id
    if (-not $id) { throw 'Register-GdModule：清单里没有 Id' }
    if ($global:GdModules.Contains($id)) {
        Write-GdLog "模块 $id 已经注册过(注册表不动，本次忽略)—— 检查 lib\ 下是不是有两份 $id-module.ps1" -Level WARN
        return $null
    }
    $global:GdModules[$id] = $Manifest
    return $Manifest
}

function Get-GdModules {
    <# 注册表里的模块，按注册顺序(= 文件名顺序)。-AvailableOnly 只要这棵树上真的装了的那几路：
        判据是"那一路的助手库在不在"，和 Get-GdRequiredPathKeys / status.ps1 用的是同一个口径。 #>
    param([switch]$AvailableOnly)
    $all = @($global:GdModules.Values)
    if (-not $AvailableOnly) { return $all }
    $lib = Join-Path (Get-GdRoot) 'lib'
    return @($all | Where-Object { Test-Path -LiteralPath (Join-Path $lib ([string]$_.AssistantLib)) })
}

function Get-GdModule {
    param([Parameter(Mandatory)][string]$Id)
    if (-not $global:GdModules.Contains($Id)) { return $null }
    return $global:GdModules[$Id]
}

function Get-GdModuleIds {
    param([switch]$AvailableOnly)
    return @((Get-GdModules -AvailableOnly:$AvailableOnly) | ForEach-Object { [string]$_.Id })
}

function Test-GdModuleInstalled {
    <# 这一路"装没装在这棵树上"。-Key 选判据：
        AssistantLib(默认) = 助手库在不在 → "这一路能不能跑"
        OrchestrationLib   = 编排文件在不在 → "有没有入口把这一路当一款游戏来管"
        注意：合体/单游戏的判据函数 Test-GdMergedEntry 住在 lib\decisions-common.ps1(老消费方都在那里用)，
        本文件不重复定义同名函数 —— 两份库先后 dot-source 时"后点的覆盖先点的"是顺序依赖坑，别踩。 #>
    param([Parameter(Mandatory)]$Module, [ValidateSet('AssistantLib', 'OrchestrationLib')][string]$Key = 'AssistantLib')
    $f = [string]$Module.$Key
    if (-not $f) { return $false }
    return (Test-Path -LiteralPath (Join-Path (Get-GdRoot) "lib\$f"))
}

function Assert-GdModuleManifest {
    <# 清单体检：字段齐不齐、钩子是不是真调得动、声明的 PathKeys 跟 Get-GdRequiredPathKeys 认不认得对。
        这是"新增/改动模块清单"唯一的自动防线 —— 少写一个钩子在 dot-source 阶段是什么错都不会报的，
        要等到夜里计划任务跑到那一步才炸，正是本仓库最不该犯的那类错。 #>
    param([Parameter(Mandatory)]$Module)
    $bad = @()
    foreach ($k in $script:GdModuleRequiredKeys) {
        $v = $Module.$k
        if ($null -eq $v) { $bad += "缺字段 $k" }
    }
    $hooks = $Module.Hooks
    if ($null -eq $hooks) {
        $bad += '缺 Hooks 表'
    }
    else {
        foreach ($h in $script:GdModuleRequiredHooks) {
            $sb = $hooks.$h
            if ($null -eq $sb) { $bad += "缺钩子 $h" }
            elseif ($sb -isnot [scriptblock]) { $bad += "钩子 $h 不是 scriptblock(是 $($sb.GetType().Name))" }
        }
    }
    # 每一项 PathKeys 都得是 paths.<键>，且这一路真该催它(Get-GdRequiredPathKeys 只看助手库)
    foreach ($k in @($Module.PathKeys)) {
        if (-not ([string]$k)) { $bad += 'PathKeys 里有空项'; continue }
        if ((@(Get-GdRequiredPathKeys) -notcontains [string]$k)) { $bad += "PathKeys 里的 $k 不在 Get-GdRequiredPathKeys 的名单里" }
    }
    # 要管理员却没写为什么 → 提权那句会印成"方舟 → 以管理员身份重启本轮"，看着像漏了字
    if ($Module.RequiresAdmin -and -not [string]$Module.AdminWhy) { $bad += 'RequiresAdmin=true 却没写 AdminWhy' }
    return @($bad)
}

function Import-GdModules {
    <# 装载 lib\*-module.ps1。返回装载到的模块 id 列表。
        本函数只做"注册清单 + 体检"这一件事，不点各模块的业务库 ——
        在函数里 dot-source 一个文件，它定义的函数会随这个函数的作用域一起消失，
        钩子稍后就再也找不到它们(清单里存的是 scriptblock，对象能活，函数定义不能活)。
        所以点库这件事交给入口在顶层照 Get-GdModuleLibPaths 点菜，见 daily-clear.ps1 开头那三行。
        已经注册过的不重复注册(Register-GdModule 会 WARN)，所以入口可以被多次点进来。 #>
    param([switch]$Quiet, [switch]$Tolerant)
    $lib = Join-Path (Get-GdRoot) 'lib'
    $files = @(Get-ChildItem -LiteralPath $lib -Filter '*-module.ps1' -ErrorAction SilentlyContinue | Sort-Object Name)
    # Out-Null 不是摆设：清单文件里那句 Register-GdModule(...) 会把清单对象吐到输出流，
    # 在函数里 dot 就会顺着 Import-GdModules 的返回值漏出去 —— 调用方拿到的"模块 id 列表"
    # 前面就多出两个长得很像 id 的对象(test-modules 的 [m1] 第一条就是这么发现的)。
    foreach ($f in $files) { . $f.FullName | Out-Null }
    $bad = @()
    foreach ($m in @(Get-GdModules)) {
        foreach ($b in @(Assert-GdModuleManifest -Module $m)) { $bad += "$([string]$m.Id)：$b" }
    }
    if ($bad.Count) {
        $msg = "模块清单不完整：" + ($bad -join '；')
        if ($Tolerant) { Write-GdLog $msg -Level ERROR } else { throw $msg }
    }
    if (-not $Quiet -and $files.Count) {
        Write-GdLog ("已加载游戏模块：$((Get-GdModuleIds) -join ' / ')")
    }
    return @((Get-GdModules) | ForEach-Object { [string]$_.Id })
}

function Get-GdModuleLibPaths {
    <# 各模块声明的业务库(注册顺序去重后)。入口在顶层 dot-source 它们，钩子才调得动。
        -MissingOnly 只报"清单写了但文件不在"的那几项 —— 那是工程被裁坏了，不是"这一路没装"：
        助手库(maa.ps1 / m9a.ps1)可以不在(单游戏仓库正常形态)，但 *-module.ps1 声明了编排库却缺文件，
        跑起来只会在半夜的计划任务里炸出一句看不懂的"无法将…识别为 cmdlet"。 #>
    param([switch]$MissingOnly)
    $lib = Join-Path (Get-GdRoot) 'lib'
    $seen = @{}
    $out = @()
    foreach ($m in @(Get-GdModules)) {
        foreach ($f in @($m.Libs)) {
            $name = [string]$f
            if (-not $name -or $seen.ContainsKey($name)) { continue }
            $seen[$name] = $true
            $p = Join-Path $lib $name
            if (Test-Path -LiteralPath $p) { if ($MissingOnly) { continue } ; $out += $p }
            else { if ($MissingOnly) { $out += $name } }
        }
    }
    return @($out)
}

function Invoke-GdModuleHook {
    <# 框架调模块的唯一入口。三步都写清楚是谁的锅：模块没注册 / 清单没给这个钩子 / 钩子自己抛。
        钩子里面调的是各游戏原有的函数(Resolve-GdArkDecision / Start-GdArkDaily …)，本层不复制任何业务逻辑。 #>
    param(
        [Parameter(Mandatory)]$Module,
        [Parameter(Mandatory)][string]$Hook,
        [hashtable]$Params = @{}
    )
    $sb = $Module.Hooks.$Hook
    if ($null -eq $sb) { throw "模块 $([string]$Module.Id) 没有 $Hook 钩子(清单少写了一项？)" }
    return (& $sb @Params)
}

function Get-GdModuleEnvRows {
    <# 这一路要探的路径清单(标签 -> 路径)。由模块钩子给出，框架只做"路径填了没/在不在"的统一染色。
        以前这份清单在 daily-clear.ps1 里写死六行，单游戏薄壳里又写三行 —— 现在两边都是零行。 #>
    param([Parameter(Mandatory)]$Module, [Parameter(Mandatory)]$S)
    $rows = Invoke-GdModuleHook -Module $Module -Hook EnvProbes -Params @{ S = $S }
    if ($null -eq $rows) { return [ordered]@{} }
    return $rows
}

function Select-GdModules {
    <# 把"命令行想跑哪几路"翻译成模块对象列表。
        -Ids 为空 = 这棵树上装了的全跑；认不出来的 id 当场报错，别让它变成"静默少跑一款"。 #>
    param([string[]]$Ids)
    $avail = @(Get-GdModules -AvailableOnly)
    if (-not $avail.Count) {
        throw "这棵树上没有可加载的游戏模块(lib\*-module.ps1 一个都没有)，也没有任何一路的助手库 —— 检查工程是否完整"
    }
    if (-not $Ids -or (@(@($Ids) | Where-Object { $_ -and $_ -ne 'all' }).Count -eq 0)) { return $avail }
    $want = @($Ids | Where-Object { $_ -and $_ -ne 'all' })
    $picked = @()
    foreach ($id in $want) {
        $m = Get-GdModule -Id ([string]$id)
        if (-not $m) {
            throw "没有这个模块：$id(这棵树上有：$((Get-GdModuleIds) -join ' / '))。要跑某一款就先把它的 <id>-module.ps1 放进 lib\"
        }
        if (-not (Test-GdModuleInstalled -Module $m)) {
            Write-GdLog "$id 已注册但这一路没装(lib\$([string]$m.AssistantLib) 不在)，本轮跳过它" -Level WARN
            continue
        }
        $picked += $m
    }
    if (-not $picked.Count) { throw "选中的模块一个都没装：$($want -join ' / ')" }
    return @($picked)
}

function Set-GdModuleState {
    <# 两道手动门(活动确认 / 商店领空)的统一分发入口。
        参数名是各入口自己的事(-AckArk vs -Ack)，所以入口只管把"用户敲了哪个开关"报成 Verb；
        真正干什么由模块清单的 Ack / ShopState 钩子决定。 #>
    param(
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][ValidateSet('Ack', 'ShopDone', 'ShopReset')][string]$Verb,
        [Parameter(Mandatory)]$S
    )
    $m = Get-GdModule -Id $Id
    if (-not $m) { throw "没有这个模块：$Id" }
    if ($Verb -eq 'Ack') { return (Invoke-GdModuleHook -Module $m -Hook Ack -Params @{ S = $S }) }
    return (Invoke-GdModuleHook -Module $m -Hook ShopState -Params @{ S = $S; Done = ($Verb -eq 'ShopDone') })
}

function Invoke-GdDailyRound {
    <# 一轮日常的公共流程：环境探测 → 提权闸门 → 判定 → (dry-run 预览 | 真跑 + 收尾) → 汇总/历史/通知。
        合体入口和单游戏薄壳现在调的是同一个它，差别只剩"命令行叫什么名字"，由参数传进来。
        返回退出码(0 全成 / 1 部分成 / 2 有失败)，由入口 exit —— 不在框架里 exit，好让自检能拿返回值断言。

        -Gates 是 [{Id;Verb}] 列表，表示"用户这次还敲了哪几道门的开关"；先处理完门再进主流程，
        和拆分前 daily-clear.ps1 里那六行 if 的先后完全一致。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][string]$ElevateScript,     # 提权时要重跑的那个入口(只有入口自己知道自己的路径)
        [hashtable]$BoundParams = @{},                    # 入口的 $PSBoundParameters，原样转给提权子进程
        [AllowEmptyCollection()][AllowNull()][array]$Gates = @(),
        [string[]]$Ids = @(),
        [hashtable]$Opt = @{},
        [switch]$Force,
        [switch]$DryRun,
        [switch]$SkipCleanup,
        [switch]$NoElevate,
        [ValidateSet('Id', 'ShortName')][string]$NameField = 'Id',
        [string]$NoticeOwner = '每日',
        [string]$Tag = 'manual'
    )
    $mods = @(Select-GdModules -Ids $Ids)

    # ---------- 环境检查：要探的条目由各模块给出 ----------
    $envOk = @()
    $envBad = @()
    foreach ($m in $mods) {
        $rows = Get-GdModuleEnvRows -Module $m -S $S
        foreach ($k in @($rows.Keys)) {
            $p = [string]$rows[$k]
            if (Test-GdPathReady $p) { $envOk += $k } else { $envBad += "$k → $(if ($p) { $p } else { '(没填)' })" }
        }
    }
    Write-GdLog "环境检查：$($envOk.Count) 项就绪"
    if ($envBad.Count) {
        $envBad | ForEach-Object { Write-GdLog "缺失：$_" -Level ERROR }
        Write-GdLog "环境不完整：见上面缺失项(可跑 setup.ps1 或改 config\settings.json 的路径)" -Level ERROR
        return 2
    }

    # ---------- 提权闸门：只要本轮里有任意一款要管理员，就要提权 ----------
    # 三句提示里的"为什么要管理员/不满足会怎样"各款不一样(方舟是端游是管理员进程，1999 压根不要管理员)，
    # 所以那些话存在清单的 AdminWhy / AdminHint / AdminOkNote 里 —— 框架不许知道 MAA、端游这些名字。
    $needAdmin = @($mods | Where-Object { $_.RequiresAdmin -and ($S.runtime.$([string]$_.ElevationFlag) -ne $false) })
    if ($needAdmin.Count) {
        $who = (@($needAdmin | ForEach-Object { [string]$_.ShortName }) -join '/')
        $why = (@($needAdmin | ForEach-Object { [string]$_.AdminWhy } | Where-Object { $_ }) -join '；')
        $hint = (@($needAdmin | ForEach-Object { [string]$_.AdminHint } | Where-Object { $_ }) -join '；')
        $okNote = (@($needAdmin | ForEach-Object { [string]$_.AdminOkNote } | Where-Object { $_ }) -join '；')
        if (Test-GdAdmin) {
            Write-GdLog "管理员权限已具备$(if ($okNote) { "($okNote)" })"
        }
        elseif ($NoElevate) {
            Write-GdLog "非管理员且指定了 -NoElevate：$(if ($hint) { "$hint，" })${who}这一路会偏脆" -Level WARN
        }
        elseif ($DryRun) {
            Write-GdLog "非管理员：dry-run 无所谓，真正跑${who}时会自动提权(弹一次 UAC)" -Level WARN
        }
        else {
            Write-GdLog "${who}$why → 以管理员身份重启本轮，请在 UAC 弹窗点「是」" -Level WARN
            $code = Start-GdElevatedSelf -Script $ElevateScript -Bound $BoundParams -Force @('-NoElevate')
            Write-GdLog "提权子轮结束，退出码 $code(它的日志同样写在 logs\，不在本文件里)" -Level $(if ($code -eq 0) { 'OK' } else { 'WARN' })
            return [int]$code
        }
    }

    # ---------- 两道手动门 ----------
    $gateHit = @($Gates | Where-Object { $_ })
    foreach ($g in $gateHit) {
        Set-GdModuleState -Id ([string]$g.Id) -Verb ([string]$g.Verb) -S $S
    }
    if ($gateHit.Count) {
        if ($DryRun) { Write-GdLog '[dry-run] 仅记录确认/领空状态，结束' -Level OK; return 0 }
        Write-GdLog '状态已更新；继续执行本轮日常'
    }

    # ---------- 判定 ----------
    # 判定失败=今天不知道该跑什么，硬跑只会乱点。这里必须自己接住：裸 throw 到顶层是一屏
    # "Exception: C:\...\modules.ps1:298 / Line | ..." 那种内部报错(见 common.ps1 的 Stop-GdBeforeRun 注释)。
    $plan = [ordered]@{}
    foreach ($m in $mods) {
        $o = @{} + $Opt
        $o['Force'] = [bool]$Force
        try {
            $decision = Invoke-GdModuleHook -Module $m -Hook Resolve -Params @{ S = $S; Opt = $o }
        } catch {
            Stop-GdBeforeRun "$($m.ShortName) 这一路今天判定不出来：$($_.Exception.Message)(详情见 logs\$([System.IO.Path]::GetFileName($global:GD.LogFile)))"
        }
        $plan[[string]$m.Id] = [pscustomobject]@{
            Module   = $m
            Decision = $decision
        }
    }

    if ($DryRun) {
        Write-GdLog '[dry-run] 预览将要下发的配置，不启动任何程序' -Level STEP
        foreach ($id in $plan.Keys) {
            Invoke-GdModuleHook -Module $plan[$id].Module -Hook Preview -Params @{ S = $S; Decision = $plan[$id].Decision }
        }
        Write-GdLog '[dry-run] 完成' -Level OK
        return 0
    }

    # ---------- 运行 ----------
    # 登记活体标记：从这一行起，别的进程(面板/另一个终端)才认得出"有人正在跑"。
    # 放在 dry-run 返回之后是刻意的 —— 预演不占助手、不占端游，登记了反而会把面板挡在门外。
    Start-GdRunLock -Games $(if ($Ids) { $Ids -join '+' } else { 'all' }) -Tag $Tag
    if ($S.runtime.blockSleep) { Enter-GdNoSleep -KeepScreenOn ([bool]$S.runtime.keepScreenOn) }
    $results = [ordered]@{}
    $fatal = $null
    try {
        foreach ($id in $plan.Keys) {
            $results[$id] = Invoke-GdModuleHook -Module $plan[$id].Module -Hook Run -Params @{ S = $S; Decision = $plan[$id].Decision }
        }
    } catch {
        $fatal = $_.Exception.Message
        Write-GdLog "本轮异常中断：$fatal" -Level ERROR
    } finally {
        # ---------- 收尾(无论成败都要做) ----------
        if (-not $SkipCleanup) {
            Write-GdLog '--- 收尾 ---' -Level STEP
            foreach ($id in $plan.Keys) {
                try { Invoke-GdModuleHook -Module $plan[$id].Module -Hook Stop -Params @{ S = $S } }
                catch { Write-GdLog "收尾这一步没做成：$($_.Exception.Message)" -Level WARN }
            }
        }
        Exit-GdNoSleep
        # 撤活体标记。硬杀(面板停不掉提权轮、系统重启)走不到这里 —— 那种残留由读方按 PID 判活识破，
        # 所以标记不需要额外的心跳或超时字段。
        Stop-GdRunLock
    }

    # ---------- 汇总 ----------
    # 措辞得跟着入口走：合体入口的历史行/汇总行用模块 id(ark / r1999，和 history.csv 里一模一样)，
    # 单游戏薄壳用的是中文短名(方舟)。所以这一层不做选择，把"用哪个字段"交给调用方。
    $lines = @()
    $allOk = $true
    foreach ($id in $results.Keys) {
        $m = $plan[$id].Module
        $r = $results[$id]
        if ($r.Status -ne 'ok') { $allOk = $false }
        $mins = if ($r.Duration) { " $([math]::Round($r.Duration / 60, 1)) 分钟" } else { '' }
        $mode = if ($plan[$id].Decision) { [string]$plan[$id].Decision.Mode } else { 'skip' }
        $lines += "$($m.$NameField)：$($r.Status)$mins — $($r.Detail)"
        Add-GdHistory -Game $id -Status $r.Status -Mode $mode -Detail $r.Detail -DurationSec $r.Duration
    }
    $summary = $lines -join "`n"
    if ($fatal) { $allOk = $false; $summary = "本轮异常中断：$fatal`n$summary" }
    Write-GdLog "=== 本轮结束：$(if ($allOk) { '全部完成' } else { '存在未完成项' }) ===" -Level $(if ($allOk) { 'OK' } else { 'WARN' })
    Write-Host $summary

    # 气泡第一行是"每款跑的是什么方案"，用中文短名 + ' / ' 分隔(和拆分前合体入口的写法逐字一致)
    $modeLine = (@($plan.Keys) | ForEach-Object { "$($plan[$_].Module.ShortName)：$(if ($plan[$_].Decision) { [string]$plan[$_].Decision.Mode } else { 'skip' })" }) -join ' / '
    $noticeText = "$modeLine`n$summary"
    if (-not $allOk) { $noticeText += "`n(详情见 logs\$([System.IO.Path]::GetFileName($global:GD.LogFile)))" }
    # 注意 ${NoticeOwner} 的大括号不能省：CJK 是合法的变量名字符，"$NoticeOwner日常…" 会被解析成
    # 一个叫 $NoticeOwner日常已完成 的不存在变量 → 展开成空串 → Send-GdNotice 的 Mandatory Title 绑不上，
    # 整轮在"全部完成"日志之后、写通知之前抛异常退出(退出码 1，2026-09-25 晚实机踩中)。
    Send-GdNotice -Title $(if ($allOk) { "${NoticeOwner}日常已完成" } else { "${NoticeOwner}日常需要人工看一眼" }) -Text $noticeText -Level $(if ($allOk) { 'OK' } else { 'WARN' })

    if ($allOk) { return 0 }
    if (@($results.Keys).Count -gt 0 -and (@($results.Values | Where-Object { $_.Status -eq 'ok' }).Count -gt 0)) { return 1 }
    return 2
}
