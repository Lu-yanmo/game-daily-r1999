# setup.ps1 — 第一次用这个仓库要跑的东西：探测助手装在哪 → 生成 config\settings.json
#
#   pwsh -File setup.ps1                      # 探测并生成(已有 settings.json 时不会覆盖已填好的项)
#   pwsh -File setup.ps1 -DryRun              # 只打印将要写入的内容
#   pwsh -File setup.ps1 -Force               # 完全按模板重建(会丢掉你手改过的其他字段，慎用)
#   pwsh -File setup.ps1 -MaaDir "D:\Tools\MAA" -M9aDir "D:\M9A" -ArkExe "D:\Arknights\Arknights.exe" -VmIndex 5
#                                             # 探测不到/探错了，手工指定(相对路径按当前目录解析)
#
# 探测优先级：命令行指定 > 现有 settings.json 里已存在的路径 > 正在运行的进程 > 卸载表 > 各固定磁盘浅层扫描(深度≤3)。
# 探测完一定要看输出：脚本只保证"路径存在"，不保证那是你想用的那份助手配置。
param(
    [string]$MaaDir,
    [string]$M9aDir,
    [string]$MuMuDir,
    [string]$ArkExe,
    [string]$VmIndex,
    [switch]$Force,
    [switch]$DryRun,
    [switch]$SkipScan
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '.')).Path
. (Join-Path $root 'lib\common.ps1')
. (Join-Path $root 'lib\detect.ps1')

$templatePath = Join-Path $root 'config\settings.example.json'
$outPath = Join-Path $root 'config\settings.json'
if (-not (Test-Path -LiteralPath $templatePath)) { throw "模板不存在：$templatePath" }
$cfg = Read-GdJson -Path $templatePath
$existing = $null
if (Test-Path -LiteralPath $outPath) {
    $existing = Read-GdJson -Path $outPath
    Write-Host "已存在 config\settings.json → 只补空缺项(-Force 才整体重建)" -ForegroundColor DarkGray
}

function Resolve-GdDirArg {
    param([string]$Value)
    if (-not $Value) { return $null }
    $p = if ([System.IO.Path]::IsPathRooted($Value)) { $Value } else { (Join-Path (Get-Location) $Value) }
    return (Get-Item -LiteralPath $p -ErrorAction SilentlyContinue).FullName
}
function Resolve-GdFileArg {
    param([string]$Value)
    if (-not $Value) { return $null }
    $p = if ([System.IO.Path]::IsPathRooted($Value)) { $Value } else { (Join-Path (Get-Location) $Value) }
    return (Get-Item -LiteralPath $p -ErrorAction SilentlyContinue).FullName
}

# 每一项：命令行 > 现有配置(仍有效) > 进程 > 扫描
function Select-GdItem {
    param(
        [Parameter(Mandatory)][string]$Label,
        [string]$Manual,
        [string]$Current,
        [scriptblock]$Probe
    )
    if ($Manual) { return @{ Value = $Manual; Source = "命令行 -$Label" } }
    if ($Current -and (Test-Path -LiteralPath $Current)) { return @{ Value = $Current; Source = '沿用现有 settings.json' } }
    $r = & $Probe
    if ($r) { return $r }
    return @{ Value = $null; Source = '没探测到(用 -MaaDir/-M9aDir/-MuMuDir/-ArkExe 手工指定)' }
}

$results = [ordered]@{}
if (-not $SkipScan) { Write-Host '开始探测(固定磁盘浅层扫描，几秒到几十秒)…' -ForegroundColor DarkGray } else { Write-Host '-SkipScan：跳过磁盘扫描' -ForegroundColor DarkGray }

$probeMaa = { Resolve-GdCandidate -ProcessNames @('MAA', 'MaaAssistantArknights') -ExeNames @('MAA.exe') -NameHints @('*MAA*', 'MaaAssistant*') -SkipScan:$SkipScan }
$probeM9A = { Resolve-GdCandidate -ProcessNames @('m9a', 'M9A', 'MFAAvalonia') -ExeNames @('m9a.exe') -NameHints @('*M9A*', '*MFAA*') -SkipScan:$SkipScan }
$probeMuMu = { Resolve-GdCandidate -ProcessNames @('MuMuNxMain', 'MuMuNxDevice', 'MuMuVM') -ExeNames @('MuMuManager.exe') `
                                     -NameHints @('*MuMu*') -UninstallLike @('*MuMu*') -RelExe (Join-Path 'nx_main' 'MuMuManager.exe') `
                                     -TrimSegment 'nx_main' -SkipScan:$SkipScan }
