# lib/r1999-module.ps1 — 《重返未来：1999》(MuMu + M9A) 作为框架模块的清单
#
# 和 lib\ark-module.ps1 同一套形状，业务逻辑仍留在 lib\m9a.ps1 / r1999-decisions.ps1 /
# r1999-daily.ps1 / mumu.ps1 / r1999-status.ps1 里，本文件不复制任何实现。
# ⚠ 改口(11-04 刀④)：上面那句里的 r1999-status.ps1 已整件出仓 —— 名册两枚改问核(见 ark-module.ps1
#   同处注释，两枚 verb 两路共用)、展示四枚逐字内联进下面的钩子(实例列表那枚连"故意不套 try/catch"也照抄)。
# ⚠ 改口(11-04 刀⑤)：Resolve／Preview／Run／Stop／Ack／ShopState／SupervisorEvidence 七枚块体同日出仓 ——
#   六族的 round 消费面已随转接刀·运行面改成核内直答(gate/stop＝刀①、preview＝刀②、resolve 判定面＝刀③)，
#   本清单不再在其上合成第三层答手。监督那枚的对面是 C# 宿主批(11-02 刀③ 起)，完成线规格已在
#   tools\panel-cs\Supervisor.Host.cs 的 SpecOf 里同值另写(AssistantProc／AssistantLog／DonePattern／
#   ProgressPattern 四格逐字在)，摘 PS 副本消灭的是双真身不是语义；这一路本来就**不给 SupervisorRestart**
#   (MuMu 冷启动几分钟，误判代价远超多跑一轮)，这一条安排不因摘证据格而变。Stop 里那句
#   "跑完关模拟器"也随刀① 进了核(stop:r1999 臂现拍 stop-m9a＋closeEmulatorAfter 真值再加 stop-mumu)。
#   ⚠ EnvProbes 是唯一留下的必需格：它的对面不是函数调用而是**源文本解析**
#   (tools\panel-cs\Panel.Summary.cs 的 ProbeSpecs 按一行正则去清单里抠这段)，判语全文在 lib\modules.ps1 末尾那段。
#
# 这一路和方舟在契约上的真实差别只有两处，都在这里吸收掉，框架不需要认识它们：
#   ① 1999 的判定结果除了方案(Mode)还要带一个 M9A 实例对象 → 原来靠 Resolve 钩子把它平铺进返回对象；
#      ⚠ 刀⑤ 改口：这一层现由 round-resolve 核内直答臂承担(11-03 刀③)，本清单不再复写。
#   ② 1999 不需要管理员，但跑完可能要关 MuMu 虚拟机 → RequiresAdmin=$false，那段原来挂在收尾钩子里；
#      ⚠ 刀⑤ 改口：closeEmulatorAfter 现住核内 stop:r1999 臂(11-03 刀①)，本清单不再复写。

Register-GdModule ([pscustomobject]@{
        Id               = 'r1999'
        Name             = '重返未来：1999(MuMu + M9A)'
        ShortName        = '1999'
        AssistantLib     = 'm9a.ps1'          # 这一路能不能跑(11-04 刀⑦ 笔(a) 起它同时是合体判据那一把尺的唯一读数)
        RequiresAdmin    = $false             # M9A 操作的是模拟器里的安卓，不提权也连得上
        ElevationFlag    = 'arkRequiresElevation'   # 本路没有这个开关(RequiresAdmin 已是 false，框架不会读到这里)
        # 这一路不要管理员，三句提示都用不上 —— 空串是"字段齐但没话要说"，Assert 认这个形状。
        AdminWhy         = ''
        AdminHint        = ''
        AdminOkNote      = ''
        SettingsBlock    = 're1999'          # $S.re1999 —— 注意 settings 里的块名是 re1999，模块 id 是 r1999
        PathKeys         = @('m9aDir', 'mumuDir')
        # 同 ark-module.ps1：入口在顶层点的业务库，r1999-decisions.ps1 由 lib\decisions.ps1 门面负责点。
        # 11-06 转接刀·运行面刀⑮ 笔(d)：vision-rescue.ps1(两路共用的兜底内核)与 vision-mumu.ps1(New-GdVisionTarget-mumu 的出处)随三件本体整件出仓——
        #   端点/拍屏/点屏三段真身早在 C#(tools\panel-cs\Gate.Host.cs 端点直身＋Live.cs mumu-screen-shot／mumu-screen-click)，闸序住核内 resolve-rescue-context；本清单不再声明 PS 后端。
        Libs             = @('m9a.ps1', 'mumu.ps1')
        # 11-04 转接刀·运行面刀⑦ 笔(c)：'r1999-daily.ps1' 整件出仓，故此处不再列它。前置现拍＝岛里逐枚点名
        # (Set-GdR1999ShopState／Resolve-GdR1999Decision／Show-GdR1999Preview／Start-GdR1999Daily／
        # Get-GdM9ADriveExe／Invoke-GdR1999DailyRust)岛外零活 PS 调用点逐枚现拍(§11.235)：判定/预览/收尾三门
        # 已随运行面刀①~③ 进核直答，-RShopDone／-RShopReset 那两档走 gates 表→核内 verb，从不叫这枚壳；
        # 起跑/收尾那两把闸(更新器与活动审计)的家在地板件 lib\rust-round.ps1，不随本件出仓。
        # 文件头那句「业务逻辑仍留在 …r1999-daily.ps1… 里」自此按历史引文读(摘走真身不等于摘走语义，家逐枚点名)：
        # 商店态/当日确认的写盘＋话术在 tools\panel-cs\Round.Data.cs，判定构造段在 Orchestration.cs，
        # MuMu/M9A 拉起与收尾在 Chain.cs，链体在 tools\rustcore\src\main.rs——PS 侧不再有一份，"不复制任何实现"仍成立。
        # 这一路要管理员吗？清单这里答"No"，兜底那边也一样：NeedsAdmin=$false 现在写在核内 resolve-rescue-context 的 mumu 默认(11-06 刀⑮ 笔(d) 前它在 New-GdVisionTarget-mumu，那件已整件出仓)。
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
