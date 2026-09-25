# lib/common.ps1 — 公共工具(PowerShell 7)
# 被 daily-clear.ps1 / status.ps1 / install-task.ps1 dot-source 引入

# 只用 1.0：允许访问不存在的 JSON 属性(返回 $null)，配置文件字段经常随版本变化
Set-StrictMode -Version 1.0
$ErrorActionPreference = 'Stop'
$PSDefaultParameterValues['*:Encoding'] = 'utf8'

# 中文输出别乱码：把当前进程的控制台/管道编码统一到 UTF-8
try {
    [System.Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    [System.Console]::InputEncoding  = [System.Text.Encoding]::UTF8
} catch { }

$global:GD = [pscustomobject]@{
    Root    = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    Logs    = $null
    State   = $null
    Backup  = $null
    LogFile = $null
}
$global:GD.Logs   = Join-Path $global:GD.Root 'logs'
$global:GD.State  = Join-Path $global:GD.Root 'state'
$global:GD.Backup = Join-Path $global:GD.Root 'backups'
foreach ($d in @($global:GD.Logs, $global:GD.State, $global:GD.Backup)) {
    if (-not (Test-Path -LiteralPath $d)) { New-Item -ItemType Directory -Path $d -Force | Out-Null }
}

function Get-GdRoot { $global:GD.Root }
function Get-GdDir  { param([string]$Name) $global:GD.$Name }

# 进程内缓存(StrictMode 下必须先声明再读)
$global:GdStageTable = $null

function Write-GdLog {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$Message,
        [ValidateSet('INFO', 'WARN', 'ERROR', 'OK', 'STEP')][string]$Level = 'INFO',
        [switch]$Quiet
    )
    $ts = Get-Date -Format 'HH:mm:ss'
    $line = "[$ts][$Level] $Message"
    if ($global:GD.LogFile) { Add-Content -LiteralPath $global:GD.LogFile -Value "[$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')]$line" -Encoding utf8 }
    if (-not $Quiet) {
        switch ($Level) {
            'ERROR' { Write-Host $line -ForegroundColor Red }
            'WARN'  { Write-Host $line -ForegroundColor Yellow }
            'OK'    { Write-Host $line -ForegroundColor Green }
            'STEP'  { Write-Host $line -ForegroundColor Cyan }
            default { Write-Host $line }
        }
    }
}

function Start-GdTranscript {
    param([Parameter(Mandatory)][string]$Tag)
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $global:GD.LogFile = Join-Path $global:GD.Logs "run-$stamp-$Tag.log"
    Write-GdLog "日志文件：$($global:GD.LogFile)"
}

function Read-GdJson {
    param([Parameter(Mandatory)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { throw "找不到文件：$Path" }
    $text = [System.IO.File]::ReadAllText($Path)
    if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
    return $text | ConvertFrom-Json
}

function Write-GdJson {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)]$Object,
        [int]$Depth = 100
    )
    $json = $Object | ConvertTo-Json -Depth $Depth
    [System.IO.File]::WriteAllText($Path, $json, (New-Object System.Text.UTF8Encoding($false)))
}

