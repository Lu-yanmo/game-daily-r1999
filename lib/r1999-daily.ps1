# lib/r1999-daily.ps1 — 重返未来：1999 这一路的"日常动作"：状态标记 / 方案判定 / dry-run 预览 / 真正跑一轮
#
# 与 lib\ark-daily.ps1 同一批拆分产物，搬动规则和那里注释里写的完全一致
# (闭包 $S 改成显式参数；结果 return 给入口脚本，不再直接写 $results)。
# 公开命令行(-Ack1999 / -RShopDone / -RPlan / -Force)仍然长在 daily-clear.ps1 里。
#
# 依赖：调用方先点 lib\common.ps1、lib\r1999-decisions.ps1(或聚合的 lib\decisions.ps1)、
#       lib\m9a.ps1、lib\mumu.ps1。本文件不重复点它们，理由见 lib\decisions.ps1 开头。
#
# ── 冻结(2026-09-27，PS→C# 批次 5)────────────────────────────────────────────
# Resolve-GdR1999Decision 的**构造段规则**(-Force 跳确认门、-Plan≠auto 无条件覆盖、两段文案与键序)
# 已迁到 tools\panel-cs\Orchestration.cs(verb：apply-r1999-daily-intent；契约见 docs\ps-migration-batch0.md)。
# 壳里留的是"活"：Get-R1999Event 取数、Resolve-R1999Instance 实例解析、"1999 方案：…"日志行与返回形状；
# Set-Gd*Ack/ShopState 与 Start-GdR1999Daily 整函数留壳(起进程/切实例归后续批次)。
# 改规则请改 C# 并跑 tools\probe-orch-golden.ps1 对拍，不要在这里加规则。
# ──────────────────────────────────────────────────────────────────────────────
#
# ── 追加冻结(2026-09-27，PS→C# 批次 8)─────────────────────────────────────────
# Start-GdR1999Daily 的 $retryable 分流(五状态跳过清单 + "画面被兜清楚才翻案"的 or 支、
#  retryOnce 合取、if/elseif 链的落穿语义与四句文案)已迁到 tools\panel-cs\Orchestration.cs
#  (verb：decide-daily-retry，-Game r1999)。壳里留的正是"活"：Stop-M9A、sleep 20、重跑 $run9、
#  日志回放。旧判据段在分流处逐行 # 前缀冻结(test-r1999 钉 $retryable 表达式与话术字面锚)。
#  改分流规则/话术请改 Orchestration.cs 并复拍 tools\probe-live-golden.ps1，不要在这里加 if。
# ──────────────────────────────────────────────────────────────────────────────
#
# ── 追加冻结(2026-10-12，换宿主 B-2d④「r1999 链体进核＋PS 侧 executor」)───────────
# Start-GdR1999Daily 自此是**调度器**：默认关 = 一字不变走回退段 Invoke-GdR1999DailyLegacy(原身逐字保留，
# 上面批次 5/批次 8 那两段冻结自此只对这一份成立)；开关拨开 = Invoke-GdR1999DailyRust 起内置件 gdcore.exe
# 走 --drive-m9a-daily 的问-答-重放。链体(八格动作的顺序/Level/话术/异常边界)与 run 真做已迁到
# tools\rustcore\src\main.rs 的 r1999_daily_chain；**这一档的契约号随批次逐次升，本行不钉死它** ——
# 现值只由 lib\common.ps1 的 Get-GdRustContract 一处理，落冻结段里写死一枚数字就是双源事实(本仓已为此改过三次口)；执行器只剩现场七格
# (updgate/takeover/mumu/cfg/sleep/stop/audit)与 ctx 名册十五枚的逐枚点名取键，**零自有日志**。
# 两路共用同一把回退开关 Test-GdRustDailyPath(不开第三枚键，两路同进退)；接线点仍只此一处。
# ⚠ 回退段必须排在文件末尾那节**之前**、逐行 # 前缀的冻结段一枚不许删：test-r1999 的四组静态锚
#   与那把"更新器件名首现早于接管函数名首现"的行序锚都钉在这份身上(接管函数名在下面 executor 里还会出现一次)。
# 改链体/话术/名册请改 main.rs 并复拍 B-2d⑤ 那枚等值门(probe-m9adrive-rust-golden.ps1，⑤ 立)，不要在这里加 if。
# ──────────────────────────────────────────────────────────────────────────────

