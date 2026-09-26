# lib/vision-mumu.ps1 — 视觉兜底的**模拟器后端**：adb 拍屏 + adb 点一下
#
# 为什么 1999 这一路也要接：M9A 和 MAA 得的是同一种病 —— 模板集里没有的界面它看不见。
# 实测 2026-09-25 19:24 那一轮，1999 客户端在模拟器里下 1.91G 更新包，10 个任务里 8 个「任务失败」，
# 我们的日志只会说"进程提前退出"，而画面上一眼就能看出是"停在更新/下载界面"。
# 人眼一看就知道的那一步(把弹窗点掉 / 认出还卡在哪个界面)交给视觉模型。
#
# 与端游后端最要紧的三点差别(都写进靶子了，判定层不用知道)：
#   1) 不要管理员：adb 连的是虚拟机里的安卓，宿主机提不提权跟它无关 → NeedsAdmin=$false；
#   2) 截图是整块安卓屏，没有标题栏和边框 → ClientLeft/ClientTop 报 0、ClientW/ClientH 报图的宽高，
#      于是 lib\vision-rescue.ps1 那个通用的千分比→坐标函数算出来正好是设备像素；
#   3) 点击是 `adb shell input tap`，消息进的是虚拟机，**不会碰到宿主机上用户正在用的窗口** →
#      没有"抢前台"这一步，Grab 给 $null。
#
# 契约(靶子要提供什么)写在 lib\vision-rescue.ps1 的文件头；命名约定 New-GdVisionTarget-<kind>。
#
# 依赖：调用方先点 lib\common.ps1、lib\vision-rescue.ps1(提供 Get-GdShotStats)，
# 本路的 lib\mumu.ps1 在不在都能跑(用到它的地方都先 Get-Command 探路)。

function Get-GdMuMuAdbEndpoint {
    <# 把"该用哪个 adb、打哪个序列号"定下来，三个来源按可信度排：
       ① 调用方直接给的 $Conn(m9a.ps1 从实例配置里读到的 AdbDevice，那是这一路真要用的设备)；
       ② MuMuManager 的 `adb -v <index>` —— 虚拟机在跑时才给得出，输出形态各家版本不一，
          所以只从整段文本里抠第一个 `ip:port`，不认字段名；没跑时它会回 errcode -201；
       ③ `adb devices` 里第一个状态为 device 的序列号(兜到 ② 也拿不到时的情况，比如用户自己连过)。
       绝不写死 16384/7555 这类端口：换了 MuMu 版本或实例编号就错，而且本机专属值不许进源码(见 test-modules 的 m8)。
       返回 @{Adb;Serial;Why}，拿不到时 Adb/Serial 为空、Why 是能进日志的一句话。 #>
    param([Parameter(Mandatory)]$S, [object]$Conn = $null)
    $adb = ''
    if ($Conn -and $Conn.Adb) { $adb = [string]$Conn.Adb }
    if (-not $adb -and (Get-Command Get-GdMuMu -ErrorAction SilentlyContinue)) {
        try { $adb = [string](Get-GdMuMu -S $S).Adb } catch { $adb = '' }
    }
    # 到这儿就没了：paths.mumuAdb 存的是"相对 mumuDir 的文件名"(nx_main\adb.exe)，
    # 自己拼是 Get-GdMuMu 的活，这里再兜一层只会把一个相对路径当 adb 用、报出看不懂的"命令不存在"。
    if (-not $adb -or -not (Test-GdPathReady $adb)) {
        return [pscustomobject]@{ Adb = ''; Serial = ''; Why = '找不到 MuMu 自带的 adb(先跑 setup.ps1 填 paths.mumuDir)' }
    }

    $serial = ''
    if ($Conn -and $Conn.Serial) { $serial = [string]$Conn.Serial }
    if (-not $serial -and (Get-Command Get-GdMuMu -ErrorAction SilentlyContinue)) {
        $m = $null
        try { $m = Get-GdMuMu -S $S } catch { $m = $null }
        if ($m -and (Test-GdPathReady $m.Manager)) {
            $r = Invoke-GdNative -File $m.Manager -Arguments @('adb', '-v', $m.Index) -TimeoutSeconds 30
            $hit = [regex]::Match([string]($r.Out + $r.Err), '\b\d{1,3}(?:\.\d{1,3}){3}:\d{2,5}\b')
            if ($hit.Success) { $serial = $hit.Value }
        }
    }
    if (-not $serial) {
        # 最后一次机会：已经连着的设备里挑一个。宁可挑到"另一台设备"也不要因为版本差异整个兜底不生效？
        # 不是 —— 挑错设备会把点击打进别的模拟器实例，所以这里只在**恰好只有一台**时才认。
        $dev = Invoke-GdNative -File $adb -Arguments @('devices') -TimeoutSeconds 20
        $ons = @([regex]::Matches([string]$dev.Out, '(?m)^(\S+)\s+device\b') | ForEach-Object { $_.Groups[1].Value })
        if ($ons.Count -eq 1) { $serial = $ons[0] }
    }
    if (-not $serial) {
        return [pscustomobject]@{ Adb = $adb; Serial = ''; Why = '拿不到 adb 序列号：MuMu 虚拟机多半没在跑(MuMuManager adb 会回 vm not running)' }
    }
    return [pscustomobject]@{ Adb = $adb; Serial = $serial; Why = '' }
}

