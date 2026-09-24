<#
  status.ps1 — 只读体检：环境 + 今日判定 + MAA/M9A 现状 + 最近记录。不会启动任何程序、不改任何配置。
  用法： pwsh -File status.ps1

  2026-09-23 拆分：本文件只剩"外壳"——分节编号、路径染色、以及那五行都适用的记录段。
  两款各自的体检段落函数搬到 lib\ark-status.ps1 / lib\r1999-status.ps1 了。
  外壳靠"这个仓库里到底有没有那一路的函数"决定要不要对应那一段：
  所以将来只发布一款的仓库里，同一个 status.ps1 会自动只长那一款的样子，不需要为导出改一行代码。
#>
# 双引擎兼容：Windows PowerShell 5.1(系统自带) 与 PowerShell 7 都要能跑，别再加 #Requires 版本门把访客挡在门外。
[CmdletBinding()]
param([switch]$Offline)

$ErrorActionPreference = 'Stop'
# 有什么点什么：lib 里的模块按这个顺序 dot-source，文件不在就跳过。
# 本地工作副本 7 个都在；只发布一款时导出的仓库里没有另一款那几个文件，于是体检自动只剩那一路 —— 不靠改代码。
foreach ($lib in @('common', 'decisions', 'maa', 'm9a', 'mumu', 'ark-status', 'r1999-status')) {
    $libPath = Join-Path $PSScriptRoot "lib\$lib.ps1"
    if (Test-Path -LiteralPath $libPath) { . $libPath }
}

$S = Get-GdSettings -Quiet

$script:sec = 0
function Show-GdSectionHeader {
    param([Parameter(Mandatory)][string]$Title)
    $script:sec++
    Write-Host "`n[$script:sec] $Title" -ForegroundColor Cyan
}
function Test-GdFunc {
    param([Parameter(Mandatory)][string]$Name)
    return [bool](Get-Command -Name $Name -ErrorAction SilentlyContinue)
}
function Show-GdEnvRows {
    param([Parameter(Mandatory)]$Rows)
    foreach ($k in $Rows.Keys) {
        $v = [string]$Rows[$k]
        # 没填的那一项要单列一种状态：访客第一次跑(只生成了 settings.json、路径还空着)时
        # 最该看到的就是"到底哪几项还没填"，而不是 Test-Path 抛出来的空字符串错误。
        if (-not $v) { Write-Host ("  {0,-10} {1} {2}" -f $k, '未填', '(settings.json 里这一项空着 → 跑 setup.ps1)') -ForegroundColor Yellow; continue }
        $ok = Test-GdPathReady $v
        Write-Host ("  {0,-10} {1} {2}" -f $k, $(if ($ok) { 'OK  ' } else { '缺失' }), $v) -ForegroundColor $(if ($ok) { 'Gray' } else { 'Red' })
    }
}
function Invoke-GdTolerant {
    <# 体检是只读的：某一段读不出来(助手没装、路径没填、配置文件换了版本)就印一行红字继续往下走。
        整段抛异常退出会让人以为 status.ps1 本身坏了，反而看不到"哪几项没就绪"这件真正有用的事。 #>
    param([Parameter(Mandatory)][scriptblock]$Body)
    try { & $Body } catch { Write-Host "  这一段读不出来：$($_.Exception.Message)" -ForegroundColor Red }
}

# 这个仓库里装了哪几路，看的就是"那路的段落函数在不在"，不猜配置
$arkSide = (Test-GdFunc 'Get-GdArkEnvRows')
$rSide = (Test-GdFunc 'Get-GdR1999EnvRows')
# 提示语里"下一步敲什么"必须跟着本仓库入口的参数名走：合体工程 → -AckArk/-RShopDone，单游戏仓库 → -Ack/-ShopReset。
# 以前这六行映射只写在 daily-clear.ps1 里，于是 status 打出来的是短名、合体入口却不认(2026-09-24 发现)。
if (Test-GdFunc 'Sync-GdEntryVerbs') { Sync-GdEntryVerbs }

Show-GdSectionHeader '环境'
if ($arkSide) { Show-GdEnvRows (Get-GdArkEnvRows -S $S) }
if ($rSide) { Show-GdEnvRows (Get-GdR1999EnvRows -S $S) }
if ($arkSide) { Show-GdArkStatusVersion -S $S }
if ($rSide) { Show-GdR1999StatusVersion -S $S }

Show-GdSectionHeader '进程 / 模拟器'
$procNames = @()
if ($arkSide) { $procNames += @(Get-GdArkProcNames) }
if ($rSide) { $procNames += @(Get-GdR1999ProcNames) }
foreach ($n in $procNames) {
    Write-Host ("  {0,-10} {1}" -f $n, $(if (Test-GdProcessRunning -Name $n) { '正在运行' } else { '未运行' }))
}
if ($rSide) { Show-GdR1999MuMuState -S $S }

Show-GdSectionHeader '今日判定'
Invoke-GdTolerant { if ($arkSide) { Show-GdArkStatusPlan -S $S -Offline:$Offline } }
Invoke-GdTolerant { if ($rSide) { Show-GdR1999StatusPlan -S $S } }

if ($arkSide) { Show-GdSectionHeader 'MAA 队列现状'; Invoke-GdTolerant { Show-GdArkQueueState -S $S } }
if ($rSide) { Show-GdSectionHeader 'M9A 实例现状'; Invoke-GdTolerant { Show-GdR1999InstanceList -S $S } }

Show-GdSectionHeader '活动确认 / 商店领空状态'
if ($arkSide) { Show-GdGateState -Game 'ark' }
if ($rSide) { Show-GdGateState -Game 'r1999' }

Show-GdSectionHeader '最近 5 条运行记录'
$hist = Join-Path (Get-GdDir 'State') 'history.csv'
if (Test-Path -LiteralPath $hist) { Get-Content -LiteralPath $hist -Tail 5 | ForEach-Object { Write-Host "  $_" } }
else { Write-Host '  (还没有跑过)' }
Write-Host ''