function Set-GdR1999Ack {
    <# -Ack1999：确认"这一版的主线活动我已经手动打过" → 之后日常才允许跑活动方案 #>
    param([Parameter(Mandatory)]$S)
    $d = Get-R1999Event -S $S -IgnoreShopDone
    if (-not $d.EventKey) { Write-GdLog "1999 当前没有待确认活动：$($d.Reason)" -Level WARN }
    else {
        Set-GdAckKey -Game 'r1999' -Key $d.EventKey
        Write-GdLog "已确认 1999 活动 $($d.EventKey)[$($d.EventTip)]→ 日常将使用活动方案" -Level OK
    }
}

function Set-GdR1999ShopState {
    <# -RShopDone / -RShopReset：标记或撤销"活动商店已领空"。-Done 为真=标记，为假=撤销 #>
    param([Parameter(Mandatory)]$S, [switch]$Done)
    $d = Get-R1999Event -S $S -IgnoreShopDone
    if (-not $d.EventKey) { Write-GdLog "1999 当前没有进行中的活动可标记：$($d.Reason)" -Level WARN }
    elseif ($Done) {
        Set-GdShopDoneKey -Game 'r1999' -Key $d.EventKey
        Write-GdLog "已标记 1999 活动 $($d.EventKey)[$($d.EventTip)] 商店领空 → 之后跑[$($S.re1999.planToInstance.idle)]实例" -Level OK
    }
    elseif (Remove-GdShopDoneKey -Game 'r1999' -Key $d.EventKey) {
        Write-GdLog "已撤销 $($d.EventKey) 的领空标记 → 恢复活动方案" -Level OK
    } else {
        Write-GdLog "$($d.EventKey) 本来就没有领空标记" -Level WARN
    }
}

function Resolve-GdR1999Decision {
    <# 转发壳(批次 5)：-Force/-Plan 两段构造的规则在 C#(apply-r1999-daily-intent)；
       这里取数(Get-R1999Event)、解析实例(Resolve-R1999Instance)、打日志并把 {Decision;Instance} 交回。
       参数故意叫 -Plan 而不是 -RPlan：本文件只管 1999，带前缀的名字留给合体入口 daily-clear.ps1。 #>
    param([Parameter(Mandatory)]$S, [string]$Plan = 'auto', [switch]$Force)
    $r1999 = Get-R1999Event -S $S
    $d = Invoke-GdCli -Verb 'apply-r1999-daily-intent' -Body ([pscustomobject]@{
        decision = $r1999; plan = $Plan; force = [bool]$Force
    })
    $r1999 = $d.Decision
    foreach ($w in @($d.Warnings)) { Write-GdLog $w -Level WARN }
    try { $inst = Resolve-R1999Instance -S $S -PlanKey $r1999.Instance }
    catch { Write-GdLog "1999 实例解析失败：$($_.Exception.Message)" -Level ERROR; throw }
    Write-GdLog "1999 方案：$($r1999.Mode) → 实例[$($inst.Name)]— $($r1999.Reason)"
    return [pscustomobject]@{ Decision = $r1999; Instance = $inst }
}

function Show-GdR1999Preview {
    <# -DryRun：预览将切换的 M9A 实例、ADB 序列号和 MuMu 虚拟机状态，不启动任何程序 #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance)
    $prev = Set-M9AInstance -S $S -Instance $Instance
    $prev.Changes | ForEach-Object { Write-GdLog "  M9A：$_" }
    $conn = Get-M9AConnection -S $S -Instance $Instance
    Write-GdLog "  ADB：$($conn.Serial)($($conn.Name))"
    $st = Get-GdMuMuState -S $S
    Write-GdLog "  MuMu 虚拟机 $($S.mumu.vmIndex)：$(if ($st.AndroidStarted) { '运行中' } else { '未运行(将自动启动)' })"
}