function Save-GdMuMuShot {
    <# adb 拍一张屏 → PNG 文件 → 换算成与端游那一路同形状的截图对象。
       为什么不用 `exec-out screencap -p` 直接接 stdout：Invoke-GdNative 按**文本**读子进程输出，
       PNG 二进制过一遍编码就废了。所以走"设备里落一张 + pull 出来"这两步，两边都不碰二进制管道。 #>
    param(
        [Parameter(Mandatory)][string]$Adb,
        [Parameter(Mandatory)][string]$Serial,
        [Parameter(Mandatory)][string]$Path
    )
    if (-not (Test-GdPathReady $Adb) -or -not $Serial) {
        return [pscustomobject]@{ Ok = $false; Why = 'adb 或序列号没定下来，拍不了屏' }
    }
    $remote = '/sdcard/gd-rescue-shot.png'
    $cap = Invoke-GdNative -File $Adb -Arguments @('-s', $Serial, 'shell', 'screencap', '-p', $remote) -TimeoutSeconds 40
    if ($cap.Code -ne 0) {
        return [pscustomobject]@{ Ok = $false; Why = "screencap 失败(退出码 $($cap.Code))：$((($cap.Out + $cap.Err) -replace '\s+', ' ').Trim())" }
    }
    $dir = Split-Path -Parent $Path
    if ($dir -and -not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir -Force) }
    $pull = Invoke-GdNative -File $Adb -Arguments @('-s', $Serial, 'pull', $remote, $Path) -TimeoutSeconds 40
    if ($pull.Code -ne 0 -or -not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{ Ok = $false; Why = "pull 失败(退出码 $($pull.Code))：$((($pull.Out + $pull.Err) -replace '\s+', ' ').Trim())" }
    }
    Add-Type -AssemblyName System.Drawing -ErrorAction SilentlyContinue
    $bmp = $null
    try {
        $bmp = New-Object System.Drawing.Bitmap($Path)
        $w = [int]$bmp.Width; $h = [int]$bmp.Height
        if ($w -lt 200 -or $h -lt 150) {
            return [pscustomobject]@{ Ok = $false; Why = "模拟器截屏尺寸异常 ${w}x${h}(安卓那边还没起画面)" }
        }
        $stats = Get-GdShotStats -Bmp $bmp -W $w -H $h
        return [pscustomobject]@{
            Ok = $true; Path = $Path; ImgW = $w; ImgH = $h
            ClientLeft = 0; ClientTop = 0; ClientW = $w; ClientH = $h    # 整块屏就是"客户区"，通用换算因此直接给出设备像素
            AvgLum = $stats.AvgLum; Sig = $stats.Sig; Hash = (Get-FileHash -LiteralPath $Path -Algorithm MD5).Hash; Why = ''
        }
    } catch {
        return [pscustomobject]@{ Ok = $false; Why = "读不了这张 PNG：$($_.Exception.Message)" }
    } finally {
        if ($bmp) { $bmp.Dispose() }
    }
}

function Send-GdMuMuTap {
    <# 往模拟器屏幕上那个点发一次点击，返回 @{Mode;Sent} —— 与 Send-GdArkClick 同一个出口形状。
       为什么这里可以很"直接"：input tap 的事件投进的是虚拟机里的安卓，宿主机的前台窗口、
       用户正在用的程序都碰不到，所以不需要端游那套"抢前台才敢真实点击"的顾虑。 #>
    param(
        [Parameter(Mandatory)][string]$Adb,
        [Parameter(Mandatory)][string]$Serial,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y
    )
    $r = Invoke-GdNative -File $Adb -Arguments @('-s', $Serial, 'shell', 'input', 'tap', "$X", "$Y") -TimeoutSeconds 30
    if ($r.Code -ne 0) {
        return [pscustomobject]@{ Mode = "adb input tap 失败(退出码 $($r.Code))"; Sent = $false }
    }
    return [pscustomobject]@{ Mode = 'adb input tap(点进虚拟机，不碰宿主机窗口)'; Sent = $true }
}