function Backup-GdFile {
    param([Parameter(Mandatory)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    $name = [System.IO.Path]::GetFileName($Path)
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $dest = Join-Path $global:GD.Backup "$name.$stamp.bak"
    Copy-Item -LiteralPath $Path -Destination $dest -Force
    # 只保留每个文件最近 20 份备份
    Get-ChildItem -LiteralPath $global:GD.Backup -Filter "$name.*.bak" |
        Sort-Object LastWriteTime -Descending | Select-Object -Skip 20 | Remove-Item -Force
    return $dest
}

# 该催哪几个助手目录 = 这个工程里真的装了哪几路的助手库。
# 合体工程里三个都在(行为与以前逐字一致)；只发一款时，另一个游戏的库不在树里，
# 就不该为一个玩家根本不会去装的路径天天报警 —— 那类警告只会让人以为 setup 没做完。
function Get-GdRequiredPathKeys {
    param([string]$Root)
    if (-not $Root) { $Root = $global:GD.Root }
    $keys = @()
    foreach ($pair in @(@{ Key = 'maaDir'; Lib = 'maa.ps1' }, @{ Key = 'm9aDir'; Lib = 'm9a.ps1' }, @{ Key = 'mumuDir'; Lib = 'mumu.ps1' })) {
        if (Test-Path -LiteralPath (Join-Path $Root "lib\$($pair.Lib)")) { $keys += $pair.Key }
    }
    return $keys
}

function Stop-GdBeforeRun {
    <#
      入口脚本专用的"这一轮没跑"退法：一句话写进日志、一句话打在控制台上，然后按 2 号退出码走人。
      比裸 throw 好的地方：throw 到了 PowerShell 顶层会变成一屏
        "Exception: C:\...\common.ps1:120 / Line | / 120 | throw ..."
      这种内部报错格式 —— 双击 exe 的玩家看了只会以为程序坏了(2026-09-24 的"exe 打不开"就有这一半原因)。
    #>
    param([Parameter(Mandatory)][string]$Message, [int]$Code = 2)
    Write-GdLog "这一轮没跑：$Message" -Level ERROR
    exit $Code
}

function Get-GdSettings {
    param([switch]$Quiet, [string[]]$RequirePaths, [switch]$TellAndExit)
    $path = Join-Path $global:GD.Root 'config\settings.json'
    if (-not (Test-Path -LiteralPath $path)) {
        $msg = '还没有 config\settings.json。先跑 setup.ps1(自动探测助手安装位置并生成)，或把 config\settings.example.json 复制成 settings.json 后手填 paths。'
        if ($TellAndExit) { Stop-GdBeforeRun $msg }
        throw $msg
    }
    $s = Read-GdJson -Path $path
    if (-not $RequirePaths) { $RequirePaths = @(Get-GdRequiredPathKeys) }
    $missing = @()
    foreach ($k in $RequirePaths) {
        $p = $s.paths.$k
        if (-not $p) {
            $missing += $k
            Write-GdLog "settings.json 的 paths.$k 是空的 → 跑 setup.ps1 自动探测助手安装位置，或把 config\settings.example.json 里那一项手填上" -Level WARN -Quiet:$Quiet
        }
        elseif (-not (Test-GdPathReady $p)) { Write-GdLog "settings.json 的 paths.$k 不存在：$p" -Level WARN -Quiet:$Quiet }
    }
    # 空路径 = 这一路压根跑不起来。要干活的入口(daily-clear / install-task 不带 -Quiet)在这里一次点齐所有空项：
    # 放它们走下去只会变成"无法将自变量绑定到参数 'Base'，因为它是空字符串"那种看不懂的绑定错误
    # —— 2026-09-24 exe 首跑撞的就是这个(setup 没探测 → dry-run 退出码 1 + 一句 PowerShell 内部报错)。
    # 只读诊断方(status.ps1 / 探针 / 自检)都是 -Quiet：它们要的就是"把没填的项继续列出来"，不该被这里挡住。
    if (-not $Quiet -and $missing.Count) {
        $msg = "paths 里这 $($missing.Count) 项还空着：$($missing -join ' , ') → 先跑 setup.ps1 自动探测助手安装位置(或手填 config\settings.json)"
        if ($TellAndExit) { Stop-GdBeforeRun $msg }
        throw $msg
    }
    return $s
}

function Test-GdPathReady {
    <# 填了且真的存在才算就绪。空串要自己挡掉：Test-Path -LiteralPath '' 在 5.1 和 7 上都是抛异常，不是返 $false #>
    param([AllowEmptyString()][string]$Path)
    if (-not $Path) { return $false }
    return [bool](Test-Path -LiteralPath $Path)
}

function Join-GdPath {
    <#
      base 没填(paths.* 是空串)时返回空串，而不是抛"无法将自变量绑定到参数 'Base'"：
      环境检查/体检要能把这一项显示成「未填写」，让访客看到的是一行"缺什么"，不是一段内部报错。
      真正会拿这个结果去干活的入口由 Get-GdSettings(非 -Quiet)先硬停，所以放宽这里不会把错误咽掉。
    #>
    param([AllowEmptyString()][string]$Base, [Parameter(Mandatory)][string[]]$Parts)
    if (-not $Base) { return '' }
    $p = $Base
    foreach ($part in @($Parts)) {
        if (-not $part) { continue }
        $p = Join-Path $p $part
    }
    return $p
}

# --- 防止系统在跑日常时睡眠/被屏保盖屏 ---
$global:GdSleepHandle = $false
$global:GdSaverRestore = $null    # $null=本轮没动屏保；$true/$false=收尾时要复原成的值
function Add-GdSleepTypeDef {
    if ($global:GdSleepHandle) { return }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class GdPower {
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern uint SetThreadExecutionState(uint esFlags);
    public const uint ES_CONTINUOUS       = 0x80000000;
    public const uint ES_SYSTEM_REQUIRED  = 0x00000001;
    public const uint ES_DISPLAY_REQUIRED = 0x00000002;

    // ES_DISPLAY_REQUIRED 只管显示器电源/睡眠计时；屏保看的是"最后一次输入"，
    // 脚本不动鼠标，到点照样全屏盖住端游窗口 → 必须显式关掉。
    [DllImport("user32.dll", EntryPoint = "SystemParametersInfo", SetLastError = true)]
    public static extern bool SPIGetActive(uint action, uint param, ref bool pvParam, uint flags);
    [DllImport("user32.dll", EntryPoint = "SystemParametersInfo", SetLastError = true)]
    public static extern bool SPISetActive(uint action, uint param, IntPtr pvParam, uint flags);
    public const uint SPI_GETSCREENSAVEACTIVE = 0x0010;
    public const uint SPI_SETSCREENSAVEACTIVE = 0x0011;
    public const uint SPIF_SENDWININICHANGE   = 0x0002;
}
'@ -ErrorAction Stop
    $global:GdSleepHandle = $true
}
function Get-GdScreensaverStateFile { Join-Path $global:GD.State 'screensaver.restore' }
function Get-GdScreensaverActive {
    Add-GdSleepTypeDef
    $v = $false
    if ([GdPower]::SPIGetActive([GdPower]::SPI_GETSCREENSAVEACTIVE, 0, [ref]$v, 0)) { return [bool]$v }
    return $null
}
function Set-GdScreensaverActive {
    param([Parameter(Mandatory)][bool]$Active)
    Add-GdSleepTypeDef
    $pv = if ($Active) { [uint32]1 } else { [uint32]0 }
    return [bool][GdPower]::SPISetActive([GdPower]::SPI_SETSCREENSAVEACTIVE, $pv, [IntPtr]::Zero,
                                         [GdPower]::SPIF_SENDWININICHANGE)
}
function Enter-GdNoSleep {
    param([bool]$KeepScreenOn = $false)
    try {
        Add-GdSleepTypeDef
        $flags = [GdPower]::ES_CONTINUOUS -bor [GdPower]::ES_SYSTEM_REQUIRED
        if ($KeepScreenOn) { $flags = $flags -bor [GdPower]::ES_DISPLAY_REQUIRED }
        [void][GdPower]::SetThreadExecutionState($flags)
        Write-GdLog "已临时禁止系统睡眠(跑完自动恢复)"
        if ($KeepScreenOn) {
            $sf = Get-GdScreensaverStateFile
            # 上一轮被硬杀(任务超时/断电)不会走 finally → 先照落盘值复原，
            # 否则这里会把"已被上轮关掉"误读成本来就关，屏保从此永久停用。
            if (Test-Path -LiteralPath $sf) {
                if ((Get-Content -LiteralPath $sf -First 1 -ErrorAction SilentlyContinue) -eq '1') {
                    [void](Set-GdScreensaverActive -Active $true)
                    Write-GdLog '上一轮遗留的屏保关闭已复原' -Level WARN
                }
                Remove-Item -LiteralPath $sf -Force -ErrorAction SilentlyContinue
            }
            $cur = Get-GdScreensaverActive
            if ($null -eq $cur) {
                Write-GdLog '读屏保状态失败，本轮不动屏保' -Level WARN
            } elseif ($cur) {
                $global:GdSaverRestore = $true
                if (Set-GdScreensaverActive -Active $false) {
                    Set-Content -LiteralPath $sf -Value '1' -Encoding ascii -NoNewline -ErrorAction SilentlyContinue
                    Write-GdLog "跑动期间已关闭屏保(跑完恢复)"
                } else {
                    $global:GdSaverRestore = $null
                    Write-GdLog '关闭屏保失败：屏保可能中途盖住游戏窗口' -Level WARN
                }
            } else {
                Write-GdLog '屏保本来就是关闭状态，无需改动'
            }
        }
    } catch { Write-GdLog "禁止睡眠失败：$($_.Exception.Message)" -Level WARN }
}
function Exit-GdNoSleep {
    try {
        if ($global:GdSleepHandle) { [void][GdPower]::SetThreadExecutionState([GdPower]::ES_CONTINUOUS) }
        if ($null -ne $global:GdSaverRestore) {
            [void](Set-GdScreensaverActive -Active $global:GdSaverRestore)
            Remove-Item -LiteralPath (Get-GdScreensaverStateFile) -Force -ErrorAction SilentlyContinue
            Write-GdLog '屏保已恢复'
            $global:GdSaverRestore = $null
        }
    } catch { }
}

# --- 进程工具 ---
function Test-GdProcessRunning {
    param([Parameter(Mandatory)][string]$Name)
    return [bool](Get-Process -Name $Name -ErrorAction SilentlyContinue)
}
function Stop-GdProcessTree {
    param([Parameter(Mandatory)][string]$Name, [switch]$Force)
    $procs = @(Get-Process -Name $Name -ErrorAction SilentlyContinue)
    if ($procs.Count -eq 0) { return 0 }
    foreach ($p in $procs) {
        try { if ($Force) { $p.Kill() } else { [void]$p.CloseMainWindow() } } catch { }
    }
    if (-not $Force) {
        $deadline = (Get-Date).AddSeconds(20)
        while ((Get-Date) -lt $deadline -and (Get-Process -Name $Name -ErrorAction SilentlyContinue)) { Start-Sleep -Milliseconds 800 }
        foreach ($p in @(Get-Process -Name $Name -ErrorAction SilentlyContinue)) { try { $p.Kill() } catch { } }
    }
    return $procs.Count
}

function Invoke-GdNative {
    <# 运行外部命令，返回 @{Code;Out;Err} #>
    param(
        [Parameter(Mandatory)][string]$File,
        [string[]]$Arguments = @(),
        [int]$TimeoutSeconds = 60
    )
    if (-not (Test-Path -LiteralPath $File)) { throw "命令不存在：$File" }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $File
    $psi.Arguments = ($Arguments -join ' ')
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.WorkingDirectory = Split-Path -Parent $File
    $p = [System.Diagnostics.Process]::Start($psi)
    $outTask = $p.StandardOutput.ReadToEndAsync()
    $errTask = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
        try { $p.Kill() } catch { }
        return @{ Code = -1; Out = ''; Err = "timeout after ${TimeoutSeconds}s" }
    }
    return @{ Code = $p.ExitCode; Out = $outTask.Result; Err = $errTask.Result }
}

# --- 日常编排通用两件：接管关闭 / 单步包异常(2026-09-23 从 daily-clear.ps1 搬来，两边行为一致) ---
function Invoke-GdTakeover {
    <# MAA/M9A 只有在自己退出时才会写配置，运行中改配置会被覆盖 -> 先接管关闭 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][string[]]$Names,
        [Parameter(Mandatory)][string]$Label
    )
    foreach ($n in $Names) {
        if (-not $n) { continue }
        if (-not (Test-GdProcessRunning -Name $n)) { continue }
        if (-not $S.runtime.takeOverAssistant) {
            throw "$Label($n) 正在运行；要么手动关掉再跑，要么把 settings.runtime.takeOverAssistant 设为 true"
        }
        Write-GdLog "$Label 正在运行 → 先关掉再改配置(避免它退出时把配置写回旧值)" -Level WARN
        [void](Stop-GdProcessTree -Name $n)
        Start-Sleep -Seconds 4
    }
}

