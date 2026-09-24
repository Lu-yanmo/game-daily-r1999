# lib/detect.ps1 — setup.ps1 用的安装位置探测(纯查找，不写任何文件)

$script:GdSkipSegments = @('windows', 'appdata', 'programdata', 'windowsapps', '$recycle.bin',
                           'system volume information', 'winsxs', 'temp', 'node_modules', '.git')
# 下载/桌面/解压目录里的副本经常是"下过没删的旧版本"，排序时压到最后(实测本机就有两份 Downloads 里的 M9A)
$script:GdJunkSegments = @('downloads', 'desktop', 'temp', 'tmp', 'inbox', '备份', 'backup', 'backups', '解压', 'extract', 'extracted')

function Test-GdJunkPath {
    param([Parameter(Mandatory)][string]$Path)
    $low = $Path.ToLowerInvariant()
    foreach ($seg in $script:GdJunkSegments) { if ($low -match "[\\/]$([regex]::Escape($seg))[\\/]") { return $true } }
    # "xx (2)\m9a.exe" 这类浏览器重命名的副本目录
    if ($low -match '[\\/]\(\d+\)[\\/]') { return $true }
    return $false
}

function Format-GdCandidates {
    <# 把候选列表压成一行给人看(嵌套引号在双引号字符串里容易出事，所以单独做) #>
    param([object[]]$Pool, [string]$Prop = 'Dir', [int]$First = 3)
    $vals = @($Pool | Where-Object { $_ } | ForEach-Object { [string]$_.$Prop } | Select-Object -Unique | Select-Object -First $First)
    return ($vals -join ' ; ')
}