function Invoke-GdR1999DailyLegacy {
    <# 真正跑一轮 1999：起 MuMu → 切实例 → 跑 M9A → 该重试就重试 → 关掉 M9A。返回 @{Status;Detail;Duration}
       (B-2d④ 起这一份是**回退段**：逐字保留，同名调度器与换头执行器在本节末尾另立) #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance)
    $inst = $Instance
    Write-GdLog '--- 重返未来：1999(MuMu + M9A)---' -Level STEP
    $selfEngine = (Get-Process -Id $PID).Path   # 起子进程用当前引擎自己(pwsh 7 —— 本仓库只支持 7)，两处外挂共用
    <# M9A 自动更新闸(2026-09-30 用户口径「检查到更新自己更就行了」)：必须放在起 M9A 之前 ——
       更新器见 m9a 进程在跑就拒动(且永不硬杀)。整链(备份/下载/覆盖/校验)最坏十几分钟，
       但任何失败只 WARN 不挡当日轮；已是最新时子进程秒退，日常不拖节奏。契约在 tools\m9a-update.ps1 头注。 #>
    try {
        $updOut = @(& $selfEngine -NoProfile -ExecutionPolicy Bypass -File (Join-Path $global:GD.Root 'tools\m9a-update.ps1') -Apply 2>&1)
        $updCode = $LASTEXITCODE
        if ($updCode -eq 0) {
            $updLine = (@($updOut | Where-Object { [string]$_ -match '已是最新|更新完成' }) | Select-Object -Last 1)
            Write-GdLog ('M9A 自动更新闸：' + [string]$updLine)
        } else {
            Write-GdLog ('M9A 自动更新未完成(退出码 ' + $updCode + ')，不挡当日轮：' + ((@($updOut | ForEach-Object { [string]$_ })) -join ' / ')) -Level WARN
        }
    } catch {
        Write-GdLog "M9A 自动更新闸本身抛错，跳过它照常跑：$($_.Exception.Message)" -Level WARN
    }
    Invoke-GdTakeover -S $S -Names @((Get-M9APaths -S $S).Name) -Label 'M9A'
    if (-not (Start-GdMuMu -S $S)) { Write-GdLog 'MuMu 未能确认启动，仍尝试让 M9A 自行处理' -Level WARN }
    $m9aCfg = Set-M9AInstance -S $S -Instance $inst -Apply
    $m9aCfg.Changes | ForEach-Object { Write-GdLog "  M9A：$_" }
    $run9 = { Invoke-M9ARun -S $S -Instance $inst }
    $r = Invoke-GdStep -Name '1999' -Body $run9
    <#
      ===== 批次 8：$retryable 分流改问 decide-daily-retry(Orchestration.cs) =====
      判据(not-started/empty-queue/blocked 只有 Rescued 才翻案)、if/elseif 链的落穿语义
      (retryOnce 关着不重试也照样落告警句)与全部话术都在 C#；本壳只回放日志 + 落地"活"
      (Stop-M9A / sleep 20 / 重跑)。旧的 $retryable 表达式逐行 # 前缀冻结在下方 —— test-r1999
      按那些字面钉锚(块注释不可嵌套，只能这么保)。
    #>
    #     $retryable = ($r.Status -notin @('ok', 'not-started', 'empty-queue', 'unknown-format', 'blocked')) -or
    #                  ($r.Rescued -and $r.Status -in @('not-started', 'empty-queue', 'blocked'))
    $d = Invoke-GdCli -Verb 'decide-daily-retry' -Body ([pscustomobject]@{
        game      = 'r1999'
        status    = [string]$r.Status
        rescued   = [bool]$r.Rescued
        retryOnce = [bool]$S.runtime.retryOnce
    })
    foreach ($l in @($d.Logs)) {
        $lvl = [string]$l.Level
        if ($lvl -and ($lvl -ne 'INFO')) { Write-GdLog ([string]$l.Text) -Level $lvl } else { Write-GdLog ([string]$l.Text) }
    }
    $retryable = [bool]$d.Retryable
    if ($retryable) {
        Stop-M9A -S $S
        Start-Sleep -Seconds 20
        $r = Invoke-GdStep -Name '1999重试' -Body $run9
    }
    <# 批次 8 冻结段：旧 if/elseif 的两句 WARN/ERROR 兜底话术(原文语义见 Orchestration.cs r1999 支)。
       实测 09-25 19:24：客户端在模拟器里下 1.91G 更新包，10 个任务里 8 个「任务失败」，
       重试只是再空转 5 分钟 —— "游戏没在界面上"的形态重试无用，要人去处理。 #>
    #     if ($retryable -and $S.runtime.retryOnce) {
    #         Write-GdLog $(if ($r.Rescued) { '1999 本轮未成功，但视觉兜底把挡路的界面点掉了 → 关闭 M9A 后重试一次' }
    #                       else { '1999 本轮未成功 → 关闭游戏后重试一次' }) -Level WARN
    #         Stop-M9A -S $S
    #         Start-Sleep -Seconds 20
    #         $r = Invoke-GdStep -Name '1999重试' -Body $run9
    #     } elseif ($r.Status -eq 'unknown-format') {
    #         Write-GdLog '1999 日志标志一条都没认出来 → 重试同样看不懂，直接记入本轮告警(见下面的处理建议)' -Level ERROR
    #     } elseif ($r.Status -eq 'blocked') {
    #         Write-GdLog '1999 从头一个任务起就没进游戏 → 是客户端/资源要更新或模拟器状态不对，重试没有意义，直接记入本轮告警' -Level ERROR
    #     } elseif ($r.Status -in @('not-started', 'empty-queue')) {
    #         Write-GdLog '1999 队列没开始/空跑完成 → 重试没有意义，直接记入本轮告警' -Level WARN
    #     }
    Stop-M9A -S $S
    <# 活动打完没打完 —— 让 agent 看画面判断(2026-09-30 用户口径，工具 tools\1999-event-audit.ps1)：
       只在队列真跑完的形态问；工具内部对"没有待确认活动"自带秒退，且自己负责把被 M9A「关闭游戏」
       顺手关掉的虚拟机重新起起来、用完关回去(09-30 18:21 首跑实锤的时序)。
       判定 yes 才会 ack，unknown/no 维持 event-pending 照旧等人。整段最坏七八分钟(起 VM+拉游戏+问一次)，
       任何失败只 WARN —— 审计是加分项，不许把当日轮带崩。 #>
    if ($r.Status -in @('ok', 'empty-queue')) {
        try {
            $audOut = @(& $selfEngine -NoProfile -ExecutionPolicy Bypass -File (Join-Path $global:GD.Root 'tools\1999-event-audit.ps1') -Apply 2>&1)
            $audCode = $LASTEXITCODE
            $audLine = (@($audOut | ForEach-Object { [string]$_ }) | Where-Object { $_ -match '审计结论|无事发生|维持 event-pending' } | Select-Object -Last 1)
            if ($audCode -eq 0) { Write-GdLog ('1999 活动审计：' + [string]$audLine) }
            else { Write-GdLog ('1999 活动审计没审成(退出码 ' + $audCode + ')，维持现状：' + ((@($audOut | ForEach-Object { [string]$_ })) -join ' / ')) -Level WARN }
        } catch {
            Write-GdLog "1999 活动审计本身抛错，跳过它照常收尾：$($_.Exception.Message)" -Level WARN
        }
    }
    return $r
}