function Invoke-GdStep {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Body)
    try {
        $r = & $Body
        if ($r.Status -ne 'ok') { Write-GdLog "$Name 结束状态：$($r.Status) — $($r.Detail)" -Level WARN }
        return $r
    } catch {
        return @{ Status = 'error'; Detail = "$Name 异常：$($_.Exception.Message)" }
    }
}

# --- 网络：Windows PowerShell 5.1 默认不启用 TLS 1.2，走 HTTPS 前必须先打开 ---
function Enable-GdTls12 {
    <#
      5.1 的 ServicePointManager 默认只到 TLS 1.0/1.1，多数 CDN 直接握手失败；PowerShell 7 默认就带 TLS 1.2+，
      所以这里只做"或上去"、不覆盖系统已有值，且整体包 try —— 拿不到就按引擎默认走，不该因此弄挂日常。
      调用方在 Invoke-RestMethod / Invoke-WebRequest 之前点一下即可(幂等，重复调用无副作用)。
    #>
    try {
        $cur = [int][System.Net.ServicePointManager]::SecurityProtocol
        if (-not ($cur -band 3072)) {   # 3072 = Tls12；Tls13 在 .NET Framework 上会被 setter 拒掉，故只补 1.2
            [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]($cur -bor 3072)
        }
    } catch { }
}

