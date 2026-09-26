# lib/vision-rescue.ps1 — 「卡住了就把画面交给视觉模型」这一路兜底里**与后端无关**的那半
#
# 为什么要有它：模板匹配只认自己集里的东西。端游 splash 上叠一个它没见过的新活动弹窗
# (2026-09-24 那个中秋节活动窗口就是)，MAA 会一直在原地点 START —— asst.log 里 exec_times
# 从 1 涨到 316、模板分数始终 0.99，gui.log 却整整 22 分钟一个字不写。
# 人眼一看就知道「先把弹窗关掉」，模板匹配永远看不到这一步 → 这一步交给视觉模型。
# 2026-09-26 起 1999 那一路(MuMu + M9A)也接上了：卡在更新界面/弹窗上同样是"人眼一看就知道"。
#
# 分工：本文件只管"问模型 → 抠 JSON → 换算点位 → 点一下 → 确认画面真的变了"。
# 它不认识端游，也不认识模拟器 —— 截图和点击都向一个"靶子"(target)要，靶子由那一路的后端文件给：
#   lib\vision-ark.ps1   → New-GdVisionTarget-ark    (PrintWindow 拍窗口 + 抢前台真实点击)
#   lib\vision-mumu.ps1  → New-GdVisionTarget-mumu   (adb screencap/pull 拍屏 + adb input tap)
# 于是方舟仓库只带 ark 那一份、1999 仓库只带 mumu 那一份，本文件两边共用
# (归属表见 tools\publish-split.ps1；两份模块清单都列了它，Get-GdModuleLibPaths 按名字去重、只点一次)。
#
# 第三层：问模型这件事本身(HTTP)交给 tools\vision\ask.py，由 Invoke-GdVisionRaw 起子进程跑它。
# 为什么单拎出去：Invoke-RestMethod 把"超时/连不上/key 不对/端点写错/被限流"揉成同一句 WebException，
# 而兜底要回答的正是"这次没问成，该改哪儿"；Python 侧按状态分好类、429/5xx 还会退避重试。
# **搬走的只有发请求**：解析回复、换算点位、判断能不能点仍然在这个文件里 —— 那部分是安全边界，
# 有 tools\test-ark.ps1 的断言盯着，不跟着 HTTP 一起搬家。解释器怎么找见 lib\common.ps1 的 Get-GdPythonExe。
# key 只经环境变量给子进程：不进命令行、不进请求文件、不进日志、不进诊断包。
#
# 后端怎么找：Get-GdVisionTarget 按 `New-GdVisionTarget-<kind>` 这个命名约定 Get-Command 探路，
# 找不到就返回 $null → 调用方拿到一句"这棵树上没有这一路的兜底后端"。
# 和 lib\maa.ps1 里 `Get-Command Invoke-GdVisionRescue` 同一套纪律：**不靠导出时改写源码**。
#
# 靶子必须提供什么(契约写在这里，后端文件照着实现)：
#   Kind / Name            后端名 + 日志里"已拍 X"的那个 X
#   Enabled / Why          这一路自己的开关；关掉时那句原因必须带 settings 里的键名(t13 按它断言)
#   NeedsAdmin / AdminWhy  点这一路要不要提权(端游要，模拟器不要)
#   Scene / UserText       提示词开头"这是什么画面"的文案 —— 判定与点位算法对此完全无感
#   Alive                  scriptblock：对象现在还在不在(窗口关了 / 虚拟机没起来)
#   Shot(-Path)            scriptblock：拍一张 → @{Ok;Path;ImgW;ImgH;ClientLeft;ClientTop;ClientW;ClientH;AvgLum;Sig;Hash;Why}
#   Click(-X,-Y)           scriptblock：往换算后的坐标点一下 → @{Mode;Sent}
#   Grab / Desktop         可选(端游那套"抢前台 + 现场状态")；模拟器给 $null，本文件不调它
#
# 边界(故意收得很紧，两路共用)：
#   * 只在异常路径被调用(判定层认为卡住之后)，正常一轮**一个 API 请求都不会发**；
#   * 只让它「关掉挡路的东西」，明确禁止它点领取/前往参与/确定这类会把号往活动里带的按钮；
#   * 一轮最多救 visionMaxRescue 次(次数归调用方管)，每次只点 1 下，点完必须看到画面变了才算数；
#   * 拿不到 key / 后端前置条件不满足 / 截图是黑的 → 说明原因后直接放弃，绝不硬猜。
#
# 端游那一路的权限硬要求(现在写在 lib\vision-ark.ps1)：Arknights.exe 的清单是 requireAdministrator，
# 中完整性的进程对它 PostMessage 返回 false、PrintWindow 也失败(全都不报错，只是没反应)。
# 计划任务那一路(RunLevel Highest)本来就有提权，所以定时跑不受影响；手动跑别加 -NoElevate。
#
# 依赖：调用方先点 lib\common.ps1。两份后端文件各点各的库，本文件不点它们。