# ===== B-2d④ 换头(2026-10-12)：调度器 + Rust 驱动档 executor =====
# 上面那一整段自今起是**回退段**(Invoke-GdR1999DailyLegacy)，逐字保留、行为一字不变；
# 同名入口 Start-GdR1999Daily 变成**调度器**：开关(Test-GdRustDailyPath，住 common.ps1)拨开 →
# Invoke-GdR1999DailyRust 起内置件 gdcore.exe 走换头重入协议(驱动档 --drive-m9a-daily：问-答-重放)；
# 拨关(默认) → 走回退段。这一路**共用 ark 那一路同一把开关**(两路同进退，不开第三枚键 —— 理由见
# common.ps1 那处头注)；接线点仍只此一处：改协议先改 main.rs 再复拍门，不要在这里加规则。
# ⚠ 本节必须排在回退段**之后**：tools\test-r1999.ps1:275 钉的是"更新器件名首现早于接管函数名首现"的
#   行序锚，而下面 takeover 动作里就要写那枚接管函数 —— 本节挪到回退段之前，那把锚当场红。
function Start-GdR1999Daily {
    <# 调度器：两分支返回值同形(@{Status;Detail;Duration?})，调用方无感 ——
       modules.ps1:419-423 只读这三格(时长/历史)，回退段那句 return $r 多带的 Rescued/Tasks 格没人读，
       所以核内 done 信封只带三格不是少了东西，是"框架射程就只有这三格"(plan §11.39 ①-9)。 #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance)
    if (Test-GdRustDailyPath -S $S) { return Invoke-GdR1999DailyRust -S $S -Instance $Instance }
    return Invoke-GdR1999DailyLegacy -S $S -Instance $Instance
}