# --- 权限(端游 + MAA 要管理员) ---
function Test-GdAdmin {
    <# 当前进程是否已提权(管理员) #>
    try {
        $id = [Security.Principal.WindowsIdentity]::GetCurrent()
        return (New-Object Security.Principal.WindowsPrincipal($id)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } catch { return $false }
}

function Get-GdRelaunchArgList {
    <# 把已绑定的参数还原成命令行，用于 -Verb RunAs 重启自己(纯函数，便于自检) #>
    param(
        [Parameter(Mandatory)][string]$Script,
        [hashtable]$Bound = @{},
        [string[]]$Force = @(),
        [string[]]$Omit = @()
    )
    $skip = @($Omit) + @('DryRun')   # DryRun 默认不带：提权重启只为真实运行
    $out = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', ('"{0}"' -f $Script))
    foreach ($k in $Force) { $out += $k }
    foreach ($k in $Bound.Keys) {
        if ($k -in $skip) { continue }
        $v = $Bound[$k]
        if ($v -is [switch] -or $v -is [bool]) { if ($v) { $out += "-$k" }; continue }
        if ($null -eq $v) { continue }
        $out += "-$k"; $out += '"' + ([string]$v -replace '"', '\"') + '"'
    }
    return , $out
}

function Get-GdLocalPwsh {
    <# 找"能直接 RunAs"的 pwsh：本机安装版(Program Files)可以用；Store/WindowsApps 版不行 #>
    $cands = @()
    if ($PSHOME) { $cands += (Join-Path $PSHOME 'pwsh.exe') }
    $cands += 'C:\Program Files\PowerShell\7\pwsh.exe'
    foreach ($c in $cands) {
        if ($c -and (Test-Path -LiteralPath $c) -and $c -notmatch '\\WindowsApps\\') { return $c }
    }
    return $null
}

function Get-GdElevateBatchLine {
    <# 纯函数：拼出临时批处理里的那一行(便于单测，注意逗号优先级别再把参数拆散) #>
    param([Parameter(Mandatory)][string]$Pwsh, [Parameter(Mandatory)][string[]]$ArgList)
    $exe = if ($Pwsh -match '\s') { ('"{0}"' -f $Pwsh) } else { $Pwsh }
    return ('{0} {1}' -f $exe, ($ArgList -join ' '))
}

function Start-GdElevatedSelf {
    <#
      以管理员身份重跑当前脚本，等待子进程结束并返回其退出码；-1 表示 UAC 被取消/失败。
      为什么要 cmd 载体：pwsh 若是 Microsoft Store(WindowsApps) 版，ShellExecute 的 runas
       verb 提不了它，报错还会写成"操作已被用户取消"，看起来像用户点了"否"。
    #>
    param(
        [Parameter(Mandatory)][string]$Script,
        [hashtable]$Bound = @(),
        [string[]]$Force = @()
    )
    $argList = Get-GdRelaunchArgList -Script $Script -Bound $Bound -Force $Force
    $workdir = Split-Path -Parent $Script

    $local = Get-GdLocalPwsh
    if ($local) {
        try {
            $p = Start-Process -FilePath $local -ArgumentList ($argList -join ' ') -Verb RunAs -WorkingDirectory $workdir -PassThru
            $p.WaitForExit()
            return [int]$p.ExitCode
        } catch {
            Write-GdLog "本机 pwsh 提权失败($($_.Exception.Message)) → 改用 cmd 载体" -Level WARN
        }
    }

    $pwsh = (Get-Process -Id $PID -ErrorAction SilentlyContinue).Path
    if (-not $pwsh) { $pwsh = (Get-Command pwsh.exe -ErrorAction SilentlyContinue).Source }
    if (-not $pwsh) { throw '找不到 pwsh.exe，无法提权重跑' }

    $line = Get-GdElevateBatchLine -Pwsh $pwsh -ArgList $argList
    $tmp  = Join-Path $env:TEMP ('gd-elevate-{0}.cmd' -f (Get-Date -Format 'yyyyMMddHHmmss'))
    Set-Content -LiteralPath $tmp -Value "@echo off`r`nchcp 65001 >nul`r`n$line`r`nexit /b %ERRORLEVEL%" -Encoding ascii
    try {
        $cmdExe = Join-Path $env:SystemRoot 'System32\cmd.exe'
        $p = Start-Process -FilePath $cmdExe -ArgumentList @('/d', '/c', ('"{0}"' -f $tmp)) -Verb RunAs -WindowStyle Hidden -PassThru
        $p.WaitForExit()
        $code = [int]$p.ExitCode
    } catch {
        Write-GdLog "提权被取消或失败：$($_.Exception.Message)" -Level ERROR
        $code = -1
    }
    Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    return $code
}

# --- 通知 ---
function Send-GdNotice {
    param(
        [Parameter(Mandatory)][string]$Title,
        [Parameter(Mandatory)][string]$Text,
        [string]$Level = 'INFO'
    )
    $payload = @{ time = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'); title = $Title; text = $Text; level = $Level }
    $payload | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $global:GD.State 'last-notice.json') -Encoding utf8

    $settings = $null
    try { $settings = Read-GdJson -Path (Join-Path $global:GD.Root 'config\settings.json') } catch { }

    if ($settings -and $settings.notify.webhook) {
        try {
            Enable-GdTls12
            Invoke-RestMethod -Method Post -Uri $settings.notify.webhook -ContentType 'application/json' -TimeoutSec 10 -Body ($payload | ConvertTo-Json) | Out-Null
            Write-GdLog "已推送 webhook 通知" -Quiet
        } catch { Write-GdLog "webhook 推送失败：$($_.Exception.Message)" -Level WARN -Quiet }
    }
    if (-not $settings -or $settings.notify.toast) {
        try {
            Add-Type -AssemblyName System.Windows.Forms -ErrorAction Stop
            Add-Type -AssemblyName System.Drawing -ErrorAction Stop
            $ni = New-Object System.Windows.Forms.NotifyIcon
            $ni.Icon = [System.Drawing.SystemIcons]::Information
            $ni.BalloonTipIcon = [System.Windows.Forms.ToolTipIcon]::Info
            $ni.BalloonTipTitle = $Title
            $ni.BalloonTipText = ($Text -replace "`r?`n", ' ')
            $ni.Visible = $true
            $ni.ShowBalloonTip(12000)
            Start-Sleep -Seconds 3
            $ni.Dispose()
        } catch { Write-GdLog "桌面通知发送失败(不影响日常本身)：$($_.Exception.Message)" -Level WARN -Quiet }
    }
}

function Show-GdVersionFile {
    <# 体检用：从助手的 json 里挑一个能当版本号的字段，读不到只打一行黄字，绝不抛 #>
    param([Parameter(Mandatory)][string]$Label, [AllowEmptyString()][string]$Dir, [Parameter(Mandatory)][string]$File)
    if (-not $Dir) { Write-Host "  $Label 版本：助手目录还没填(跑 setup.ps1)" -ForegroundColor Yellow; return }
    $p = Join-Path $Dir $File
    if (-not (Test-Path -LiteralPath $p)) { return }
    try {
        $j = Read-GdJson -Path $p
        $v = if ($j.Update.Name) { $j.Update.Name } elseif ($j.version) { $j.version } else { '?' }
        Write-Host "  $Label 版本：$v($([System.IO.Path]::GetFileName($p))，$((Get-Item -LiteralPath $p).LastWriteTime.ToString('yyyy-MM-dd')))"
    } catch { Write-Host "  $Label 版本读取失败：$($_.Exception.Message)" -ForegroundColor Yellow }
}

function Add-GdHistory {
    param(
        [Parameter(Mandatory)][string]$Game,
        [Parameter(Mandatory)][string]$Status,
        [string]$Mode = '',
        [string]$Detail = '',
        $DurationSec = $null
    )
    $file = Join-Path $global:GD.State 'history.csv'
    if (-not (Test-Path -LiteralPath $file)) {
        '"time","game","mode","status","duration_sec","detail"' | Set-Content -LiteralPath $file -Encoding utf8
    }
    $dur = if ($null -ne $DurationSec) { "$([math]::Round($DurationSec))" } else { '' }
    $fields = @(
        (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'),
        $Game, $Mode, $Status, $dur, ($Detail -replace "`r?`n", ' | ')
    )
    $row = ($fields | ForEach-Object { '"' + ([string]$_ -replace '"', '""') + '"' }) -join ','
    Add-Content -LiteralPath $file -Value $row -Encoding utf8
}

# --- 活体标记(跨进程互斥用，面板只读不写) ---
# 为什么需要它：面板只看得见自己起的子进程。定时任务、命令行、另一个面板窗口起的轮次它一概不知道，
# 于是"再来一轮"会把两轮叠到同一个助手/端游上(实机撞过：19:11 从命令行点火，面板全程显示空闲)。
# 判据为什么是 PID 而不是"日志最近有没有新行"：实机量过，MAA 跑基建(Infrast)那一段能连着
# 10 分 41 秒一行都不写(2026-09-25 18:50:44 Recruit → 19:01:25 Infrast)，按日志新鲜度判会把
# "正在跑基建"看成空闲。只有进程活没活不会骗人。
function Start-GdRunLock {
    param(
        [Parameter(Mandatory)][string]$Games,
        [string]$Tag = ''
    )
    $file = Join-Path $global:GD.State 'running.json'
    # 已有别的轮在跑：这里只把话说出来，不拦。拦的活交给面板和计划任务那两侧 ——
    # 在判定层硬拦会让 06:00/18:00 撞上时整轮直接失败，而"失败"对无人值守的人来说
    # 比"多跑一遍"更难发现。要改成硬拦，得先有"两轮叠跑真把助手搞坏了"的证据。
    $prev = Get-GdRunLock
    if ($prev -and $prev.Alive) {
        Write-GdLog "已有一轮在跑(PID $($prev.Pid)，$($prev.Started) 起，games=$($prev.Games)) —— 本轮照跑，两轮会抢同一个助手/端游" -Level WARN
    }
    $obj = [ordered]@{
        pid     = $PID
        started = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
        games   = $Games
        tag     = $Tag
        log     = [string]$global:GD.LogFile
    }
    try {
        $obj | ConvertTo-Json -Compress | Set-Content -LiteralPath $file -Encoding utf8
    } catch {
        Write-GdLog "活体标记没写进去：$($_.Exception.Message)(面板这一轮就认不出有人在跑)" -Level WARN
    }
}

# 读标记并判活。返回 $null = 压根没有标记；.Alive=$false = 上一轮崩了没清掉的残留
# (残留按"没在跑"处理是刻意的：否则一次硬杀会把面板永久挡在门外)。
function Get-GdRunLock {
    $file = Join-Path $global:GD.State 'running.json'
    if (-not (Test-Path -LiteralPath $file)) { return $null }
    try {
        $o = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
        if ($null -eq $o.pid) { return $null }
        $alive = $true
        try {
            # 只问"有没有这个 PID"。Get-Process 在 5.1/7 上找不到进程时抛的类型并不一致
            # (ProcessException / InvalidOperationException 都见过)，所以不分类型，一律按"没了"处理。
            $p = Get-Process -Id ([int]$o.pid) -ErrorAction Stop
            $p.Dispose()
        } catch { $alive = $false }
        return [pscustomobject]@{
            Alive   = $alive
            Pid     = [int]$o.pid
            Started = [string]$o.started
            Games   = [string]$o.games
            Tag     = [string]$o.tag
            Log     = [string]$o.log
        }
    } catch { return $null }
}

function Stop-GdRunLock {
    $file = Join-Path $global:GD.State 'running.json'
    if (-not (Test-Path -LiteralPath $file)) { return }
    try {
        # 只删自己登记的那一份：万一别的轮已经覆盖了它，删就等于把别人的撞车保护拆了
        $txt = [string](Get-Content -LiteralPath $file -Raw)
        if ($txt -match ('"pid"\s*:\s*' + $PID + '[\s,}]')) { Remove-Item -LiteralPath $file -Force }
    } catch { }
}

# --- 状态文件(活动确认门等) ---
function Get-GdStateObject {
    param([Parameter(Mandatory)][string]$Name, $Default = $null)
    $file = Join-Path $global:GD.State "$Name.json"
    if (Test-Path -LiteralPath $file) { return Read-GdJson -Path $file }
    return $Default
}
function Set-GdStateObject {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)]$Value)
    Write-GdJson -Path (Join-Path $global:GD.State "$Name.json") -Object $Value
}

