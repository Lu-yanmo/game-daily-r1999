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

# ---------- 版本号：全工程只有一处写着它，就是仓库根那个 VERSION 文件 ----------
# 为什么要单独一个文件：以前版本只活在 git tag 上(v2026.09.26 那次临时起的)，代码里、exe 上、面板上
# 都没有"这是哪一版"的读数 —— 玩家报问题时第一句"你用的哪版"就没法答。做成文件之后：
# PowerShell 读它(Get-GdVersion)、C# 面板读它(标题栏 + 诊断包里的 版本.txt)、exe 读它(--version)、
# build-exe 拿它盖进产物的文件属性(AssemblyVersion / AssemblyFileVersion)，打 tag 也用它。
# 五个下游同一个来源；tools\test-modules.ps1 的 [m12] 钉住"不许有第二处写死版本号"(注释里写一遍也算第二处)。
$global:GdVersionCache = $null
function Get-GdVersion {
    param([string]$File, [switch]$Refresh)
    # -File 只给自检喂假文件用(规矩：只读自检不许要求机器现场状态)，这一支不碰缓存。
    $path = Join-Path $global:GD.Root 'VERSION'
    if ($File) { $path = $File }
    elseif ($global:GdVersionCache -and -not $Refresh) { return $global:GdVersionCache }
    $v = ''
    if (Test-Path -LiteralPath $path) {
        try { $v = ([string](Get-Content -LiteralPath $path -TotalCount 1)).Trim() } catch { $v = '' }
    }
    # 读不到 / 形状不对都不抛：体检和面板是只读的，宁可说"版本号读不出来"也别把整段体检带走。
    if ($v -notmatch '^\d+\.\d+\.\d+$') { $v = '0.0.0' }
    if (-not $File) { $global:GdVersionCache = $v }
    return $v
}

# 进程内缓存(StrictMode 下必须先声明再读)
$global:GdStageTable = $null
# Python 定位结果：GdPythonExe 存找到的路径(空串=确实没有)，GdPythonProbed 区分"还没查过"和"查过没有"
# —— 少了后者的话，没装 Python 的机器上每问一次模型就要重跑一遍 `python -c`(还会撞上商店占位程序)
$global:GdPythonExe = ''
$global:GdPythonProbed = $false

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
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Object, [int]$Depth = 100)
    $json = $Object | ConvertTo-Json -Depth $Depth
    [System.IO.File]::WriteAllText($Path, $json, (New-Object System.Text.UTF8Encoding($false)))
}

# ---------- PS→C# 转发(批次 1 起判定层的真身住在 tools\panel-cs\Decisions.cs) ----------
# 契约全文在 docs\ps-migration-batch0.md §2：机读结果只认 --out 信封文件，stdout 不参与判定。
# 探针 tools\probe-cli-contract.ps1 已在 5.1/7.6 双引擎把这套握手钉绿，这里是它的正式化。
function Get-GdCliExe {
    <# 判定层 exe 是编译产物、永不入仓(.gitignore 的规矩，和 C# 面板现编现用同源)：
       没编译过/被人清掉 → 现场自愈编一次(build.ps1 -Quiet 里已含 System.Web.Extensions 引用链)；
       连 csc 都没有(Quiet 退 3)或编失败 → 才说一句能照着做的人话。 #>
    $exe = Join-Path $global:GD.Root 'tools\panel-cs\panel-cs.exe'
    if (-not (Test-Path -LiteralPath $exe)) {
        Write-GdLog 'C# 判定层未编译，现场编一次(之后复用)' -Level WARN
        # 必须掐掉这条调用的成功流：csc 的一句 warning(如 CS0219)会原样流进 build.ps1 的输出，
        # 而这个 & 写在函数体内 —— 返回值会被污染成 Object[]，首元素是"…\tools\panel-cs\x.cs(行,列): warning…"。
        # 之后 `& $exe` 拿到相对路径形态的串会被 PowerShell 按"模块\命令"解析，炸出
        # CouldNotAutoLoadModule"无法加载模块 'tools'"(2026-09-27 导出验收链两树各红一步的根因)。
        & (Join-Path $global:GD.Root 'tools\panel-cs\build.ps1') -Quiet | Out-Null
        if (-not (Test-Path -LiteralPath $exe)) { throw "C# 判定层编译失败：跑 tools\panel-cs\build.ps1 看具体报错(产物 $exe 仍不存在)" }
    }
    return $exe
}

