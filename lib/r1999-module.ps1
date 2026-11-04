# lib/r1999-module.ps1 — 《重返未来：1999》(MuMu + M9A) 作为框架模块的清单
#
# 和 lib\ark-module.ps1 同一套形状，业务逻辑仍留在 lib\m9a.ps1 / r1999-decisions.ps1 /
# r1999-daily.ps1 / mumu.ps1 / r1999-status.ps1 里，本文件不复制任何实现。
# ⚠ 改口(11-04 刀④)：上面那句里的 r1999-status.ps1 已整件出仓 —— 名册两枚改问核(见 ark-module.ps1
#   同处注释，两枚 verb 两路共用)、展示四枚逐字内联进下面的钩子(实例列表那枚连"故意不套 try/catch"也照抄)。
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
            # B-2d④ 起 Start-GdR1999Daily 与核内驱动档执行器同名同签名；10-24 刀② 撤掉分流后它只剩一条臂：
            # 直接 return Invoke-GdR1999DailyRust —— 链体在内置件 gdcore.exe 里，本路 executor 只代做现场七格。
            # 回退臂 Invoke-GdR1999DailyLegacy 已随真身出仓(逐字在钉位 6157448:lib/r1999-daily.ps1)。这一行仍**不该加 if**。
            Run       = { param($S, $Decision) return (Start-GdR1999Daily -S $S -Instance $Decision.Instance) }
            Stop      = {
                param($S)
                # ⚠ 刀①(2026-11-03)：round 的 stop:r1999 已改核内直答(stop-m9a＋closeEmulatorAfter 真值开
                # stop-mumu)，上面那层 PS 臂已摘 ⇒ 这一钩子格自此没有 round 消费面，体暂留(1eq.④)，去留归后续刀。
                Stop-M9A -S $S
                # "跑完关模拟器"是这一路自己的设置项，原来写在入口的 finally 里并按 -Games 判断；
                # 挪进收尾钩子后，"这一路本轮到底跑没跑"就是天然的判据。
                if ($S.runtime.closeEmulatorAfter) { Stop-GdMuMu -S $S }
            }
            Ack       = { param($S) Set-GdR1999Ack -S $S }
            ShopState = { param($S, $Done) Set-GdR1999ShopState -S $S -Done:([bool]$Done) }

            # ---- 监督一族的可选钩子(11-02 刀③ 起答方是 C# 宿主批，旧真身 lib\supervisor.ps1 已出仓)。这一路刻意**不给 SupervisorRestart**：
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

            # ---- status.ps1 那套只读体检的可选钩子，形状和 ark-module.ps1 里那组完全一致
            #      (11-04 刀④：lib\r1999-status.ps1 整件出仓，名册问核、话术内联)。----
            StatusRows      = {
                param($S)
                $d = Invoke-GdCli -Verb 'resolve-status-env-rows' -Body ([pscustomobject]@{
                    id = 'r1999'; m9aDir = $S.paths.m9aDir; m9aInstancesDir = $S.paths.m9aInstancesDir;
                    mumuDir = $S.paths.mumuDir; mumuManager = $S.paths.mumuManager
                })
                return @($d.rows)   # 三行 label/shown/state：M9A／M9A 实例／MuMu
            }
            StatusVersion   = { param($S) Show-GdVersionFile -Label 'M9A' -Dir $S.paths.m9aDir -File 'interface.json' }
            StatusProcs     = {
                param($S)
                # MuMu 的虚拟机状态是另一行(下面 StatusProcExtra)，不混在这枚名册里 —— 照拆分前口径。
                $d = Invoke-GdCli -Verb 'resolve-status-proc-names' -Body ([pscustomobject]@{ id = 'r1999' })
                return @($d.names)
            }
            # 进程段后面还要补一行模拟器状态：方舟那边没有这一行，所以它是独立钩子而不是塞进 StatusProcs。
            StatusProcExtra = {
                param($S)
                try {
                    $st = Get-GdMuMuState -S $S
                    # 读不到就说读不到、并把原因带上(没填 MuMu 目录 / MuMuManager 报错)，别把"没探到"粉饰成"未运行"
                    if (-not $st.Available) { Write-Host "  MuMu：读不到($($st.Raw))" -ForegroundColor Yellow; return }
                    Write-Host "  MuMu VM $($S.mumu.vmIndex)：$(if ($st.AndroidStarted) { '安卓已启动' } elseif ($st.ProcessStarted) { '进程在但安卓未就绪' } else { '未运行' })($($st.Name))"
                } catch { Write-Host "  MuMu 状态读取失败：$($_.Exception.Message)" -ForegroundColor Yellow }
            }
            StatusPlan      = {
                param($S, $Offline)   # 1999 的判定不联网，$Offline 忽略(签名与 ark 那枚对齐)
                $r = Get-R1999Event -S $S
                try {
                    $inst = Resolve-R1999Instance -S $S -PlanKey $r.Instance
                    Write-Host "  1999：$($r.Mode) → 实例[$($inst.Name)]  ← $($r.Reason)" -ForegroundColor $(if ($r.Mode -eq 'event-pending') { 'Yellow' } else { 'Gray' })
                    Write-Host "        任务链：$($inst.Tasks -join ' → ')"
                } catch { Write-Host "  1999：$($r.Reason)；实例解析失败：$($_.Exception.Message)" -ForegroundColor Red }
            }
            StatusSection   = {
                param($S)
                # 扫 config\instances + 行式的本体在 C#(m9a-instance-lines)。
                # 注意没保护 try/catch 是故意的：旧实现读 app 设置失败就直接往上抛，展示层不吞(外壳的 Invoke-GdTolerant 接)。
                $dir = Join-GdPath $S.paths.m9aDir @($S.paths.m9aInstancesDir)
                # 同 ark 那枚 StatusSection：路径没填就讲清是"还没 setup"，别抛一句看不懂的绑定错误
                if (-not (Test-GdPathReady $dir)) { Write-Host "  读不到 M9A 实例目录：$dir(助手目录还没填 → 先跑 setup.ps1)" -ForegroundColor Yellow; return }
                $d = Invoke-GdCli -Verb 'm9a-instance-lines' -Body ([pscustomobject]@{
                    dir     = $dir
                    appFile = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aAppSettings))
                })
                foreach ($l in @($d.Lines)) { Write-Host $l }
            }
        }
        StatusSectionTitle = 'M9A 实例现状'
    })