function Get-GdRescueShotDir { Join-Path $global:GD.State 'rescue' }

function New-GdRescueSkip {
    <# "这次没救成，原因是……"的统一出口：Tried=$false 表示连一次点击都没发出去 #>
    param([string]$Why)
    return [pscustomobject]@{ Tried = $false; Done = $false; Detail = $Why }
}

function Get-GdShotStats {
    <# 从一张位图算两样东西：平均亮度(整屏黑的时候模型会一本正经地胡说八道，白花一次钱，所以在这里就拦掉)
       + 16x9 的粗亮度签名(判"点完画面到底变没变")。
       端游和模拟器的截图都走这里 → 两路的"变没变"用的是同一把尺子。
       为什么不用文件哈希：端游背景有噪点动画，同一画面两次截图哈希都不一样
       (2026-09-24 就是这么误判过一次"救活了")。 #>
    param(
        [Parameter(Mandatory)]$Bmp,
        [Parameter(Mandatory)][int]$W,
        [Parameter(Mandatory)][int]$H,
        [int]$Step = 7
    )
    $sum = 0L; $n = 0
    $acc = New-Object 'int[]' 144; $cnt = New-Object 'int[]' 144
    for ($y = 0; $y -lt $H; $y += $Step) {
        for ($x = 0; $x -lt $W; $x += $Step) {
            $c = $Bmp.GetPixel($x, $y)
            $l = [int](($c.R * 30 + $c.G * 59 + $c.B * 11) / 100)
            $sum += $l; $n++
            $gy = [int]($y / $H * 16); if ($gy -gt 15) { $gy = 15 }
            $gx = [int]($x / $W * 9);  if ($gx -gt 8)  { $gx = 8 }
            $k = $gy * 9 + $gx
            $acc[$k] += $l; $cnt[$k]++
        }
    }
    # /8 量化：把抗锯齿和噪点造成的 1~2 档抖动压掉，剩下的才是"这块区域真的换了内容"
    $sig = for ($k = 0; $k -lt 144; $k++) { if ($cnt[$k]) { [int][math]::Round($acc[$k] / $cnt[$k] / 8) } else { 0 } }
    return [pscustomobject]@{ AvgLum = [math]::Round($sum / [math]::Max(1, $n)); Sig = ($sig -join ',') }
}

function Get-GdVisionTarget {
    <# 靶子取号处：只做"按命名约定找函数"这一件事，所以本文件可以一个 ark/mumu 的字面量都不写，
       也不必知道后端用哪个路径、要不要管理员。
       返回 $null = 这棵树上没有这一路的后端(单游戏仓库的正常形态)，调用方按"这一轮不救"处理。 #>
    param([Parameter(Mandatory)]$S, [string]$Kind = 'ark', $Conn = $null)
    if (-not $Kind) { $Kind = 'ark' }
    $builder = "New-GdVisionTarget-$($Kind.ToLowerInvariant())"
    if (-not (Get-Command $builder -ErrorAction SilentlyContinue)) { return $null }
    if ($null -ne $Conn) { return (& $builder -S $S -Conn $Conn) }
    return (& $builder -S $S)
}

