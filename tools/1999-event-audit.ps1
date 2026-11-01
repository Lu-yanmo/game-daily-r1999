<#
  tools\1999-event-audit.ps1 — 「活动打没打完」从人嘴转给 agent：拍屏 → 视觉三态判定 → 只有明确 yes 才自动 ack。

  来历(2026-09-30 用户口径「另外活动打没打完让agent判断」)：event-pending 的语义是"活动进行中，
  系统不敢自动跑，等人来入口脚本的 ack 开关(单游戏仓 -Ack / 合体 -Ack1999)确认"。查过数据面：M9A 的 data\activity\cn.json 只有
  活动期元数据(版本/起止/类型)，**没有任何玩家进度** —— 所以判据只有一条路：模拟器里的 1999 画面 +
  视觉模型(qwen3.8-flash，通道 tools\vision\ask.py)。三条硬纪律：
    1) 只问不点：不碰 vision-rescue 的点击通道，截屏只走 adb(Save-GdMuMuShot)；
    2) 三态回答 yes / no / unknown —— 只有**画面证据明确的 yes** 才自动 Set-GdR1999Ack，
       no/unknown 维持 pending 照旧等人；宁可多问人一次，不许猜着 ack(ack 的下游是"放开自动刷活动方案")；
    3) 证据全留底：截图、模型原话、判定、有没有 ack，都落 state\1999-progress-audit\，事后可审计可回滚。
  一个时序事实决定了流程形态：M9A 任务队列的最后一项是「关闭游戏」，轮到收尾时游戏已经关了 ——
  想拍到活动进度就得用 adb monkey 把 1999 重新拉起来(这是本工具唯一"碰"游戏的动作，等价于点桌面图标)，
  拍完且只在我们拉起的情况下 `am force-stop` 还原现场。

    pwsh -File tools\1999-event-audit.ps1           # 只读：问一次、打印判定，永不 ack
    pwsh -File tools\1999-event-audit.ps1 -Apply    # 判定 yes → 自动 ack
    pwsh -File tools\1999-event-audit.ps1 -Image 某张.png   # 跳过模拟器，拿现成图只验"问+判+留底"后半段

  退出码：0 = 审计完成(yes/no/unknown 都算，无待确认活动也算无事发生) / 1 = 审不了(模拟器、Python、
  key、截屏、问模型任一环节不可用) / 2 = 意外错(抛出来的)
  提示词不许外挪：system prompt 里"yes 的门槛"就是本工具的全部安全性所在，改它 = 改护栏语义。
#>
[CmdletBinding()]
param(
    [switch]$Apply,
    [string]$Image = ''    # 给一张现成 PNG 就跳过模拟器整段(排障/拿历史图复问用)
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $root 'lib\common.ps1')
. (Join-Path $root 'lib\decisions.ps1')      # 门面：Get-R1999Event/Set-GdAckKey 这条判定链由它点进来(daily-clear 同款顺序)
. (Join-Path $root 'lib\modules.ps1')
[void](Import-GdModules -Quiet)
foreach ($libPath in (Get-GdModuleLibPaths)) { . $libPath }
# 本工具会往日志里写"下一步等价于敲哪个开关"(下面现问 Get-GdEntryVerb)：单游戏导出仓库拿到干净短名(-Ack)，
# 本地合体工程拿到本入口真有的长名(-Ack1999)——上面 Import-GdModules 已把清单装进注册表，11-01 丙形真转接(§11.176 ⑥)
# 之后这一支是"现问"而不是"点完库再同步一次"：合体/单游戏两档由注册表自己答，不再需要每枚入口各抄一遍映射。
# 写死任何一个都会在另一棵树上变成"照抄就跑不起来"的提示语 —— tools/test-lib.ps1 的 [t1d] 就是专门抓这个的。

$S = Get-GdSettings -TellAndExit
$ev = Get-R1999Event -S $S
if ($ev.Mode -ne 'event-pending') {
    Write-Host ('  当前不是待确认形态(mode={0}：{1})，无事发生' -f $ev.Mode, $ev.Reason)
    exit 0
}
Write-Host ('  待确认活动：{0}[{1}] —— 判定源只认画面，不猜' -f $ev.EventKey, $ev.EventTip) -ForegroundColor Cyan

