# tools/test-lib.ps1 — 自检脚手架：引擎无关的那半套(计数、假 state 的存与取、公共库检查)
# 不单独跑，由 test-ark.ps1 / test-r1999.ps1 用 dot-source 引进来，所以这里的
# $script:Pass / $S 都落在引进它的那一路脚本的作用域里，两路各记各的数。
#
# 这一份会原样进两个导出仓库，因此只能"文件在就 dot、不在就少一侧能力"，
# 绝不能硬 dot 另一个游戏才有的 lib 文件(和 lib\decisions.ps1 同一个道理)。

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $root 'lib\common.ps1')
# 名单 = 两份模块清单声明的 Libs 的并集(-common/decisions 是两边共用的底座，也一并点)。
# 这件事以前只在注释里"由 [m3] 钉着"，实际 [m3] 只钉了反方向(清单不许引用别人的库)，于是 2026-09-26
# 加了 ark-farm.ps1 之后：ark-daily 的掉落记账调用在自检里认不到函数、一路掉进 catch 打一句 WARN 就过了，
# 全套件照样全绿 —— 那是假绿。现在名单由下面的 $GdTestDotLibs 交给 [m3] 逐条对账，漏一项当场变红。
# 仍然带 Test-Path 守卫：单游戏导出仓里另一款的库不在，就少点一份、少一侧能力，不炸。
# 11-04 转接刀·运行面刀⑦ 笔(c/d)：'ark-daily'／'r1999-daily' 两枚随编排岛整件出仓，本名单自此八枚。
# 上面那句"名单＝两份清单声明的 Libs 的并集"仍然成立，只是两侧同时少两枚才闭合：只删这一份而清单没删
# ＝少装载一路(那一路的库认不到函数)，只删清单而这一份没删＝多一个恒不命中的名字、谁也看不出。
# 上面那条讲"当年 ark-daily 的调用在自检里认不到函数"的史注按仓内口径不改口——它说的那枚件现在在仓外，
# 故事讲的是[为什么立这条对账]，不是[现在点的是谁]。
# 11-06 转接刀·运行面刀⑬ 笔(c)：'ark-farm' 整件出仓(判定+记账真身全在核内 Chain.cs／Orchestration.cs)，名单自此不点它。
# 11-06 转接刀·运行面刀⑮ 笔(d)：'vision-rescue'／'vision-ark'／'vision-mumu' 三枚随三件本体整件出仓，名单自此四枚(与两份清单 Libs 同帧摘——只删清单＝少装载一路，只删名单＝多一个恒不命中的名字，test-modules 那格量两头闭合)。
$script:GdTestDotLibs = @('decisions', 'maa', 'm9a', 'mumu')
foreach ($lib in $script:GdTestDotLibs) {
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

# 屏保读数：06:00 方舟连败的根因，判据必须能脱离本机注册表被验完。
# 全部喂构造出来的 $Cfg(只读自检不许要求机器现场状态：这台机器解除没解除都不该让套件变红)，
# 最后两条才碰真注册表，而且只要求"落在四态之一、不许抛" —— 不押它是哪一种。
function Show-GdCommonScreensaverChecks {
    Write-Host "`n[t1g] 屏保读数(武装没有 / 是谁挂的 / 解除工具会不会误写)"
    $none = [pscustomobject]@{ Saver = ''; Product = ''; Active = '0'; TimeoutSec = 0 }
    $armed = [pscustomobject]@{ Saver = 'C:\Windows\system32\wpxscreensaver64.scr'; Product = 'Wallpaper Engine Screensaver'; Active = '1'; TimeoutSec = 300 }
    $off = [pscustomobject]@{ Saver = 'C:\Windows\system32\some.scr'; Product = ''; Active = '0'; TimeoutSec = 600 }
    # 门内现问薄壳(转接刀·屏保稳态族)：lib\common.ps1 那三枚同规则真身已出仓，唯一事实源在核内
    # (Decisions.cs 的 ScreensaverWho／ScreensaverVerdict ＋ Round.cs 的 ScreensaverConfig)——
    # 判据一格没动，只是取数那半段从"调 PS 壳"换成"问出口"。假读数不带 Found 键，直接递进去。
    function GdSvVerdict { param($cfg) return Invoke-GdCli -Verb 'resolve-screensaver-verdict' -Body ([pscustomobject]@{ cfg = $cfg }) }
    function GdSvWho { param($cfg) return [string](Invoke-GdCli -Verb 'resolve-screensaver-who' -Body ([pscustomobject]@{ cfg = $cfg })).Text }
    # 本机真读数走的那半段：现问 read-screensaver-config，Found:false 必须折成 cfg=null(读不到那一档)，
    # 否则核内会把"不知道"读成"没挂"—— 与 status.ps1／unarm-screensaver.ps1 同一句折算。
    function GdSvLiveVerdict {
        $d = Invoke-GdCli -Verb 'read-screensaver-config' -Body ([pscustomobject]@{})
        return Invoke-GdCli -Verb 'resolve-screensaver-verdict' -Body ([pscustomobject]@{ cfg = $(if ($d.Found) { $d } else { $null }) })
    }
    $vNone = GdSvVerdict -cfg $none
    $vArmed = GdSvVerdict -cfg $armed
    $vOff = GdSvVerdict -cfg $off
    $vNull = GdSvVerdict -cfg $null
    Check '没挂 .scr → none，且不算风险' ($vNone.Kind -eq 'none' -and -not $vNone.Risk) ($vNone.Text)
    Check '挂了且武装位=1 → armed，是唯一要标红的一档' ($vArmed.Kind -eq 'armed' -and $vArmed.Risk) ($vArmed.Kind)
    Check '挂了但武装位=0 → off，不吓人(日常"只关不复"就落这档)' ($vOff.Kind -eq 'off' -and -not $vOff.Risk) ($vOff.Kind)
    Check '注册表读不到 → 说的是"判断不了"，绝不显示成没事' ($vNull.Kind -eq 'unknown' -and $vNull.Text -match '读不到' -and -not $vNull.Risk) ($vNull.Text)
    Check 'armed 的长话里带得出解除命令(让人看完知道下一步敲什么)' ($vArmed.Detail -match 'unarm-screensaver') ($vArmed.Detail)
    Check 'off 的长话里说明它不是终态(会被重新武装)' ($vOff.Detail -match '不是终态')
    Check '是谁挂的：ProductName 认 Wallpaper Engine' ((GdSvWho -cfg $armed) -eq 'Wallpaper Engine')
    Check '是谁挂的：读不到 ProductName 时退回文件名，不印 8.3 短名' ((GdSvWho -cfg $off) -eq 'some')
    Check '面板那行(短话)不带箭头和路径，卡片装得下' ($vArmed.Text.Length -le 30 -and $vArmed.Text.IndexOf('→') -lt 0) ($vArmed.Text)
    # 本机真读数：只验"不许抛 + 落在四态里"，不押它此刻是哪一种(解除/没解除都该绿)
    $live = $null
    $threw = $false
    try { $live = GdSvLiveVerdict } catch { $threw = $true }
    Check '读本机注册表不抛，且结论落在 none/armed/off/unknown 四种之一' (-not $threw -and @('none','armed','off','unknown') -contains $live.Kind) ($live.Text)
    # 语义闸的形状钉(转接刀·屏保稳态族)：三处生产消费面那句 Found:false ⇒ 递 cfg=null 必须还在**问那一枚 verb 的同一行**上。
    # 为什么只能按形状钉：这台机器注册表恒能读到(Found 真)⇒ 把折算整句摘掉也不会有任何现读格变红，
    # 而摘掉的后果是把"读不到"交给核内读成"没挂 .scr(没事)"—— 那一档话术正是体检要避免的假安心。
    # 只数调用行(带 Invoke-GdCli 那句)，注释里同名的 verb 字面不算账；一行调用都没有／调用行里没那句折算都算这格红。
    # 分路盲区改口(推仓74 出货面现红)：lib\ark-daily.ps1 是方舟专有件，1999 那棵导出树里本来就没有它，
    #   旧的"不在即红"把分路差当成本格的罪(导出侧那一趟交回 182 通过／1 失败，主仓侧 183 通过／0 失败)。
    #   处方照本件头注那套点库的形制(文件在就点、不在就少一侧能力)＋本件那句"有 lib\maa.ps1 才催 maaDir"＝路标分档，
    #   松的只许是"判的对象"、不许是判据：docs\rust-core-plan.md 在场＝合体工作树，那件不在照样红(真身被删／改名不许静默放过)；
    #   那本 plan 不在＝单游戏出货树，那一枚消费面本就不在这批货里，明印 [跳过] 而不是静默。
    #   ⇒ 这一格自此在只带另一路的出货树上少量一枚消费面(方舟那棵树仍三枚全量)；工作树侧的射程一字未减，
    #     非分路那两件(status.ps1／tools\unarm-screensaver.ps1)在任何一棵树里缺了都照样红。
    # 11-04 转接刀·运行面刀⑦ 笔(c/d)：上面那枚分路件 lib\ark-daily.ps1 随编排岛整件出仓 ⇒ 本格的靶自三枚改两枚，
    #   折算的**生产消费面 3→2**(status.ps1／tools\unarm-screensaver.ps1 两枚仍每棵树都在)。判据因此是**收紧**不是放宽：
    #   $foldPerGame 空了＝分路豁免通道恒不开，任何一棵树里缺这两枚都当场红。那套 [跳过] 形制故意留着(见下面 $foldSkip
    #   仍在打印)——以后若又长出只属于某一路的消费面，形制还在；把它整段删掉才是把这条路堵死。
    #   ⚠ 上面那段"分路盲区改口(推仓74)"按史注读，不改口：它记的是当时那次出货面现红的处方，那枚件现在在仓外。
    $foldFiles = @('status.ps1', 'tools\unarm-screensaver.ps1')
    $foldPerGame = @()
    $foldMiss = @()
    $foldSkip = @()
    $foldWork = Test-Path -LiteralPath (Join-Path $root 'docs\rust-core-plan.md')
    foreach ($foldFile in $foldFiles) {
        $foldPath = Join-Path $root $foldFile
        if (-not (Test-Path -LiteralPath $foldPath)) {
            if ($foldWork -or -not ($foldPerGame -contains $foldFile)) { $foldMiss += ($foldFile + ' 不在') }
            else { $foldSkip += $foldFile }
            continue
        }
        $foldLines = @(Get-Content -LiteralPath $foldPath -Encoding utf8)
        $foldCalls = @($foldLines | Select-String -SimpleMatch "Invoke-GdCli -Verb 'resolve-screensaver-verdict'")
        if ($foldCalls.Count -lt 1) { $foldMiss += ($foldFile + ' 没问那一枚 verb'); continue }
        $foldNoFold = @($foldCalls | Where-Object { $_.Line -notmatch '\.Found' })
        if ($foldNoFold.Count -gt 0) { $foldMiss += ($foldFile + ':' + $foldNoFold[0].LineNumber) }
    }
    $foldNote = $foldMiss -join ' / '
    if ($foldSkip.Count -gt 0) { $foldNote = ($foldNote + ' [跳过 分路专有] ' + ($foldSkip -join ',')) }
    Check 'Found:false 折 cfg=null 那句语义闸在本树在场的生产消费面各自还在(与现问同一行)' ($foldMiss.Count -eq 0) $foldNote
    # 解除工具：默认只读这道规矩用行序钉死 —— 写注册表的调用一个都不许出现在两个显式分支闸门之前
    $toolPath = Join-Path $root 'tools\unarm-screensaver.ps1'
    if (Test-Path -LiteralPath $toolPath) {
        $t = @(Get-Content -LiteralPath $toolPath -Encoding utf8)
        $restoreAt = @($t | Select-String -SimpleMatch 'if ($Restore)' | Select-Object -First 1).LineNumber
        $applyAt = @($t | Select-String -SimpleMatch 'if (-not $Apply)' | Select-Object -First 1).LineNumber
        # 两个 if 块各自的收尾括号(顶格 })：写操作只有落在 Restore 块内、或在 Apply 闸门闭合之后才算被门管住
        $restoreEnd = @($t | Select-String -Pattern '^\}' | Where-Object { $_.LineNumber -gt $restoreAt } | Select-Object -First 1).LineNumber
        $applyEnd = @($t | Select-String -Pattern '^\}' | Where-Object { $_.LineNumber -gt $applyAt } | Select-Object -First 1).LineNumber
        $writes = @($t | Select-String -Pattern 'Set-ItemProperty|Remove-ItemProperty|New-ItemProperty|Set-GdScreensaverActive' | ForEach-Object { $_.LineNumber })
        $ungated = @($writes | Where-Object {
                -not ($_ -gt $restoreAt -and $_ -lt $restoreEnd) -and -not ($_ -gt $applyEnd)
            })
        Check 'tools\unarm-screensaver.ps1 的写操作(注册表 + SPI)全在 -Restore / -Apply 两道闸门之内(裸跑=只读)' `
              ($restoreAt -gt 0 -and $applyAt -gt 0 -and $restoreEnd -gt $restoreAt -and $applyEnd -gt $applyAt -and $ungated.Count -eq 0) `
              ("Restore=$restoreAt..$restoreEnd Apply=$applyAt..$applyEnd 漏门行 " + $(if ($ungated.Count) { $ungated -join ',' } else { '无' }))
        # 只读那一支的退出码必须跟着读数走(0=没风险 / 1=仍武装) —— 它同时是别的脚本"想知道屏保武装没有"的闸，
        # 只读一律退 0 会让这道闸永远说没事。取 -not $Apply 到它的收尾括号之间那几行(数组是 0 基，行号是 1 基)。
        $roLines = @()
        if ($applyAt -gt 0 -and $applyEnd -gt $applyAt + 1) { $roLines = @($t[$applyAt..($applyEnd - 2)]) }
        $roExits = @($roLines | Select-String -SimpleMatch 'exit')
        Check '裸跑(-not $Apply 那一支)的退出码跟着 Risk 走，不是写死 0' `
              (@($roExits | Where-Object { $_ -match 'Risk' }).Count -eq 1 -and (@($roExits | Where-Object { $_ -match 'exit\s+0\s*$' }).Count -eq 0)) `
              ('这一支的 exit: ' + $(if ($roExits.Count) { ($roExits | ForEach-Object { $_.Line.Trim() }) -join ' / ' } else { '一个都没有' }))
    } else {
        Check 'tools\unarm-screensaver.ps1 在(屏保解除要有它)' $false
    }
}

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
    # 时间换算的证性格自 2026-11-03 起问的是核内那一份(verb convert-utc8-to-ms／format-unix-ms)：
    # PS 侧那两枚真身已出仓，这四格不再量本地实现，改量"出口答得对不对"；输入字面逐字照旧，不趁搬轴改判据。
    $ms = (Invoke-GdCli -Verb 'convert-utc8-to-ms' -Body ([pscustomobject]@{ text = '2026-09-04 12:00:00' })).Ms
    Check 'UTC+8 时间转换' ($ms -eq ([DateTimeOffset]::new(2026, 9, 4, 4, 0, 0, [TimeSpan]::Zero).ToUnixTimeMilliseconds())) "得到 $ms"
    Check '斜杠日期同样可解析' ($ms -eq (Invoke-GdCli -Verb 'convert-utc8-to-ms' -Body ([pscustomobject]@{ text = '2026-09-04 12:00:00' })).Ms)
    Check '坏日期返回 null'  ($null -eq (Invoke-GdCli -Verb 'convert-utc8-to-ms' -Body ([pscustomobject]@{ text = '不是时间' })).Ms)
    $fmt = (Invoke-GdCli -Verb 'format-unix-ms' -Body ([pscustomobject]@{ ms = $ms })).Text
    Check '毫秒格式化到分钟' ($fmt -eq '09-04 12:00') $fmt
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
    Check '1999 的库不在本仓库 → 不催 m9aDir/mumuDir(两键都挂 m9a.ps1)' ((($here -contains 'm9aDir') -and ($here -contains 'mumuDir')) -eq (Test-Path -LiteralPath (Join-Path $root 'lib\m9a.ps1'))) ($here -join ',')
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('gd-req-' + [Guid]::NewGuid().ToString('N'))
    try {
        foreach ($case in @(
                @{ Libs = @('maa.ps1'); Warn = '只发方舟：只要 MAA'; Want = 'maaDir' },
                @{ Libs = @('m9a.ps1'); Warn = '只发 1999：要 M9A + MuMu'; Want = 'm9aDir,mumuDir' },
                @{ Libs = @('maa.ps1', 'm9a.ps1'); Warn = '合体工程：三个都催(与拆分前一致，mumuDir 挂 1999 路的库)'; Want = 'maaDir,m9aDir,mumuDir' })) {
            $one = Join-Path $tmp ([Guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Path (Join-Path $one 'lib') -Force | Out-Null
            foreach ($l in $case.Libs) { Set-Content -LiteralPath (Join-Path $one "lib\$l") -Value '' -Encoding utf8 }
            # 刀⑯ mumu.ps1 出仓(11-07)后 want 不能再由件名现算(mumuDir 挂 m9a.ps1 那一对)：三案改手写 Want
            Check "$($case.Warn) → $($case.Want)" ((@(Get-GdRequiredPathKeys -Root $one) -join ',') -eq $case.Want)
        }
        Check '空仓库壳子/不存在的目录返回空列表(不抛)' $((@(Get-GdRequiredPathKeys -Root (Join-Path $tmp 'no-such-root'))).Count -eq 0)
    } finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# 单游戏仓库的提示语必须写它自己的参数名(否则用户照着日志敲 -AckArk，入口根本没这个参数)。
# 11-01 丙形真转接(§11.176 ⑥)改口：verb 名不再有"进程内可被现场覆盖的那张表"——取名函数每次现问模块注册表，
# 问不到就回落干净短名。本枚件由 test-ark / test-r1999 点起来，那两本只载 common＋decisions、注册表不在场，
# 所以这里量到的是回落那一支；"注册表在场⇒清单自己声明的长名"那一支由 tools\test-modules.ps1 [m6] 钉：它装载得起来，
# 而且拿入口自己的 param() 做对照，两把尺不同轴 —— 把装载面搬过来只会并成一把，反而问不出新东西。
# 旧那两格(现场覆盖只影响被写的那一款／还原后回到默认)随写侧一起退役：没有那张表就没有"盖了没还原"那一形，
# 这一路不再量它；改量的是名册来源在不在、回落支逐枚逐款是不是短名、短名按被问的 verb 现拼还是写死。
function Show-GdCommonVerbChecks {
    Write-Host "`n[t1c] 提示语里的参数名(现问注册表，问不到回落短名)"
    # 两本名册都现读：verb 名册来自取名函数自己的 ValidateSet(那才是名单事实源)，游戏名册来自 Get-GdGameIds。
    # 写在这里的枚数不作数(旧那句「Sync 一次改的是六格」就是这么一笔陈旧数，§11.176 ⑧ 登记的正是这一形)。
    $verbAttr = @((Get-Command Get-GdEntryVerb).Parameters['Verb'].Attributes |
        Where-Object { $_ -is [System.Management.Automation.ValidateSetAttribute] })
    $verbRoster = @()
    if ($verbAttr.Count) { $verbRoster = @($verbAttr[0].ValidValues) }
    Check '[t1c] verb 名册现读自 Get-GdEntryVerb 的 ValidateSet(断言面跟着它走，不跟注释里的枚数走)' ($verbRoster.Count -ge 3) ($verbRoster -join ' / ')
    $gRoster = @(Get-GdGameIds)
    Check '[t1c] 游戏名册现读自 Get-GdGameIds(注册表不在场时它给本仓库一直认的那几枚 id)' (($gRoster.Count -ge 2) -and (@($gRoster) -contains 'ark')) ($gRoster -join ' / ')
    $dArk = @((Get-GdEntryVerb -Game ark -Verb Ack), (Get-GdEntryVerb -Game ark -Verb ShopReset))
    Check '[t1c] 默认提示用单游戏入口的干净短名' (($dArk -join ',') -eq '-Ack,-ShopReset') ($dArk -join ' , ')
    # 这一格是本枚件的射程边界，不是对机器现场的要求：注册表不在场才走得到回落支。哪天这两本也点 modules.ps1，
    # 这一格会红 —— 那不是坏消息，是"两把尺并成一把"的信号，届时把 [m6] 的对照面搬过来再动这里。
    Check '[t1c] 本进程未装载模块注册表(回落支在此，长名那一支归 [m6] 钉)' ($null -eq (Get-Command Get-GdModules -ErrorAction SilentlyContinue))
    $combos = 0
    $longHits = 0
    foreach ($gid in $gRoster) {
        foreach ($vb in $verbRoster) {
            $combos = $combos + 1
            if ((Get-GdEntryVerb -Game ([string]$gid) -Verb ([string]$vb)) -ne ('-' + [string]$vb)) { $longHits = $longHits + 1 }
        }
    }
    Check '[t1c] 逐枚逐款现问不留一枚长名(注册表不在场时每一格都该是短名)' ($longHits -eq 0) "组合 $combos 枚 / 非短名 $longHits 枚"
    $perWant = @($verbRoster | ForEach-Object { '-' + [string]$_ })
    $perGot = @($verbRoster | ForEach-Object { [string](Get-GdEntryVerb -Game ark -Verb ([string]$_)) })
    Check '[t1c] 回落按被问的 verb 现拼(写死任何一枚就在这里红)' (($perGot -join ',') -eq ($perWant -join ',')) ($perGot -join ' , ')
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