function Get-GdVisionSystemPrompt {
    <# 提示词单独成一个函数：这样自检能直接断言"禁止点领取"这类关键约束还在，
       而不是改一句话把安全边界改没了都没人发现。
       -Scene 只换开头那段"这是什么画面"，后面那几条规则两路共用、一个字不改；
       默认值是端游那套文案 —— ark 一路的措辞不许变，tools\test-ark.ps1 的 t13 也盯着它。
       2026-09-26 起允许最多 3 个候选点位：实测模型认得弹窗、却把 × 认成了"暂不查看"那类相邻按钮，
       一次只给一个点位时"点完画面没变"就只能整轮放弃；给它三个按把握排序的备选，
       兜底就能在**同一次问价**里连着试(试错不额外花钱，也不额外问模型)。
       -AllowReward 是第 3 条规则的口子(默认关)：开了之后允许它给"领取日常奖励"那类按钮。
       为什么默认关、又为什么要这个口子：兜底的本职是"把挡路的东西弄走"，点领取会把号往奖励里去，
       判错一次就是拿账号换一次不确定；但每日签到/邮箱领奖励恰好也是"人眼一看就该点"的那类，
       所以做成**要人在面板里明写勾选**的可选档，而不是默认行为，也不是永远不许。 #>
    param([string]$Scene = '', [bool]$AllowReward = $false)
    $head = if ($Scene) { [string]$Scene } else { @'
你是明日方舟端游自动化流程里的"看屏幕的人"。自动化助手(MAA)不认识界面上没收录过的弹窗，有时会一直在原地
点同一个按钮。给你一张端游窗口截图，请判断画面上有没有挡住主流程的弹窗，并指出该点哪里把它关掉。
'@.TrimEnd() }
    # 两条规则只差"领取"这一个词能不能给，其余(不碰充值/前往参与/开始行动)一个字都不松。
    $rule3 = if ($AllowReward) { @'
3. 这一轮已允许"领取日常奖励"：画面里有**明确的**领取/一键领取/邮箱领奖励按钮时可以给出它。
   但"前往参与""开始行动""确定""充值"这类会改变游戏进度或花钱的按钮绝对不给，即使它在画面正中。
'@ } else { @'
3. 绝对不要给"领取""前往参与""确定""开始行动""充值"这类会推进游戏状态的按钮，即使它在画面正中。
'@ }
    $body = @'

只输出一个 JSON 对象，不要 markdown 代码块、不要任何解释文字：
{"blocking": true 或 false, "what": "不超过20字，说明挡住的是什么", "clicks": [{"x": 整数, "y": 整数, "why": "不超过14字"}]}

必须遵守的规则：
1. x、y 是相对这张图片宽、高的千分比(0-1000)，不是像素。换算：像素x = x/1000*图片宽。
2. clicks 最多 3 个，按你的把握从大到小排。只允许给"关闭/取消/暂不/跳过/知道了/X"这类**只会把弹窗关掉**的位置。
'@ + $rule3.TrimEnd() + @'
4. 关闭按钮通常在该弹窗的右上角(常常是一个 X 或小圆圈)。弹窗有多个关闭位时，把最像关闭的那个排第一。
5. 画面上没有挡住流程的弹窗 → blocking 给 false、clicks 给空数组。拿不准也一律给空数组。
6. 不要输出坐标以外的多余字段。
'@
    return $head + $body
}

function Get-GdVisionVerdictFromText {
    <# 把模型回复解析成 @{Blocking;What;X;Y;Why;Clicks}，解析不出来返回 $null。
       模型偶尔会裹一层 ```json 或前后带一句话，所以从整段文本里"抠"第一个 JSON 对象，
       而不是假设整段就是 JSON。纯字符串处理，自检直接喂样本。
       X/Y/Why 一直等于 Clicks 的第一个：老代码和 t13 那批断言认的是这三个字段，
       新的多候选循环认 Clicks —— 加字段不破约，改名字才破约。 #>
    param([string]$Text)
    if ([string]::IsNullOrWhiteSpace($Text)) { return $null }
    $t = $Text -replace '```[a-zA-Z]*', ''
    $start = $t.IndexOf('{')
    if ($start -lt 0) { return $null }
    $depth = 0; $json = ''
    for ($i = $start; $i -lt $t.Length; $i++) {
        $ch = $t[$i]
        if ($ch -eq '{') { $depth++ }
        elseif ($ch -eq '}') { $depth-- }
        if ($depth -lt 0) { break }
        $json += $ch                                  # 收尾那个 } 也要留着，否则抠出来的是半个 JSON
        if ($depth -eq 0 -and $i -gt $start) { break }
    }
    if (-not $json) { return $null }
    try { $o = $json | ConvertFrom-Json } catch { return $null }
    # 只留"三个 x/y 都是整数"的那几个：模型给出 {"x":null} 或把 y 写成 "右上角" 时，
    # 后面 Get-GdRescueClickPoint 的 TryParse 会把它拒掉，但那样一条坏点位就把两个好点位一起带走了。
    $clicks = @(foreach ($c in @($o.clicks)) {
        if ($null -eq $c) { continue }
        $cx = 0; $cy = 0
        if (-not ([int]::TryParse([string]$c.x, [ref]$cx) -and [int]::TryParse([string]$c.y, [ref]$cy))) { continue }
        [pscustomobject]@{ X = $cx; Y = $cy; Why = [string]$c.why }
    })
    if ($clicks.Count -gt 3) { $clicks = @($clicks | Select-Object -First 3) }
    $first = if ($clicks.Count) { $clicks[0] } else { $null }
    [pscustomobject]@{
        Blocking = [bool]$o.blocking
        What     = [string]$o.what
        X        = $(if ($first) { $first.X } else { $null })
        Y        = $(if ($first) { $first.Y } else { $null })
        Why      = $(if ($first) { $first.Why } else { '' })
        Clicks   = $clicks
    }
}

function Get-GdRescueClickPoint {
    <# 模型 verdict(千分比，相对整张截图) → 后端点击要的**那幅图的坐标**。
       端游拍的是整窗位图，里面有标题栏和边框，所以要先减掉客户区在位图里的原点，再按客户区尺寸裁住边界；
       模拟器拍的就是整块屏幕，它的靶子把 ClientLeft/ClientTop 报 0、ClientW/ClientH 报图的宽高，
       同一个函数算出来正好是设备像素 —— 所以这里不需要认识后端。
       越界(说明模型指到画面外/标题栏上，或者它压根没给坐标)返回 $null → 宁可这一轮不救。 #>
    param(
        [Parameter(Mandatory)]$Shot,
        [Parameter(Mandatory)]$Verdict,
        [int]$Max = 1000
    )
    if ($null -eq $Verdict.X -or $null -eq $Verdict.Y) { return $null }
    $nx = 0; $ny = 0
    if (-not ([int]::TryParse([string]$Verdict.X, [ref]$nx) -and [int]::TryParse([string]$Verdict.Y, [ref]$ny))) { return $null }
    if ($nx -lt 0 -or $nx -gt $Max -or $ny -lt 0 -or $ny -gt $Max) { return $null }
    $px = [int][math]::Round($nx / $Max * $Shot.ImgW)     # 位图里的像素
    $py = [int][math]::Round($ny / $Max * $Shot.ImgH)
    $cx = $px - [int]$Shot.ClientLeft                     # 客户区坐标
    $cy = $py - [int]$Shot.ClientTop
    if ($cx -lt 0 -or $cy -lt 0) { return $null }
    if ($cx -ge [int]$Shot.ClientW -or $cy -ge [int]$Shot.ClientH) { return $null }
    # Why 是从 verdict 那里顺带过来的：调用方要把"这个点位模型说它是干什么的"打进日志，
    # 否则日志里只有三个坐标，回看时谁也说不清当时点的是 × 还是"暂不查看"。
    return [pscustomobject]@{ ClientX = $cx; ClientY = $cy; BitmapX = $px; BitmapY = $py; NormX = $nx; NormY = $ny;
        Why = $(if ($Verdict.Why) { [string]$Verdict.Why } else { '' }) }
}

function Get-GdVisionApiKey {
    <# 环境变量优先(便于不落盘)，其次 settings.runtime.visionApiKey。
       key 只读不打印：日志里永远只出现"有没有配"，不出现内容。 #>
    param([Parameter(Mandatory)]$S)
    $k = $env:DASHSCOPE_API_KEY
    if (-not $k -and $S.runtime -and $S.runtime.visionApiKey) { $k = [string]$S.runtime.visionApiKey }
    if ($k) { return $k.Trim() }
    return ''
}

function Resolve-GdVisionEndpoint {
    <# 从控制台复制下来的往往是"基础地址"(以 /v1 或 /compatible-mode/v1 结尾)，而 POST 要打到
       .../chat/completions。两种写法都给同一个结果，已经带全的原样返回 —— 免得玩家对着 README
       里那个默认值发愣：他自己的地址末尾少了 /chat/completions 就会 404，而这看起来像"模型不可用"。 #>
    param([string]$Base)
    $b = if ($Base) { ([string]$Base).Trim().TrimEnd('/') } else { '' }
    if (-not $b) { $b = 'https://dashscope.aliyuncs.com/compatible-mode/v1' }
    if ($b -match '(?i)/chat/completions$') { return $b }
    return "$b/chat/completions"
}

function Get-GdVisionAskDir { Join-Path $global:GD.State 'vision' }

function Invoke-GdVisionRaw {
    <# 一次"问模型"的**原始**往返：起 Python 子进程跑 tools\vision\ask.py，把它的回复原样带回来。
       为什么这一层交给 Python：超时/连不上/key 不对/端点写错/被限流这几件事，
       Invoke-RestMethod 只会揉成一句 WebException，而兜底最需要的恰恰是"这次没问成，该改哪儿"；
       Python 侧按 HTTP 状态分好了类，还带 429/5xx 的退避重试(lib\common.ps1 的 Get-GdPythonExe 说怎么找到解释器)。
       分工没变的那一半：它**只**管发请求。解析回复、换算点位、判断能不能点，全都还在本文件里
       —— 那里有 tools\test-ark.ps1 现成的断言守着"禁止点领取"这类安全边界，不跟 HTTP 一起搬走。
       -Context 是"自动化这边为什么在问"(卡在哪个任务、重复点了多少次)，拼到用户那句话后面；
       同一张画面，带不带这句会挑出不同的"挡路货"，而兜底一轮只有那几次机会。
       key 只经环境变量给子进程：不进命令行(进程列表看得见)、不进请求文件、不进日志、不进诊断包。
       返回 @{Ok;Text;Model;HttpStatus;ElapsedMs;Attempts;Error;RequestPath}，任何一步不对都不抛，
       由调用方决定是 throw(兜底)还是打印(只问不点的诊断入口)。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][string]$ImagePath,
        [string]$Scene = '',
        [string]$UserText = '',
        [string]$Context = ''
    )
    $key = Get-GdVisionApiKey -S $S
    if (-not $key) { return [pscustomobject]@{ Ok = $false; Error = '没有视觉模型的 API key(settings.runtime.visionApiKey 或环境变量 DASHSCOPE_API_KEY)' } }
    $script = Join-Path (Get-GdRoot) 'tools\vision\ask.py'
    if (-not (Test-Path -LiteralPath $script)) {
        return [pscustomobject]@{ Ok = $false; Error = '缺少 tools\vision\ask.py(问模型这一层的实现)，这一棵树上问不了模型' }
    }
    $py = Get-GdPythonExe -S $S
    if (-not $py) {
        return [pscustomobject]@{ Ok = $false; Error = '没找到可用的 Python 3(问模型这一层用它跑)：跑 setup.ps1 -WithPython 自动装一份到 state\python\，或把 runtime.pythonExe 指到自己装的那份' }
    }
    $model = if ($S.runtime.visionModel) { [string]$S.runtime.visionModel } else { 'qwen3.8-flash' }
    $sec = [int]$S.runtime.visionTimeoutSeconds
    if ($sec -le 0) { $sec = 45 }
    if (-not $UserText) { $UserText = '这是当前画面。按规则只回一个 JSON 对象。' }
    if ($Context) {
        # 上下文截到 200 字：它是"给模型的一句处境说明"，不是日志转储。
        # 一次问价的钱按 token 算，而 Detail 里那串任务名/日志文件名对模型判断画面毫无帮助。
        $ctx = ([string]$Context) -replace '\s+', ' '
        if ($ctx.Length -gt 200) { $ctx = $ctx.Substring(0, 200) + '…' }
        $UserText = "$UserText  自动化这边的处境：$ctx"
    }
    $dir = Get-GdVisionAskDir
    if (-not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir -Force) }
    $reqPath = Join-Path $dir ('ask-' + (Get-Date -Format 'MMdd-HHmmss-fff') + '.json')
    # 提示词太长、还带引号和换行，塞命令行必然出乱子；图片路径也可能有空格 → 一律走请求文件
    $req = [ordered]@{
        image            = [string]$ImagePath
        system_prompt    = (Get-GdVisionSystemPrompt -Scene $Scene -AllowReward ([bool]$S.runtime.visionAllowReward))
        user_text        = $UserText
        endpoint         = (Resolve-GdVisionEndpoint -Base ([string]$S.runtime.visionBaseUrl))
        model            = $model
        temperature      = 0                 # 兜底要的是可复现，不是发挥
        max_tokens       = 300
        timeout_seconds  = $sec
        audit            = (Join-Path $dir 'calls.jsonl')
    }
    Write-GdJson -Path $reqPath -Object ([pscustomobject]$req)
    # 子进程总超时 = 单次超时 ×(重试次数 + 1)+ 启动余量。Python 侧默认重试 2 次(只在 429/5xx 上重试)，
    # 所以最坏是 3×45s；不给够这个数会在它还在退避重试时就把进程杀掉，白得到一个"超时"。
    $r = Invoke-GdChild -File $py -TimeoutSeconds ($sec * 3 + 30) -Env @{ DASHSCOPE_API_KEY = $key } `
                        -Arguments @($script, '--request', $reqPath)
    $out = ("$($r.Out)").Trim()
    $parsed = $null
    if ($out) {
        # ask.py 保证 stdout 只有一个 JSON 对象；万一解释器往前面打了别的(版本警告、site 报错)，
        # 从后往前找第一行能解析的就够了 —— 别整段 ConvertFrom-Json，那样一句警告就能把兜底弄挂。
        foreach ($line in @(($out -split "`r?`n") | Where-Object { $_ -match '^\s*\{' } | ForEach-Object { $_ })) {
            try { $parsed = $line | ConvertFrom-Json; break } catch { }
        }
    }
    if (-not $parsed) {
        $why = "Python 没回 JSON(退出码 $($r.Code))"
        $tail = ("$($r.Err)").Trim()
        if ($r.Code -eq -1) { $why = "问模型超时(Python 侧 $((Get-Date).ToString('HH:mm:ss')) 前没回)" }
        elseif ($tail) { $why += "：$tail" }
        elseif ($out) { $why += "：$out" }
        return [pscustomobject]@{ Ok = $false; Error = $why; RequestPath = $reqPath }
    }
    Clear-GdVisionAskDir
    return [pscustomobject]@{
        Ok          = [bool]$parsed.ok
        Text        = [string]$parsed.text
        Model       = [string]$parsed.model
        HttpStatus  = [int]$parsed.http_status
        ElapsedMs   = [int]$parsed.elapsed_ms
        Attempts    = [int]$parsed.attempts
        Error       = [string]$parsed.error
        RequestPath = $reqPath
    }
}

function Invoke-GdVisionAsk {
    <# 问完直接给解析好的 verdict(兜底只要这一个结果)。任何一步不对都 throw，由调用方统一降级成"这轮不救"。
       单独成函数就是为了给自检一个可以整体替换的假实现(自检绝不联网、也不起子进程)。
       -Scene/-UserText 由靶子传进来：同一个模型，问端游窗口和问模拟器屏幕的话不一样。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][string]$ImagePath,
        [string]$Scene = '',
        [string]$UserText = '',
        [string]$Context = ''
    )
    $raw = Invoke-GdVisionRaw -S $S -ImagePath $ImagePath -Scene $Scene -UserText $UserText -Context $Context
    if (-not $raw.Ok) { throw ([string]$raw.Error) }
    return (Get-GdVisionVerdictFromText -Text $raw.Text)
}

function Test-GdShotChanged {
    <# 点完到底有没有真变：比两张图的 16x9 粗亮度签名，数"变了多少格"。
       阈值(≥2 格、每格 ≥3 档)是按端游实测噪声定的：噪点动画顶多让个别格子抖 1~2 档，
       而"关掉一个弹窗"必然动到一整片区域。纯字符串处理 → 自检能直接喂样本，不用真截图。 #>
    param(
        [Parameter(Mandatory)]$Before,
        [Parameter(Mandatory)]$After,
        [int]$MinCells = 2,
        [int]$MinDelta = 3
    )
    if (-not $Before.Sig -or -not $After.Sig) {
        return [pscustomobject]@{ Known = $false; Changed = $false; Cells = -1; Detail = '没有画面签名可比对(截图那一步就没成)' }
    }
    $a = @($Before.Sig -split ',' | ForEach-Object { [int]$_ })
    $b = @($After.Sig  -split ',' | ForEach-Object { [int]$_ })
    if ($a.Count -ne $b.Count) {
        return [pscustomobject]@{ Known = $false; Changed = $false; Cells = -1; Detail = "两次画面尺寸不同($($a.Count) 格 vs $($b.Count) 格)" }
    }
    $diff = 0
    for ($i = 0; $i -lt $a.Count; $i++) { if ([math]::Abs($a[$i] - $b[$i]) -ge $MinDelta) { $diff++ } }
    return [pscustomobject]@{ Known = $true; Changed = ($diff -ge $MinCells); Cells = $diff
        Detail = "画面变了 $diff 格(判据：≥$MinCells 格、每格 ≥$MinDelta 档)" }
}

function Invoke-GdVisionRescue {
    <# 兜底总入口：拿靶子 → 截图 → 问模型 → 点一下 → 再截图确认画面真的变了。
       返回 @{Tried;Done;Detail}。Tried=是否真的动过手；Done=是否点完并且画面确实变了。
       一切前置条件不满足都返回 Tried=$false + 一句人话，调用方只管把 Detail 拼进结论里。
       -Target 不给就是 'ark'：lib\maa.ps1 那几个调用点一个字都不用改。
       -Context 是"为什么要问这一句"(卡在哪个任务/重复点了多少次)，会拼进给模型的那句话。
       为什么值得带：同一张主界面，"MAA 说它在找活动公告的 ×"和什么都不说，模型挑哪个是挡路货不一样；
       而兜底一次就一轮里那几次机会，问得准比问得省重要。上下文只有一句话的长度，不塞日志原文。
       闸门顺序不许调换：后端在不在 → 开关 → 权限 → key → Python → 对象还在不在 → 截图。
       (前几道都排在"截图"之前，自检才能在不碰窗口、不联网的前提下把这几条各演一遍；
        开关那句的原因必须带 settings 里的键名，t13 就按那几个字断言"关了确实一次都不试"。) #>
    param(
        [Parameter(Mandatory)]$S,
        [string]$Tag = 'stall',
        [string]$Target = 'ark',
        [string]$Context = '',
        $Conn = $null
    )
    $t = Get-GdVisionTarget -S $S -Kind $Target -Conn $Conn
    if (-not $t) {
        return (New-GdRescueSkip "这棵树上没有 $Target 这一路的视觉兜底后端(没有 New-GdVisionTarget-$Target 这个函数)")
    }
    if (-not $t.Enabled) { return (New-GdRescueSkip [string]$t.Why) }
    if ($t.NeedsAdmin -and -not (Test-GdAdmin)) {
        $why = if ($t.AdminWhy) { [string]$t.AdminWhy } else { '这一路的对象要管理员权限，这个权限下点它不会有任何反应' }
        return (New-GdRescueSkip "当前不是管理员权限：$why，跳过兜底")
    }
    if (-not (Get-GdVisionApiKey -S $S)) {
        return (New-GdRescueSkip '没配视觉模型 key(见 settings 里的 runtime.visionApiKey)，本轮不尝试兜底')
    }
    # Python 这条闸门排在截图之前，和 key 同一个道理：不问模型也能先把"这棵树上问不成模型"说清楚，
    # 而且省掉一次白拍 —— 更要紧的是自检能在不碰窗口、不起子进程的前提下演到这一条。
    if (-not (Get-GdPythonExe -S $S)) {
        return (New-GdRescueSkip '这台机器上没有可用的 Python 3(问模型那一层用它跑)：跑 setup.ps1 -WithPython 装一份到 state\python\，或把 runtime.pythonExe 指到已有的那份')
    }
    if (-not (& $t.Alive)) { return (New-GdRescueSkip "$($t.Name)已经不在了，兜底没有对象") }

    $dir = Get-GdRescueShotDir
    $stamp = Get-Date -Format 'MMdd-HHmmss'
    $before = & $t.Shot (Join-Path $dir "$($t.Kind)-$Tag-$stamp-before.png")
    if (-not $before.Ok) { return (New-GdRescueSkip "截图失败：$($before.Why)") }
    if ([int]$before.AvgLum -lt 8) {
        return (New-GdRescueSkip "截图几乎是全黑(平均亮度 $($before.AvgLum))：桌面可能锁了或窗口没在渲染，问模型只会白花钱")
    }
    Write-GdLog "视觉兜底：已拍$($t.Name)($($before.ImgW)x$($before.ImgH)，亮度 $($before.AvgLum))，交给模型判断" -Level WARN
    $v = $null
    try { $v = Invoke-GdVisionAsk -S $S -ImagePath $before.Path -Scene ([string]$t.Scene) -UserText ([string]$t.UserText) -Context $Context }
    catch { return (New-GdRescueSkip "模型调用失败：$($_.Exception.Message)") }
    if (-not $v) { return (New-GdRescueSkip '模型没按格式回 JSON，不拿它的话当依据') }
    if (-not $v.Blocking) { return (New-GdRescueSkip "模型认为没有弹窗挡路($($v.What))") }
    # 换算 + 裁边界都在 Get-GdRescueClickPoint 里做，这里先把落不到画面里的那几个剔掉：
    # 宁可"一个可用点位都没有"直接放弃，也不要在真点击时才发现第 2 个点位在标题栏上。
    $points = @(foreach ($c in @($v.Clicks)) {
        $one = Get-GdRescueClickPoint -Shot $before -Verdict $c
        if ($one) { $one }
    })
    if (-not $points.Count) {
        return (New-GdRescueSkip "模型说挡路的是$($v.What)，但它给的坐标($($v.X),$($v.Y))落不到画面里 → 不点")
    }
    <#
      按把握从高到低逐个试，谁点完画面真变了就算救活。三次机会都来自同一次问价，所以：
      不额外花钱、不额外等模型、也不会出现"换了个答案把上一个还回去"。
      每次点击之后都要补拍一张再比签名 —— 这是唯一能证明"点动了"的证据(2026-09-24 那次
      哈希不同但弹窗还在，就是缺这一步)。最后一轮的 after 就是这一轮的结论图，中间那几张随 Clear 掉。
    #>
    $tries = @()
    $after = $null
    foreach ($pt in $points) {
        Write-GdLog "视觉兜底：$($v.What) → 点$($t.Name) $($pt.ClientX),$($pt.ClientY)($($pt.Why))" -Level WARN
        $click = & $t.Click ([int]$pt.ClientX) ([int]$pt.ClientY)
        Start-Sleep -Seconds 6
        $after = & $t.Shot (Join-Path $dir "$($t.Kind)-$Tag-$stamp-after$($tries.Count + 1).png")
        if (-not $after.Ok) {
            Clear-GdRescueShotDir
            return (New-GdRescueSkip "点完补拍失败：$($after.Why)($($click.Mode))，无法确认是否生效")
        }
        $chg = Test-GdShotChanged -Before $before -After $after
        $tries += "点位 $($pt.ClientX),$($pt.ClientY)($($click.Mode))→$($chg.Detail)"
        if (-not $chg.Known) {
            Clear-GdRescueShotDir
            return [pscustomobject]@{ Tried = $true; Done = $false;
                Detail = "按模型说的把$([string]$v.What)点了一下($($tries[-1]))，但$($chg.Detail) → 不算救活" }
        }
        if ($chg.Changed) {
            Clear-GdRescueShotDir
            return [pscustomobject]@{ Tried = $true; Done = $true;
                Detail = "已按模型判断关掉$([string]$v.What)：$(if ($tries.Count -gt 1) { "试了 $($tries.Count) 个点位，" } else { '' })$($tries[-1])" }
        }
    }
    Clear-GdRescueShotDir
    return [pscustomobject]@{ Tried = $true; Done = $false;
        Detail = "按模型说的把$([string]$v.What)点了 $($tries.Count) 个候选点位($($tries -join '；'))，画面都没动 → 这一路走不通" }
}

function Clear-GdVisionFiles {
    <# 兜底的临时东西都是取证用的，别把 state 堆成几百个文件：按时间留最近 Keep 个。
       两路共用同一个目录，截图文件名开头是靶子的 kind(ark-/mumu-)，翻图时一眼分得清是谁的画面。 #>
    param([Parameter(Mandatory)][string]$Dir, [string]$Pattern = '*.png', [int]$Keep = 24)
    if (-not (Test-Path -LiteralPath $Dir)) { return }
    $stale = @(Get-ChildItem -LiteralPath $Dir -Filter $Pattern -ErrorAction SilentlyContinue |
               Sort-Object LastWriteTime -Descending | Select-Object -Skip $Keep)
    foreach ($f in $stale) { Remove-Item -LiteralPath $f.FullName -Force -ErrorAction SilentlyContinue }
}

function Clear-GdRescueShotDir { Clear-GdVisionFiles -Dir (Get-GdRescueShotDir) -Pattern '*.png' -Keep 24 }

function Get-GdRescueLedgerPath { Join-Path $global:GD.State 'rescue.json' }

function Add-GdRescueRecord {
    <# 救火记账：这一轮兜了几次、每次救活没有。写在 state\rescue.json，和 tools\vision\ask.py 的
       state\vision\calls.jsonl **分开放**，因为两本账回答的问题不一样：
         calls.jsonl = "这一次请求发出去了吗、HTTP 什么状态、花了多少毫秒"(接口层，一行一次请求)
         rescue.json = "哪一路、哪个环节、兜完画面到底变没变"(判定层，玩家和面板看的是这本)
       为什么由调用方(maa.ps1 / m9a.ps1)点它、内核自己不点：内核不知道自己是给哪一路兜的
       (-Target 只是后端名，"这一轮算 ark 还是 r1999"是调用方的事)，而记账恰恰要这个归属。
       只留最近 Keep 条：这是一本"回看上几轮"的账，不是日志归档。 #>
    param(
        [Parameter(Mandatory)]$S,
        [Parameter(Mandatory)][string]$Game,
        [Parameter(Mandatory)]$Result,
        [string]$Tag = '',
        [string]$Target = '',
        [int]$Keep = 60
    )
    $path = Get-GdRescueLedgerPath
    $ledger = [ordered]@{ version = 1; updated = ''; events = @() }
    # Read-GdJson 在文件不存在时是 throw，而"第一次救火"恰恰就是文件不存在那次 ——
    # 入口的 $ErrorActionPreference='Stop' 会让这句 throw 一路冒到调用方，把"记一笔账"升级成"整轮失败"。
    if (Test-Path -LiteralPath $path) {
        $old = $null
        try { $old = Read-GdJson -Path $path } catch { Write-GdLog "救火账本旧内容读不动(按空账继续)：$($_.Exception.Message)" -Level WARN }
        if ($old -and $old.events) { $ledger.events = @($old.events) }
    }
    $detail = [string]$Result.Detail
    $ledger.events = @($ledger.events) + @{
        time   = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
        game   = $Game
        target = $(if ($Target) { $Target } else { [string]$Result.Target })
        tag    = $Tag
        tried  = [bool]$Result.Tried
        done   = [bool]$Result.Done
        detail = ($detail -replace "`r?`n", ' | ')
    }
    if ($ledger.events.Count -gt $Keep) {
        $ledger.events = @($ledger.events | Select-Object -Last $Keep)
    }
    $ledger.updated = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
    # 账本写坏不能弄挂日常：兜底本身已经在异常路径上了，再抛一次等于把"卡住"升级成"整轮失败"。
    try { Write-GdJson -Path $path -Object ([pscustomobject]$ledger) } catch { Write-GdLog "救火账本没写进去：$($_.Exception.Message)" -Level WARN }
    return $true
}

function Clear-GdVisionAskDir {
    <# 请求文件里只有提示词和本机截图路径，没有任何 key(python 侧只从环境变量读)；
       留最近 12 份 = 够回看"这轮兜底到底问了什么"，又不会堆成日志。calls.jsonl 不删(它是账本，一行一条)。 #>
    Clear-GdVisionFiles -Dir (Get-GdVisionAskDir) -Pattern 'ask-*.json' -Keep 12
}
