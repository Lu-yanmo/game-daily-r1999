<#
  tools\m9a-update.ps1 — M9A 整包更新器：检测到新版自己更，不再"等人去 M9A 界面点更新"。

  来历(2026-09-30 用户口径「检查到更新自己更就行了」)：以前版本差只能从 M9A 运行日志里事后看
  (lib\m9a.ps1 的 resCurrent/resLatest 段，冻结)；官方口径 4.x 是"程序+资源整包"更新、命令行没有
  更新入口。本工具把这条链路搬到界面外：现版本(装包 interface.json 的 version) 对 GitHub 最新
  release tag，不一样就"进程闸 → 整目录备份 → 下载解包 → robocopy 覆盖(用户态除外) → 回读校验 →
  失败回退"。装包目录名(v4.7.1 那种)是历史遗留，官方界面更新也是原地覆盖 —— 本工具同样不改目录。

    pwsh -File tools\m9a-update.ps1            # 只读体检：现版本/最新版/会动什么，一个字节不写
    pwsh -File tools\m9a-update.ps1 -Apply     # 真更新(已是最新则无事发生)

  退出码(只读和 -Apply 同一套规矩，调用方可以直接判)：
    0 = 已是最新(或 -Apply 已更新成功)
    1 = 只读模式发现新版没动手(想让轮子自动更的调用方请带 -Apply)
    2 = 拒了/失败(M9A 在跑、版本源读不出、网络或下载失败、校验失败 —— 校验失败时已回退，装包目录完好)
  纪律：本工具**永不杀 M9A 进程** —— 在跑就退让，调用方先关它(lib\rust-round.ps1 的 updgate 臂把这一发挂在起跑前，
  那时 M9A 必然没跑；11-04 刀⑦ 起旧调用方 lib\r1999-daily.ps1 整件出仓，本两行按等长改写不落行数)。用户态(config、appsettings.json、data、logs 等)被覆盖步排除在外，
  但 interface.json 必须跟着覆盖 —— 它是版本核，不覆盖就永远校不过。
#>
[CmdletBinding()]
param(
    [switch]$Apply
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $root 'lib\common.ps1')

# ---------- 只读侧三件 ----------

function Get-GdM9AInstall {
    <# 装包目录从 settings(paths.m9aDir) 来；版本源 = 该目录 interface.json 的 version；
       仓库地址也从 interface.json 的 github 字段现学(抠不出 owner/repo 才回落官方仓)。
       返回 @{Dir; Current; Repo; Exe} —— Exe 是**带扩展名**的文件名：进程闸那用得去扩展名(Get-Process
       要 'm9a')，Test-Path 校验用得留原样('m9a.exe')。09-30 实弹第一发就栽在把两者混成一个字段：
       装包里根本没有叫 'm9a' 的文件，校验永远假红、把成功的更新回退掉。 #>
    $st = Get-GdSettings
    $dir = [string]$st.paths.m9aDir
    if (-not $dir -or -not (Test-Path -LiteralPath $dir)) { throw "M9A 装包目录不在：$dir" }
    $iface = Join-Path $dir 'interface.json'
    if (-not (Test-Path -LiteralPath $iface)) { throw "M9A 版本源 interface.json 不在：$iface(本工具不猜版本)" }
    $j = Read-GdJson -Path $iface
    $cur = [string]$j.version
    if (-not $cur) { throw 'interface.json 里没有 version 字段，判不了现版本' }
    $repo = 'MAA1999/M9A'
    $gh = [string]$j.github
    if ($gh -and $gh -match 'github\.com/([^/]+/[^/]+)') { $repo = ($Matches[1] -replace '/+$', '') }
    $exe = [string]$st.paths.m9aExe
    if (-not $exe) { $exe = 'm9a.exe' }
    return @{ Dir = $dir; Current = $cur; Repo = $repo; Exe = $exe }
}

function Get-GdM9ALatestTag {
    <# 取 https://github.com/<repo>/releases/latest 的 302 落地地址里的 tag。
       只用 curl.exe(Win10+ 自带) + 纯重定向：不进 GitHub API 配额、不用 token(09-30 实测匿名 API 会 403)。
       网络失败一律 throw，由调用方决定是 WARN 还是拒跑。 #>
    param([Parameter(Mandatory)][string]$Repo)
    $url = "https://github.com/$Repo/releases/latest"
    $raw = & curl.exe -sI -o NUL -w '%{redirect_url}' --max-time 30 $url
    $loc = ('' + $raw).Trim()
    if ($loc -notmatch '/tag/([^&?]+)$') { throw "没从 $url 拿到重定向 tag(curl 回：$raw)" }
    return $Matches[1]
}

