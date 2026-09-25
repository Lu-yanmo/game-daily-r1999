<#
  status.ps1 — 只读体检：环境 + 今日判定 + MAA/M9A 现状 + 最近记录。不会启动任何程序、不改任何配置。
  用法： pwsh -File status.ps1

  2026-09-23 拆分：本文件只剩"外壳"——分节编号、路径染色、以及那五行都适用的记录段。
  两款各自的体检段落函数在 lib\ark-status.ps1 / lib\r1999-status.ps1。
  2026-09-25 模块化：外壳不再写"ark 这一段、r1999 那一段"，改成遍历 lib\modules.ps1 注册表里
  这一棵树真的装了的模块，逐段问它的钩子在不在(在就打印，不在就整段跳过)。
  所以将来只发布一款的仓库里，同一个 status.ps1 自动只长那一款的样子；再加第三款游戏也不用改这个文件 ——
  只要它的 <id>-module.ps1 里带上 StatusRows / StatusPlan 这几个钩子。
#>
# 双引擎兼容：Windows PowerShell 5.1(系统自带) 与 PowerShell 7 都要能跑，别再加 #Requires 版本门把访客挡在门外。
[CmdletBinding()]
param([switch]$Offline)

$ErrorActionPreference = 'Stop'
# 有什么点什么：lib 里的模块按这个顺序 dot-source，文件不在就跳过。
# 本地工作副本都在；只发布一款时导出的仓库里没有另一款那几个文件，于是体检自动只剩那一路 —— 不靠改代码。
foreach ($lib in @('common', 'decisions', 'modules', 'maa', 'm9a', 'mumu', 'ark-status', 'r1999-status')) {
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
function Get-GdStatusModule {
    <# 这棵树上该出现在体检里的模块 = 注册了、而且这一路的助手库在。
        注册表空 = 这棵树缺了 lib\modules.ps1 或 *-module.ps1(工程被裁坏了)，
        宁可在这里说清楚，也不要印一份"看着正常其实少了四段"的体检 —— 那种半成品会把人往错方向带。 #>
    if (-not (Test-GdFunc 'Get-GdModules')) { throw 'status.ps1 点不到 lib\modules.ps1 —— 这个工程缺文件，体检没法做' }
    $mods = @(Get-GdModules -AvailableOnly)
    if (-not $mods.Count) { throw '没有可体检的模块(没有任何 lib\*-module.ps1，或那几路的助手库都不在这棵树上)' }
    return $mods
}

# 注册表要先装满，后面的 Sync-GdEntryVerbs / 体检分段问的都是它
[void](Import-GdModules -Quiet)

# 提示语里"下一步敲什么"必须跟着本仓库入口的参数名走：合体工程 → -AckArk/-RShopDone，单游戏仓库 → -Ack/-ShopReset。
# 以前这六行映射只写在 daily-clear.ps1 里，于是 status 打出来的是短名、合体入口却不认(2026-09-24 发现)。
if (Test-GdFunc 'Sync-GdEntryVerbs') { Sync-GdEntryVerbs }

$mods = @(Get-GdStatusModule)

Show-GdSectionHeader '环境'
foreach ($m in $mods) {
    if ($m.Hooks.StatusRows) { Show-GdEnvRows (& $m.Hooks.StatusRows $S) }
}
foreach ($m in $mods) {
    if ($m.Hooks.StatusVersion) { & $m.Hooks.StatusVersion $S }
}

Show-GdSectionHeader '进程 / 模拟器'
foreach ($m in $mods) {
    if (-not $m.Hooks.StatusProcs) { continue }
    foreach ($n in @(& $m.Hooks.StatusProcs $S)) {
        Write-Host ("  {0,-10} {1}" -f $n, $(if (Test-GdProcessRunning -Name $n) { '正在运行' } else { '未运行' }))
    }
}
foreach ($m in $mods) {
    if ($m.Hooks.StatusProcExtra) { & $m.Hooks.StatusProcExtra $S }
}

Show-GdSectionHeader '今日判定'
foreach ($m in $mods) {
    if (-not $m.Hooks.StatusPlan) { continue }
    $mm = $m
    Invoke-GdTolerant { & $mm.Hooks.StatusPlan $S ([bool]$Offline) }
}

# 每一路自己那一节(MAA 队列 / M9A 实例)：标题由模块自己给，所以两节仍然按注册顺序出现在这里
foreach ($m in $mods) {
    if (-not ($m.Hooks.StatusSection -and $m.StatusSectionTitle)) { continue }
    Show-GdSectionHeader ([string]$m.StatusSectionTitle)
    $mm = $m
    Invoke-GdTolerant { & $mm.Hooks.StatusSection $S }
}

Show-GdSectionHeader '活动确认 / 商店领空状态'
foreach ($m in $mods) { Show-GdGateState -Game ([string]$m.Id) }

Show-GdSectionHeader '最近 5 条运行记录'
$hist = Join-Path (Get-GdDir 'State') 'history.csv'
if (Test-Path -LiteralPath $hist) { Get-Content -LiteralPath $hist -Tail 5 | ForEach-Object { Write-Host "  $_" } }
else { Write-Host '  (还没有跑过)' }
Write-Host ''
