# tools/probe-m9a-history.ps1 — 只读：统计历史日志里各实例真正开始过多少任务(定位"队列没开始")
param([string]$LogDir = '')     # 不传就用 config\settings.json 里 paths.m9aDir 下的 logs
$ErrorActionPreference = 'Stop'
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'lib\common.ps1')
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'lib\m9a.ps1')

if (-not $LogDir) {
    $LogDir = (Get-M9APaths -S (Get-GdSettings -Quiet)).LogDir
    Write-Host "日志目录取自 settings.json：$LogDir" -ForegroundColor DarkGray
}
if (-not (Test-Path -LiteralPath $LogDir)) { Write-Host "日志目录不存在：$LogDir(用 -LogDir 指定)" -ForegroundColor Yellow; exit 1 }

foreach ($f in (Get-ChildItem -LiteralPath $LogDir -Filter '*.log' | Sort-Object Name)) {
    $lines = Get-Content -LiteralPath $f.FullName -Encoding utf8
    $tasks = @($lines | Where-Object { $_ -match '开始任务：' })
    $qStart = @($lines | Where-Object { $_ -match '队列任务开始' }).Count
    $allDone = @($lines | Where-Object { $_ -match '任务已全部完成' })
    Write-Host ("=== {0}  开始任务 {1} 行 / 队列任务开始 {2} / 全部完成 {3}" -f $f.Name, $tasks.Count, $qStart, $allDone.Count)
    $byInst = @{}
    foreach ($l in $tasks) {
        $inst = if ($l -match 'inst=([^/\]]+)/(\w+)') { "$($Matches[1])/$($Matches[2])" } else { '(无实例标记)' }
        if (-not $byInst.ContainsKey($inst)) { $byInst[$inst] = 0 }
        $byInst[$inst]++
    }
    foreach ($k in $byInst.Keys | Sort-Object) { Write-Host ("    任务行 {0,3} x {1}" -f $byInst[$k], $k) }
    foreach ($l in $allDone) {
        $inst = if ($l -match 'inst=([^/\]]+)/(\w+)') { "$($Matches[1])/$($Matches[2])" } else { '(无实例标记)' }
        $ts = if ($l -match '^\[([^\]]+)\]') { $Matches[1] } else { '?' }
        Write-Host ("    全部完成 @ {0}  实例={1}" -f $ts, $inst)
    }
}