function Test-GdM9AVersionGap {
    <# v 前缀可有可无：各剥掉后比字面。只比"等不等"—— releases/latest 恒为最新，
       不做日期判断也不解析 semver，不一样就更新(官方整包语义下没有"跳版本"这回事)。 #>
    param([string]$Current, [string]$Latest)
    $c = $Current -replace '^[vV]', ''
    $l = $Latest -replace '^[vV]', ''
    return ($c -ne $l)
}

# ---------- 写入侧两件 ----------

function Invoke-GdRobo {
    <# robocopy 退出码 0-7 都算成功(8+ 才是失败)，这是它和一切普通程序的差别；
       参数按数组原样传，空格路径不用额外引。失败 throw 时把最后几行带出来，日志好查。 #>
    param([Parameter(Mandatory)][string[]]$RoboArgs)
    $out = @(& robocopy.exe @($RoboArgs) 2>&1)
    $code = $LASTEXITCODE
    if ($code -ge 8) {
        $tail = ($out | Select-Object -Last 3) -join ' | '
        throw "robocopy 失败(退出码 $code)：$($RoboArgs -join ' ') —— 末行：$tail"
    }
    return $code
}

function Invoke-GdM9AApply {
    <# 完整落地序列：备份 → 下载 → 解包 → 覆盖 → 校验；校验不过自动回退备份再抛。
       备份整目录(M9A 装包 770MB 量级，robocopy 增量盘对盘约 1-3 分钟)，只留最近一轮。 #>
    param([Parameter(Mandatory)]$Install, [Parameter(Mandatory)][string]$Tag)
    $dir = $Install.Dir
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $bakRoot = 'C:\M9A-backup'
    if (-not (Test-Path -LiteralPath $bakRoot)) { [void](New-Item -ItemType Directory -Path $bakRoot) }
    $backup = Join-Path $bakRoot "pre-$Tag-$stamp"
    $zip = Join-Path ([System.IO.Path]::GetTempPath()) "gd-m9a-$Tag-$stamp.zip"
    $unz = Join-Path ([System.IO.Path]::GetTempPath()) "gd-m9a-$Tag-$stamp"
    try {
        Write-Host ("  1/5 备份整目录到：{0}" -f $backup)
        [void](Invoke-GdRobo @($dir, $backup, '/E', '/R:1', '/W:1', '/NFL', '/NDL', '/NJH', '/NJS'))
        <# 裁剪放在建好**新**备份之后：按名倒序留第一份(就是刚建的这份)再删其余。
           09-30 首版把裁剪建备份之前，结果"留最近一轮"实际留了两轮 —— 顺序即语义。 #>
        $oldBaks = @(Get-ChildItem -LiteralPath $bakRoot -Directory -Filter 'pre-*' -ErrorAction SilentlyContinue |
                     Sort-Object Name -Descending | Select-Object -Skip 1)
        foreach ($bk in $oldBaks) { Remove-Item -LiteralPath $bk.FullName -Recurse -Force }
        if ($oldBaks.Count -gt 0) { Write-Host ("  1/5 剪掉更早备份 {0} 份" -f $oldBaks.Count) }

        $asset = "M9A-win-x86_64-$Tag-MFAA.zip"
        $url = "https://github.com/$($Install.Repo)/releases/download/$Tag/$asset"
        Write-Host "  2/5 下载整包(约 200MB，最多等 20 分钟)：$url"
        & curl.exe -sS -L --fail --retry 2 --max-time 1200 -o $zip $url
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $zip)) { throw "下载失败(curl 退出码 $LASTEXITCODE)：$url" }
        $len = (Get-Item -LiteralPath $zip).Length
        if ($len -lt 10000000) { throw "包体只有 $len 字节，当成坏包(不敢往装包里灌)" }

        Write-Host '  3/5 解包'
        if (Test-Path -LiteralPath $unz) { Remove-Item -LiteralPath $unz -Recurse -Force }
        Expand-Archive -LiteralPath $zip -DestinationPath $unz -Force
        $src = $unz
        if (-not (Test-Path -LiteralPath (Join-Path $src 'interface.json'))) {
            <# 官方 zip 顶层套一个同名目录是常态：interface.json 只有一处才认，多处宁可抛。 #>
            $cand = @(Get-ChildItem -LiteralPath $unz -Directory |
                      Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'interface.json') })
            if ($cand.Count -ne 1) { throw "包里 interface.json 不唯一(找到 $($cand.Count) 个顶层目录带它)，解包形态和预期不符" }
            $src = $cand[0].FullName
        }

        Write-Host '  4/5 覆盖装包(用户态除外：appsettings.json 与 config/data/backup/logs/debug/temp 目录)'
        [void](Invoke-GdRobo @($src, $dir, '/E',
            '/XF', 'appsettings.json',
            '/XD', 'config', 'data', 'backup', 'logs', 'debug', 'temp',
            '/R:2', '/W:2', '/NFL', '/NDL', '/NJH', '/NJS'))

        Write-Host '  5/5 回读校验'
        $j2 = Read-GdJson -Path (Join-Path $dir 'interface.json')
        $after = ([string]$j2.version) -replace '^[vV]', ''
        $exeOk = Test-Path -LiteralPath (Join-Path $dir $Install.Exe)
        if ($after -ne ($Tag -replace '^[vV]', '') -or -not $exeOk) {
            throw "覆盖后版本核不回读(读到 '$after'，期望 '$Tag'；主程序存在=$exeOk) —— 正在回退备份"
        }
        Write-Host ("  更新完成：{0} → {1}(备份留在 {2})" -f $Install.Current, $Tag, $backup)
        return $true
    } catch {
        $failMsg = $_.Exception.Message
        Write-Host "  更新失败：$failMsg" -ForegroundColor Red
        if (Test-Path -LiteralPath (Join-Path $backup 'interface.json')) {
            Write-Host ('  回退：把备份 {0} 原样镜像回装包' -f $backup) -ForegroundColor Yellow
            try {
                [void](Invoke-GdRobo @($backup, $dir, '/MIR', '/R:2', '/W:2', '/NFL', '/NDL', '/NJH', '/NJS'))
                Write-Host '  回退完成，装包目录恢复更新前状态' -ForegroundColor Yellow
            } catch {
                Write-Host "  回退也失败了：$($_.Exception.Message) —— 备份还在 $backup，得人来收拾" -ForegroundColor Red
            }
        }
        throw "M9A 自动更新未完成：$failMsg"
    } finally {
        if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue }
        if (Test-Path -LiteralPath $unz) { Remove-Item -LiteralPath $unz -Recurse -Force -ErrorAction SilentlyContinue }
    }
}