function Wait-GdCondition {
    param(
        [Parameter(Mandatory)][scriptblock]$Test,
        [Parameter(Mandatory)][int]$TimeoutSeconds,
        [int]$PollSeconds = 5,
        [string]$What = 'condition'
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (& $Test) { return $true }
        Start-Sleep -Seconds $PollSeconds
    }
    Write-GdLog "等待超时(${TimeoutSeconds}s)：$What" -Level WARN
    return $false
}

function Get-GdFileTail {
    <# 从指定字节偏移开始读取新增内容，返回 @{Text;Offset} #>
    param(
        [Parameter(Mandatory)][string]$Path,
        [long]$Offset = 0
    )
    if (-not (Test-Path -LiteralPath $Path)) { return @{ Text = ''; Offset = 0 } }
    $fs = [System.IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
    try {
        if ($Offset -gt $fs.Length) { $Offset = 0 }   # 日志被轮转
        [void]$fs.Seek($Offset, 'Begin')
        $sr = New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8)
        $text = $sr.ReadToEnd()
        return @{ Text = $text; Offset = $fs.Length }
    } finally { $fs.Dispose() }
}

function Get-GdWindowByTitle {
    <# 按主窗口标题找进程(MAA 就是靠窗口标题认端游的) #>
    param([Parameter(Mandatory)][string]$Pattern)
    foreach ($p in @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 })) {
        try { if ($p.MainWindowTitle -match $Pattern) {
            return [pscustomobject]@{ Name = $p.ProcessName; Id = $p.Id; Title = [string]$p.MainWindowTitle }
        } } catch { }
    }
    return $null
}

