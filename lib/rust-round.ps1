# rust-round.ps1 — B-4②「入口转接」：把合体入口那一句日常交给核内的十段骨架
<#
  本文件是什么(2026-10-16 落定，判据与批次依据见 docs\rust-core-plan.md §11.64 与其就地补记、§11.65)：
  它是 daily-clear.ps1 与 tools\rustcore\gdcore.exe(--daily-round) 之间的**转接层**，不是第三条日常流程。
  十段的编排、话术、退出码判据全在核内(main.rs 的 daily_round_chain 是事实源)；这里只干三件事——
    ① 把命令行那 13 枚实参装配成 ctx(一轮一定稿，键名册 15 枚逐枚点名)；
    ② 起核、按 ask 回执把**外发那些格**交给这棵树上的真身件去做，再写 res 起下一轮；
    ③ 把 done 里的 result.code 交给入口 exit(真退出码只在这一格，rc 恒 0，两者不许混)。

  ⚠ 外发名册的**枚数随批次变**(10-17 R9-a「四格收编」由 18 收成 13)，唯一口径是下面那枚
  `Get-GdRoundOuterActs`，本头注不抄数(抄了就会变成第二处事实源、且是第一处过期那一处)。
  同批新立的 `--relaunch-entry` 是一枚**可选入程参数**：值现取现交、落 %TEMP%、会话结束即删，
  绝不进 journal／ctx／ask／侧信道；递不到就不带 ⇒ 核那一臂答 None ⇒ 老路照旧(见那两处注释)。

  默认档(2026-10-16 所有者拍板)：**默认走这里**。它原来那一条显式回退路＝PS 十段(lib\modules.ps1 的
  Invoke-GdDailyRound)，已于 2026-10-20 乙批② 连真身一块划掉(退役单元「断兜底」)：拿不到内置件、
  主闸被人拨关、ctx 装配炸 —— 这三种形状现在一律是**整轮拒跑(2)**，这一路不再有第二条腿。
  ⚠ 代价照实在册(§11.105)，两条不对称写在这里而不是藏在日志里：
    · 出货那棵树带内置件 gdcore.exe(进包名单 1 件、过契约号门才进) ⇒ 玩家那一路不受影响；
    · 公开仓按口径是"源码进仓、产物不入仓" ⇒ 拿 git 克隆直跑入口的人从此需要先自己编内置件，
      落点是一句「先跑 tools\rustcore\build.ps1」，不是悄悄换人跑。

  原先「为什么这里不动 Invoke-GdDailyRound 一个字」那一整段随真身一块退场：它讲的是钉位与
  tree-drift，而乙批① 已经把轮骨架门 old 侧换成 git blob、第三侧摘除 ⇒ 改那五份真身不再当场红。

  为什么不复用 lib\ark-daily.ps1 / lib\r1999-daily.ps1 那两枚同名候选序件：那两处的理由(装载顺序
  不许变成隐式依赖)在这里同样成立，且本文件只被 daily-clear.ps1 点，而那两路各自的业务库是**条件装载**的
  (树上没装 1999 就没有 r1999-daily.ps1)。代价照旧登记进 §11.44 那笔"多处一份字面"的欠账：三处。

  ⚠ 本文件里**不许出现任何判据**：判定、话术、三档退出码、崩路语义都在核内；这里只做"取数、装配、回放、回执"。
  多打一行日志就等于同一句日志打两遍(链上每一句由核内侧信道产生，这里只照原级别回放)。
#>

function Get-GdRoundCoreExe {
    <# 内置件 gdcore.exe 的候选序：出货树形态(随包走) → cargo 形态(开发树直跑)。
       两者都不在返回 ''，由调用方处理 —— 与两枚现出口那两处逐字同形(第三份字面，欠账登记见本文件头注)。 #>
    $cands = @(
        (Join-Path $global:GD.Root 'tools\rustcore\gdcore.exe'),
        (Join-Path $global:GD.Root 'tools\rustcore\target\release\gdcore.exe')
    )
    foreach ($c in $cands) { if (Test-Path -LiteralPath $c) { return $c } }
    return ''
}