function Invoke-GdCli {
    <#
      唯一转发通道：把 Body 序列化成临时 JSON 走 --in，读回 --out 信封后把 data 交出去。
      退出码语义(0 完成 / 1 内部错 / 2 前置缺或规则拒)：非 0 一律 throw —— 判定层拿不到可信结论时，
      宁可这一轮拒跑，也不带着猜出来的配置继续跑(与 Stop-GdBeforeRun 同哲学)。
      throw 的是信封 error 原话：Resolve-R1999Instance 的调用方(Show-GdDecision)会把这句并进 Reason。
    #>
    param(
        [Parameter(Mandatory)][string]$Verb,
        [Parameter(Mandatory)]$Body,
        [string[]]$More = @()
    )
    $stamp = [guid]::NewGuid().ToString('N')
    $inPath = Join-Path ([System.IO.Path]::GetTempPath()) "gd-cli-$stamp.in.json"
    $outPath = Join-Path ([System.IO.Path]::GetTempPath()) "gd-cli-$stamp.out.json"
    try {
        Write-GdJson -Path $inPath -Object $Body -Depth 32
        $exe = Get-GdCliExe
        & $exe $Verb "--in=$inPath" "--out=$outPath" @($More) | Out-Null
        $code = $LASTEXITCODE
        if (-not (Test-Path -LiteralPath $outPath)) { throw "C# 侧($Verb)没吐出结果文件(退出码 $code)" }
        $env0 = Read-GdJson -Path $outPath
        if ($null -eq $env0 -or [string]$env0.verb -ne $Verb) { throw "C# 侧($Verb)信封不对或缺 verb 字段" }
        if ([int]$env0.exit -ne $code) { throw "C# 侧($Verb)退出码($code)与信封 exit($($env0.exit))不一致" }
        if (-not $env0.ok) { throw ([string]$env0.error) }
        return $env0.data
    } finally {
        Remove-Item -LiteralPath $inPath, $outPath -Force -ErrorAction SilentlyContinue
    }
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
$global:GdDeskTypeDef = $false    # Get-GdInputDesktopName 的 Add-Type 只做一次
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
function Get-GdScreensaverConfig {
    <# 只读注册表：这台机器的屏保"现在是怎么装的"。刻意不碰 SPI ——
       SPI 现值在会话已被屏保接管时和注册表不一致(2026-09-26 06:00：日志「关闭屏保失败」，
       注册表 ScreenSaveActive 仍是 1)，而体检要看的正是**下次到点会不会触发**那个稳态。
       也正因为它是稳态：任何程序(实测 Wallpaper Engine 的 wpxscreensaver64.scr)把它改回去，这里当场看得见。
       读不到返回 $null —— 调用方一律按"不知道"处理，绝不许据此判"没装屏保"。 #>
    param([string]$RegPath = 'HKCU:\Control Panel\Desktop')
    $dp = $null
    try { $dp = Get-ItemProperty -LiteralPath $RegPath -ErrorAction Stop } catch { return $null }
    if (-not $dp) { return $null }
    $scr = ''
    try { $scr = [string]$dp.'SCRNSAVE.EXE' } catch { $scr = '' }
    $prod = ''
    if ($scr) { try { $prod = [string](Get-Item -LiteralPath $scr -ErrorAction Stop).VersionInfo.ProductName } catch { } }
    $to = 0
    try { if ($dp.ScreenSaveTimeOut) { $to = [int]$dp.ScreenSaveTimeOut } } catch { }
    $act = ''
    try { $act = [string]$dp.ScreenSaveActive } catch { $act = '' }
    return [pscustomobject]@{ Saver = $scr; Product = $prod; Active = $act; TimeoutSec = $to }
}

function Get-GdScreensaverWho {
    <# 纯函数：这个 .scr 是谁。优先用它自己的 ProductName(实测 wpxscreensaver64.scr →
       "Wallpaper Engine Screensaver")，读不到才退回文件名 —— 面板那行要给人名，不给 8.3 短名。 #>
    param($Cfg)
    if (-not $Cfg -or -not $Cfg.Saver) { return '' }
    $prod = [string]$Cfg.Product
    if ($prod -match '(?i)wallpaper\s*engine') { return 'Wallpaper Engine' }
    if ($prod) { return $prod }
    try { return [System.IO.Path]::GetFileNameWithoutExtension([string]$Cfg.Saver) } catch { return [string]$Cfg.Saver }
}

