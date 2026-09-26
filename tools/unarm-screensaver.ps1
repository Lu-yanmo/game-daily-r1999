<#
  tools\unarm-screensaver.ps1 — 把"到点会有全屏程序接管输入桌面"这件事断根（只动当前用户，不需要管理员）。

  为什么要它：06:00 / 18:00 两轮方舟连败的根因是屏保桌面接管会话（日志里那行
  `点击环境探测：输入桌面=Screen-saver`）。日常脚本已经改成"起跑时关屏保、收尾不再恢复"，
  但那只是把武装位拨到 0 —— 注册表里仍挂着 Wallpaper Engine 的 wpxscreensaver64.scr，
  任何一次屏保设置改动都可能把它重新点亮。本脚本删的是**挂着的 .scr 本身**，删了就没有东西可拉起。

  默认只读：不加参数就是体检，把现值、要动哪几项、动完的复检打出来，一个字节都不写。

    pwsh -File tools\unarm-screensaver.ps1            # 看现值 + 会改什么(不写)
    pwsh -File tools\unarm-screensaver.ps1 -Apply     # 真解除(原值先备份进 state\screensaver.unarm.json)
    pwsh -File tools\unarm-screensaver.ps1 -Restore   # 照备份原样装回去(后悔药)

  改完不用注销：脚本会往 HWND_BROADCAST 发一次 WM_SETTINGCHANGE，让正在跑的会话立刻重读。
  已经全屏跑着的屏保不会因此退出(那要真实输入或它自己到点)，脚本会当场把输入桌面名报给你。

  退出码(只读和 -Apply 同一套规矩，别的脚本可以直接判)：0=已断根 / 1=还没断根(仍武装或只关不复) / 2=读不到注册表或没有备份。
#>
[CmdletBinding()]
param(
    [switch]$Apply,
    [switch]$Restore
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $root 'lib\common.ps1')

$reg = 'HKCU:\Control Panel\Desktop'
$backupFile = Join-Path (Get-GdDir 'State') 'screensaver.unarm.json'

function Show-GdScreensaverReading {
    param([string]$Label)
    $cfg = Get-GdScreensaverConfig
    $v = Get-GdScreensaverVerdict -Cfg $cfg
    Write-Host ("  {0}：{1}" -f $Label, $v.Detail) -ForegroundColor $(switch ($v.Kind) { 'armed' { 'Red' } 'none' { 'Green' } 'off' { 'Green' } default { 'Yellow' } })
    return $cfg
}

function Set-GdSettingChangedBroadcast {
    <# 让正在跑的会话立刻重读 HKCU\Control Panel\Desktop。
        只发一条广播消息，不注入输入、不碰别的进程 —— 所以它不需要提权，也不会替你把屏保点掉。 #>
    try {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class GdSettingChange {
    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern IntPtr SendMessageTimeoutW(IntPtr hWnd, uint msg, IntPtr wParam, string lParam,
        uint fuFlags, uint uTimeout, out IntPtr lpdwResult);
}
'@
        $res = [IntPtr]::Zero
        # HWND_BROADCAST=0xFFFF, WM_SETTINGCHANGE=0x1A, SMTO_ABORTIFHUNG=0x0002, 参数 "Windows Shell"
        [void][GdSettingChange]::SendMessageTimeoutW([IntPtr]0xFFFF, 0x1A, [IntPtr]::Zero, 'Windows Shell', 0x0002, 3000, [ref]$res)
        return $true
    } catch {
        Write-Host "  广播 WM_SETTINGCHANGE 失败：$($_.Exception.Message)(注册表已经改好了，注销/重登一次生效)" -ForegroundColor Yellow
        return $false
    }
}

Write-Host '--- 屏保解除工具(只动当前用户，默认只读) ---'
$desk = ''
try { $desk = Get-GdInputDesktopName } catch { }
Write-Host ("  输入桌面现在 = {0}" -f $(if ($desk) { $desk } else { '读不到' }))
if ($desk -and $desk -ne 'Default') {
    Write-Host "  注意：会话已经被屏保/锁屏桌面接管，这个工具不会(也不该)替你把画面点掉。" -ForegroundColor Yellow
    Write-Host "        你要动一下鼠标/按键把它退出；解除本身写的是注册表，不受影响。" -ForegroundColor Yellow
}