function Get-GdRoundOuterActs {
    <# **外发 5 格的名册(唯一口径)**。尺子的出处＝§11.64 ③：按"码面动作字面量 × 两路"展开，
       与核内 round_answer 那个 `_ => return Ok(None)` 的注释名单同源；直答那一族不在这里(核内自己答)，
       take_act 那三格(mark/obsrec/admin)不发 ask 因此也不在。
       ⚠ **R9-a(10-17) 把这一族从 18 收成 13**：`round-history:ark`／`round-history:r1999`／`round-probe:ark`／
       `svdetail`／`round-screensaver` 五枚改由核内直答(换答方不换语义，判据与话术仍在 panel-cs 那四枚 verb 里)，
       这里**同批删臂**——两侧只改一边就是"核还出 ask 而壳不认识"＝整轮拒跑(2)，对账由 round 门的应答册钉。
       ⚠ **刀①(2026-11-03) 再收四枚，13→9**：`round-gate:ark`／`round-gate:r1999`／`stop:ark`／`stop:r1999`
       改核内直答(问三枚 set-* verb 与 stop-maa-and-game/stop-m9a(+stop-mumu)，问法与落账形状逐字不动)，
       臂与名册同批摘；closeEmulatorAfter 随 stop 臂回 ROUND_RUNTIME_KEYS(§11.55 ③ 那条取键裁决翻案)。
       ⚠ **刀②(2026-11-03) 再收两枚，9→7**：`round-preview:ark`／`round-preview:r1999` 改核内直答
       (问 apply-maa-daily-config/apply-m9a-instance/ark-find-window 与 read-m9a-connection/mumu-state，
       日志面四段/三段逐字不动)，臂与名册同批摘；模块清单的 Preview 钩子体暂留(摘臂不摘格，1eq.④ 先例)。
       ⚠ **刀③(2026-11-03) 再收两枚，7→5**：`round-resolve:ark`／`round-resolve:r1999` 改核内直答
       (判定面进核：六发问序走判定族十枚 verb——compose-decision-inputs/read-ark-activity(注入缝)/
       decide-ark/apply-ark-daily-intent×2/resolve-ark-farm-decision 与 read-r1999-activity/
       decide-r1999/apply-r1999-daily-intent/r1999-instance/read-m9a-connection，data 形状逐字同 PS 臂)，
       臂与名册同批摘；Resolve 钩子体与 decisions 三本暂留(摘臂不摘格，1eq.④ 先例)。
       ⚠ **`round-elevate` 刻意留在名册里**：它按拍板"全收编"走 `--relaunch-entry` 那枚**可选**入程参数，
       缺参数／文件读不动／形状坏 ⇒ 核答 `None` ⇒ 照旧出 ask(§11.66 那条"恒绿的判据就是没验过的判据"的正面用法：
       入程通道是新立的，必须留一条"没递到就退回老路"的落点，而不是把它升级成整轮拒跑)。
       白名单之外收到的一律当场拒跑("先改 main.rs 再改这里，不许放行"——两枚现出口那条老规矩)：
       两侧有一边漏改，宁可这一轮没跑，也不许把不认识的动作当成成功回执吞掉。
       ⚠ 不要把驱动档那十四枚白名单(lib\ark-daily.ps1)当这枚名册用，那是另一族的数(§11.64 ③ 倒数第二段)。 #>
    return @(
        'round-elevate', 'round-supervisor-start', 'round-supervisor-stop',
        'updgate', 'audit'
    )
}

function Test-GdRustRoundPath {
    <# 主闸(乙批② 起只剩这一把)：读的就是"内置件整体上不上路径"那两枚现成的键，与 lib\common.ps1 里
       那把已随 10-24 刀④ 出仓的换头主闸同源同判据($env:GD_RUST_OFF=1、runtime.rustOff 含 'gdcore')；轮档不许越过主闸。
       ⚠ 这里**不再读** $env:GD_RUST_ROUND_OFF 与 runtime.rustRoundOff：那两枚开关的内容是"退回 PS 十段"，
       十段真身已随乙批② 划掉 ⇒ 键留着就是两处"拨了没反应"的骗人配置，比没有键更坏。
       (判据变更本要拍板，这一条的所有者决定就是 10-20 那句「两扇都要，按乙→丙排」里的乙档。)
       返回 $false 的语义也从"走原身"换成"这一轮拒跑(2)"(见 Invoke-GdDailyRoundDispatch)：
       有人拨了"不许起内置件"、而树上已没有第二条腿 ⇒ 只能停下来把话说清楚，不许嘴上答应、手上照样起核。
       缺键/形状坏一律当**不关**(缺省＝起核)，与那把已出仓的换头主闸同形(缺键宽容)；
       真正的现场保险不在这把闸上：拿不到内置件或判定 exe 时 dispatch 自己会拒跑(不是跳过、不是假成功)。 #>
    param($S)
    try { if ($env:GD_RUST_OFF -eq '1') { return $false } } catch { }
    try {
        if ($S -and $S.runtime) {
            $off = @()
            if ($S.runtime.rustOff) { $off = @($S.runtime.rustOff | ForEach-Object { [string]$_ }) }
            if ($off -contains 'gdcore') { return $false }
        }
    } catch { }
    return $true
}

function Get-GdRoundMaaRunPayload {
    <# ctx 的 maaRun 那一格：与 lib\ark-daily.ps1 里 executor 写 ctx 的那枚 maaRun **同一名册**(paths 六枚＋runtime 七枚)，
       因为核内用的是同一把解析器(maarun_parse)，键名与 config\settings.json 逐字同名。
       **取键逐枚点名是硬约束**：$S.runtime 整块里住着视觉兜底的 API key(visionApiKey)，
       序列化进来等于把 key 递进核、递进 ctx/journal 两本会留盘的账。
       benignMaaErrors 用 List[string] 累加而不是 @(...)：那枚键没填时 @($null) 会占成一条空串错误项(在案老坑)。 #>
    param([Parameter(Mandatory)]$S)
    $benign = New-Object System.Collections.Generic.List[string]
    foreach ($bm in $S.runtime.benignMaaErrors) { $benign.Add([string]$bm) }
    return [pscustomobject]@{
        paths = [pscustomobject]@{
            maaDir       = [string]$S.paths.maaDir
            maaExe       = [string]$S.paths.maaExe
            maaConfig    = [string]$S.paths.maaConfig
            maaGuiLog    = [string]$S.paths.maaGuiLog
            maaAsstLog   = [string]$S.paths.maaAsstLog
            arknightsExe = [string]$S.paths.arknightsExe
        }
        runtime = [pscustomobject]@{
            arkStartTimeoutMinutes = [int]$S.runtime.arkStartTimeoutMinutes
            arkTimeoutMinutes      = [int]$S.runtime.arkTimeoutMinutes
            stallTimeoutMinutes    = [int]$S.runtime.stallTimeoutMinutes
            idleConfirmSeconds     = [int]$S.runtime.idleConfirmSeconds
            abortOnMaaStallWarning = [bool]$S.runtime.abortOnMaaStallWarning
            visionMaxRescue        = [int]$S.runtime.visionMaxRescue
            benignMaaErrors        = $benign
        }
    }
}

