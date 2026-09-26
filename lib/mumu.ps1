# lib/mumu.ps1 — MuMu 模拟器 12 的启停与 adb 探活
#
# ── 冻结(2026-09-26，PS→C# 批次 3)────────────────────────────────────────────
# 三段"给一段输出现一个结论"的规则已迁 tools\panel-cs\Processes.cs：
#   Get-GdMuMuState 的 JSON 解读段(verb：parse-mumu-state)、
#   Connect-GdAdb 的 devices 判定段(parse-adb-devices)、Get-Gd1999Pid 的 pidof 剥数字段(parse-adb-pidof)。
# 这里留下的只有起外部进程(Invoke-GdNative)与"目录没填"的现场检查 —— 启停编排本身归批次 5/6。
# 改这些判定/文案 = 改 C# + 跑 tools\probe-process-golden.ps1 对拍，不要在这里加规则。
# ──────────────────────────────────────────────────────────────────────────────

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
    # 转发壳：非零退出/空输出/坏 JSON/三字段解读全在 C#(parse-mumu-state)，这里只起进程与递交 {code,out,err}。
    $d = Invoke-GdCli -Verb 'parse-mumu-state' -Body ([pscustomobject]@{ code = [int]$r.Code; out = [string]$r.Out; err = [string]$r.Err })
    return [pscustomobject]@{
        Available = [bool]$d.Available; AndroidStarted = [bool]$d.AndroidStarted
        ProcessStarted = [bool]$d.ProcessStarted; Name = [string]$d.Name; Raw = [string]$d.Raw
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
    # 转发壳：serial 转义匹配 + "\tdevice" 两条判据在 C#(parse-adb-devices)
    $d = Invoke-GdCli -Verb 'parse-adb-devices' -Body ([pscustomobject]@{ out = [string]$r.Out; serial = $Serial })
    return [bool]$d.Ok
}

function Get-Gd1999Pid {
    param([Parameter(Mandatory)][string]$AdbPath, [Parameter(Mandatory)][string]$Serial, [Parameter(Mandatory)][string]$Package)
    $r = Invoke-GdNative -File $AdbPath -Arguments @('-s', $Serial, 'shell', 'pidof', $Package) -TimeoutSeconds 25
    # 转发壳：剥非数字/取 pid 在 C#(parse-adb-pidof)；Found:false = "游戏没在跑"，翻回旧语义的 $null
    $d = Invoke-GdCli -Verb 'parse-adb-pidof' -Body ([pscustomobject]@{ out = [string]$r.Out })
    if ($d.Found) { return [int]$d.Pid }
    return $null
}