$probeArk = { Resolve-GdCandidate -ProcessNames @('Arknights', 'HypergryffClient') -ExeNames @('Arknights.exe') `
                                    -NameHints @('*Arknights*', '*明日方舟*', 'Game', 'games') -UninstallLike @('*Arknights*', '*明日方舟*') -PickProp 'Exe' -SkipScan:$SkipScan }

$cur = $null
if ($existing) { $cur = $existing.paths }
# 只探"这个仓库真用得上"的助手：口径与 Get-GdRequiredPathKeys 一致(看 lib\ 里有哪些助手库)。
# 单游戏导出仓库里没有 lib\m9a.ps1，就不该问一个只玩方舟的人"你的 M9A 装在哪"。
$hasArk = Test-Path -LiteralPath (Join-Path $root 'lib\maa.ps1')
$hasR1999 = Test-Path -LiteralPath (Join-Path $root 'lib\m9a.ps1')
$probed = @{}
if ($hasArk) {
    $probed['paths.maaDir'] = Select-GdItem -Label 'MaaDir' -Manual (Resolve-GdDirArg $MaaDir) -Current $(if ($cur) { [string]$cur.maaDir }) -Probe $probeMaa
    $probed['paths.arknightsExe'] = Select-GdItem -Label 'ArkExe' -Manual (Resolve-GdFileArg $ArkExe) -Current $(if ($cur) { [string]$cur.arknightsExe }) -Probe $probeArk
}
if ($hasR1999) {
    $probed['paths.m9aDir'] = Select-GdItem -Label 'M9aDir' -Manual (Resolve-GdDirArg $M9aDir) -Current $(if ($cur) { [string]$cur.m9aDir }) -Probe $probeM9A
    $probed['paths.mumuDir'] = Select-GdItem -Label 'MuMuDir' -Manual (Resolve-GdDirArg $MuMuDir) -Current $(if ($cur) { [string]$cur.mumuDir }) -Probe $probeMuMu
}
# 打印顺序保持和拆分前一模一样(合体工程逐项一致)，只是不再列出这个仓库没探的那几项
foreach ($k in @('paths.maaDir', 'paths.m9aDir', 'paths.mumuDir', 'paths.arknightsExe')) {
    if ($probed.ContainsKey($k)) { $results[$k] = $probed[$k] }
}

# MuMu 虚拟机：装了 MuMu 就能列出 index/名字，只有一个时自动填，多个时必须让人指定
$vmNote = ''
if ($hasR1999 -and $results['paths.mumuDir'].Value) {
    $mgr = Join-Path $results['paths.mumuDir'].Value ([string]$cfg.paths.mumuManager)
    $vms = @(Get-GdMuMuVmList -Manager $mgr)
    if ($VmIndex) {
        $pick = @($vms | Where-Object { $_.Index -eq $VmIndex })
        $results['mumu.vmIndex'] = @{ Value = $VmIndex; Source = '命令行 -VmIndex' }
        if ($pick) { $results['mumu.vmNameHint'] = @{ Value = $pick[0].Name; Source = '按 -VmIndex 匹配' } }
    } elseif ($existing -and [string]$existing.mumu.vmIndex -and ($vms.Count -eq 0 -or @($vms | Where-Object { $_.Index -eq [string]$existing.mumu.vmIndex }).Count)) {
        $results['mumu.vmIndex'] = @{ Value = [string]$existing.mumu.vmIndex; Source = '沿用现有 settings.json' }
        $results['mumu.vmNameHint'] = @{ Value = [string]$existing.mumu.vmNameHint; Source = '沿用现有 settings.json' }
    } elseif ($vms.Count -eq 1) {
        $results['mumu.vmIndex'] = @{ Value = $vms[0].Index; Source = '探测(MuMu 里只有一台虚拟机)' }
        $results['mumu.vmNameHint'] = @{ Value = $vms[0].Name; Source = '探测' }
    } elseif ($vms.Count -gt 1) {
        $vmNote = "MuMu 里有多台虚拟机，必须用 -VmIndex 指定装了 1999 的那台：$(($vms | ForEach-Object { "$($_.Index)=$($_.Name)" }) -join ' ; ')"
        $results['mumu.vmIndex'] = @{ Value = $null; Source = '多台虚拟机，未指定' }
    } else {
        $vmNote = 'MuMuManager 没列出任何虚拟机( MuMu 没装好或版本不支持 info )，装好后重跑 setup.ps1'
        $results['mumu.vmIndex'] = @{ Value = $null; Source = '无虚拟机可列' }
    }
}

Write-Host ''
Write-Host '=== 探测结果 ===' -ForegroundColor Cyan
foreach ($k in $results.Keys) {
    $v = $results[$k]
    $mark = if ($v.Value) { 'OK  ' } else { 'MISS' }
    $color = if ($v.Value) { 'Green' } else { 'Yellow' }
    Write-Host ("  [{0}] {1,-20} = {2}   ← {3}" -f $mark, $k, $(if ($v.Value) { $v.Value } else { '(待填)' }), $v.Source) -ForegroundColor $color
}
if ($vmNote) { Write-Host "  提示：$vmNote" -ForegroundColor Yellow }

$missing = @($results.Keys | Where-Object { -not $results[$_].Value })
if ($DryRun) { Write-Host "`n-DryRun：没有写入任何文件" -ForegroundColor Cyan; exit $(if ($missing.Count) { 2 } else { 0 }) }

# 写入(以模板为底，把现有 settings.json 里非 paths 的改动尽量保留)
# 只回写"这一步真的探过"的项：单游戏仓库的模板里根本没有 mumu / m9aDir 这些节点，硬赋会抛在 null 上。
function Set-GdSetupValue {
    param([object]$Node, [Parameter(Mandatory)][string]$Name, [AllowEmptyString()][string]$Value)
    if (-not $Node) { return }
    # 已有属性就地赋值：Add-Member -Force 虽然也能改值，但会把这个键挪到 JSON 末尾，白让 settings.json 换版式。
    if ($Node.PSObject.Properties.Name -contains $Name) { $Node.$Name = $Value }
    else { $Node | Add-Member -NotePropertyName $Name -NotePropertyValue $Value | Out-Null }
}
$pathKeys = @($results.Keys | Where-Object { $_ -like 'paths.*' })
if (-not $Force -and $existing) {
    $merged = $existing
    foreach ($k in $pathKeys) {
        if ($results[$k].Value) { Set-GdSetupValue -Node $merged.paths -Name ($k -replace '^paths\.', '') -Value ([string]$results[$k].Value) }
    }
    if ($results['mumu.vmIndex'].Value) { Set-GdSetupValue -Node $merged.mumu -Name 'vmIndex' -Value ([string]$results['mumu.vmIndex'].Value) }
    if ($results['mumu.vmNameHint'].Value) { Set-GdSetupValue -Node $merged.mumu -Name 'vmNameHint' -Value ([string]$results['mumu.vmNameHint'].Value) }
    $target = $merged
} else {
    $target = $cfg
    foreach ($k in $pathKeys) {
        Set-GdSetupValue -Node $target.paths -Name ($k -replace '^paths\.', '') -Value ([string]$results[$k].Value)
    }
    if ($results['mumu.vmIndex'].Value) { Set-GdSetupValue -Node $target.mumu -Name 'vmIndex' -Value ([string]$results['mumu.vmIndex'].Value) }
    if ($results['mumu.vmNameHint'].Value) { Set-GdSetupValue -Node $target.mumu -Name 'vmNameHint' -Value ([string]$results['mumu.vmNameHint'].Value) }
}
Backup-GdFile -Path $outPath | Out-Null
Write-GdJson -Path $outPath -Object $target -Depth 100
Write-Host "`n已写入 $outPath" -ForegroundColor Green

if ($missing.Count) {
    Write-Host "还有 $($missing.Count) 项没填：$($missing -join ' , ')" -ForegroundColor Yellow
    $need = @()
    if ($hasArk) { $need += '方舟要 maaDir + arknightsExe' }
    if ($hasR1999) { $need += '1999 要 m9aDir/mumuDir/vmIndex' }
    Write-Host "再跑一次 setup.ps1(不带 -SkipScan：它会扫常见安装位置并逐项问你)，或直接把 config\settings.json 里那几项手填上。$($need -join '，')；没填的那一路跑不了。" -ForegroundColor Yellow
    Write-Host "`n下一步：pwsh -File status.ps1   然后   pwsh -File daily-clear.ps1 -DryRun"
    exit 2
}
Write-Host "`n必填项齐了。下一步：" -ForegroundColor Green
Write-Host '  pwsh -File status.ps1                 # 七段自检(环境/进程/判定/队列)'
Write-Host '  pwsh -File daily-clear.ps1 -DryRun    # 预览要下发什么配置，不启动任何程序'
Write-Host '  pwsh -File daily-clear.ps1            # 真跑一轮(会弹一次 UAC)'
Write-Host '  pwsh -File install-task.ps1           # 确认能跑通后再注册每天定时的计划任务(时间点看 config\settings.json 的 schedule.times)'
exit 0