function Get-GdRoundM9aRunPayload {
    <# ctx 的 m9aRun 那一格：名册与 lib\r1999-daily.ps1 里 `Invoke-GdR1999DailyRust` 的取键段同把(paths 三枚＋conn 两枚＋instance 两枚＋package＋runtime 六/七枚；10-24 刀② 起回退臂出仓、那一名册只剩 executor 这一份，按坐标清账批口径不引行号)。
       ⚠ **这一格给不出"真值"，只能给"不许退化的回落档"**，这是 B-4② 实测推翻题面的那一结(§11.65)：
         · instance —— 真身是判定层按方案挑的(lib\r1999-daily.ps1:83)，方案键名册里根本没有 default，
           在这里猜一枚＝替判定层做决定 ⇒ 写空串，由核内段 5 拿判定回执就地覆盖(round_m9a_apply_decision)。
         · conn.serial —— 由实例配置派生(Get-M9AConnection 读那份实例 json)，实例没定下来之前无从取 ⇒ 同样留空、段 5 覆盖。
         · conn.adb —— **不许留空**：核内 m9a_scen_core 的硬闸就是 m9aDir/adb 为空即 Err，
           而轮档的解析取的是**宽松**(main.rs 那段注释写明：合体轮可能压根没装那一路)，Err 之后整份载荷
           退化成默认体(paths/package/超时全丢)——段 5 只补 instance/conn 两格，补不回其余。
           回落档取 (Get-GdMuMu).Adb：与真身 Get-M9AConnection 缺 AdbPath 时**同一处来路**、纯路径算术、
           不要求任何现场状态(不起进程、不读实例配置)，正是"一轮一定稿"时刻唯一拿得住的那一枚。
       m9aCliArgs 走三态：键没写＝真身"没填"档＝开命令行入口 ⇒ $null 时**干脆不加键**，不许塌成 false。 #>
    param([Parameter(Mandatory)]$S)
    $adb = ''
    if (Get-Command Get-GdMuMu -ErrorAction SilentlyContinue) {
        try { $adb = [string](Get-GdMuMu -S $S).Adb } catch { $adb = '' }
    }
    $rt = [ordered]@{
        m9aInstanceArg           = [string]$S.runtime.m9aInstanceArg
        m9aForceStart            = [bool]$S.runtime.m9aForceStart
        visionMaxRescue          = [int]$S.runtime.visionMaxRescue
        r1999StartTimeoutMinutes = [int]$S.runtime.r1999StartTimeoutMinutes
        r1999TimeoutMinutes      = [int]$S.runtime.r1999TimeoutMinutes
        stallTimeoutMinutes      = [int]$S.runtime.stallTimeoutMinutes
    }
    if ($null -ne $S.runtime.m9aCliArgs) { $rt.Add('m9aCliArgs', [bool]$S.runtime.m9aCliArgs) }
    return [pscustomobject]@{
        paths    = [pscustomobject]@{
            m9aDir    = [string]$S.paths.m9aDir
            m9aExe    = [string]$S.paths.m9aExe
            m9aLogDir = [string]$S.paths.m9aLogDir
        }
        conn     = [pscustomobject]@{ adb = $adb; serial = '' }
        instance = [pscustomobject]@{ id = ''; name = '' }
        package  = [string]$S.re1999.package
        runtime  = [pscustomobject]$rt
    }
}

function Get-GdRoundCtx {
    <# ctx 名册 15 枚(核内 round_ctx_parse 逐枚点名的那 15 枚)，**一轮一定稿**：
       journal 的 init 与 ctx 做全等校验，换 ctx 就得起新 journal ⇒ 事后补不进任何一格，
       段 5 才定得下来的那两格(1999 的实例/adb 落点)只能靠核内就地覆盖，这是 B-4② 需要一处核内改造的全部原因。
       pid 一枚必填、且**必须由壳写**($PID：撤锁只撤自己登记那一份，核自己的进程号在第一发 ask 之后就不是它了)。 #>
    param(
        [Parameter(Mandatory)]$S,
        [AllowNull()][string[]]$Ids = @(),
        [AllowNull()][array]$Gates = @(),
        [hashtable]$Opt = @{},
        [bool]$Force = $false,
        [bool]$DryRun = $false,
        [bool]$SkipCleanup = $false,
        [bool]$NoElevate = $false,
        [string]$NameField = 'Id',
        [string]$NoticeOwner = '每日',
        [string]$Tag = 'manual'
    )
    $idList = New-Object System.Collections.Generic.List[string]
    if ($null -ne $Ids) { foreach ($x in @($Ids)) { $idList.Add([string]$x) } }
    $gList = New-Object System.Collections.Generic.List[object]
    foreach ($g in @($Gates)) {
        if (-not $g) { continue }        # 真身 `$Gates | Where-Object { $_ }`：空项静默滤，不算错
        $gList.Add([pscustomobject]@{ id = [string]$g.Id; verb = [string]$g.Verb })
    }
    $ob = [ordered]@{}
    foreach ($k in @($Opt.Keys)) { $ob[[string]$k] = $Opt[$k] }
    $logFile = ''
    try { $logFile = [string]$global:GD.LogFile } catch { $logFile = '' }
    # Split-Path -Leaf 对空串会当场抛("无法将路径为空")—— 没日志文件这件事本身不该炸掉 ctx 装配
    $logLeaf = ''
    if ($logFile) { $logLeaf = [string](Split-Path -Leaf $logFile) }
    return [pscustomobject]@{
        ids          = $idList
        gates        = $gList
        opt          = [pscustomobject]$ob
        force        = [bool]$Force
        dryRun       = [bool]$DryRun
        skipCleanup  = [bool]$SkipCleanup
        noElevate    = [bool]$NoElevate
        nameField    = [string]$NameField
        noticeOwner  = [string]$NoticeOwner
        tag          = [string]$Tag
        logLeaf      = $logLeaf
        logPath      = $logFile
        pid          = [long]$PID
        maaRun       = Get-GdRoundMaaRunPayload -S $S
        m9aRun       = Get-GdRoundM9aRunPayload -S $S
    }
}

