# lib/detect.ps1 — setup.ps1 用的安装位置探测(纯查找，不写任何文件)
#
# ── 冻结(2026-09-26，PS→C# 批次 2)────────────────────────────────────────────
# Resolve-GdCandidate 的探测本体 —— 三个候选来源(在跑的进程 / 卸载表 / 磁盘浅层 BFS)、
# Skip/Junk 段过滤、卸载表值去引号剥前缀、nx_main 退回安装根、稳定排序、按目录去重、
# "探测：候选 N 个…"文案 —— 全部迁到 tools\panel-cs\Detect.cs(verb：detect-installs；
# 契约见 docs\ps-migration-batch0.md)。本文件留下的壳只转参、拆 {Found,Value,Source}。
# 所以：改探测规则/文案请改 C# 那份并跑 `panel-cs --statetest` 对拍(原对拍门甲档第十刀退役)，不要在这里加规则。
# 故意留在 PS 侧的一件事：
#   Get-GdFixedRoot —— 壳把 roots 组装好递给 verb，本机固定盘口径只此一处(C# 自检直接注入 roots，不读这里)。
# Get-GdMuMuVmList 批次 2 时说"要起外部进程、归批次 3"—— 批次 3 已落地：JSON→VM 列表的解读本体
# 在 Processes.cs(verb：parse-mumu-vms)，这里只剩"文件在不在 + 起 MuMuManager"的外壳。
# 原内部函数(Test-GdJunkPath/Format-GdCandidates/Test-GdSkipPath/Get-GdChildDir/Find-GdInstallDir/
# Get-GdRunningExeDir/Get-GdUninstallHintDir/Get-GdUninstallHint/Add-GdTrimmedDir/Select-GdUniqueDir)
# 已整函数删除，Detect.cs 是唯一事实源。Candidates 键合体工程里从来没人消费，壳里保留为 $null 撑形状。
# ──────────────────────────────────────────────────────────────────────────────

function Get-GdFixedRoot {
    <# 本机所有固定磁盘根目录(C:\ D:\ E:\ ...)，可移动盘和光驱不算 #>
    @(try {
        [System.IO.DriveInfo]::GetDrives() | Where-Object { $_.DriveType -eq [System.IO.DriveType]::Fixed -and $_.IsReady } |
            ForEach-Object { $_.RootDirectory.FullName.TrimEnd('\') + '\' }
    } catch { @('C:\') })
}

function Resolve-GdCandidate {
    <#
      转发壳：一类软件的探测合并(进程→卸载表→磁盘扫描)全在 C#(detect-installs)。
      PickProp='Dir' 取安装目录，'Exe' 取可执行文件全路径(端游用)；RelExe/TrimSegment 是 MuMu 那套
      "exe 在 nx_main\ 里、配置要填安装根"的协议参数 —— 值原样递给 verb，规则不在这里。
      无命中回 $null(setup.ps1 靠 if($r) 分流)；命中回 @{Value;Source;Candidates}，
      Candidates 恒为 $null(见头注)，Value/Source 就是旧实现的取用字段。
    #>
    param(
        [Parameter(Mandatory)][string[]]$ProcessNames,
        [Parameter(Mandatory)][string[]]$ExeNames,
        [Parameter(Mandatory)][string[]]$NameHints,
        [string[]]$UninstallLike = @(),
        [string]$PickProp = 'Dir',
        [string]$RelExe = '',
        [string]$TrimSegment = '',
        [switch]$SkipScan
    )
    $d = Invoke-GdCli -Verb 'detect-installs' -Body ([pscustomobject]@{
        processNames  = @($ProcessNames)
        exeNames      = @($ExeNames)
        nameHints     = @($NameHints)
        uninstallLike = @($UninstallLike)
        pickProp      = $PickProp
        relExe        = $RelExe
        trimSegment   = $TrimSegment
        skipScan      = [bool]$SkipScan
        roots         = @(Get-GdFixedRoot)
    })
    if (-not $d.Found) { return $null }
    return @{ Value = [string]$d.Value; Source = [string]$d.Source; Candidates = $null }
}

function Get-GdMuMuVmList {
    <#
      转发壳(批次 3)：MuMuManager info --vmindex all → @{Index;Name} 列表的解析真身在
      tools\panel-cs\Processes.cs(verb：parse-mumu-vms)。这里只保留两件壳内的事：
      起外部进程(归编排层，本批不迁)和把 Vms 数组还原成旧调用方吃的 [pscustomobject] 列表。
      改 JSON→列表的规则请改 C# 那份并跑 panel-cs --proctest 对真值。
    #>
    param([Parameter(Mandatory)][string]$Manager)
    if (-not (Test-Path -LiteralPath $Manager)) { return @() }
    $raw = try { & $Manager 'info' '--vmindex' 'all' 2>$null | Out-String } catch { '' }
    if (-not $raw.Trim()) { return @() }
    $d = Invoke-GdCli -Verb 'parse-mumu-vms' -Body ([pscustomobject]@{ raw = $raw })
    return @(foreach ($v in @($d.Vms)) { [pscustomobject]@{ Index = [string]$v.Index; Name = [string]$v.Name } })
}