function Get-GdFixedRoot {
    <# 本机所有固定磁盘根目录(C:\ D:\ E:\ ...)，可移动盘和光驱不算 #>
    @(try {
        [System.IO.DriveInfo]::GetDrives() | Where-Object { $_.DriveType -eq [System.IO.DriveType]::Fixed -and $_.IsReady } |
            ForEach-Object { $_.RootDirectory.FullName.TrimEnd('\') + '\' }
    } catch { @('C:\') })
}

function Test-GdSkipPath {
    param([Parameter(Mandatory)][string]$Path)
    $low = $Path.ToLowerInvariant()
    foreach ($seg in $script:GdSkipSegments) { if ($low -match "[\\/]$([regex]::Escape($seg))[\\/]") { return $true } }
    return $false
}

function Get-GdChildDir {
    <# 子目录：跳过软链/junction(否则会绕圈)，异常目录(权限)直接忽略 #>
    param([Parameter(Mandatory)][string]$Path)
    try {
        [System.IO.Directory]::GetDirectories($Path) | ForEach-Object {
            try {
                $di = New-Object System.IO.DirectoryInfo($_)
                if (($di.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) { $_ }
            } catch { $_ }
        }
    } catch { @() }
}

function Find-GdInstallDir {
    <#
      在固定磁盘的浅层目录里找"目录名像助手 + 里面有指定 exe"的位置。
      规则：命中的目录继续往下走；没命中的目录只多走两层(为了覆盖 C:\Program Files\NetEase\MuMu Player 12 这种)。
      返回按"最近修改"倒序的候选列表，调用方决定取哪个、要不要问用户。
    #>
    param(
        [Parameter(Mandatory)][string[]]$ExeNames,
        [Parameter(Mandatory)][string[]]$NameHints,
        [int]$MaxDepth = 4,
        [string[]]$Roots = (Get-GdFixedRoot)
    )
    $found = New-Object System.Collections.Generic.List[object]
    $queue = New-Object System.Collections.Generic.Queue[object]
    foreach ($r in $Roots) { $queue.Enqueue(@{ Path = $r; Depth = 0 }) }
    $guard = 0
    while ($queue.Count -gt 0 -and $guard -lt 40000) {
        $guard++
        $cur = $queue.Dequeue()
        $depth = [int]$cur.Depth
        foreach ($child in (Get-GdChildDir -Path $cur.Path)) {
            if (Test-GdSkipPath -Path $child) { continue }
            $leaf = Split-Path -Leaf $child
            $hit = $false
            foreach ($hint in $NameHints) { if ($leaf -like $hint) { $hit = $true; break } }
            if ($hit) {
                foreach ($exe in $ExeNames) {
                    $full = Join-Path $child $exe
                    if (Test-Path -LiteralPath $full) {
                        $ti = try { (Get-Item -LiteralPath $full).LastWriteTime } catch { Get-Date 0 }
                        $found.Add([pscustomobject]@{ Dir = $child; Exe = $full; LastWrite = $ti; Junk = (Test-GdJunkPath -Path $child) })
                    }
                }
            }
            if ($hit -or $depth + 1 -lt $MaxDepth) {
                if (($depth + 1) -lt $MaxDepth) { $queue.Enqueue(@{ Path = $child; Depth = $depth + 1 }) }
            }
        }
    }
    # 正常安装目录优先，其次才看下载/备份目录里的副本；同级内按最近改动
    return @($found | Sort-Object @{ Expression = { $_.Junk } }, @{ Expression = 'LastWrite'; Descending = $true })
}

function Get-GdRunningExeDir {
    <# 正在跑的进程最可信：直接拿它的可执行文件所在目录(非提权进程读不到 Path，忽略即可) #>
    param([Parameter(Mandatory)][string[]]$ProcessNames, [Parameter(Mandatory)][string[]]$ExeNames)
    $out = @()
    foreach ($pn in $ProcessNames) {
        $paths = @()
        try { $paths = @(Get-Process -Name $pn -ErrorAction SilentlyContinue | ForEach-Object { $_.Path }) } catch { $paths = @() }
        foreach ($p in (@($paths | Where-Object { $_ }) | Select-Object -Unique)) {
            if ($ExeNames -contains (Split-Path -Leaf $p)) {
                $out += [pscustomobject]@{ Dir = (Split-Path -Parent $p); Exe = $p; LastWrite = (Get-Date); Junk = $false }
            }
        }
    }
    return $out
}

function Get-GdUninstallHintDir {
    <# 卸载表里登记的目录：MuMu / 端游这类商业软件常只在这里能找到 #>
    param(
        [Parameter(Mandatory)][string[]]$DisplayNameLike,
        [Parameter(Mandatory)][string[]]$ExeNames
    )
    $found = @()
    foreach ($h in (Get-GdUninstallHint -DisplayNameLike $DisplayNameLike)) {
        foreach ($v in @($h.Value)) {
            $cand = ($v -replace '^[^,;]*,', '') -replace '"', ''
            $cand = $cand.Trim()
            if (-not $cand) { continue }
            $dir = if (Test-Path -LiteralPath $cand -PathType Leaf) { Split-Path -Parent $cand } else { $cand }
            foreach ($exe in $ExeNames) {
                $full = Join-Path $dir $exe
                if (Test-Path -LiteralPath $full) { $found += [pscustomobject]@{ Dir = $dir; Exe = $full; LastWrite = (Get-Date); Junk = $false; Via = "卸载表[$($h.Name)]" } }
                $nested = Join-Path (Join-Path $dir 'nx_main') $exe
                if (Test-Path -LiteralPath $nested -PathType Leaf) { $found += [pscustomobject]@{ Dir = $dir; Exe = $nested; LastWrite = (Get-Date); Junk = $false; Via = "卸载表[$($h.Name)]" } }
            }
        }
    }
    return $found
}

function Get-GdUninstallHint {
    <# 卸载表里的 InstallLocation：MuMu 这类商业软件常只在这里登记 #>
    param([Parameter(Mandatory)][string[]]$DisplayNameLike)
    $keys = @(
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    $out = @()
    foreach ($k in $keys) {
        $items = try { Get-ItemProperty -Path $k -ErrorAction SilentlyContinue } catch { $null }
        foreach ($it in @($items)) {
            if (-not $it) { continue }
            $dn = [string]$it.DisplayName
            foreach ($pat in $DisplayNameLike) {
                if ($dn -like $pat) {
                    foreach ($prop in @('InstallLocation', 'InstallSource', 'DisplayIcon')) {
                        $v = [string]$it.$prop
                        if ($v) { $out += [pscustomobject]@{ Name = $dn; Prop = $prop; Value = $v } }
                    }
                }
            }
        }
    }
    return $out
}

function Add-GdTrimmedDir {
    <# 把「其实指向安装子目录」的候选往上退一层，保证配置里填的是安装根。
       只处理末级目录名正好等于 $TrimSegment 的项；Exe 改写成 根目录\RelExe(相对路径已含子目录)。 #>
    param([object[]]$Pool, [Parameter(Mandatory)][string]$TrimSegment, [string]$RelExe = '')
    $out = @()
    foreach ($c in @($Pool | Where-Object { $_ })) {
        $dir = [string]$c.Dir
        if ($dir -and ((Split-Path -Leaf $dir) -ieq $TrimSegment)) {
            $parent = Split-Path -Parent $dir
            if ($parent) {
                $exe = if ($RelExe) { Join-Path $parent $RelExe } else { $c.Exe }
                $via = [string]$c.Via
                $out += [pscustomobject]@{ Dir = $parent; Exe = $exe; LastWrite = $c.LastWrite; Junk = $c.Junk; Via = $(if ($via) { $via } else { '由子目录退回安装根' }) }
                continue
            }
        }
        $out += $c
    }
    return @($out)
}

function Select-GdUniqueDir {
    <# 按 Dir 去重，保留排序靠前的那个(排序已经是「非副本 + 最近改动」优先) #>
    param([object[]]$Pool)
    $seen = @{}; $out = @()
    foreach ($c in @($Pool | Where-Object { $_ })) {
        $key = ([string]$c.Dir).TrimEnd('\').ToLowerInvariant()
        if (-not $key -or $seen.ContainsKey($key)) { continue }
        $seen[$key] = $true; $out += $c
    }
    return @($out)
}

function Resolve-GdCandidate {
    <#
      一类软件的探测合并成一次调用：正在跑的进程 → 卸载表 → 磁盘浅层扫描。
      PickProp='Dir' 取安装目录，'Exe' 取可执行文件全路径(端游用)。
      RelExe：exe 不在安装目录根上时(MuMu 的 MuMuManager.exe 在 nx_main\ 里)传相对路径。
      TrimSegment：安装根目录要往上退一层时用(MuMu 的卸载表/直接命中都可能给到 nx_main，
                   而 settings 里 paths.mumuManager 已经带 nx_main\ 前缀，填成子目录会拼出双层路径)。
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
    $pool = @()
    $pool += @(Get-GdRunningExeDir -ProcessNames $ProcessNames -ExeNames $ExeNames)
    if ($UninstallLike.Count) { $pool += @(Get-GdUninstallHintDir -DisplayNameLike $UninstallLike -ExeNames $ExeNames) }
    if (-not $SkipScan) {
        $exeList = if ($RelExe) { @($RelExe) } else { $ExeNames }
        $pool += @(Find-GdInstallDir -ExeNames $exeList -NameHints $NameHints)
    }
    if ($TrimSegment) {
        $pool = @(Add-GdTrimmedDir -Pool $pool -TrimSegment $TrimSegment -RelExe $(if ($RelExe) { $RelExe } elseif ($ExeNames.Count) { $ExeNames[0] } else { '' }))
    }
    $pool = @($pool | Where-Object { $_ } | Sort-Object -Stable @{ Expression = { [bool]$_.Junk } }, @{ Expression = 'LastWrite'; Descending = $true })
    if ($TrimSegment) { $pool = @(Select-GdUniqueDir -Pool $pool) }
    if (-not $pool.Count) { return $null }
    $pick = $pool[0]
    $shown = Format-GdCandidates -Pool $pool -Prop $PickProp
    $src = "探测：候选 $($pool.Count) 个，取「$($pick.$PickProp)」(前几个：$shown)"
    if ($pick.Junk) { $src += ' ← 这个位置像下载/备份副本，确认是不是你要用的那份' }
    return @{ Value = [string]$pick.$PickProp; Source = $src; Candidates = $pool }
}

function Get-GdMuMuVmList {
    <# MuMuManager info --vmindex all → @{Index;Name} 列表(装好 MuMu 后靠它填 vmIndex/vmNameHint) #>
    param([Parameter(Mandatory)][string]$Manager)
    if (-not (Test-Path -LiteralPath $Manager)) { return @() }
    $raw = try { & $Manager 'info' '--vmindex' 'all' 2>$null | Out-String } catch { '' }
    if (-not $raw.Trim()) { return @() }
    try { $j = $raw | ConvertFrom-Json } catch { return @() }
    $out = @()
    foreach ($prop in $j.PSObject.Properties) {
        $out += [pscustomobject]@{ Index = [string]$prop.Name; Name = [string]$prop.Value.name }
    }
    return $out
}