# ---------- 主流程 ----------

Write-Host '--- M9A 整包更新器(默认只读，-Apply 真更) ---'
$install = $null
try { $install = Get-GdM9AInstall } catch {
    Write-Host "  装包状态读不出：$($_.Exception.Message)" -ForegroundColor Red
    exit 2
}
Write-Host ('  现版本：{0}(装包 {1}，仓库 {2})' -f $install.Current, $install.Dir, $install.Repo)
$latest = ''
try { $latest = Get-GdM9ALatestTag -Repo $install.Repo } catch {
    Write-Host "  最新版查不到：$($_.Exception.Message)" -ForegroundColor Red
    exit 2
}
Write-Host "  最新版：$latest"
if (-not (Test-GdM9AVersionGap -Current $install.Current -Latest $latest)) {
    Write-Host '  已是最新，无事发生'
    exit 0
}
Write-Host ('  检测到新版：{0} → {1}' -f $install.Current, $latest) -ForegroundColor Cyan
if (-not $Apply) {
    Write-Host '  (只读模式，没动手。要自动更：pwsh -File tools\m9a-update.ps1 -Apply)'
    exit 1
}
$procName = [System.IO.Path]::GetFileNameWithoutExtension($install.Exe)
$procs = @(Get-Process -Name $procName -ErrorAction SilentlyContinue)
if ($procs.Count -gt 0) {
    Write-Host "  $procName 进程在跑($($procs.Count) 个)：拒绝更新，也绝不硬杀 —— 先让 M9A 关上再跑" -ForegroundColor Red
    exit 2
}
try {
    [void](Invoke-GdM9AApply -Install $install -Tag $latest)
    exit 0
} catch {
    exit 2
}