function Start-GdRoundHostNoSleep {
    <# 宿主侧的睡眠屏蔽(返回答yes/no，不打日志——那句"已临时禁止系统睡眠"由核内骨架说，这里再说一遍就是打两遍)。
       为什么必须有这一枚：SetThreadExecutionState 的有效期是**调用进程活着的那段时间**，而驱动协议一 ask 一进程 ——
       核内段 8 那次直答设的位在 gdcore 退出时就散了。真身那一版里设位的是 daily-clear 自己这个进程，
       它整轮都在 ⇒ 屏蔽真的生效。转接之后这个"谁在设"的事实变了，只能在壳侧补回来，否则 -BlockSleep 变成一句空话。
       落点选在**看到 round-supervisor-start 那一发 ask 之后**：真身的顺序正是段 7 监督 → 段 8 屏蔽，
       而段 1-7 之前会出结果的三档(env 不完整／门只记录／判定炸／提权重启)在真身里都不设位 ⇒ 用名册里
       这一段唯一的锚当触发点，dry-run 与门那一档就自然不会被误设。 #>
    param([Parameter(Mandatory)]$S)
    try {
        if (-not $S.runtime.blockSleep) { return $false }
        Add-GdSleepTypeDef
        $flags = [GdPower]::ES_CONTINUOUS -bor [GdPower]::ES_SYSTEM_REQUIRED
        if ($S.runtime.keepScreenOn) { $flags = $flags -bor [GdPower]::ES_DISPLAY_REQUIRED }
        [void][GdPower]::SetThreadExecutionState($flags)
        return $true
    } catch { return $false }
}

function Stop-GdRoundHostNoSleep {
    <# 撤位只在真设过时做(Exit-GdNoSleep 那句 $global:GdSleepHandle 的语义)；撤失败一句都不吭(真身整枚 catch{})。 #>
    param([bool]$WasSet)
    if (-not $WasSet) { return }
    try { [void][GdPower]::SetThreadExecutionState([GdPower]::ES_CONTINUOUS) } catch { }
}

