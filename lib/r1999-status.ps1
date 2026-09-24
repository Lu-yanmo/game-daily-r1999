# lib/r1999-status.ps1 — 重返未来：1999 这一路的"体检段落"(与 lib\ark-status.ps1 同一套接口形状)
# 为什么这么拆、外壳怎么用，见 lib\ark-status.ps1 开头那段注释。
# 依赖：调用方先点 lib\common.ps1、lib\decisions.ps1、lib\m9a.ps1、lib\mumu.ps1。

function Get-GdR1999EnvRows {
    <# [1] 环境：1999 这边要探的三个路径 #>
    param([Parameter(Mandatory)]$S)
    return [ordered]@{
        'M9A'      = $S.paths.m9aDir
        'M9A 实例'  = (Join-GdPath $S.paths.m9aDir @($S.paths.m9aInstancesDir))
        'MuMu'     = (Join-GdPath $S.paths.mumuDir @($S.paths.mumuManager))
    }
}

function Show-GdR1999StatusVersion {
    param([Parameter(Mandatory)]$S)
    Show-GdVersionFile -Label 'M9A' -Dir $S.paths.m9aDir -File 'interface.json'
}

function Get-GdR1999ProcNames {
    <# [2] 进程：M9A 本体。MuMu 的虚拟机状态是另一行(下面那个函数)，不混在这里 #>
    return @('m9a')
}

function Show-GdR1999MuMuState {
    <# [2] 进程 / 模拟器：MuMuManager 那句 vboxmanage 风格的探活，读不到只打黄字 #>
    param([Parameter(Mandatory)]$S)
    try {
        $st = Get-GdMuMuState -S $S
        # 读不到就说读不到、并把原因带上(没填 MuMu 目录 / MuMuManager 报错)，别把"没探到"粉饰成"未运行"
        if (-not $st.Available) { Write-Host "  MuMu：读不到($($st.Raw))" -ForegroundColor Yellow; return }
        Write-Host "  MuMu VM $($S.mumu.vmIndex)：$(if ($st.AndroidStarted) { '安卓已启动' } elseif ($st.ProcessStarted) { '进程在但安卓未就绪' } else { '未运行' })($($st.Name))"
    } catch { Write-Host "  MuMu 状态读取失败：$($_.Exception.Message)" -ForegroundColor Yellow }
}

function Show-GdR1999StatusPlan {
    <# [3] 今日判定：1999 那两行(方案 -> 实例名 + 任务链) #>
    param([Parameter(Mandatory)]$S)
    $r = Get-R1999Event -S $S
    try {
        $inst = Resolve-R1999Instance -S $S -PlanKey $r.Instance
        Write-Host "  1999：$($r.Mode) → 实例[$($inst.Name)]  ← $($r.Reason)" -ForegroundColor $(if ($r.Mode -eq 'event-pending') { 'Yellow' } else { 'Gray' })
        Write-Host "        任务链：$($inst.Tasks -join ' → ')"
    } catch { Write-Host "  1999：$($r.Reason)；实例解析失败：$($_.Exception.Message)" -ForegroundColor Red }
}

function Show-GdR1999InstanceList {
    <# [5] M9A 实例现状：把 config\instances 里的实例逐个列出来，标出当前活动的那个 #>
    param([Parameter(Mandatory)]$S)
    $dir = Join-GdPath $S.paths.m9aDir @($S.paths.m9aInstancesDir)
    # 同 Show-GdArkQueueState：路径没填就讲清是"还没 setup"，别抛一句看不懂的绑定错误
    if (-not (Test-GdPathReady $dir)) { Write-Host "  读不到 M9A 实例目录：$dir(助手目录还没填 → 先跑 setup.ps1)" -ForegroundColor Yellow; return }
    $app = Read-GdJson -Path (Join-GdPath $S.paths.m9aDir @($S.paths.m9aAppSettings))
    foreach ($f in @(Get-ChildItem -LiteralPath $dir -Filter '*.json')) {
        $c = Read-GdJson -Path $f.FullName
        $mark = if ([string]$c.InstanceName -eq [string]$app.'Instances.LastActiveName') { ' <== 当前' } else { '' }
        Write-Host "  $($c.InstanceName)($($f.BaseName))$mark — $((@($c.CurrentTasks) | ForEach-Object { ($_ -split '<\|\|\>')[0] }) -join '/')"
    }
}