function New-GdVisionTarget-mumu {
    <# 模拟器这一路的靶子。命名照 lib\vision-rescue.ps1 的约定，由 Get-GdVisionTarget 找过来。
       $Conn 由调用方传(m9a.ps1 手上有实例配置)；不传就现场去问 MuMuManager —— 两件事都放在
       Alive/Shot 里现场做，因为"虚拟机起没起"在两三次拍图之间真的会变。
       开关是新键 mumuVisionRescue(默认开，写法与端游那个一致：显式写 false 才关)。 #>
    param([Parameter(Mandatory)]$S, [object]$Conn = $null)
    $on = ($S.runtime.mumuVisionRescue -ne $false)
    # 端点(adb 路径 + 序列号)不在这里定死：Shot/Click/Alive 各自现查一次。
    # 理由是"虚拟机起没起、adb server 有没有被别的工具重启过"在两三次拍图之间真的会变，
    # 而拿着一个过期序列号去 screencap，报回来的是一句看不懂的 adb 错误。

    $alive = {
        param([string]$AdbPath = '', [string]$Ser = '')
        if (-not $AdbPath -or -not $Ser) {
            $again = Get-GdMuMuAdbEndpoint -S $S -Conn $Conn
            if (-not $again.Adb -or -not $again.Serial) { return $false }
            $AdbPath = $again.Adb; $Ser = $again.Serial
        }
        $dev = Invoke-GdNative -File $AdbPath -Arguments @('devices') -TimeoutSeconds 20
        if (([string]$dev.Out) -match [regex]::Escape($Ser)) { return $true }
        # 没列出来不代表连不上：MuMu 重启过、宿主机 adb server 是新起的都会这样，先补一次 connect 再判。
        if (Get-Command Connect-GdAdb -ErrorAction SilentlyContinue) {
            try { return [bool](Connect-GdAdb -S $S -AdbPath $AdbPath -Serial $Ser) } catch { return $false }
        }
        return [bool]((Invoke-GdNative -File $AdbPath -Arguments @('connect', $Ser) -TimeoutSeconds 30).Code -eq 0)
    }.GetNewClosure()

    $shot = {
        param([string]$Path)
        $cur = Get-GdMuMuAdbEndpoint -S $S -Conn $Conn
        if (-not $cur.Adb -or -not $cur.Serial) { return [pscustomobject]@{ Ok = $false; Why = $cur.Why } }
        return (Save-GdMuMuShot -Adb $cur.Adb -Serial $cur.Serial -Path $Path)
    }.GetNewClosure()

    $click = {
        param([int]$X, [int]$Y)
        $cur = Get-GdMuMuAdbEndpoint -S $S -Conn $Conn
        if (-not $cur.Adb -or -not $cur.Serial) { return [pscustomobject]@{ Mode = 'adb 通道断了'; Sent = $false } }
        return (Send-GdMuMuTap -Adb $cur.Adb -Serial $cur.Serial -X $X -Y $Y)
    }.GetNewClosure()

    return [pscustomobject]@{
        Kind       = 'mumu'
        Name       = 'MuMu 模拟器画面'
        Enabled    = [bool]$on
        Why        = $(if (-not $on) { 'mumuVisionRescue=false，模拟器这一路的视觉兜底已关闭' } else { '' })
        NeedsAdmin = $false      # adb 连的是虚拟机里的安卓，宿主机提不提权与它无关
        AdminWhy   = ''
        Scene      = @'
你是《重返未来：1999》自动化流程里的"看屏幕的人"。游戏跑在 MuMu 模拟器里，自动化助手(M9A)只认
自己模板集里的界面，遇到没收录过的弹窗或更新提示就会一直在原地重试。给你一张模拟器屏幕截图，
请判断画面上有没有挡住主流程的东西(活动公告、更新/下载提示、登录弹窗、网络错误框这类)，并指出该点哪里把它关掉。
'@.TrimEnd()
        UserText   = '这是模拟器里 1999 的当前画面。按规则只回一个 JSON 对象。'
        Shot       = $shot
        Click      = $click
        Alive      = $alive      # 不带参数调用 → 自己现查一次端点，再问 adb devices/补一次 connect
        Grab       = $null       # 没有"抢前台"这一步：点击进的是虚拟机，宿主机的前台窗口不受影响
    }
}