function Invoke-GdDailyRoundRust {
    <# 轮档执行器：问-答-重放的驱动循环。返回**本轮退出码**(0/1/2)，由入口 exit；
       拒跑那一族不裸 throw，一律走 Stop-GdBeforeRun 的人面形状(一句"这一轮没跑：…"+退 2) ——
       这一档的调用方是入口自己，没有人在上面 catch，裸 throw 到顶层就是一屏 "Exception: rust-round.ps1:…"。
       回执形状是协议：ok 真不许带 err 键、假不许带 data 键(main.rs drive_merge_res 逐字校验)。
       前后缀话术由核内拼装(「手动门没敲进去(id verb)：」「…这一路今天判定不出来：」「…的预览没做成：」
       「活动图自证没做起来：…(不影响本轮)」)，这里**只回异常原文**，不许把话术抄一份到壳侧。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$ElevateScript,
        [hashtable]$BoundParams = @{},
        [string]$CoreExe = '',
        [string]$PanelCs = ''
    )
    # 两枚内置件由**调用方**(dispatch)定妥才进来：那一边负责"拿不到就退原身"那一档，
    # 走到这里还缺就是内部装配漏了 —— 这一处只把话说死，不再自己找第二遍(两处各找一次会给出两个不同答案)。
    if (-not $CoreExe -or -not $PanelCs) { Stop-GdBeforeRun '转接层装配漏了内置件或判定 exe 的落点(内部错，非现场问题)' }
    $exe = $CoreExe
    $panel = $PanelCs
    $selfEngine = (Get-Process -Id $PID).Path    # 两处外挂子进程(更新闸/审计闸)共用当前引擎自己，与两枚现出口同一句

    # 一次日常 = 一本账：ctx 换了 journal 就不能复用(内核按 init 与 ctx 全等校验)，每次开头清场重起。
    $journal = Join-Path $global:GD.State 'rust-round-journal.ndjson'
    $ctxFile = Join-Path $global:GD.State 'rust-round-ctx.json'
    $resFile = Join-Path $global:GD.State 'rust-round-res.json'
    $sink = Join-Path $global:GD.State 'rust-round-log-sink.ndjson'
    foreach ($f in @($journal, $resFile, $sink)) { if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath $f -Force } }
    # 侧信道一本会话一个文件(核侧只追加不截断)：偏移在这里归零，之后由 Read-GdSinkIncrement 按**字节**推进。
    # 一律用 long：[ref] 会记住初值的类型，这里留 int 以后写 long 会被往回截(偏移算错=同一句日志重放)。
    $sinkOffset = [long]0
    Write-GdJson -Path $ctxFile -Object $Ctx

    # R9-a：提权那一发的**入程参数**(可选)。拼参语义仍住 PS 侧(`Get-GdRelaunchArgList` 是事实源、不搬)，
    #   这里只把"入口脚本路径＋已经拼好的参数数组"写成一份临时 JSON 交给核，核原样转交 start-elevated-round。
    # ⚠ 这一份文件里是本机绝对路径 ⇒ 落 `%TEMP%`、会话结束即删，**绝不进 journal／ctx／ask 信封／侧信道**
    #   (与核内那句"Paths 的值绝不进 journal"同源)。
    # ⚠ 拼不出来就不交这一枚：核那一臂答 `None` ⇒ 照旧出 ask ⇒ 下面那臂代答(老路一字未动)，
    #   把"入参没递到"升级成整轮拒跑是把一条降级通道拆成一条硬依赖，那一刀本批不许。
    $entryFile = ''
    try {
        $entryArgs = @(Get-GdRelaunchArgList -Script $ElevateScript -Bound $BoundParams -Force @('-NoElevate'))
        $entryFile = Join-Path ([System.IO.Path]::GetTempPath()) ('gd-round-entry-' + [guid]::NewGuid().ToString('N') + '.json')
        Write-GdJson -Path $entryFile -Object ([ordered]@{ Script = [string]$ElevateScript; ArgList = $entryArgs })
    } catch {
        $entryFile = ''
        Write-GdLog "提权入参没拼出来($($_.Exception.Message)) → 本轮的 round-elevate 照旧由转接层代答" -Level WARN
    }

    # 外层硬闸(10-20，与两枚现出口同源同形)：这一档手上**没有**专用的"round 超时"，取两路业务超时里宽的那枚 +15。
    # 转接层一段最多做的现场动作不会超过单路那一路的内层超时 ⇒ 外层只会比它更宽、不抢判据；
    # 又必须留在计划任务 PT3H 之内，否则外层闸形同虚设(宽的那枚是 1999 那一路，加 15 分钟仍在上限之内)。
    # ⚠ 故意不把两枚键的现值抄进注释 —— 它们是活账(10-20 上午"取数帧"那笔账的反方向)。
    # 两枚键都没填时兜底 85，不许兜底成 0(那等于 15 分钟就砍)。
    $hardMin = [int]$S.runtime.arkTimeoutMinutes
    $m9aMin = [int]$S.runtime.r1999TimeoutMinutes
    if ($m9aMin -gt $hardMin) { $hardMin = $m9aMin }
    if ($hardMin -le 0) { $hardMin = 85 }
    $hardMs = ($hardMin + 15) * 60000
    $outer = @(Get-GdRoundOuterActs)
    $esHeld = $false
    $round = 0
    try {
        while ($true) {
            $round += 1
            if ($round -gt 500) { Stop-GdBeforeRun "转接层驱动轮次超上限(疑似死循环)：journal=$journal" }
            # 信封落点每轮换新文件，防读到上一轮读数；写不下去时 gdcore 退 stdout 末行(内核侧既定纪律)
            $envFile = Join-Path ([System.IO.Path]::GetTempPath()) ('gd-round-env-' + [guid]::NewGuid().ToString('N') + '.json')
            try {
                $coreArgs = @(
                    '--daily-round', '--i-round', '--journal', $journal, '--ctx', $ctxFile,
                    '--res', $resFile, '--panel-cs', $panel, '--state', $global:GD.State,
                    '--log-sink', $sink, '--root', $global:GD.Root, '--out', $envFile
                )
                # 第八道闸之外的都是**可选**参数：这一枚不带 ⇒ 核那一臂答 None ⇒ 照旧 ask(见上面入参那段)
                if ($entryFile) { $coreArgs = $coreArgs + @('--relaunch-entry', $entryFile) }
                $cap = Start-GdNativeStreamed -Exe $exe -SinkPath $sink -SinkOffset ([ref]$sinkOffset) -Arguments $coreArgs -OnSinkRow {
                    param($r)
                    $lvl = [string]$r.Level
                    $txt = [string]$r.Text
                    if ($lvl -and ($lvl -ne 'INFO')) { Write-GdLog $txt -Level $lvl } else { Write-GdLog $txt }
                } -HardWaitMs $hardMs
                $envText = ''
                if ([int]$cap.StreamTimeouts -gt 0) {
                    # 口径照两枚现出口(lib\ark-daily.ps1 那处 #473 定档)：这一格每轮必现、不是异常也不是回归，
                    # 持端的是外部常驻服务(adb server 一类)，真日志走 --log-sink、进度判据走 journal ⇒ INFO 一行。
                    Write-GdLog "转接层：收流按有界等待放弃 $([int]$cap.StreamTimeouts) 路 stdout/stderr(外部常驻服务持写端，stdout 兜底那一路本轮不可用；真日志见 --log-sink)"
                }
                if ($cap.WaitTimedOut) {
                    # 第三种病(10-20，与两枚现出口同形)：核没退出、被外层硬闸收尸。这一支往下必然走
                    # "没给出信封"那条拒跑(Stop-GdBeforeRun 的人面形状)，所以本轮**有行可归因**而不是无声缺勤。
                    Write-GdLog "转接层：核 $([int]($hardMs / 1000))s 仍没退出，已按外层硬闸结束它(它是没答，不是答错) —— 本轮按失败收" -Level WARN
                }
                if (Test-Path -LiteralPath $envFile) { $envText = [System.IO.File]::ReadAllText($envFile) }
                else {
                    foreach ($ln in @($cap.Lines)) { $t = ([string]$ln).TrimStart(); if ($t.StartsWith('{')) { $envText = $t } }
                }
                if (-not $envText) {
                    Stop-GdBeforeRun "转接层：gdcore 没给出信封(rc=$($cap.ExitCode))；输出末三行：$((@($cap.Lines) | Select-Object -Last 3) -join ' / ')"
                }
                $envelope = $envText | ConvertFrom-Json
                if ([string]$envelope.role -ne 'B3b-round-daily') {
                    Stop-GdBeforeRun "转接层：信封 role 不对($($envelope.role)) —— 这枚 gdcore.exe 不是轮骨架档那件？"
                }
                $want = Get-GdRustContract
                if ([string]$envelope.contract -ne $want) {
                    Stop-GdBeforeRun "转接层：gdcore 契约号($($envelope.contract))与壳侧期望($want)不一致 —— 先重跑 tools\rustcore\build.ps1 再跑本轮"
                }
                # 链上每一句已在上面边跑边从侧信道按原级别回放完毕；信封 logs 键留而不读(B-2c② 起恒为空)，读两遍就是打两遍。
                $kind = [string]$envelope.kind
                if ($kind -eq 'done') {
                    $dr = $envelope.result
                    # 真身那句 Write-Host $summary(reachedSummary 为假的那几档＝没走到汇总，本来就不该印)
                    if ([bool]$dr.reachedSummary) { Write-Host ([string]$dr.summary) }
                    return [int]$dr.code
                }
                if ($kind -eq 'fail') {
                    # Fail 信封＝核内**进十段之前**那两句 throw(段 1 选人／段 2 之前 ctx 坏)，
                    # 真身对应的形状是"terminating error 冒到入口 catch ⇒ 一句 ERROR ＋ 退 1"(§11.64 ⑥ 末段、
                    # 门的 X1 里 bad-id／unregistered 两档的 Z|1| 就是它)——**不是** 2 号"这一轮没跑"。
                    # 转接层若在这里走 Stop-GdBeforeRun，就把"点了没注册的那一路"从 1 悄悄升成 2 ⇒ 计划任务
                    # 的告警口径跟着变。所以这一格只把原文照原级别落一行，再把 1 交回入口去 exit。
                    Write-GdLog ([string]$envelope.error) -Level ERROR
                    return 1
                }
                if ($kind -ne 'ask') { Stop-GdBeforeRun "转接层：信封 kind 不认识($kind) —— 协议两侧不同源？" }

                $seq = [int]$envelope.seq
                $act = [string]$envelope.action
                $prm = $envelope.params
                if ($act -notin $outer) {
                    # 白名单之外走拒跑通道而不是 throw：不认识的动作＝协议不同源，既不许被当成"动作失败"吞掉，
                    # 也不该给敲日常的人一屏内部报错(§11.64 ⑦：转接之后答错的典型形状就是"整轮拒跑")。
                    Stop-GdBeforeRun "转接层：收到不认识的动作 '$act'(seq=$seq) —— 先改 main.rs 再改这里，不许放行"
                }
                $ok = $true
                $data = $null
                $err = $null
                try {
                    switch ($act) {
                        'round-elevate' {
                            # R9-a 之后这一臂是**回落通道**：入程参数递到了就由核内直答(改调 start-elevated-round，
                            # 回执只取 Code)，没递到(拼参炸／文件坏)才走到这里 ⇒ 老路一字未动。
                            # 入口路径/命令行那半仍是外侧的事(本机路径不进信封)：重跑的还是入口自己。
                            $ec = Start-GdElevatedSelf -Script $ElevateScript -Bound $BoundParams -Force @('-NoElevate')
                            $data = [pscustomobject]@{ Code = [int]$ec }
                        }
                        'round-supervisor-start' {
                            # ── 监督族刀①(2026-11-02 接线)：这一臂现问宿主批的 start-supervisor(真身 tools\panel-cs\Supervisor.Host.cs::Start)──
                            # 刀③(同日)把回落臂摘净：它调的 Start-GdSupervisor 真身随 lib\supervisor.ps1 退役，那三段自此是死臂 ——
                            #   留着不叫兜底，是把"核不可达"这一档悄悄交给一枚已经不在场的 PS 命令去答。
                            # 回执形状一字未变：还是 Available/Err 两枚键，核侧段 7 的读法照旧(main.rs 那句「Available 缺席按在处理」不动)。
                            # Available=false 那一档换了答方：改指前答的是"PS 命令在不在场"，现在答的是"核不可达 ⇒ 这一轮没有监督"。
                            #   两档读数在人面上同形 —— 核段 7 只在 Available 为真且 Err 非空时补一句 WARN，而这两档的 Err 都恒空，
                            #   所以那句人话仍由写手侧落盘，读数以盘上那条为准(这条判语从刀① 继承，不随臂一起摘)。
                            # 信封仍只喂四枚 runtime 原始值(与退役前 Get-GdSupervisorSettingsEval 同一把名册)，
                            #   整枚 settings 不进信封——「取键逐枚点名、visionApiKey 不许进核」那条判据也管这条通道。
                            $core = $null
                            try {
                                $core = Invoke-GdCli -Verb 'start-supervisor' -Body ([pscustomobject]@{
                                    settings = [pscustomobject]@{ runtime = [pscustomobject]@{
                                        supervisor                 = $S.runtime.supervisor
                                        supervisorIntervalSeconds  = $S.runtime.supervisorIntervalSeconds
                                        supervisorQuietMinutes     = $S.runtime.supervisorQuietMinutes
                                        supervisorMaxInterventions = $S.runtime.supervisorMaxInterventions
                                    } }
                                    games    = [string]$prm.games
                                    ownerPid = [int]$PID
                                })
                            } catch { $core = $null }
                            if ($null -ne $core) {
                                $data = [pscustomobject]@{ Available = [bool]$core.Available; Err = [string]$core.Err }
                            } else {
                                $data = [pscustomobject]@{ Available = $false; Err = '' }
                            }
                            # 段 7 是段 8 的唯一锚：真身的顺序就是"监督起来 → 设睡眠屏蔽"。
                            if (-not $esHeld) { $esHeld = Start-GdRoundHostNoSleep -S $S }
                        }
                        'round-supervisor-stop' {
                            # 这一臂现问 stop-supervisor(真身 Supervisor.Host.cs::Stop，宽限 75 秒与退役前 Stop-GdSupervisor 的默认值同尺)；
                            # 回落臂同刀③ 摘净，Available=false 那一档的语义差同 start 那一臂的注释。这一枚两档都没有"开关"那一说。
                            $core = $null
                            try {
                                $core = Invoke-GdCli -Verb 'stop-supervisor' -Body ([pscustomobject]@{})
                            } catch { $core = $null }
                            if ($null -ne $core) {
                                $data = [pscustomobject]@{ Available = [bool]$core.Available; Err = [string]$core.Err }
                            } else {
                                $data = [pscustomobject]@{ Available = $false; Err = '' }
                            }
                        }
                        # ---- 刀① 收编(2026-11-03)：`round-gate:<id>`／`stop:<id>` 四枚臂已摘——核内 round_answer
                        #      直答(问法与落账形状逐字不动，data 恒 $true)，这里不再有"壳替核答"的那一截。
                        #      ⚠ **一次删两侧**：main.rs 的 `round_answer` 若还答 None 而这里留着臂，死臂会遮住
                        #      "收到不认识的动作"拒跑；反过来核先答了这里还问，ask 会走拒跑 ⇒ 整轮没跑(2)。
                        #      这条对账由核内 --selftest 第 28 节的 S28_ROSTER（7→5，刀③ 起；与下面的名册同数）与
                        #      R-gate-call 格（门格的 set-* 直答消费）钉着，不靠人记得。
                        #      ⚠ **刀③(2026-11-03) 再摘 `round-resolve:<id>` 两臂**：判定面进核——两臂的
                        #      六发问序（ark＝compose-decision-inputs→read-ark-activity(注入缝)→decide-ark→
                        #      apply-ark-daily-intent×2 夹 resolve-ark-farm-decision；r1999＝compose→
                        #      read-r1999-activity(paths 直答)→Ready 分档→apply-r1999-daily-intent→
                        #      r1999-instance(planToInstance 前置核臂照做)→read-m9a-connection(不许抛)）
                        #      与 Conn 平铺形状逐字搬进 main.rs，这里不再有"壳替核答"的那一截。
                        # ---- 刀② 收编(2026-11-03)：`round-preview:ark`／`round-preview:r1999` 两臂同批摘——
                        #      直答臂在核内问 apply-maa-daily-config(不给 apply ⇒ 预览档)／apply-m9a-instance(apply:false)
                        #      ＋ark-find-window／read-m9a-connection／mumu-state，日志面四段/三段逐字同 PS 臂形状；
                        #      模块清单的 Preview 钩子体暂留(摘臂不摘格，去留归后续刀)。
                        # ---- 刀① 收编(2026-11-03)：`stop:ark`／`stop:r1999` 两臂同批摘——直答臂在核内问
                        #      stop-maa-and-game／stop-m9a(+stop-mumu)，Lines 回放与 data 恒 true 同 PS 臂形状；
                        #      模块清单的 Stop 钩子体暂留(1eq.④ 摘臂不摘格，去留归后续刀)。
                        'updgate' {
                            # 回退段那一段的"活"(起更新器子进程)原样搬来，两句闸话由链体按 code 三态决定；
                            # 挑行与全量拼串两个口径照回退段逐字同形，否则同一轮换头前后日志对不上。
                            $gOut = @(& $selfEngine -NoProfile -ExecutionPolicy Bypass -File (Join-Path $global:GD.Root 'tools\m9a-update.ps1') -Apply 2>&1)
                            $gCode = $LASTEXITCODE
                            $gLine = (@($gOut | Where-Object { [string]$_ -match '已是最新|更新完成' }) | Select-Object -Last 1)
                            $data = [pscustomobject]@{
                                code   = [int]$gCode
                                line   = [string]$gLine
                                joined = ((@($gOut | ForEach-Object { [string]$_ })) -join ' / ')
                            }
                        }
                        'audit' {
                            # 核内已按状态名册判过"只在 ok/empty-queue 才问"，这里只管起工具与读数。
                            $gOut = @(& $selfEngine -NoProfile -ExecutionPolicy Bypass -File (Join-Path $global:GD.Root 'tools\1999-event-audit.ps1') -Apply 2>&1)
                            $gCode = $LASTEXITCODE
                            $gLine = (@($gOut | ForEach-Object { [string]$_ }) | Where-Object { $_ -match '审计结论|无事发生|维持 event-pending' } | Select-Object -Last 1)
                            $data = [pscustomobject]@{
                                code   = [int]$gCode
                                line   = [string]$gLine
                                joined = ((@($gOut | ForEach-Object { [string]$_ })) -join ' / ')
                            }
                        }
                    }
                } catch {
                    $ok = $false
                    $err = $_.Exception.Message
                }
                $rf = [ordered]@{ seq = $seq; action = $act; ok = $ok }
                if ($ok) { $rf.data = $data } else { $rf.err = $err }
                Write-GdJson -Path $resFile -Object $rf
            } finally {
                if (Test-Path -LiteralPath $envFile) { Remove-Item -LiteralPath $envFile -Force }
            }
        }
    } finally {
        Stop-GdRoundHostNoSleep -WasSet $esHeld
        # 入程参数那一份带本机绝对路径，会话一结束就删(删失败一句不吭：那是 %TEMP% 的残渣，不是本轮的结论)
        if ($entryFile -and (Test-Path -LiteralPath $entryFile)) { try { Remove-Item -LiteralPath $entryFile -Force } catch { } }
    }
}

