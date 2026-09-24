# lib/mumu.ps1 — MuMu 模拟器 12 的启停与 adb 探活

function Get-GdMuMu {
    param([Parameter(Mandatory)]$S)
    return [pscustomobject]@{
        Manager = (Join-GdPath $S.paths.mumuDir @($S.paths.mumuManager))
        Adb     = (Join-GdPath $S.paths.mumuDir @($S.paths.mumuAdb))
        Index   = [string]$S.mumu.vmIndex
        Name    = [string]$S.mumu.vmNameHint
    }
}

function Get-GdMuMuState {
    param([Parameter(Mandatory)]$S)
    $m = Get-GdMuMu -S $S
    # MuMu 目录还没填(访客没跑 setup)时别把空串喂给 Invoke-GdNative：那会变成
    # "无法将自变量绑定到参数 'File'，因为它是空字符串" —— 只读体检里看到的就是一句内部报错。
    if (-not (Test-GdPathReady $m.Manager)) {
        return [pscustomobject]@{ Available = $false; Raw = 'MuMu 目录还没填(先跑 setup.ps1)'; AndroidStarted = $false; ProcessStarted = $false }
    }
    $r = Invoke-GdNative -File $m.Manager -Arguments @('info', '-v', $m.Index) -TimeoutSeconds 40
    if ($r.Code -ne 0 -or -not $r.Out.Trim()) {
        return [pscustomobject]@{ Available = $false; Raw = ($r.Out + $r.Err); AndroidStarted = $false; ProcessStarted = $false }
    }
    try {
        $j = $r.Out | ConvertFrom-Json
        return [pscustomobject]@{
            Available = $true; AndroidStarted = [bool]$j.is_android_started
            ProcessStarted = [bool]$j.is_process_started; Name = [string]$j.name; Raw = $r.Out
        }
    } catch {
        return [pscustomobject]@{ Available = $false; AndroidStarted = $false; ProcessStarted = $false; Raw = $r.Out }
    }
}

function Start-GdMuMu {
    param([Parameter(Mandatory)]$S, [switch]$DryRun)
    $m = Get-GdMuMu -S $S
    $st = Get-GdMuMuState -S $S
    if (-not $st.Available) { throw "MuMuManager 无法读取虚拟机信息($($m.Index))：$($st.Raw)" }
    if ($st.AndroidStarted) { Write-GdLog "MuMu 虚拟机 $($m.Index)($($st.Name))已在运行"; return $true }

    Write-GdLog "启动 MuMu 虚拟机 $($m.Index)($($st.Name))" -Level STEP
    if ($DryRun) { Write-GdLog '[dry-run] 跳过实际启动' -Level STEP; return $false }
    $r = Invoke-GdNative -File $m.Manager -Arguments @('control', '-v', $m.Index, 'launch') -TimeoutSeconds 120
    if ($r.Code -ne 0) { Write-GdLog "launch 返回码 $($r.Code)：$($r.Err)$($r.Out)" -Level WARN }
    return (Wait-GdCondition -Test { (Get-GdMuMuState -S $S).AndroidStarted } -TimeoutSeconds ([int]$S.runtime.mumuBootTimeoutSeconds) -PollSeconds 6 -What 'MuMu 安卓系统启动')
}

function Stop-GdMuMu {
    param([Parameter(Mandatory)]$S)
    $m = Get-GdMuMu -S $S
    $st = Get-GdMuMuState -S $S
    if (-not ($st.AndroidStarted -or $st.ProcessStarted)) { Write-GdLog 'MuMu 未在运行，无需关闭'; return }
    Write-GdLog "关闭 MuMu 虚拟机 $($m.Index)" -Level STEP
    [void](Invoke-GdNative -File $m.Manager -Arguments @('control', '-v', $m.Index, 'shutdown') -TimeoutSeconds 120)
}

function Connect-GdAdb {
    param([Parameter(Mandatory)]$S, [Parameter(Mandatory)][string]$AdbPath, [Parameter(Mandatory)][string]$Serial)
    if (-not (Test-Path -LiteralPath $AdbPath)) { throw "adb 不存在：$AdbPath" }
    [void](Invoke-GdNative -File $AdbPath -Arguments @('connect', $Serial) -TimeoutSeconds 30)
    $r = Invoke-GdNative -File $AdbPath -Arguments @('devices') -TimeoutSeconds 30
    return ($r.Out -match [regex]::Escape($Serial) -and $r.Out -match "`tdevice")
}

function Get-Gd1999Pid {
    param([Parameter(Mandatory)][string]$AdbPath, [Parameter(Mandatory)][string]$Serial, [Parameter(Mandatory)][string]$Package)
    $r = Invoke-GdNative -File $AdbPath -Arguments @('-s', $Serial, 'shell', 'pidof', $Package) -TimeoutSeconds 25
    $digits = ($r.Out -replace '\D', '')
    if ($digits) { return [int]$digits }
    return $null
}