if ($Restore) {
    if (-not (Test-Path -LiteralPath $backupFile)) {
        Write-Host "  没有备份文件($backupFile) → 不知道原来挂着什么，什么都不动。" -ForegroundColor Red
        exit 2
    }
    $old = Get-Content -LiteralPath $backupFile -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($old.Saver) {
        New-ItemProperty -LiteralPath $reg -Name 'SCRNSAVE.EXE' -Value ([string]$old.Saver) -PropertyType String -Force | Out-Null
    } else {
        Remove-ItemProperty -LiteralPath $reg -Name 'SCRNSAVE.EXE' -ErrorAction SilentlyContinue
    }
    Set-ItemProperty -LiteralPath $reg -Name 'ScreenSaveActive' -Value ([string]$old.Active) -Type String
    if ($old.TimeoutSec) { Set-ItemProperty -LiteralPath $reg -Name 'ScreenSaveTimeOut' -Value ([string]$old.TimeoutSec) -Type String }
    [void](Set-GdSettingChangedBroadcast)
    Write-Host "  已按备份装回：" -NoNewline
    $null = Show-GdScreensaverReading '复检'
    Write-Host "  备份文件留着没删(再 -Restore 一次还是这份)，装回 Wallpaper Engine 的屏保请顺手也在它自己的设置里确认一遍。"
    exit 0
}

$cfg = Show-GdScreensaverReading '现值'
if ($null -eq $cfg) {
    Write-Host "  注册表读不到($reg) → 什么都不动。" -ForegroundColor Red
    exit 2
}
$planned = @()
if ($cfg.Saver) { $planned += ('删除 SCRNSAVE.EXE（现在挂着 ' + (Get-GdScreensaverWho -Cfg $cfg) + '）') }
if ($cfg.Active -ne '0') { $planned += 'ScreenSaveActive 置 0' }

if (-not $planned.Count) {
    Write-Host '  已经是"没挂 .scr 且未武装"的稳态，无事可做。' -ForegroundColor Green
    exit 0
}
Write-Host '  要动的项：'
foreach ($p in $planned) { Write-Host "    · $p" }

if (-not $Apply) {
    Write-Host '  这次只读：加 -Apply 才真的写。' -ForegroundColor Yellow
    Write-Host '  (Windows 侧等价的点法：设置 → 个性化 → 锁屏界面 → 屏幕保护程序 → 下拉选「(无)」→ 确定。)'
    # 退出码照本工具的规矩走(0=已断根 / 1=还没断根)，不只读就一律 0 ——
    # 这样它同时也是"别的脚本想知道屏保武装没有"的一道可判的闸(和 status.ps1 那一行同源：都是 Get-GdScreensaverVerdict)。
    exit $(if ((Get-GdScreensaverVerdict -Cfg $cfg).Risk) { 1 } else { 0 })
}

# 写之前把三项原值落到备份里：-Restore 靠它，而且万一 Wallpaper Engine 那边还想要屏保，
# 用户能看到"原来挂的是哪个文件"，不用去猜 8.3 短名 WPXSCR~1.SCR 是什么。
$bak = [ordered]@{
    Saver      = [string]$cfg.Saver
    Product    = [string]$cfg.Product
    Active     = [string]$cfg.Active
    TimeoutSec = [int]$cfg.TimeoutSec
}
($bak | ConvertTo-Json) | Set-Content -LiteralPath $backupFile -Encoding UTF8
Write-Host ("  原值已备份：{0}" -f $backupFile)

if ($cfg.Saver) { Remove-ItemProperty -LiteralPath $reg -Name 'SCRNSAVE.EXE' -ErrorAction SilentlyContinue }
Set-ItemProperty -LiteralPath $reg -Name 'ScreenSaveActive' -Value '0' -Type String
# 顺带把 SPI 武装位也压一次：会话正常时它和注册表同步，会话被接管时它会失败——失败不影响上面的稳态
try { [void](Set-GdScreensaverActive -Active $false) } catch { }
[void](Set-GdSettingChangedBroadcast)

$after = Show-GdScreensaverReading '解除后'
$v = Get-GdScreensaverVerdict -Cfg $after
if ($v.Kind -eq 'none') {
    Write-Host '  成了：系统里再没有到点会全屏接管的东西。日常那一路不再需要"关屏保"。' -ForegroundColor Green
} else {
    Write-Host ("  还没断根(复检={0})：{1}" -f $v.Kind, $v.Detail) -ForegroundColor Red
}
Write-Host '  过几分钟再看一眼：要是复检又变回"仍武装"，说明 Wallpaper Engine 在自己的设置里把屏保重新注册了 ——'
Write-Host '  那就要在它自己的设置界面上关（Windows 这层关不干净），本工具下次 -Apply 也照样给你压回来。'
exit $(if ($v.Kind -eq 'none') { 0 } else { 1 })
