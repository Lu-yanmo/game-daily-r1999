# lib/vision-rescue.ps1 — 「卡住了就把画面交给视觉模型」这一路兜底里**与后端无关**的那半
#
# 为什么要有它：模板匹配只认自己集里的东西。端游 splash 上叠一个它没见过的新活动弹窗
# (2026-09-24 那个中秋节活动窗口就是)，MAA 会一直在原地点 START —— asst.log 里 exec_times
# 从 1 涨到 316、模板分数始终 0.99，gui.log 却整整 22 分钟一个字不写。
# 人眼一看就知道「先把弹窗关掉」，模板匹配永远看不到这一步 → 这一步交给视觉模型。
# 2026-09-26 起 1999 那一路(MuMu + M9A)也接上了：卡在更新界面/弹窗上同样是"人眼一看就知道"。
#
# 分工：本文件只管"问模型 → 抠 JSON → 换算点位"这几枚零件。"点一下 → 确认画面真的变了"那截编排
# 已在 2026-10-28 甲档第二十刀刀③ 出仓：判据住 Rust 核(tools\rustcore\src\main.rs 的 vision_rescue，
# --selftest 第 19 节六档＋第 30 节 ark 27 档金样)，开关话术住 C#(tools\panel-cs\Live.cs 的
# resolve-rescue-context，--livetest 两格盯"关了确实一次都不试"且那句带 settings 的键名)。
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
# 找不到就返回 $null，由调用方自己给一句原因(现在活着的调用方是 lib\supervisor.ps1 的"人眼看一眼"那一路)。
# 同一条纪律的原话是拿 `lib\maa.ps1 里 Get-Command Invoke-GdVisionRescue` 作比 —— 那枚比较对象与它比的
# 东西如今都不在现树了(maa.ps1 现读零枚 Get-Command)，但纪律本身不许变：**不靠导出时改写源码**。
#
# 靶子必须提供什么(契约写在这里，后端文件照着实现)：
#   Kind / Name            后端名 + 日志里"已拍 X"的那个 X
#   Enabled / Why          这一路自己的开关；关掉时那句原因必须带 settings 里的键名(C# 的
#                          resolve-rescue-context 出这句，--livetest 两格按键名断言)
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
#
# ── 冻结(2026-09-26，PS→C# 批次 4)────────────────────────────────────────────
# 纯规则段已迁到 tools\panel-cs\Vision.cs(契约见 docs\ps-migration-batch0.md)：
#   Get-GdVisionSystemPrompt     → verb build-vision-prompt(提示词五条规则与两个分支的全文在 C#，t13 逐字盯)
#   Get-GdVisionVerdictFromText  → verb parse-vision-verdict(抠 JSON/坏点位过滤/X-Y-Why 兼容字段)
#   Get-GdRescueClickPoint       → verb vision-click-point(千分比→客户区换算与越界拒绝)
#   Resolve-GdVisionEndpoint     → verb resolve-vision-endpoint
#   Test-GdShotChanged           → verb shot-changed(签名格数比对；那枚转发壳已随甲档第二十刀刀③ 出仓，
#                                  判据由 Vision.SelfTest.cs 的四形与 Rust 第 19 节的"点了但画面没变"格接守)
#   Invoke-GdVisionRaw 的 stdout 回扫段 → verb parse-vision-reply(起子进程与请求组装仍在本壳，Context 截 200 也在)
#   Add-GdRescueRecord 的字段合成段 → verb rescue-entry(读写盘、Test-Path 守卫与裁剪留本壳；
#                                  那枚壳整件随第二十刀刀③ 出仓，落盘面由 Rust 第 19 节读-改-写格＋第 30 节 H5/H6 接守)
# 所以：改规则/文案请改 Vision.cs 并跑 panel-cs --visiontest 对真值，不要在这里加规则。
# 现在留在 PS 侧的：Get-GdVisionTarget(命名约定探路)、Invoke-GdVisionAsk、Get-GdShotStats(要 Bitmap)、
# Get-GdVisionApiKey(key 绝不进 JSON 信封)、Clear-* 系(摸盘的)——都是监督心跳与"只问不点"诊断入口还要用的零件。
# 曾经按"编排层"理由留壳的 Invoke-GdVisionRescue / New-GdRescueSkip / Clear-GdRescueShotDir /
# Get-GdRescueLedgerPath / Test-GdShotChanged / Add-GdRescueRecord 六枚已于 2026-10-28 甲档第二十刀刀③ 出仓：
# 它的对面不再是 PS 活臂，而是两代 Rust 核＋金样，靶子闭包那套形状在核里由 resolve-rescue-context 一份 verb 回执复现。
# vision-mumu 整份留壳：test-r1999 把两段正则钉在函数定义文本上，薄壳就红。
# ──────────────────────────────────────────────────────────────────────────────