function Invoke-GdDailyRoundDispatch {
    <# 调度器(乙批② 起只有一条腿)：入口链不动、参数面一字不加，它只做四件事——
       读主闸、定位内置件与判定 exe、装配 ctx、把整轮交给 Invoke-GdDailyRoundRust。
       返回退出码(0/1/2)，三分语义与核内 done 信封的 result.code 同源 ⇒ 入口那句 exit $code 不必改。
       ⚠ 参数表**必须与核内 daily_round 那 13 枚实参一一对应**(入口转接批立的那族锚验的就是这件事)，
         差别只在下面两枚门/自检专用注入档：
         -CoreExe/-PanelCs 让轮骨架门的转接层侧能喂假内置件与假判定 exe(生产永不带，同 --fixture-* 那一族)。
       ⚠ 原「拿不到内置件就落一句 WARN、再逐字走 PS 原身十段」那一档与「显式拨关走原身」那一档，
         已于 2026-10-20 乙批② 一块换成**拒跑**(Stop-GdBeforeRun：一句人话 + 退 2，不是一屏内部报错)。
         "内置件永远不是硬依赖"那句老话在**轮档这一路**不再成立(单游戏那两路的 legacy 照旧在)；
         代价、受影响的形态(公开仓 git 克隆侧)与两枚退场的开关都写在本文件头注。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][string]$ElevateScript,
        [hashtable]$BoundParams = @{},
        [AllowEmptyCollection()][AllowNull()][array]$Gates = @(),
        [string[]]$Ids = @(),
        [hashtable]$Opt = @{},
        [switch]$Force,
        [switch]$DryRun,
        [switch]$SkipCleanup,
        [switch]$NoElevate,
        [ValidateSet('Id', 'ShortName')][string]$NameField = 'Id',
        [string]$NoticeOwner = '每日',
        [string]$Tag = 'manual',
        [string]$CoreExe = '',
        [string]$PanelCs = ''
    )
    if (-not (Test-GdRustRoundPath -S $S)) {
        Stop-GdBeforeRun '主闸把内置件拨下了线(GD_RUST_OFF=1 或 runtime.rustOff 含 gdcore)，而轮档这条路的 PS 十段兜底已退役 ⇒ 本轮不跑(要把日常交回内置件就把主闸拨回来)'
    }
    $core = $CoreExe
    if (-not $core) { $core = Get-GdRoundCoreExe }
    $panel = $PanelCs
    if (-not $panel) { try { $panel = Get-GdCliExe } catch { $panel = '' } }
    if (-not $core -or -not $panel) {
        $miss = '内置件 gdcore.exe'
        if (-not $core -and -not $panel) { $miss = '内置件 gdcore.exe 与判定 exe' }
        elseif (-not $panel) { $miss = '判定 exe' }
        Stop-GdBeforeRun "转接层拿不到${miss} ⇒ 本轮不跑(开发树先跑 tools\rustcore\build.ps1 出内置件；用 git 克隆的公开仓按口径不带编译产物，这一路从乙批② 起没有 PS 十段兜底)"
    }
    $ctx = $null
    try {
        $ctx = Get-GdRoundCtx -S $S -Ids $Ids -Gates $Gates -Opt $Opt -Force ([bool]$Force) -DryRun ([bool]$DryRun) `
                              -SkipCleanup ([bool]$SkipCleanup) -NoElevate ([bool]$NoElevate) -NameField $NameField -NoticeOwner $NoticeOwner -Tag $Tag
    } catch {
        # ctx 装配炸＝连"这一轮要问哪些格"都没定下来，比真身的"判定不出来"更早 ⇒ 同样走拒跑通道，别一屏内部报错
        Stop-GdBeforeRun "本轮的轮上下文没装配起来：$($_.Exception.Message)"
    }
    return Invoke-GdDailyRoundRust -S $S -Ctx $ctx -ElevateScript $ElevateScript -BoundParams $BoundParams -CoreExe $core -PanelCs $panel
}