function Get-GdScreensaverVerdict {
    <# 纯函数：读数 → 一句面板短文 + 一句体检长话 + 要不要管。
       喂字典不喂现场(只读自检不许要求机器现场状态，见 project memory)：$Cfg 给假的就能全表覆盖。
       四档，判据顺序是认真的：
         unknown —— 注册表读不到，只能报"不知道"(Risk=$false：不知道就别吓唬人，但也别说没事)
         none    —— 没挂 .scr，这条路断根了，到点没人接管输入桌面
         armed   —— 挂了 .scr 且武装位=1 → 到点必接管会话，方舟那一路起跑前置判定会直接早退
         off     —— 挂了 .scr 但武装位=0 → 暂不触发；日常"只关不复"就是落到这一档，
                    所以它不是终态：任何程序重新拉闸(或用户在屏保设置里点一下)就回到 armed #>
    param($Cfg, [int]$TimeoutFloorSec = 0)
    $who = Get-GdScreensaverWho -Cfg $Cfg
    if ($null -eq $Cfg) {
        return @{ Kind = 'unknown'; Risk = $false; Text = '屏保：注册表读不到，判断不了';
                  Detail = '屏保：读不到 HKCU\Control Panel\Desktop，判断不了武装没有(不代表没事)' }
    }
    if (-not $Cfg.Saver) {
        return @{ Kind = 'none'; Risk = $false; Text = '屏保：没挂 .scr(不会接管会话)';
                  Detail = '屏保：系统里没有指定屏幕保护程序 → 到点不会有任何东西盖住端游窗口，06:00 那一路安全' }
    }
    $secs = [int]$Cfg.TimeoutSec
    if ($TimeoutFloorSec -gt 0 -and $secs -lt $TimeoutFloorSec) { $secs = $TimeoutFloorSec }
    # 卡片上那一行用的是 $short，体检长话用的是 $whoSecs —— 长话自己再缀一遍"屏保："会念成
    # 「屏保：仍武装着(屏保：Wallpaper Engine/300秒)」那种套娃(2026-09-26 首次实跑 status.ps1 就是这样)，所以这里把前缀拆干净。
    $whoSecs = $(if ($who) { $who } else { '某个 .scr' }) + '/' + $secs + '秒'
    $short = '屏保：' + $whoSecs
    if ($Cfg.Active -eq '1') {
        return @{ Kind = 'armed'; Risk = $true; Text = ($short + ' 仍武装');
                  Detail = ('屏保：' + $whoSecs + ' 仍武装 → 到点会接管输入桌面，方舟那一路起跑前置判定会直接早退 not-started；' +
                            '解法：tools\unarm-screensaver.ps1 -Apply(或 设置→个性化→锁屏界面→屏幕保护程序→选「(无)」)') }
    }
    return @{ Kind = 'off'; Risk = $false; Text = ($short + ' 已解除');
              Detail = ('屏保：' + $whoSecs + '，武装位是 0 → 到点不触发(日常"只关不复"就停在这档)。' +
                        '注意它不是终态：' + $(if ($who) { $who } else { '挂屏保的那个程序' }) + ' 的设置里仍勾着屏保时，改一次设置就可能被重新武装，体检看到这行变成"仍武装"就是这个原因') }
}