# --- 计划任务(免 UAC 的管理员通道) ---
function Get-GdTaskState {
    param([Parameter(Mandatory)][string]$Name)
    $t = Get-ScheduledTask -TaskName $Name -ErrorAction SilentlyContinue
    if (-not $t) { return $null }
    $info = $t | Get-ScheduledTaskInfo
    return [pscustomobject]@{
        State = [string]$t.State
        Last  = [string]$info.LastRunTime
        Result = $(try { [int]$info.LastTaskResult } catch { $null })
        Next  = [string]$info.NextRunTime
    }
}

# --- 任意"键值映射"读写(JSON 对象 / PSCustomObject / hashtable 都兼容) ---
function Get-GdMapValue {
    param($Map, [Parameter(Mandatory)][string]$Key)
    if ($null -eq $Map) { return $null }
    if ($Map -is [System.Collections.IDictionary]) {
        if ($Map.Contains($Key)) { return $Map[$Key] }
        return $null
    }
    $prop = $Map.PSObject.Properties[$Key]
    if ($prop) { return $prop.Value }
    return $null
}

function Set-GdMapValue {
    <# 返回被修改后的映射(hashtable 原地改，PSCustomObject 用 Add-Member -Force) #>
    param($Map, [Parameter(Mandatory)][string]$Key, $Value)
    if ($null -eq $Map) { throw 'Set-GdMapValue：映射为空' }
    if ($Map -is [System.Collections.IDictionary]) { $Map[$Key] = $Value; return $Map }
    $Map | Add-Member -NotePropertyName $Key -NotePropertyValue $Value -Force
    return $Map
}

