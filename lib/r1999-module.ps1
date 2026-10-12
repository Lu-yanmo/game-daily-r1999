# lib/r1999-module.ps1 — 《重返未来：1999》(MuMu + M9A) 作为框架模块的清单
#
# 和 lib\ark-module.ps1 同一套形状，业务逻辑仍留在 lib\m9a.ps1 / r1999-decisions.ps1 /
# r1999-daily.ps1 / mumu.ps1 / r1999-status.ps1 里，本文件不复制任何实现。
#
# 这一路和方舟在契约上的真实差别只有两处，都在这里吸收掉，框架不需要认识它们：
#   ① 1999 的判定结果除了方案(Mode)还要带一个 M9A 实例对象 → Resolve 钩子把它平铺进返回对象；
#   ② 1999 不需要管理员，但跑完可能要关 MuMu 虚拟机 → RequiresAdmin=$false，收尾钩子里带上那段。

Register-GdModule ([pscustomobject]@{
        Id               = 'r1999'
        Name             = '重返未来：1999(MuMu + M9A)'
        ShortName        = '1999'
        AssistantLib     = 'm9a.ps1'
        OrchestrationLib = 'r1999-daily.ps1'
        RequiresAdmin    = $false             # M9A 操作的是模拟器里的安卓，不提权也连得上
        ElevationFlag    = 'arkRequiresElevation'   # 本路没有这个开关(RequiresAdmin 已是 false，框架不会读到这里)
        # 这一路不要管理员，三句提示都用不上 —— 空串是"字段齐但没话要说"，Assert 认这个形状。
        AdminWhy         = ''
        AdminHint        = ''
        AdminOkNote      = ''
        SettingsBlock    = 're1999'          # $S.re1999 —— 注意 settings 里的块名是 re1999，模块 id 是 r1999
        PathKeys         = @('m9aDir', 'mumuDir')
        # 同 ark-module.ps1：入口在顶层点的业务库，r1999-decisions.ps1 由 lib\decisions.ps1 门面负责点。
        # 兜底那两份：vision-rescue.ps1 是两路共用的内核(ark 清单里也有它，按文件名去重只点一次)，
        # vision-mumu.ps1 是这一路自己的后端(adb 截屏 + adb input tap，即 New-GdVisionTarget-mumu 的出处)。
        Libs             = @('m9a.ps1', 'mumu.ps1', 'vision-rescue.ps1', 'vision-mumu.ps1', 'r1999-daily.ps1')
        # 这一路要管理员吗？清单这里答"No"，兜底那边也一样：NeedsAdmin=$false 写在 New-GdVisionTarget-mumu。
        # 两处不冲突 —— 前者管"跑这一路要不要提权"，后者管"点这一路的画面要不要提权"，1999 两样都不要。
        VisionKind       = 'mumu'             # 兜底对象是"模拟器屏幕"，不是"1999 这款游戏"，所以 kind 不叫 r1999
        EntryVerbs       = @{
            Merged = @{ Ack = '-Ack1999'; ShopDone = '-RShopDone'; ShopReset = '-RShopReset' }
        }
        Hooks = [pscustomobject]@{
            EnvProbes = {
                param($S)
                return [ordered]@{
                    'M9A 目录'        = $S.paths.m9aDir
                    'M9A appsettings' = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aAppSettings))
                    'MuMu Manager'    = (Join-GdPath $S.paths.mumuDir @($S.paths.mumuManager))
                }
            }
            Resolve   = {
                param($S, $Opt)
                # -Plan 没给就等于 auto：不能把 $null 直接递给 Resolve-GdR1999Decision，
                # 它的 param 默认值只在"参数没出现"时才生效，递 $null 会让 $Plan -ne 'auto' 成立、
                # 把这一轮判成"命令行指定 (空)"那种莫名其妙的方案。
                $plan = if ($Opt.Plan) { [string]$Opt.Plan } else { 'auto' }
                $r = Resolve-GdR1999Decision -S $S -Plan $plan -Force:([bool]$Opt.Force)
                $d = $r.Decision
                return [pscustomobject]@{ Mode = $d.Mode; Instance = $r.Instance; Decision = $d }
            }
            Preview   = { param($S, $Decision) Show-GdR1999Preview -S $S -Instance $Decision.Instance }
            # B-2d④ 起 Start-GdR1999Daily 是**调度器**：同名，但内部照 Test-GdRustDailyPath 分流 ——
            # 开关拨开走 Invoke-GdR1999DailyRust（链体在内置件里，本路 executor 只代做现场七格），
            # 关着走 Invoke-GdR1999DailyLegacy（原身逐字保留）。这一行因此**不该加 if**，回退只管在这一把闸上。
            Run       = { param($S, $Decision) return (Start-GdR1999Daily -S $S -Instance $Decision.Instance) }
            Stop      = {
                param($S)
                Stop-M9A -S $S
                # "跑完关模拟器"是这一路自己的设置项，原来写在入口的 finally 里并按 -Games 判断；
                # 挪进收尾钩子后，"这一路本轮到底跑没跑"就是天然的判据。
                if ($S.runtime.closeEmulatorAfter) { Stop-GdMuMu -S $S }
            }
            Ack       = { param($S) Set-GdR1999Ack -S $S }
            ShopState = { param($S, $Done) Set-GdR1999ShopState -S $S -Done:([bool]$Done) }

            # ---- 监督 agent(lib\supervisor.ps1)的可选钩子。这一路刻意**不给 SupervisorRestart**：
            #      MuMu 冷启动要好几分钟，误判一次的代价远超"多跑一轮"，所以 1999 出了状况只发通知。----
            SupervisorEvidence = {
                param($S)
                $P = Get-M9APaths -S $S
                return @{
                    AssistantProc   = $P.Name
                    AssistantLog    = (Get-M9ADailyLog -P $P)     # M9A 每天一本 log-yyyyMMdd.log
                    DonePattern     = '启动任务完成|任务已全部完成'
                    ProgressPattern = '开始任务：|队列任务开始|用户操作：启动任务'   # 「启动任务完成」单独出现只等于连上了模拟器，要先见过进度线
                }
            }

            # ---- status.ps1 那套只读体检的可选钩子，形状和 ark-module.ps1 里那组完全一致 ----
            StatusRows      = { param($S) Get-GdR1999EnvRows -S $S }
            StatusVersion   = { param($S) Show-GdR1999StatusVersion -S $S }
            StatusProcs     = { param($S) Get-GdR1999ProcNames }
            # 进程段后面还要补一行模拟器状态：方舟那边没有这一行，所以它是独立钩子而不是塞进 StatusProcs。
            StatusProcExtra = { param($S) Show-GdR1999MuMuState -S $S }
            StatusPlan      = { param($S, $Offline) Show-GdR1999StatusPlan -S $S }   # 1999 的判定不联网，$Offline 忽略
            StatusSection   = { param($S) Show-GdR1999InstanceList -S $S }
        }
        StatusSectionTitle = 'M9A 实例现状'
    })