function Get-GdScreensaverStateFile { Join-Path $global:GD.State 'screensaver.restore' }
function Get-GdInputDesktopName {
    <# 输入事件当前落到哪个桌面：正常='Default'；Lock Screen/Screen-saver/Winlogon 时鼠标键盘到不了普通窗口。
       返回 ''=读不到(按"不知道"处理，不得据此判锁屏)。2026-09-26 加：06:00 两轮方舟全是死在
       会话被屏保桌面接管上，起跑前一行就能判掉，不该再烧 12 分钟等端游。 #>
    if (-not $global:GdDeskTypeDef) {
        # 注意：这里绝不能给 Add-Type 加 -ErrorAction SilentlyContinue —— 实测那样类型会"编译成功但不落地"，
        # 之后每次 [GdDesk]:: 都静默走 catch 返回 ''(2026-09-26 排查)。用 try/catch，失败就本轮按"读不到"处理。
        # 形态也必须像 GdPower/GdWin 一样用完整 -TypeDefinition+显式 class：-Name+-MemberDefinition 在函数体内
        # 同样会"成功但不落地"(实测 2026-09-26)。
        try {
            Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class GdDesk {
    [DllImport("user32.dll")] public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool GetUserObjectInformationW(IntPtr hObj, int nIndex, StringBuilder pvInfo, uint nLength, out uint lpnLengthNeeded);
    [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr hDesktop);
}
'@
            $global:GdDeskTypeDef = $true
        } catch { return '' }
    }
    try {
        # 参数照抄 vision-ark 里实测可用的 [GdWin]::InputDesktopName：flags=OPEN_EXISTING(2)，
        # access=DESKTOP_READOBJECTS(0x0002)，UOI_NAME=2。上一版抄成 GetUserObjectName/0x100 是错的，恒返回空。
        $h = [GdDesk]::OpenInputDesktop(0x0002, $false, 0x0002)
        if ($h -eq [IntPtr]::Zero) { return '' }
        try {
            $sb = New-Object System.Text.StringBuilder 256
            $need = [uint32]0
            if ([GdDesk]::GetUserObjectInformationW($h, 2, $sb, 512, [ref]$need)) { return $sb.ToString() }
            return ''
        } finally { [void][GdDesk]::CloseDesktop($h) }
    } catch { return '' }
}
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
            # 策略(2026-09-26 改)：屏保拉闸后不再合闸。旧行为"起跑关、收尾恢复"有两个实证死穴：
            # 恢复等于每天给明早 06:00 重新武装一次屏保；而起跑时若已被屏保桌面接管，
            # SPI 关闭必失败(06:00 日志「关闭屏保失败」)，12 分钟白烧。屏保关闭自此是稳态。
            $sf = Get-GdScreensaverStateFile
            if (Test-Path -LiteralPath $sf) { Remove-Item -LiteralPath $sf -Force -ErrorAction SilentlyContinue }   # 旧机制遗留文件，直接清，不再照它合闸
            $cur = Get-GdScreensaverActive
            if ($null -eq $cur) {
                Write-GdLog '读屏保状态失败，本轮不动屏保' -Level WARN
            } elseif ($cur) {
                if (Set-GdScreensaverActive -Active $false) {
                    Write-GdLog '屏保已关闭(策略：只关不复，收尾不再恢复)'
                } else {
                    Write-GdLog '关闭屏保失败：会话可能已被屏保桌面接管(方舟会因此早退)；长期解法见 status.ps1 的「屏保」那一行(解除命令 tools\unarm-screensaver.ps1 -Apply)' -Level WARN
                }
            } else {
                Write-GdLog '屏保本来就是关闭状态(稳态)'
            }
        }
    } catch { Write-GdLog "禁止睡眠失败：$($_.Exception.Message)" -Level WARN }
}
function Exit-GdNoSleep {
    # 只撤睡眠屏蔽(线程句柄语义)；屏保按"只关不复"策略不再恢复(见 Enter-GdNoSleep)
    try {
        if ($global:GdSleepHandle) { [void][GdPower]::SetThreadExecutionState([GdPower]::ES_CONTINUOUS) }
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

function Invoke-GdChild {
    <# 和 Invoke-GdNative 同一件事，多两点：**能给子进程单独设环境变量** + 带空格的参数自己加引号。
        为什么不能直接用上面那个：① 它没有环境变量的口子，而视觉兜底的 API key 只允许走环境变量 ——
        写进命令行等于挂在进程列表和每一行日志上(Invoke-GdNative 的 -join ' ' 还会被工具页原样印出来)；
        ② 它是 `$Arguments -join ' '`，路径里有空格(用户名带空格太常见了)就会散成两个参数。
        返回值同 Invoke-GdNative：@{Code;Out;Err}，超时 Code=-1 并把进程杀掉。 #>
    param(
        [Parameter(Mandatory)][string]$File,
        [string[]]$Arguments = @(),
        [hashtable]$Env = @{},
        [int]$TimeoutSeconds = 60
    )
    if (-not (Test-Path -LiteralPath $File)) { throw "命令不存在：$File" }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $File
    $psi.Arguments = (@($Arguments | ForEach-Object {
        $a = "$_"
        if ($a -match '[\s"]') { '"' + ($a -replace '"', '\"') + '"' } else { $a }
    }) -join ' ')
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.WorkingDirectory = $global:GD.Root
    foreach ($k in @($Env.Keys)) { $psi.EnvironmentVariables[[string]$k] = [string]$Env[[string]$k] }
    $p = [System.Diagnostics.Process]::Start($psi)
    $outTask = $p.StandardOutput.ReadToEndAsync()
    $errTask = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
        try { $p.Kill() } catch { }
        return @{ Code = -1; Out = ''; Err = "timeout after ${TimeoutSeconds}s" }
    }
    return @{ Code = $p.ExitCode; Out = $outTask.Result; Err = $errTask.Result }
}

# --- Python：视觉兜底的模型调用层(tools\vision\ask.py)用它跑，见那个文件的头注 ---
function Get-GdPythonBundledDir { Join-Path $global:GD.State 'python' }
function Get-GdPythonBundledExe { Join-Path (Get-GdPythonBundledDir) 'python.exe' }

function Test-GdPythonUsable {
    <# 光"文件在"不够：Windows 在没装 Python 的机器上会放一个 `python.exe` 商店占位程序，
        跑起来只会打印一句 "Python was not found" 并回一个非 0，第一次接上时就骗过了路径检查
        (报错还报在 ask.py 那一层，人看到的是"模型调用失败"，根本不是真因)。
        所以这里真的起一次进程，并要求它把 ask.py 要用的那几个标准库都 import 成功、且是 Python 3：
        embeddable 包是精简版，万一哪天缺 ssl 就得在兜底闸门这里说清楚，而不是半夜的日志里。
        只起一次(调用方缓存)，不联网、不动任何窗口。 #>
    param([Parameter(Mandatory)][string]$Exe, [int]$TimeoutSeconds = 20)
    if (-not (Test-Path -LiteralPath $Exe)) { return $false }
    try {
        $r = Invoke-GdChild -File $Exe -TimeoutSeconds $TimeoutSeconds -Arguments @('-c', 'import sys,json,base64,ssl,urllib.request;print(sys.version_info[0])')
    } catch { return $false }
    return ($r.Code -eq 0 -and ("$($r.Out)").Trim() -eq '3')
}

function Get-GdPythonExe {
    <# 找一个能跑 tools\vision\ask.py 的 python.exe，找不到返回空串(调用方负责说人话)。
        顺序和理由：
          1) settings.runtime.pythonExe —— 人手工指定的最大，只认"绝对路径 + 文件真在 + 用得住"
          2) 环境变量 GD_PYTHON —— 不落盘的临时指定，换机器/试另一份解释器时用，也不用改配置
          3) 本仓库自己下的 embeddable 包(state\python\python.exe) —— setup.ps1 -WithPython 放进去的。
             排在 PATH 前面：我们自己下的那份行为可预期(版本、stdlib 都定死)，而 PATH 上那份可能是
             别人装的旧版、或上面说的那个商店占位程序
          4) PATH 上的 python.exe / py.exe —— 最后兜底；WindowsApps 那个占位程序直接跳过，
             连试都不试(它一定会失败，试一次白等一次进程)
        结果缓存到 $global:GdPythonExe + $global:GdPythonProbed：一轮里状态页/兜底/诊断入口会问好几次，
        而"没找到"也要缓存 —— 不然每次问都白起一遍探测进程。-Refresh 强制重查(刚装完 python 时用)。 #>
    param($S = $null, [switch]$Refresh)
    if ($Refresh) { $global:GdPythonProbed = $false }
    elseif ($global:GdPythonProbed) { return [string]$global:GdPythonExe }
    $found = ''
    $candidates = @()
    $manual = ''
    if ($S -and $S.runtime -and $S.runtime.pythonExe) { $manual = [string]$S.runtime.pythonExe }
    if (-not $manual -and $env:GD_PYTHON) { $manual = [string]$env:GD_PYTHON }
    if ($manual) { $candidates += $manual }
    $candidates += (Get-GdPythonBundledExe)
    foreach ($n in @('python', 'py')) {
        foreach ($c in @(Get-Command $n -ErrorAction SilentlyContinue)) {
            $src = [string]$c.Source
            if (-not $src) { continue }
            if ($src -like '*\WindowsApps\*') { continue }   # 商店占位程序：见 Test-GdPythonUsable
            $candidates += $src
        }
    }
    $seen = @{}
    foreach ($c in $candidates) {
        $key = $c.Trim().Trim('"').ToLowerInvariant()
        if (-not $key -or $seen.ContainsKey($key)) { continue }
        $seen[$key] = $true
        if (Test-GdPythonUsable -Exe $key) { $found = $key; break }
    }
    $global:GdPythonExe = $found
    $global:GdPythonProbed = $true
    return $found
}