function New-GdMap {
    return @{}
}

function Convert-GdMapToHashtable {
    param($Map)
    $h = @{}
    if ($null -eq $Map) { return $h }
    if ($Map -is [System.Collections.IDictionary]) {
        foreach ($k in $Map.Keys) { $h[[string]$k] = $Map[$k] }
        return $h
    }
    foreach ($p in $Map.PSObject.Properties) { $h[$p.Name] = $p.Value }
    return $h
}

function Format-GdUnixMs {
    param($Ms, [string]$Pattern = 'MM-dd HH:mm')
    if (-not $Ms) { return '?' }
    try {
        return ([DateTimeOffset]::FromUnixTimeMilliseconds([long]$Ms)).ToOffset([TimeSpan]::FromHours(8)).ToString($Pattern)
    } catch { return '?' }
}

# ---------- 本机专属值：两道扫描共用的判据(2026-09-25 从 tools\publish-split.ps1 挪上来) ----------
# 为什么在这儿：原来只有导出那一步扫得到，于是要等 publish-split 跑起来才看得见"源码里粘了本机路径"。
# 同一套判据挪进 lib\ 之后，tools\test-modules.ps1 [m8] 也能用它扫源码 —— 早一步红，就少一轮返工。
# 两个函数都不含任何一条本机字面量(值全部运行时从 settings.json / 注册表目录里取)，所以扫自己的时候不会命中自己。