function Get-GdRescueShotDir { Join-Path $global:GD.State 'rescue' }

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
    <# 转发壳(build-vision-prompt)：提示词全文(两条分支的规则 3、两处 here-string 粘连的"没换行")住在
       tools\panel-cs\Vision.cs —— 改文案去那边改并跑 panel-cs --visiontest 对真值，在这里加规则会被自检打回。
       安全边界(禁止点领取/前往参与/充值；领奖档要面板明写勾选)的由来与两次实测教训见 Vision.cs 头注。 #>
    param([string]$Scene = '', [bool]$AllowReward = $false)
    $d = Invoke-GdCli -Verb 'build-vision-prompt' -Body ([pscustomobject]@{ scene = [string]$Scene; allowReward = [bool]$AllowReward })
    return [string]$d.Prompt
}

function Get-GdVisionVerdictFromText {
    <# 转发壳(parse-vision-verdict)：抠 JSON 的括号扫描、坏点位过滤、X/Y/Why=Clicks 第一个 的兼容约定，
       本体在 Vision.cs。这里只做"信封 data → 旧返回形状"的还原，Found:false 翻回 $null。
       空文本壳内早退(与旧实现同结果)，省一次子进程 —— 兜底循环里这不是高频路径，但自检喂空样本很勤。 #>
    param([string]$Text)
    if ([string]::IsNullOrWhiteSpace($Text)) { return $null }
    $d = Invoke-GdCli -Verb 'parse-vision-verdict' -Body ([pscustomobject]@{ text = [string]$Text })
    if (-not $d.Found) { return $null }
    return [pscustomobject]@{
        Blocking = [bool]$d.Blocking
        What     = [string]$d.What
        X        = $d.X
        Y        = $d.Y
        Why      = [string]$d.Why
        Clicks   = @(foreach ($c in @($d.Clicks)) { [pscustomobject]@{ X = [int]$c.X; Y = [int]$c.Y; Why = [string]$c.Why } })
    }
}

function Get-GdRescueClickPoint {
    <# 转发壳(vision-click-point)：千分比→位图像素→客户区的换算与全部越界拒绝(含"右下角最后一个像素")
       住在 Vision.cs；两路共用一把尺子的原理(mumu 靶子把客户区报成整块屏)也记在那边注释里。 #>
    param(
        [Parameter(Mandatory)]$Shot,
        [Parameter(Mandatory)]$Verdict,
        [int]$Max = 1000
    )
    if ($null -eq $Verdict.X -or $null -eq $Verdict.Y) { return $null }
    $d = Invoke-GdCli -Verb 'vision-click-point' -Body ([pscustomobject]@{
        nx = $Verdict.X; ny = $Verdict.Y
        imgW = [int]$Shot.ImgW; imgH = [int]$Shot.ImgH
        clientLeft = [int]$Shot.ClientLeft; clientTop = [int]$Shot.ClientTop
        clientW = [int]$Shot.ClientW; clientH = [int]$Shot.ClientH
        max = [int]$Max; why = [string]$Verdict.Why
    })
    if (-not $d.Found) { return $null }
    # Why 是从 verdict 那里顺带过来的：调用方要把"这个点位模型说它是干什么的"打进日志，
    # 否则日志里只有三个坐标，回看时谁也说不清当时点的是 × 还是"暂不查看"。
    return [pscustomobject]@{ ClientX = [int]$d.ClientX; ClientY = [int]$d.ClientY; BitmapX = [int]$d.BitmapX;
        BitmapY = [int]$d.BitmapY; NormX = [int]$d.NormX; NormY = [int]$d.NormY; Why = [string]$d.Why }
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
    <# 转发壳(resolve-vision-endpoint)：补 /chat/completions 的规矩(为什么补、末尾斜杠、已写全不重补)在 Vision.cs。 #>
    param([string]$Base)
    $d = Invoke-GdCli -Verb 'resolve-vision-endpoint' -Body ([pscustomobject]@{ base = [string]$Base })
    return [string]$d.Url
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
        # 逐行挑哪个对象(parse-vision-reply)住在 Vision.cs；这里只接住"挑到了就拿来用"。
        $pick = Invoke-GdCli -Verb 'parse-vision-reply' -Body ([pscustomobject]@{ out = $out })
        if ($pick.Found) { $parsed = $pick.Parsed }
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

function Clear-GdVisionFiles {
    <# 兜底的临时东西都是取证用的，别把 state 堆成几百个文件：按时间留最近 Keep 个。
       两路共用同一个目录，截图文件名开头是靶子的 kind(ark-/mumu-)，翻图时一眼分得清是谁的画面。 #>
    param([Parameter(Mandatory)][string]$Dir, [string]$Pattern = '*.png', [int]$Keep = 24)
    if (-not (Test-Path -LiteralPath $Dir)) { return }
    $stale = @(Get-ChildItem -LiteralPath $Dir -Filter $Pattern -ErrorAction SilentlyContinue |
               Sort-Object LastWriteTime -Descending | Select-Object -Skip $Keep)
    foreach ($f in $stale) { Remove-Item -LiteralPath $f.FullName -Force -ErrorAction SilentlyContinue }
}

function Clear-GdVisionAskDir {
    <# 请求文件里只有提示词和本机截图路径，没有任何 key(python 侧只从环境变量读)；
       留最近 12 份 = 够回看"这轮兜底到底问了什么"，又不会堆成日志。calls.jsonl 不删(它是账本，一行一条)。 #>
    Clear-GdVisionFiles -Dir (Get-GdVisionAskDir) -Pattern 'ask-*.json' -Keep 12
}