function Get-GdM9ADriveExe {
    <# 内置件 gdcore.exe 的候选序：出货树形态 → cargo 形态，两者都不在返回 ''(调用方按"没有就当场报错"处理)。
       为什么不复用 lib\ark-daily.ps1 那枚同名件：lib\r1999-module.ps1:27 的 Libs 里**没有 ark-daily.ps1**
       (单跑 1999 这一路时它压根没被点)，跨文件取函数等于把"装载顺序"变成隐式依赖 ——
       宁可这里留第二份候选序，并已把"两处一份字面"登记进 plan §11.44 的欠账清单。 #>
    $cands = @(
        (Join-Path $global:GD.Root 'tools\rustcore\gdcore.exe'),
        (Join-Path $global:GD.Root 'tools\rustcore\target\release\gdcore.exe')
    )
    foreach ($c in $cands) { if (Test-Path -LiteralPath $c) { return $c } }
    return ''
}

function Invoke-GdR1999DailyRust {
    <# B-2d④ 换头执行器：把回退段那串同步逻辑换成"问-答-重放"——
       每一轮起一个 gdcore.exe：它从 journal 重放已办事项回到断点，把下一个要办的动作写成 ask 信封
       (seq/action/params)退出；这里照单执行完，把结果写 res 回执，再起下一轮。
       链体事实源在 main.rs 的 r1999_daily_chain(改它=改协议)。**八格动作里真做只有 run**
       (Invoke-M9ARun 那整段循环在核内真做，走 --m9a-run 同一套骨架)，所以白名单只有七枚、
       且这里**没有 'run' case**：白名单与 case 双删是回归抓器 —— 两侧有一边漏改就当场抛，
       不会被当成成功回执吞掉(与 ark 那一路 B-2c③ 同一处先例)。
       本函数**不自己造日志**：链上每一句都由核内侧信道 --log-sink 产生、这里按字节偏移边跑边排空后
       按原级别回放。更新闸与审计闸那两句"闸本身说了什么"由**链体**决定(退出码 0/非 0/抛三态)，
       执行器只回 {code,line,joined} 三个读数 —— 这里多打一行就等于同一句日志打两遍。
       ctx 名册十五枚**逐枚点名取键是硬约束**：$S.runtime 整块里住着视觉兜底的 API key
       (visionApiKey)，序列化进来等于把 key 递进核、递进 ctx/journal 两本会留盘的账；
       要什么取什么，多一枚都不许(算术验收：现场 runtime 四十一枚，进核只有名册那七枚)。
       m9aCliArgs 走**三态**：键没写=真身 Get-GdM9ACliEnabled 的"没填"档=开命令行入口，
       所以这里 $null 时**干脆不写这个键**，而不是写 false —— 塌成一档就把"裸启动"和"命令行"弄反了。 #>
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)]$Instance)

    $exe = Get-GdM9ADriveExe
    if (-not $exe) { throw 'B-2d④ 换头拨开了但找不到内置件 gdcore.exe —— 先跑 tools\rustcore\build.ps1(或重打 exe)再试' }
    $panel = Get-GdCliExe
    $selfEngine = (Get-Process -Id $PID).Path   # 两处外挂子进程共用当前引擎自己，与回退段 :86 同一句

    # 一次日常 = 一本账：ctx 换了 journal 就不能复用(内核按 init 与 ctx 全等校验)，每次开头清场重起。
    $journal = Join-Path $global:GD.State 'rust-m9a-daily-journal.ndjson'
    $ctxFile = Join-Path $global:GD.State 'rust-m9a-daily-ctx.json'
    $resFile = Join-Path $global:GD.State 'rust-m9a-daily-res.json'
    $sink = Join-Path $global:GD.State 'rust-m9a-daily-log-sink.ndjson'
    foreach ($f in @($journal, $resFile, $sink)) { if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath $f -Force } }
    # 侧信道一本会话一个文件(核侧只追加不截断)，偏移在这里归零、之后由 Read-GdSinkIncrement 按**字节**推进。
    # 一律用 long：[ref] 会记住初值的类型，这里留 int 以后写 long 会被往回截(偏移算错=同一句日志重放)。
    $sinkOffset = [long]0

    # ---- ctx：壳写一遍的两格(顶层 retry_once ＋ m9aRun 名册十五枚)，键名与 config\settings.json 逐字同名 ----
    # paths 三枚递的是**配置里的相对原值**(m9aDir/m9aExe/m9aLogDir)：核内那把 M9aPaths::of 自己
    # 做 Join(m9aDir, m9aExe) 与 Join(m9aDir, m9aLogDir)，与真身 Get-M9APaths 的 Join-GdPath 组法同形 ——
    # 这里若先拼成绝对路径，核内再拼一次就得到双重路径。conn 两枚取自 Get-M9AConnection(adb 是绝对落点，核内不拼)。
    $conn = Get-M9AConnection -S $S -Instance $Instance
    $rt = [ordered]@{
        m9aInstanceArg           = [string]$S.runtime.m9aInstanceArg
        m9aForceStart            = [bool]$S.runtime.m9aForceStart
        visionMaxRescue          = [int]$S.runtime.visionMaxRescue
        r1999StartTimeoutMinutes = [int]$S.runtime.r1999StartTimeoutMinutes
        r1999TimeoutMinutes      = [int]$S.runtime.r1999TimeoutMinutes
        stallTimeoutMinutes      = [int]$S.runtime.stallTimeoutMinutes
    }
    if ($null -ne $S.runtime.m9aCliArgs) { $rt.Add('m9aCliArgs', [bool]$S.runtime.m9aCliArgs) }
    Write-GdJson -Path $ctxFile -Object ([pscustomobject]@{
        retry_once = [bool]$S.runtime.retryOnce
        m9aRun     = [pscustomobject]@{
            paths    = [pscustomobject]@{
                m9aDir    = [string]$S.paths.m9aDir
                m9aExe    = [string]$S.paths.m9aExe
                m9aLogDir = [string]$S.paths.m9aLogDir
            }
            conn     = [pscustomobject]@{ adb = [string]$conn.Adb; serial = [string]$conn.Serial }
            instance = [pscustomobject]@{ id = [string]$Instance.Id; name = [string]$Instance.Name }
            package  = [string]$S.re1999.package
            runtime  = $rt
        }
    })

    $round = 0
    while ($true) {
        $round += 1
        if ($round -gt 500) { throw "B-2d④ 换头驱动轮次超上限(疑似死循环)：journal=$journal" }
        # 信封落点每轮换新文件，防读到上一轮读数；写不下去时 gdcore 退 stdout 末行(内核侧既定纪律)
        $envFile = Join-Path ([System.IO.Path]::GetTempPath()) ("gd-m9a-daily-env-" + [guid]::NewGuid().ToString('N') + ".json")
        try {
            # 边跑边排空侧信道：这一路一轮要跑十几分钟(队列+兜底)，卡住时一行不印就等于没留证。
            $cap = Start-GdNativeStreamed -Exe $exe -SinkPath $sink -SinkOffset ([ref]$sinkOffset) -Arguments @(
                '--drive-m9a-daily', '--i-drive', '--journal', $journal, '--ctx', $ctxFile,
                '--res', $resFile, '--panel-cs', $panel, '--state', $global:GD.State,
                '--log-sink', $sink, '--out', $envFile
            ) -OnSinkRow {
                param($r)
                $lvl = [string]$r.Level
                $txt = [string]$r.Text
                if ($lvl -and ($lvl -ne 'INFO')) { Write-GdLog $txt -Level $lvl } else { Write-GdLog $txt }
            }
            $envText = ''
            if ([int]$cap.StreamTimeouts -gt 0) {
                # 口径（10-16 #473 取证后定档，与 ark 那路同源同改）：持有管道写端的是链体点过的**外部常驻服务**
                # (adb server 一类被 fork 成常驻并继承发起方 stdio)，M9A 自己会退是运气、不是已验 ——
                # 那进程不归我们创建 ⇒ 这一格每轮必现，级别降 INFO、每轮一行仍可查，
                # 真日志走 --log-sink、进度判据走 journal／协议文件(见 docs\rust-core-plan.md §11.61)。
                Write-GdLog "B-2d④ 换头：收流按有界等待放弃 $([int]$cap.StreamTimeouts) 路 stdout/stderr(外部常驻服务持写端，stdout 兜底那一路本轮不可用；真日志见 --log-sink)"
            }
            if (Test-Path -LiteralPath $envFile) { $envText = [System.IO.File]::ReadAllText($envFile) }
            else {
                foreach ($ln in @($cap.Lines)) { $t = ([string]$ln).TrimStart(); if ($t.StartsWith('{')) { $envText = $t } }
            }
            if (-not $envText) {
                throw "B-2d④ 换头：gdcore 没给出信封(rc=$($cap.ExitCode))；输出末三行：$((@($cap.Lines) | Select-Object -Last 3) -join ' / ')"
            }
            $envelope = $envText | ConvertFrom-Json
            if ([string]$envelope.role -ne 'B2d-drive-m9a-daily') {
                throw "B-2d④ 换头：信封 role 不对($($envelope.role)) —— 这枚 gdcore.exe 不是 1999 驱动档那件？"
            }
            $want = Get-GdRustContract
            if ([string]$envelope.contract -ne $want) {
                throw "B-2d④ 换头：gdcore 契约号($($envelope.contract))与壳侧期望($want)不一致 —— 先重跑 tools\rustcore\build.ps1 再拨开关"
            }
            # 链体日志已在上面边跑边从侧信道按原级别回放完毕(空/INFO 走默认档，与回退段的 decide 回放同形)。
            # 信封的 logs 键与 ark 那一路同规矩：留键不删键、恒为空数组，这里不读它。
            $kind = [string]$envelope.kind
            if ($kind -eq 'done') {
                $dr = $envelope.result
                $out = @{ Status = [string]$dr.status; Detail = [string]$dr.detail }
                if ($null -ne $dr.duration) { $out.Duration = $dr.duration }
                return $out
            }
            if ($kind -eq 'fail') { throw ([string]$envelope.error) }
            if ($kind -ne 'ask') { throw "B-2d④ 换头：信封 kind 不认识($kind) —— 协议两侧不同源？" }

            $seq = [int]$envelope.seq
            $act = [string]$envelope.action
            $prm = $envelope.params
            if ($act -notin @('updgate', 'takeover', 'mumu', 'cfg', 'sleep', 'stop', 'audit')) {
                # 白名单之外直接抛(绕开下面的动作 catch)：不认识的动作 = 协议不同源，不许被当成"动作失败"混过去。
                # 名单里**没有 run**：那一格是这一路唯一的真做动作，在核内跑(B-2d④ 定档，与 ark 侧 B-2c③ 同法)。
                throw "B-2d④ 换头：收到不认识的动作 '$act'(seq=$seq) —— 先改 main.rs 再改这里，不许放行"
            }
            $ok = $true
            $data = $null
            $err = $null
            try {
                switch ($act) {
                    'updgate' {
                        # 回退段那一段的"活"(起更新器子进程)原样搬来，两句闸话由链体按 code 三态决定；
                        # 挑行与全量拼串两个口径也照回退段逐字同形，否则同一轮换头前后日志对不上。
                        $gOut = @(& $selfEngine -NoProfile -ExecutionPolicy Bypass -File (Join-Path $global:GD.Root 'tools\m9a-update.ps1') -Apply 2>&1)
                        $gCode = $LASTEXITCODE
                        $gLine = (@($gOut | Where-Object { [string]$_ -match '已是最新|更新完成' }) | Select-Object -Last 1)
                        $data = [pscustomobject]@{
                            code   = [int]$gCode
                            line   = [string]$gLine
                            joined = ((@($gOut | ForEach-Object { [string]$_ })) -join ' / ')
                        }
                    }
                    'takeover' {
                        Invoke-GdTakeover -S $S -Names @((Get-M9APaths -S $S).Name) -Label 'M9A'
                        $data = $true
                    }
                    'mumu' { $data = [bool](Start-GdMuMu -S $S) }
                    'cfg' {
                        # 只回 changes 数组，那句"  M9A：…"前缀是**链体**加的(回退段壳里有同一句) ——
                        # 这里不许自带前缀，否则同一行打两遍前缀。
                        $m9aCfg = Set-M9AInstance -S $S -Instance $Instance -Apply
                        $data = [pscustomobject]@{ changes = @($m9aCfg.Changes) }
                    }
                    'sleep' { Start-Sleep -Seconds ([int]$prm); $data = $true }
                    'stop' { Stop-M9A -S $S; $data = $true }
                    'audit' {
                        # 核内已经按状态名册判过"只在 ok/empty-queue 才问"，这里只管起工具与读数。
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
            # 回执形状是协议：ok 真不许带 err 键、假不许带 data 键(main.rs drive_merge_res 逐字校验)
            $rf = [ordered]@{ seq = $seq; action = $act; ok = $ok }
            if ($ok) { $rf.data = $data } else { $rf.err = $err }
            Write-GdJson -Path $resFile -Object $rf
        } finally {
            if (Test-Path -LiteralPath $envFile) { Remove-Item -LiteralPath $envFile -Force }
        }
    }
}
