# tools/test-lib.ps1 — 自检脚手架：引擎无关的那半套(计数、假 state 的存与取、公共库检查)
# 不单独跑，由 test-ark.ps1 / test-r1999.ps1 用 dot-source 引进来，所以这里的
# $script:Pass / $S 都落在引进它的那一路脚本的作用域里，两路各记各的数。
#
# 这一份会原样进两个导出仓库，因此只能"文件在就 dot、不在就少一侧能力"，
# 绝不能硬 dot 另一个游戏才有的 lib 文件(和 lib\decisions.ps1 同一个道理)。

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $root 'lib\common.ps1')
foreach ($lib in @('decisions', 'maa', 'm9a', 'vision-rescue')) {
    $libPath = Join-Path $root "lib\$lib.ps1"
    if (Test-Path -LiteralPath $libPath) { . $libPath }
}

$script:Pass = 0
$script:Fail = 0
function Check {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][bool]$Cond, [string]$Info = '')
    if ($Cond) { $script:Pass++; Write-Host "  PASS  $Name $Info" -ForegroundColor Green }
    else { $script:Fail++; Write-Host "  FAIL  $Name $Info" -ForegroundColor Red }
}

$S = Get-GdSettings -Quiet

# ---------- 临时改 state 文件那一段的保险：跑之前存，跑完(含抛异常)原样放回 ----------
$script:GdTestSnap = @{}
function Get-GdTestStatePath {
    param([Parameter(Mandatory)][string]$Name)
    return (Join-Path $global:GD.State "$Name.json")
}
function Save-GdTestState {
    param([Parameter(Mandatory)][string[]]$Names)
    $script:GdTestSnap = @{}
    foreach ($name in $Names) {
        $p = Get-GdTestStatePath -Name $name
        $script:GdTestSnap[$name] = if (Test-Path -LiteralPath $p) { Get-Content -LiteralPath $p -Raw -Encoding utf8 } else { $null }
    }
}
function Restore-GdTestState {
    foreach ($name in @($script:GdTestSnap.Keys)) {
        $p = Get-GdTestStatePath -Name $name
        $content = $script:GdTestSnap[$name]
        if ($null -eq $content) { Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue }
        else { Set-Content -LiteralPath $p -Value $content -Encoding utf8 -NoNewline }
    }
}
function Test-GdTestStateRestored {
    foreach ($name in @($script:GdTestSnap.Keys)) {
        $p = Get-GdTestStatePath -Name $name
        $now = if (Test-Path -LiteralPath $p) { Get-Content -LiteralPath $p -Raw -Encoding utf8 } else { $null }
        if ($now -ne $script:GdTestSnap[$name]) { return $false }
    }
    return $true
}

# ---------- 两路都要的公共库检查(lib\common.ps1 那几件小事) ----------
function Show-GdCommonTimeChecks {
    Write-Host "`n[t1] 路径/时间工具"
    Check 'Join-GdPath 拼多层' ((Join-GdPath 'C:\A' @('b', 'c.json')) -eq 'C:\A\b\c.json')
    Check 'Join-GdPath 单段'   ((Join-GdPath 'C:\A' @('b.json')) -eq 'C:\A\b.json')
    $ms = Convert-GdUtc8ToMs '2026-09-04 12:00:00'
    Check 'UTC+8 时间转换' ($ms -eq ([DateTimeOffset]::new(2026, 9, 4, 4, 0, 0, [TimeSpan]::Zero).ToUnixTimeMilliseconds())) "得到 $ms"
    Check '斜杠日期同样可解析' ($ms -eq (Convert-GdUtc8ToMs '2026-09-04 12:00:00'))
    Check '坏日期返回 null'  ($null -eq (Convert-GdUtc8ToMs '不是时间'))
    Check 'Format-GdUnixMs' ((Format-GdUnixMs $ms) -eq '09-04 12:00') (Format-GdUnixMs $ms)
}

