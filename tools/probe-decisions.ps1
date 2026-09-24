# tools/probe-decisions.ps1 — 只读诊断：把方舟/1999 两条判定链的中间数据打出来
# 用法: pwsh -NoProfile -File tools/probe-decisions.ps1
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $root 'lib\common.ps1')
. (Join-Path $root 'lib\decisions.ps1')

$S = Get-GdSettings -Quiet
# 下面打出来的判定理由里有"下一步敲什么"：合体工程要换成本仓库入口真有的长参数名，单游戏仓库保持短名
if (Get-Command Sync-GdEntryVerbs -ErrorAction SilentlyContinue) { Sync-GdEntryVerbs }

if (Get-Command Get-GdStageTable -ErrorAction SilentlyContinue) {   # 关卡表是 MAA 那边的东西，1999 仓库里没有
    Write-Host '== 关卡表 =='
    $table = Get-GdStageTable -S $S
    Write-Host "  条目 $($table.Count)  CE-5=$($table['CE-5']) LS-5=$($table['LS-5']) SK-5=$($table['SK-5'])"
    Write-Host ("  校验 CE-5,LS-5 -> " + (ConvertTo-Json -Compress (Test-GdStages -S $S -StageText 'CE-5,LS-5')))
}

Write-Host "`n== 方舟活动接口逐条 =="
if (-not $S.ark.activityApiUrl) {
    Write-Host '  (本仓库的 settings 里没有 ark.activityApiUrl → 这一节跳过)'
} else {
try {
    Enable-GdTls12
    $resp = Invoke-WebRequest -Uri $S.ark.activityApiUrl -TimeoutSec 12 -UseBasicParsing
    $data = $resp.Content | ConvertFrom-Json
} catch { Write-Host "  拉取失败：$($_.Exception.Message)"; $data = $null }

$now = Get-GdNowMs
if ($data) {
    foreach ($grp in $data.Official.PSObject.Properties) {
        Write-Host "GROUP $($grp.Name) ($($grp.Value.GetType().Name))"
        foreach ($e in $grp.Value.PSObject.Properties) {
            $v = $e.Value
            $a = Get-GdMapValue $v 'Activity'
            if (-not $a) {
                $keys = (@($v.PSObject.Properties) | ForEach-Object { $_.Name }) -join ','
                Write-Host "  $($e.Name) <无 Activity> keys=$keys"
                continue
            }
            $st = Convert-GdUtc8ToMs (Get-GdMapValue $a 'UtcStartTime')
            $en = Convert-GdUtc8ToMs (Get-GdMapValue $a 'UtcExpireTime')
            $live = ($st -le $now) -and ($now -le $en)
            Write-Host ("  {0,-12} live={1,-5} {2} | {3} ~ {4} | stages={5}" -f $e.Name, $live, (Get-GdMapValue $a 'Tip'), (Format-GdUnixMs $st), (Format-GdUnixMs $en), ((Get-GdMapValue $v 'Stages' | ForEach-Object { $_.Value }) -join ','))
        }
    }
}
}

# 两段各按"这个仓库里有没有那一路的判定库"决定打不打 —— 单游戏发布仓库里另一路的函数根本不存在，
# 直接调用会抛"术语不会被识别"，而这份文件是两边 README 都可能点到的诊断入口。
if (Get-Command Get-ArkEvent -ErrorAction SilentlyContinue) {
    Write-Host "`n== Get-ArkEvent 结论 =="
    $d = Get-ArkEvent -S $S
    Write-Host "  Mode=$($d.Mode) Stage=$($d.Stage) ClearSanity=$($d.ClearSanity)`n  Reason=$($d.Reason)"
}
if (Get-Command Get-R1999Event -ErrorAction SilentlyContinue) {
    Write-Host "`n== Get-R1999Event 结论 =="
    $r = Get-R1999Event -S $S
    Write-Host "  Mode=$($r.Mode) Instance=$($r.Instance)`n  Reason=$($r.Reason)"
    $inst = Resolve-R1999Instance -S $S -PlanKey $r.Instance
    Write-Host "  实例 $($inst.Name) id=$($inst.Id) 任务 $($inst.Tasks.Count) 个 -> $($inst.Tasks -join ' > ')"
}