function Get-GdLocalOnlyValues {
    <# 只有这台机器才成立的东西：settings.json 里的绝对路径、MuMu 虚拟机名、M9A 给每个实例生成的短 hash。
       这些东西一旦跟着源码出去，就等于把"我装在哪、我几点跑、我的实例叫什么"发给陌生仓库。
       值全部在运行时从本机配置里取，本文件里不出现任何一条字面量 —— 所以这道检查不会自己命中自己。 #>
    param([string]$LocalSettings)
    $vals = @()
    if (Test-Path -LiteralPath $LocalSettings) {
        $cfg = Read-GdJson -Path $LocalSettings
        foreach ($k in @('maaDir', 'm9aDir', 'mumuDir', 'arknightsExe')) {
            $v = [string]$cfg.paths.$k
            if ($v) { $vals += [pscustomobject]@{ Label = "paths.$k"; Value = $v } }
        }
        $hint = [string]$cfg.mumu.vmNameHint
        if ($hint) { $vals += [pscustomobject]@{ Label = 'mumu.vmNameHint'; Value = $hint } }
        # 密钥类同样算"本机专属"：视觉兜底的模型 key、通知 webhook(里面常带 token)。
        # 它们本该只活在 settings.json 里，但一旦有人在代码/README 里粘了一份，这道扫描要能抓到。
        $vkey = [string]$cfg.runtime.visionApiKey
        if ($vkey) { $vals += [pscustomobject]@{ Label = 'runtime.visionApiKey'; Value = $vkey } }
        $hook = [string]$cfg.notify.webhook
        if ($hook) { $vals += [pscustomobject]@{ Label = 'notify.webhook'; Value = $hook } }
        # 单游戏仓(方舟那一路)和新机器上 m9a 那几项可能压根没配 → 别把 $null 塞进 Join-GdPath 的
        # -Parts(它是 Mandatory，绑 null 会直接抛"无法将自变量绑定到参数 'Parts'")。
        $m9aDir = [string]$cfg.paths.m9aDir
        $instRel = [string]$cfg.paths.m9aInstancesDir
        $instDir = if ($m9aDir -and $instRel) { Join-GdPath $m9aDir @($instRel) } else { '' }
        if ($instDir -and (Test-Path -LiteralPath $instDir)) {
            $i = 0
            foreach ($f in @(Get-ChildItem -LiteralPath $instDir -Filter '*.json')) {
                $i++
                $vals += [pscustomobject]@{ Label = "m9a-instance-id#$i"; Value = $f.BaseName }
            }
        }
    }
    $vals += [pscustomobject]@{ Label = 'env.USERPROFILE'; Value = $env:USERPROFILE }
    return @($vals | Where-Object { $_.Value -and $_.Value.Trim().Length -ge 6 } |
        Sort-Object -Property Value -Unique | ForEach-Object { [pscustomobject]@{ Label = $_.Label; Value = $_.Value.Trim() } })
}

function Test-GdExportLeaks {
    <# 对一个目录做一遍"本机数据"扫描；返回 "相对路径 <- 是哪一项" 的列表，空 = 干净。
       两处调用：publish-split 扫导出目录(发出去之前那道闸)，test-modules [m8] 扫工作副本的源码。
       跳过运行时产物(logs/state/backups)、.git 和 settings.json 本身 —— 那些本来就该带着本机值。 #>
    param([string]$Dir, [object[]]$Values)
    if (-not $Values -or -not @($Values).Count) { return @() }
    $hits = @()
    foreach ($f in @(Get-ChildItem -LiteralPath $Dir -Recurse -File |
            Where-Object { $_.FullName -notmatch '\\(logs|state|backups|\.git)\\' -and $_.FullName -notmatch '\\config\\settings\.json$' -and $_.Extension -notin '.png', '.ico', '.exe', '.dll', '.zip' })) {
        $text = Get-Content -LiteralPath $f.FullName -Raw -Encoding utf8
        if (-not $text) { continue }
        $rel = $f.FullName.Substring($Dir.Length + 1)
        foreach ($v in $Values) {
            if ($text.IndexOf([string]$v.Value, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                $hits += "$rel <- $($v.Label)"
            }
        }
    }
    return @($hits | Sort-Object -Unique)
}
