<#
  run-via-task.ps1 — 免 UAC 的手动入口：触发 GameDaily-Manual(注册时就是最高权限)，等它跑完并回传退出码。

  pwsh -File run-via-task.ps1                 # 完整一轮(方舟 + 1999)，不弹 UAC
  pwsh -File run-via-task.ps1 -NoWait         # 只点火，不等结果
  pwsh -File run-via-task.ps1 -Games ark      # 只跑一路 → 计划任务参数是固定的，会自动改成本机直跑(可能弹一次 UAC)
  pwsh -File run-via-task.ps1 -Direct         # 强制本机直跑
  (单游戏发布仓库里只装得下一路的代码，-Games 这个参数就是空转：不用管它)
#>
# 双引擎兼容：Windows PowerShell 5.1(系统自带) 与 PowerShell 7 都要能跑，别再加 #Requires 版本门把访客挡在门外。
[CmdletBinding()]
param(
    [ValidateSet('all', 'ark', 'r1999')][string]$Games = 'all',
    [string]$TaskName,
    [switch]$NoWait,
    [switch]$Direct
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\common.ps1')

# 这份文件会原样进两个导出仓库，所以先问"这棵树上装了哪一路"：只有一路时 -Games 没有意义，
# 而且单游戏入口 daily-clear.ps1 压根没有 -Games 参数，硬传会当场报"找不到参数"。
$onlyGame = $null
if ((Test-Path -LiteralPath (Join-Path $PSScriptRoot 'lib\ark-daily.ps1')) -xor (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'lib\r1999-daily.ps1'))) {
    $onlyGame = if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'lib\ark-daily.ps1')) { 'ark' } else { 'r1999' }
    if ($Games -notin @('all', $onlyGame)) { Write-GdLog "本仓库只有 $onlyGame 这一路，-Games $Games 被忽略" -Level WARN }
}
$S = Get-GdSettings -Quiet
$task = if ($TaskName) { $TaskName } else { "$($S.schedule.taskPrefix)-Manual" }
$daily = Join-Path $PSScriptRoot 'daily-clear.ps1'

function Invoke-Direct {
    param([string]$Why)
    if ($Why) { Write-GdLog "$Why → 本机直跑 daily-clear.ps1(非管理员时会弹一次 UAC)" -Level WARN }
    if ($onlyGame) { & $daily -Tag manual } else { & $daily -Games $Games -Tag manual }
    exit $LASTEXITCODE
}

$exists = [bool](Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue)
if (-not $exists) { Invoke-Direct -Why "还没注册按需任务 $task(先跑 install-task.ps1)" }
if ($Direct) { Invoke-Direct -Why '-Direct' }
if (-not $onlyGame -and $Games -ne 'all') { Invoke-Direct -Why "计划任务的参数固定是 -Games all，-Games $Games 走不了任务" }

Write-GdLog "触发计划任务 $task(最高权限、免 UAC)" -Level STEP
try {
    Start-ScheduledTask -TaskName $task
} catch {
    Invoke-Direct -Why "触发 $task 失败：$($_.Exception.Message)"
}
if ($NoWait) { Write-GdLog '已点火(-NoWait 不等待结果)'; exit 0 }

$deadline = (Get-Date).AddMinutes(([double]$S.runtime.taskWaitMinutes))
$logHint = ''
$mirrorPath = ''
$mirrorOffset = 0
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 15
    $st = Get-GdTaskState -Name $task
    if ($st.State -ne 'Running') { break }
    $newest = Get-ChildItem -LiteralPath (Get-GdDir 'Logs') -Filter 'run-*.log' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($newest) {
        $logHint = $newest.FullName
        # 只回显新增行(按字节偏移续读)：每轮重抄最后一行会把控制台刷成一屏重复
        if ($mirrorPath -ne $newest.FullName) { $mirrorPath = $newest.FullName; $mirrorOffset = 0 }
        if ((Get-Item -LiteralPath $newest).Length -lt $mirrorOffset) { $mirrorOffset = 0 }
        $tail = Get-GdFileTail -Path $mirrorPath -Offset ([long]$mirrorOffset)
        $mirrorOffset = $tail.Offset
        foreach ($line in (@($tail.Text -split "`r?`n") | Where-Object { $_.Trim() })) {
            Write-Host "  [$(Get-Date -Format 'HH:mm:ss')] $line"
        }
    }
}

$st = Get-GdTaskState -Name $task
$code = if ($null -ne $st.Result) { $st.Result } else { 99 }
Write-GdLog "任务 $task 结束：状态=$($st.State) 退出码=$code(0 全部完成 / 1 部分完成 / 2 有失败) 起跑时间=$($st.Last)" -Level $(if ($code -eq 0) { 'OK' } else { 'WARN' })
if ($logHint) { Write-GdLog "本轮日志：$logHint" }
exit $code