$py = Get-GdPythonExe -S $S
if (-not $py) { Write-Host '  没找到可用的 Python 3(问模型那层用它) → 审不了' -ForegroundColor Red; exit 1 }
$key = Get-GdVisionApiKey -S $S
if (-not $key) { Write-Host '  没有视觉模型的 API key(settings.runtime.visionApiKey 或环境变量 DASHSCOPE_API_KEY) → 审不了' -ForegroundColor Red; exit 1 }
$askPy = Join-Path $root 'tools\vision\ask.py'
if (-not (Test-Path -LiteralPath $askPy)) { Write-Host '  缺 tools\vision\ask.py → 审不了' -ForegroundColor Red; exit 1 }

$auditDir = Join-Path (Get-GdDir 'State') '1999-progress-audit'
if (-not (Test-Path -LiteralPath $auditDir)) { [void](New-Item -ItemType Directory -Path $auditDir -Force) }
$ts = Get-Date -Format 'yyyyMMdd-HHmmss'
$png = ''
$weLaunched = $false
$weBootedVm = $false
$ep = $null
if ($Image) {
    <# -Image 旁路：拿一张现成图(历史截图/别的机器拷来的)只走"问模型+判定+留底"后半段。
       真机链路坏了时也能单独验证提示词与解析 —— 这就是 09-30 首验用的口子。 #>
    if (-not (Test-Path -LiteralPath $Image)) { Write-Host "  -Image 指的文件不存在：$Image → 审不了" -ForegroundColor Red; exit 1 }
    $png = $Image
    Write-Host "  用给定图片(-Image)，不碰模拟器" -ForegroundColor DarkGray
} else {
    # ---------- 第 1 步：adb 落点 → 游戏在不在 → 不在就拉起来等它进画面 ----------
    $pkg = [string]$S.re1999.package
    if (-not $pkg) { Write-Host '  settings.re1999.package 没配，连该拉起哪个应用都不知道 → 审不了' -ForegroundColor Red; exit 1 }
    <# 时序实锤(09-30 18:21 首跑)：M9A 队列末尾的「关闭游戏」把**整台虚拟机**也关了 —— 审计跑到
       MuMuManager 时报 vm not running。所以这里自己保证 VM 在：Start-GdMuMu 幂等(已起就秒过)，
       而"不是我们起的"与"是我们起的"要记下来 —— 现场还原的对称原则：谁拉起谁关，用完关的是自己拉的那份。 #>
    $vmWasUp = $false
    try { $stMm = Get-GdMuMuState -S $S; $vmWasUp = [bool]$stMm.AndroidStarted } catch { }
    if (-not (Start-GdMuMu -S $S)) { Write-Host '  MuMu 虚拟机起不来 → 审不了' -ForegroundColor Red; exit 1 }
    $weBootedVm = -not $vmWasUp
    <# 时序实锤二(09-30 19:10 首跑)：Start-GdMuMu 返回真(MuMuManager 报安卓已起)之后，
       `MuMuManager adb -v` 还要再晚几十秒才给得出 ip:port —— 客机里的 adb 守护比"安卓启动"标志起得晚。
       所以落点要有界轮询：每 6 秒问一次、最多 10 次(约一分钟)，问到就走，全空才认审不了。 #>
    $ep = $null
    for ($tryEp = 1; $tryEp -le 10; $tryEp++) {
        $ep = Get-GdMuMuAdbEndpoint -S $S
        if ($ep.Adb -and $ep.Serial) { break }
        if ($tryEp -lt 10) { Start-Sleep -Seconds 6 }
    }
    if (-not $ep.Adb -or -not $ep.Serial) { Write-Host "  拿不到 adb 落点(虚拟机起来后又等了一分钟)：$($ep.Why) → 审不了" -ForegroundColor Red; exit 1 }
    <# 实锤三(09-30 19:20)：拿到落点不等于连得上 —— 宿主机 adb server 列表是空的，直接拿序列号 `-s ip:port` 去 shell
       会报 device not found。先显式 connect 一次(Connect-GdAdb 自带 connect 后回查 devices，重复调无害)，
       与 m9a.ps1 日常路里那句「ADB 连接」是同一步。 #>
    if (-not (Connect-GdAdb -S $S -AdbPath $ep.Adb -Serial $ep.Serial)) {
        Write-Host "  adb connect $($ep.Serial) 没连上 → 审不了" -ForegroundColor Red; exit 1
    }
    if (-not (Get-Gd1999Pid -AdbPath $ep.Adb -Serial $ep.Serial -Package $pkg)) {
        Write-Host '  1999 客户端没在跑(M9A 收尾的「关闭游戏」干的) → adb monkey 拉起来，这是本工具唯一碰游戏的动作'
        $m = Invoke-GdNative -File $ep.Adb -Arguments @('-s', $ep.Serial, 'shell', 'monkey', '-p', $pkg, '-c', 'android.intent.category.LAUNCHER', '1') -TimeoutSeconds 30
        if ($m.Code -ne 0) { Write-Host "  monkey 拉起失败(退出码 $($m.Code))：$((($m.Out + $m.Err) -replace '\s+', ' ').Trim()) → 审不了" -ForegroundColor Red; exit 1 }
        $weLaunched = $true
        if (-not (Wait-GdCondition -Test { Get-Gd1999Pid -AdbPath $ep.Adb -Serial $ep.Serial -Package $pkg } -TimeoutSeconds 120 -PollSeconds 5 -What '1999 客户端进程')) {
            Write-Host '  拉了 120 秒进程还没起来 → 审不了' -ForegroundColor Red; exit 1
        }
        <# 进程在 ≠ 进了主界面：客户端还要过启动动画/登录/热更。v1 用定长等待换简单 —— 等少了拍到加载屏，
           判定会如实落 unknown(不算错)，明天再问；这条线要收紧的话改成"连拍两张等画面哈希稳定"。 #>
        Start-Sleep -Seconds 90
    }
}
try {
    if (-not $Image) {
        $png = Join-Path $auditDir "$ts.png"
        $shot = Save-GdMuMuShot -Adb $ep.Adb -Serial $ep.Serial -Path $png
        if (-not $shot.Ok) { Write-Host "  截屏失败：$($shot.Why) → 审不了" -ForegroundColor Red; exit 1 }
        Write-Host ('  截屏：{0}x{1} 平均亮度 {2}' -f $shot.ImgW, $shot.ImgH, $shot.AvgLum)
        if ([int]$shot.AvgLum -lt 8) { Write-Host '  这一屏几乎全黑，模型会一本正经地胡说 —— 照问，但结论预期是 unknown' -ForegroundColor Yellow }
    }

    # ---------- 第 2 步：问模型(自带 system prompt，不复用兜底那份点击话术) ----------
    $model = if ($S.runtime.visionModel) { [string]$S.runtime.visionModel } else { 'qwen3.8-flash' }
    $sec = [int]$S.runtime.visionTimeoutSeconds
    if ($sec -le 0) { $sec = 45 }
    $sysPrompt = @'
你是游戏《重返未来：1999》的玩家进度审计员。看的是手机截图，不是电脑桌面。
你的唯一任务：判断"当前主线/限时活动，玩家是否已经全部打完"。
回答纪律(违反任何一条就答 unknown)：
1. 只回一个 JSON 对象，别的一切文字都不要：{"verdict":"yes|no|unknown","evidence":"一句话，指出你看到的具体画面证据"}
2. 只有画面上出现明确的"全部完成"证据才许答 yes：例如活动页显示"首次通关奖励已全部领取"、活动关卡全部三星、活动理智/门票余量为 0 且无可打关卡。
3. 以下一律 unknown，不许猜：加载中/进度条/黑屏/弹窗/公告、停在主界面但进不去活动页、画面信息不足以确认完成度。
4. 明确看到"还有可打的关卡/可领未领的奖励/剩余理智还能刷"才算 no。
宁可 unknown 让人来看 —— 这个判定会直接决定系统要不要自动接管活动刷取。
'@
    $userText = ('这是《重返未来：1999》当前画面。问题：当前活动「{0}」玩家打完了吗？按 system 里的 JSON 纪律回答。' -f $ev.EventTip)
    $reqPath = Join-Path $auditDir "ask-$ts.json"
    $req = [ordered]@{
        image           = [string]$png
        system_prompt   = $sysPrompt
        user_text       = $userText
        endpoint        = (Resolve-GdVisionEndpoint -Base ([string]$S.runtime.visionBaseUrl))
        model           = $model
        temperature     = 0
        max_tokens      = 400
        timeout_seconds = $sec
        audit           = (Join-Path $auditDir 'calls.jsonl')
    }
    Write-GdJson -Path $reqPath -Object ([pscustomobject]$req)
    $run = Invoke-GdChild -File $py -TimeoutSeconds ($sec * 3 + 30) -Env @{ DASHSCOPE_API_KEY = $key } -Arguments @($askPy, '--request', $reqPath)
    $pick = Invoke-GdCli -Verb 'parse-vision-reply' -Body ([pscustomobject]@{ out = (("$($run.Out)").Trim()) })
    if (-not $pick.Found) { Write-Host "  模型那层没回可信 JSON(退出码 $($run.Code)) → 审不了" -ForegroundColor Red; exit 1 }
    $raw0 = $pick.Parsed
    if (-not [bool]$raw0.ok) { Write-Host "  问模型失败：$([string]$raw0.error) → 审不了" -ForegroundColor Red; exit 1 }
    $text = [string]$raw0.text
    Write-Host "  模型原话：$text"

    # ---------- 第 3 步：三态判定 + 留底 + (只有 yes 且 -Apply 才) ack ----------
    $verdict = 'unknown'
    $reply = $null
    try { $reply = $text | ConvertFrom-Json } catch { }
    if ($reply -and $reply.PSObject.Properties.Name -contains 'verdict' -and [string]$reply.verdict -in @('yes', 'no', 'unknown')) {
        $verdict = [string]$reply.verdict
    } elseif ($text -match '"verdict"\s*:\s*"?(yes|no|unknown)') {
        $verdict = $Matches[1]
    }
    $acked = $false
    if ($verdict -eq 'yes' -and $Apply) {
        Set-GdR1999Ack -S $S
        $acked = $true
        Write-Host "  判定 yes → 已自动确认活动(等价 daily-clear.ps1 $(Get-GdEntryVerb -Game 'r1999' -Verb 'Ack'))" -ForegroundColor Green
    } elseif ($verdict -eq 'yes') {
        Write-Host '  判定 yes，但这是只读运行(要自动 ack 请带 -Apply)' -ForegroundColor Yellow
    } else {
        Write-Host ('  判定 {0} → 维持 event-pending，照旧等人' -f $verdict)
    }
    $evidence = [ordered]@{
        ts = $ts; eventKey = [string]$ev.EventKey; eventTip = [string]$ev.EventTip
        verdict = $verdict; acked = $acked; apply = [bool]$Apply
        model = [string]$raw0.model; png = $png; weLaunched = $weLaunched; weBootedVm = $weBootedVm
        text = $text
    }
    Write-GdJson -Path (Join-Path $auditDir "$ts.json") -Object ([pscustomobject]$evidence)
    Write-Host ('  审计结论：verdict={0} acked={1}(证据在 {2}\{3}.png / .json)' -f $verdict, $acked, $auditDir, $ts)
    exit 0
} finally {
    if ($weLaunched) {
        [void](Invoke-GdNative -File $ep.Adb -Arguments @('-s', $ep.Serial, 'shell', 'am', 'force-stop', $pkg) -TimeoutSeconds 30)
        Write-Host '  现场还原：force-stop 掉我们拉起的 1999(M9A 本来也把它关了)' -ForegroundColor DarkGray
    }
    if ($weBootedVm) {
        <# 只有 VM 确实是我们起的才关它：$pkg 在这个分支里必然有值(同一 else 段)，Stop-GdMuMu 自带"没在跑就无需关闭"。 #>
        try { Stop-GdMuMu -S $S } catch { Write-Host "  关虚拟机没关成(不影响审计结论)：$($_.Exception.Message)" -ForegroundColor Yellow }
        Write-Host '  现场还原：关掉我们自己起的 MuMu 虚拟机' -ForegroundColor DarkGray
    }
}