function Show-GdCommonTailChecks {
    Write-Host "`n[t11] 空行日志(装任务脚本结尾用它排版)"
    $emptyOk = try { Write-GdLog '' -Quiet; $true } catch { $false }
    Check 'Write-GdLog 允许空消息' $emptyOk $(if (-not $emptyOk) { 'Mandatory 会把空字符串当缺参数，脚本会因此返回退出码 1' })
}

# 单游戏导出仓库的硬性要求：不能因为"另一个游戏的助手目录没填"就每次报警。
# Get-GdSettings 的必查清单由 Get-GdRequiredPathKeys 按"lib 里有哪些助手库"推出来，这里把这条规矩钉住。
function Show-GdCommonPathChecks {
    Write-Host "`n[t1b] 该催哪几个助手目录(跟着本仓库装了哪一路走)"
    $here = @(Get-GdRequiredPathKeys)
    Check '有 lib\maa.ps1 才催 maaDir'           (($here -contains 'maaDir') -eq (Test-Path -LiteralPath (Join-Path $root 'lib\maa.ps1'))) ($here -join ',')
    Check '1999 的库不在本仓库 → 不催 m9aDir/mumuDir' ((($here -contains 'm9aDir') -and ($here -contains 'mumuDir')) -eq (Test-Path -LiteralPath (Join-Path $root 'lib\m9a.ps1'))) ($here -join ',')
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('gd-req-' + [Guid]::NewGuid().ToString('N'))
    try {
        foreach ($case in @(
                @{ Libs = @('maa.ps1'); Warn = '只发方舟：只要 MAA' },
                @{ Libs = @('m9a.ps1', 'mumu.ps1'); Warn = '只发 1999：要 M9A + MuMu' },
                @{ Libs = @('maa.ps1', 'm9a.ps1', 'mumu.ps1'); Warn = '合体工程：三个都催(与拆分前一致)' })) {
            $one = Join-Path $tmp ([Guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Path (Join-Path $one 'lib') -Force | Out-Null
            foreach ($l in $case.Libs) { Set-Content -LiteralPath (Join-Path $one "lib\$l") -Value '' -Encoding utf8 }
            $want = @($case.Libs | ForEach-Object { ($_ -replace '\.ps1$', '') + 'Dir' }) -join ','
            Check "$($case.Warn) → $want" ((@(Get-GdRequiredPathKeys -Root $one) -join ',') -eq $want)
        }
        Check '空仓库壳子/不存在的目录返回空列表(不抛)' $((@(Get-GdRequiredPathKeys -Root (Join-Path $tmp 'no-such-root'))).Count -eq 0)
    } finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# 单游戏仓库的提示语必须写它自己的参数名(否则用户照着日志敲 -AckArk，入口根本没这个参数)。
# 判定层默认用干净短名，合体入口靠 Set-GdEntryVerb 覆盖 —— 这条把两侧行为都钉住。
function Show-GdCommonVerbChecks {
    Write-Host "`n[t1c] 提示语里的参数名(跟着入口走)"
    $dArk = @((Get-GdEntryVerb -Game ark -Verb Ack), (Get-GdEntryVerb -Game ark -Verb ShopReset))
    Check '默认提示用单游戏入口的干净短名' (($dArk -join ',') -eq '-Ack,-ShopReset') ($dArk -join ' , ')
    Set-GdEntryVerb -Game ark -Verb Ack -Name '-AckArk'
    $crossed = (Get-GdEntryVerb -Game r1999 -Verb Ack) -eq '-Ack'
    $overrode = (Get-GdEntryVerb -Game ark -Verb Ack) -eq '-AckArk'
    Set-GdEntryVerb -Game ark -Verb Ack -Name '-Ack'
    Check '覆盖只影响那一款(ark 改了 r1999 仍是短名)' ($overrode -and $crossed) "ark=$(Get-GdEntryVerb -Game ark -Verb Ack) r1999=$(Get-GdEntryVerb -Game r1999 -Verb Ack)"
    Check '还原后回到默认' ((Get-GdEntryVerb -Game ark -Verb Ack) -eq '-Ack')
    # 上面那六行映射从入口脚本搬进了 Sync-GdEntryVerbs(status/探针/入口都用它)，所以"这个仓库该看到哪种名字"
    # 也要在这里钉住：合体树给长名(入口只认 -AckArk)，单游戏导出树给短名(它的入口声明的就是 -Ack)。
    $merged = Test-GdMergedEntry
    $wantAck = '-Ack'; $wantShop = '-ShopDone'
    if ($merged) { $wantAck = '-AckArk'; $wantShop = '-RShopDone' }
    Sync-GdEntryVerbs
    $gotAck = Get-GdEntryVerb -Game ark -Verb Ack
    $gotShop = Get-GdEntryVerb -Game r1999 -Verb ShopDone
    Check "Sync-GdEntryVerbs 按本仓库入口给名字(合体工程=$merged)" (($gotAck -eq $wantAck) -and ($gotShop -eq $wantShop)) "ark Ack=$gotAck r1999 ShopDone=$gotShop"
    # Sync 一次改的是六格，这里也得六格全还原：[t7] 那几条断言的是"默认(短名)行为"，
    # 漏一格就会在后面假红(第一版就漏了 ShopReset 那两格)。
    foreach ($g in @('ark', 'r1999')) {
        Set-GdEntryVerb -Game $g -Verb Ack -Name '-Ack'
        Set-GdEntryVerb -Game $g -Verb ShopDone -Name '-ShopDone'
        Set-GdEntryVerb -Game $g -Verb ShopReset -Name '-ShopReset'
    }
}

# 提示语"照抄就能跑"是这一路的用户界面：日志里让用户敲一个本仓库入口没有的参数名，点了就报"找不到参数"。
# 所以静态扫一遍本仓库的 .ps1，凡是出现"入口脚本名 + 一个参数"的地方，那个参数必须在
# 本仓库入口脚本的 param() 里真的存在 —— 单游戏导出仓库里冒出合体工程的长参数名会立刻在这里被抓住。
function Show-GdCommonEntryHintChecks {
    Write-Host "`n[t1d] 提示语里敲的命令，本仓库入口真的收这个参数"
    $entry = Join-Path $root 'daily-clear.ps1'
    if (-not (Test-Path -LiteralPath $entry)) { Check '本仓库有入口脚本 daily-clear.ps1' $false "$entry"; return }
    $tokens = $null; $errs = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($entry, [ref]$tokens, [ref]$errs)
    $flags = @($ast.ParamBlock.Parameters | ForEach-Object { $_.Name.VariablePath.UserPath })
    Check "入口 $($entry | Split-Path -Leaf) 声明了 $(($flags | Sort-Object -Unique).Count) 个参数" ($flags.Count -ge 3) (($flags | Sort-Object -Unique) -join ',')
    $pat = 'daily-clear\.ps1\s+(-[A-Za-z][A-Za-z0-9]*)'
    $bad = @()
    # publish\src 下那些是"给导出仓库用的入口"，它们自己的参数由导出仓库跑这一条时校验；
    # 在本地合体工程里拿它们的 -Ack 去对 -AckArk 的入口，只会假报警。
    foreach ($f in (Get-ChildItem -LiteralPath $root -Recurse -Filter '*.ps1' | Where-Object { $_.FullName -notmatch '\\(logs|state|backups|publish)\\' })) {
        $n = 0
        foreach ($line in (Get-Content -LiteralPath $f.FullName -Encoding utf8)) {
            $n++
            foreach ($m in [regex]::Matches($line, $pat)) {
                $flag = $m.Groups[1].Value.Substring(1)
                if ($flag -notin $flags) { $bad += "$($f.Name):$n → $flag" }
            }
        }
    }
    Check '没有提示过本仓库入口不存在的参数' ($bad.Count -eq 0) ($bad -join ' , ')
}

# PowerShell 参数模式(argument mode)里，-NotePropertyValue 后面直接写 [string]$x 并不会转型：
# 它把类型字面量当字符串、再把后面的 token 一起吞成值，2026-09-24 就是这样把 settings.json 的 vmIndex
# 写成了 "[string]System.Collections.Specialized.OrderedDictionary[mumu.vmIndex].Value"。
# 静态扫一遍本仓库的 .ps1，出现这个写法就报红(写法本身永远不该有，先转成变量再传是对的)。
function Show-GdCommonScriptHygieneChecks {
    Write-Host "`n[t1e] 脚本卫生(参数模式伪转型)"
    $pat = '-Note' + 'PropertyValue\s*\['   # 拼起来，免得这条检查自己扫到自己
    $hits = @()
    foreach ($f in (Get-ChildItem -LiteralPath $root -Recurse -Filter '*.ps1' | Where-Object { $_.FullName -notmatch '\\(logs|state|backups|publish)\\' })) {
        $n = 0
        foreach ($line in (Get-Content -LiteralPath $f.FullName -Encoding utf8)) {
            $n++
            if ($line -match $pat) { $hits += "$($f.Name):$n" }
        }
    }
    Check '没有把 [类型] 直接写在 -NotePropertyValue 后面的伪转型' ($hits.Count -eq 0) ($hits -join ' , ')
}

# 第一次跑(exe 双击 → setup 还没探测)时 paths.* 全是空串。这时候两种调用方要的是两件不同的事：
# 只读诊断(status/探针/自检)得能把"哪几项没填"列出来；要干活的入口必须停下来点名 setup.ps1 ——
# 2026-09-24 第一次打包 exe 时，空路径是让 daily-clear 抛一句"参数 'Base' 是空字符串"的，玩家看不出该干什么。
function Show-GdCommonEmptyPathChecks {
    Write-Host "`n[t1f] 助手路径还没填时的行为(exe/访客第一次跑)"
    Check 'Join-GdPath base 空 → 返回空串(不抛)' ((Join-GdPath '' @('a', 'b.json')) -eq '')
    $noThrow = $true; $emptyIsFalse = $false
    try { $emptyIsFalse = (Test-GdPathReady '') -eq $false } catch { $noThrow = $false }
    Check 'Test-GdPathReady 空串算不就绪且不抛' ($noThrow -and $emptyIsFalse) $(if (-not $noThrow) { 'Test-Path -LiteralPath 收到空字符串会抛异常' })
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('gd-nopaths-' + [Guid]::NewGuid().ToString('N'))
    try {
        New-Item -ItemType Directory -Path (Join-Path $tmp 'config') -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $tmp 'config\settings.json') -Value '{"paths":{"maaDir":"","m9aDir":"","mumuDir":""}}' -Encoding utf8
        $savedRoot = $global:GD.Root
        try {
            $global:GD.Root = $tmp
            $quietOk = $true
            try { $null = Get-GdSettings -Quiet -RequirePaths @('maaDir', 'mumuDir') } catch { $quietOk = $false }
            Check '诊断方(-Quiet)照样读得下来，只是报哪一项没填' $quietOk
            $msg = ''
            try { $null = Get-GdSettings -RequirePaths @('maaDir', 'mumuDir') } catch { $msg = $_.Exception.Message }
            Check '入口(不 -Quiet)停下来，且一次点齐空项 + 点名 setup.ps1' (($msg -match 'maaDir') -and ($msg -match 'mumuDir') -and ($msg -match 'setup\.ps1')) $msg
        } finally { $global:GD.Root = $savedRoot }
    } finally { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
}

# ---------- 活体标记(跨进程互斥)：state\running.json 的登记/判活/撤销 ----------
# 这一组是面板撞车保护的唯一数据源，所以两条反向断言都不能省：
# 残留必须判成"没在跑"(否则一次硬杀会把面板永久锁死)，别人登记的必须删不掉(否则两轮互相拆保护)。
# 已知窗口：这一段会临时改写 running.json。真有一轮在跑时跑自检，面板会在那几百毫秒里
# 把本测试进程看成"在跑的那一轮"，测完快照原样还原 —— 所以别拿自检结果当"此刻有没有在跑"的依据。
function Show-GdCommonRunLockChecks {
    Write-Host "`n[t16] 活体标记 running.json"
    Save-GdTestState -Names @('running')
    try {
        $p = Get-GdTestStatePath -Name 'running'
        Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue
        Check '没有标记时 Get-GdRunLock 返回 null' ($null -eq (Get-GdRunLock))
        Check '标记文件确实不存在' (-not (Test-Path -LiteralPath $p))

        Start-GdRunLock -Games 'ark+r1999' -Tag 'manual'
        $o = Get-GdRunLock
        Check '登记后判成在跑' ($o.Alive -eq $true) $(if ($o) { "pid=$($o.Pid)" } else { '读回 null' })
        Check 'PID 记的是本进程' ($o.Pid -eq $PID) "$($o.Pid)"
        Check 'games 原样读回' ($o.Games -eq 'ark+r1999') "$($o.Games)"
        Check 'tag 原样读回' ($o.Tag -eq 'manual') "$($o.Tag)"
        Check 'started 是可解析的时间' ($( try { [void][DateTime]::ParseExact($o.Started, 'yyyy-MM-dd HH:mm:ss', $null); $true } catch { $false } )) "$($o.Started)"

        # 别人的 PID(活着、但不是本进程)：判活要是 true，而本进程的撤销必须不动它
        $other = @(Get-Process | Where-Object { $_.Id -ne $PID } | Select-Object -First 1).Id
        if ($other) {
            '{"pid":' + $other + ',"started":"2026-09-25 19:00:00","games":"ark","tag":"auto","log":""}' | Set-Content -LiteralPath $p -Encoding utf8
            $o2 = Get-GdRunLock
            Check '别人活着的 PID 判成在跑' ($o2.Alive -eq $true) "pid=$other"
            Stop-GdRunLock
            Check '本进程撤销不动别人登记的标记' (Test-Path -LiteralPath $p)
        } else {
            Check '本进程撤销不动别人登记的标记' $false '拿不到别的活进程 PID，这条没测到'
        }

        '{"pid":2147483646,"started":"2026-09-25 19:00:00","games":"ark","tag":"auto","log":""}' | Set-Content -LiteralPath $p -Encoding utf8
        $o3 = Get-GdRunLock
        Check '查无此 PID 判成残留(不在跑)' ($o3.Alive -eq $false) '面板据此放行，不会被崩溃残留永久挡住'

        Start-GdRunLock -Games 'all' -Tag 'manual'
        Stop-GdRunLock
        Check '撤销后标记文件消失' (-not (Test-Path -LiteralPath $p))

        '{ 这不是 JSON' | Set-Content -LiteralPath $p -Encoding utf8
        $o4 = $null
        $noThrow = try { $o4 = Get-GdRunLock; $true } catch { $false }
        Check '坏文件读成 null 且不抛(不能让面板/命令行因此炸)' ($noThrow -and $null -eq $o4)
    } finally { Restore-GdTestState }
    Check '测完 state 快照已原样还原' (Test-GdTestStateRestored)
}

# ---------- 收尾：打印那一行"结果"，并把计数交给调用方(test-decisions.ps1 汇总用) ----------
function Show-GdTestResult {
    param([Parameter(Mandatory)][string]$Tag)
    Write-Host "`n结果：$script:Pass 通过 / $script:Fail 失败" -ForegroundColor $(if ($script:Fail) { 'Red' } else { 'Green' })
    return [pscustomobject]@{ GdTest = $true; Tag = $Tag; Pass = $script:Pass; Fail = $script:Fail }
}