# embeddable 包：官方"绿色版"解释器，解压即用、不写注册表、不动 PATH，正适合塞在 state\ 下面。
# 版本必须钉死：换版本要换 URL，别让它悄悄跟着 python.org 的最新走(装完才发现 stdlib 行为不同)。
$global:GdPythonEmbedVersion = '3.12.10'
$global:GdPythonEmbedUrlFormat = 'https://www.python.org/ftp/python/{0}/python-{0}-embed-{1}.zip'

function Install-GdPythonEmbeddable {
    <# setup.ps1 -WithPython 的实现：下一个 embeddable 包到 state\python\，装完立刻用同一把尺子验
        (Test-GdPythonUsable)，验不过就保留旧的那份 —— 半下载的包比没有包更坏，会把兜底变成"每次都白等"。
        先解到临时目录再整目录换名，就是为了"要么换成好的，要么什么都不动"。
        URL 可以用环境变量 GD_PYTHON_EMBED_URL 整体覆盖(公司网络/镜像)，或 GD_PYTHON_EMBED_VERSION 换版本；
        它不是 settings 键：这纯粹是装东西的事，日常跑起来一个字段都不读。 #>
    param([switch]$Force)
    $dir = Get-GdPythonBundledDir
    if ((Test-Path -LiteralPath $dir) -and (Get-GdPythonExe) -and -not $Force) {
        return [pscustomobject]@{ Ok = $true; Exe = (Get-GdPythonExe); Detail = '已经有可用的 Python，不重下(-Force 才重装)' }
    }
    $ver = if ($env:GD_PYTHON_EMBED_VERSION) { [string]$env:GD_PYTHON_EMBED_VERSION } else { [string]$global:GdPythonEmbedVersion }
    $arch = if ("$env:PROCESSOR_ARCHITECTURE".ToLowerInvariant() -eq 'arm64') { 'arm64' } else { 'amd64' }
    $url = if ($env:GD_PYTHON_EMBED_URL) { [string]$env:GD_PYTHON_EMBED_URL } else { ($global:GdPythonEmbedUrlFormat -f $ver, $arch) }
    Enable-GdTls12
    $prevProgress = $ProgressPreference
    $zip = Join-Path ([System.IO.Path]::GetTempPath()) "gd-python-$ver-$arch-embed.zip"
    $tmp = "$dir.tmp"
    try {
        $ProgressPreference = 'SilentlyContinue'      # 5.1 的进度条能把下载拖慢一个数量级
        Write-GdLog "下载 Python $ver ($arch) embeddable 包 → $dir(约 10MB，只放进 state\，不进仓库)" -Level STEP
        Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing -TimeoutSec 300
        if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
        [void](New-Item -ItemType Directory -Path $tmp -Force)
        Expand-Archive -LiteralPath $zip -DestinationPath $tmp -Force
        if (-not (Test-GdPythonUsable -Exe (Join-Path $tmp 'python.exe') -TimeoutSeconds 30)) {
            return [pscustomobject]@{ Ok = $false; Exe = ''; Detail = "包解压了但里面的 python 用不了：$url(换 GD_PYTHON_EMBED_VERSION 或 GD_PYTHON_EMBED_URL 再试)" }
        }
        if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
        Move-Item -LiteralPath $tmp -Destination $dir -Force
        $exe = Get-GdPythonExe -Refresh
        if (-not $exe) { return [pscustomobject]@{ Ok = $false; Exe = ''; Detail = '装完了但当场没找到 state\python\python.exe(被杀软删了？)' } }
        return [pscustomobject]@{ Ok = $true; Exe = $exe; Detail = "已装到 $dir" }
    } catch {
        return [pscustomobject]@{ Ok = $false; Exe = ''; Detail = "下载/解压失败：$($_.Exception.Message)(地址 $url；也可自己装一份 Python 3 再填 runtime.pythonExe)" }
    } finally {
        $ProgressPreference = $prevProgress
        if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue }
        if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
    }
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
    <# 按主窗口标题找进程(MAA 就是靠窗口标题认端游的)。
       -ProcessName 给定时只认该进程名：端游窗口在进程(Arknights)上，浏览器标签页标题
       带"明日方舟"字样也会撞上标题正则(2026-09-26 msedge 误报，真端游没起却被判"已存在")。 #>
    param([Parameter(Mandatory)][string]$Pattern, [string]$ProcessName)
    foreach ($p in @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 })) {
        if ($ProcessName -and $p.ProcessName -ne $ProcessName) { continue }
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
