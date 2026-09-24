<#
  tools/check-ps51.ps1 —— "这套脚本在 Windows PowerShell 5.1 里还跑得动吗"的静态守卫。

  为什么需要它：仓库的访客里绝大多数只有系统自带的 powershell.exe(5.1)，不会为这个仓库单独装 PowerShell 7。
  5.1 与 7 的差异分两类 —— 编译器能抓到的和抓不到的，所以这个工具做三件事：

    1) 用"当前引擎"的解析器解析每个 .ps1。在 5.1 里跑这一项才是权威的：三元 ?:、??、?.、&&/||、
       class、-Parallel 这些 7 专属写法会直接变成解析错误。(在 pwsh 7 里跑只能证明"7 能解析"。)
    2) 断言编码与行尾：.ps1 必须是 UTF-8 带 BOM —— 5.1 读不到 BOM 会把 UTF-8 中文按 GBK 拆，
       连字符串终止符都能拆丢，报出一大串看不懂的错；.cmd 反过来必须无 BOM + CRLF(cmd.exe 要 CRLF)。
       空脚本(0 字节或只剩 BOM)会同时过掉解析和编码两道检查 → 单独判 EMPTY 报红，不给假绿灯。
       加 -Fix 就按这个规矩就地重写：先断言正文非空，写完回读比对，对不上立刻还原原字节。
       任何批量改字节的操作都照这个套路来 —— 本仓库有过一次教训：探 BOM 用了
       [IO.File]::Create(那是"创建并清空")，一次跑完把仓库里所有 .ps1 截成了 0 字节。
    3) 静态列一批"能解析但 5.1 运行时会炸"的写法(参数名 / cmdlet / 自动变量 / 走 HTTP 没开 TLS 1.2)。

  用法：
    powershell -File tools\check-ps51.ps1        # 在 5.1 下自检(最有价值的一次)
    pwsh -File tools\check-ps51.ps1 -Fix         # 在 7 下统一编码与行尾(改字节，先拿副本目录试一次)
    pwsh -File tools\check-ps51.ps1 -Dir <副本> -Fix   # 推荐：先在临时目录的副本上跑一遍
    pwsh -File tools\check-ps51.ps1 -Verbose2    # 把每个文件的编码情况也列出来

  退出码：0=干净；1=有解析错误或编码不合规(必须修)；2=只有"可疑写法"提示(自己判断)。
#>
[CmdletBinding()]
param(
    [string]$Dir = '.',
    [switch]$Fix,
    [switch]$Verbose2
)

$ErrorActionPreference = 'Stop'

# 输出里全是中文，重定向到文件时默认按 OEM/GBK 写会变成乱码 —— 和 lib/common.ps1 一样先统一到 UTF-8
try {
    [System.Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    [System.Console]::InputEncoding  = [System.Text.Encoding]::UTF8
} catch { }

# 能解析、但 Windows PowerShell 5.1 运行期会出问题的写法。N=说明，P=命中规则。
$badPatterns = @(
    @{ N = 'null 合并运算符(7 专属)';        P = '(?<!\\)\?\?(?!\?)' },
    @{ N = 'null 条件访问(7 专属)';          P = '[\w\)\]]\?\.' },
    @{ N = 'ForEach-Object -Parallel(7 专属)'; P = '-Parallel' },
    @{ N = 'ConvertFrom-Json -AsHashtable(7 专属)'; P = '-AsHashtable' },
    @{ N = 'utf8NoBOM 编码(5.1 没有这一项)';  P = 'utf8NoBOM' },
    @{ N = 'Get-Error(7 专属 cmdlet)';       P = '\bGet-Error\b' },
    @{ N = '$IsWindows 等(7 专属自动变量)';   P = '\$Is(Windows|Linux|MacOS)\b' },
    @{ N = '-SkipCertificateCheck(7 专属)';  P = '-SkipCertificateCheck' },
    @{ N = '$PSStyle(7 专属)';               P = '\$PSStyle\b' },
    @{ N = '#Requires -Version 7(挡 5.1 的门)'; P = '#Requires\s+-Version\s+7'; Raw = $true },
    @{ N = 'ConvertTo-Json -EscapeHandling(6+ 才有)'; P = '-EscapeHandling' },
    @{ N = 'Get-Date -AsUTC(7 专属)';        P = '-AsUTC\b' },
    @{ N = 'Test-Json(7 专属)';              P = '\bTest-Json\b' },
    @{ N = 'Join-Path 传多个子路径(5.1 只认一个)'; P = 'Join-Path\s+\S+\s+(\S+\s*,|\()?\S+,\s*\S+,' },
    @{ N = '三元表达式 a ? b : c(7 专属)';    P = '\$\w[\w\)\]]*\s*\?\s*[^?]*\s*:\s' }
)

function Get-GdTextFiles {
    param([string]$Root)
    return @(Get-ChildItem -Path $Root -Recurse -Include '*.ps1', '*.cmd', '*.bat' |
        Where-Object { $_.FullName -notmatch '\\(bin|obj|backups|logs|node_modules|\.git)\\' } |
        Sort-Object FullName)
}

function Read-GdBytes {
    param([string]$Path)
    $fs = [System.IO.File]::OpenRead($Path)
    try {
        $b = New-Object byte[] $fs.Length
        if ($b.Length -gt 0) { $fs.Read($b, 0, $b.Length) | Out-Null }
        return ,$b
    } finally { $fs.Close() }
}

function Get-GdFileKind {
    param([System.IO.FileInfo]$F)
    switch ($F.Extension.ToLowerInvariant()) {
        '.ps1' { return [pscustomobject]@{ NeedBom = $true;  NeedCrlf = $false } }
        '.cmd' { return [pscustomobject]@{ NeedBom = $false; NeedCrlf = $true  } }
        '.bat' { return [pscustomobject]@{ NeedBom = $false; NeedCrlf = $true  } }
        default { return $null }
    }
}

function Test-GdBom {
    param([byte[]]$Bytes)
    return ($Bytes.Length -ge 3 -and $Bytes[0] -eq 0xEF -and $Bytes[1] -eq 0xBB -and $Bytes[2] -eq 0xBF)
}

function Get-GdEolStats {
    param([byte[]]$Bytes)
    $lf = 0; $crlf = 0
    for ($i = 0; $i -lt $Bytes.Length; $i++) {
        if ($Bytes[$i] -ne 0x0A) { continue }
        if ($i -gt 0 -and $Bytes[$i - 1] -eq 0x0D) { $crlf++ } else { $lf++ }
    }
    return [pscustomobject]@{ Lf = $lf; Crlf = $crlf }
}

function Get-GdPlainBytes {
    <# 字节 → "去 BOM、统一 LF" 的正文。比对重写前后用同一把尺子。 #>
    param([byte[]]$Bytes)
    $skip = 0
    if (Test-GdBom -Bytes $Bytes) { $skip = 3 }
    if ($Bytes.Length -le $skip) { return '' }
    return ([System.Text.Encoding]::UTF8.GetString($Bytes, $skip, $Bytes.Length - $skip) -replace "`r`n", "`n")
}

function Set-GdFileEncoding {
    <#
      按规矩重写：.ps1 = UTF-8 BOM + LF；.cmd/.bat = UTF-8 无 BOM + CRLF。
      编码写法上的坑：UTF8Encoding($true).GetBytes() 是不写 BOM 的(BOM 只由 preamble/WriteAllText 负责)，
      所以走 [IO.File]::WriteAllText —— 它才会把 encoding.GetPreamble() 拼在最前面。

      三道保险(都是被真实事故喂出来的，这次把整个仓库的 .ps1 截成了 0 字节)：
        1) 空文件拒写：读回来 <=3 字节(0 字节或光剩个 BOM)时，"重写编码"只会把事故固化，直接返回 empty。
           顺带记住反例：探 BOM 绝不能用 [IO.File]::Create(path) —— Create 是"创建并清空"，
           想只读必须用 OpenRead/FileStream(FileMode::Open, FileAccess::Read)。
        2) 正文为空拒写：解码后 IsNullOrWhiteSpace 也当空文件处理。
        3) 写完立刻回读，用同一把尺子(Get-GdPlainBytes + 同样的行尾规则)比对正文；
           不一致就用内存里那份原始字节还原，返回 rolled-back —— 宁可留着"没修好"，也不能丢内容。
      返回：fixed / empty / rolled-back。
    #>
    param([System.IO.FileInfo]$F, [bool]$NeedBom, [bool]$NeedCrlf)
    $orig = Read-GdBytes -Path $F.FullName
    if ($orig.Length -le 3) { return 'empty' }
    $text = Get-GdPlainBytes -Bytes $orig
    if ([string]::IsNullOrWhiteSpace($text)) { return 'empty' }
    if ($NeedCrlf) { $text = $text -replace "`n", "`r`n" }
    [System.IO.File]::WriteAllText($F.FullName, $text, (New-Object System.Text.UTF8Encoding($NeedBom)))
    $after = Get-GdPlainBytes -Bytes (Read-GdBytes -Path $F.FullName)
    if ($NeedCrlf) { $after = $after -replace "`n", "`r`n" }
    if ($after -cne $text) {
        [System.IO.File]::WriteAllBytes($F.FullName, $orig)
        return 'rolled-back'
    }
    return 'fixed'
}

function Get-GdSnip {
    param([string]$Text, [int]$Width = 100)
    $t = ($Text -replace '\s+', ' ').Trim()
    if ($t.Length -gt $Width) { $t = $t.Substring(0, $Width) + '…' }
    return $t
}

$root = (Resolve-Path -LiteralPath $Dir).Path
$files = Get-GdTextFiles -Root $root
Write-Host ("引擎 PowerShell {0}   目录 {1}   待检查 {2} 个文件" -f $PSVersionTable.PSVersion, $root, $files.Count) -ForegroundColor Cyan
if ($PSVersionTable.PSVersion.Major -lt 6) {
    Write-Host '当前是 Windows PowerShell 5.1：下面的解析结果就是权威结论。' -ForegroundColor Green
}
else {
    Write-Host '当前是 PowerShell 7：这一遍只能证明"7 能解析"，5.1 的结论请再用 powershell.exe 跑一次本文件。' -ForegroundColor Yellow
}

$parseErrors = 0
$encodingErrors = 0
$suspects = 0
$needsTls = @()

foreach ($f in $files) {
    $kind = Get-GdFileKind -F $f
    if ($null -eq $kind) { continue }

    # ---------- 2) 编码 / 行尾 ----------
    $bytes = Read-GdBytes -Path $f.FullName
    if ($bytes.Length -le 3) {
        $encodingErrors++
        Write-Host ("EMPTY {0,-23} 只有 {1} 字节 —— 空脚本能同时过掉解析和编码两道检查，这是假绿灯；先确认内容是不是被截断了" -f $f.Name, $bytes.Length) -ForegroundColor Red
        continue
    }
    $hasBom = Test-GdBom -Bytes $bytes
    $eol = Get-GdEolStats -Bytes $bytes
    $badBom = ($hasBom -ne $kind.NeedBom)
    $badEol = if ($kind.NeedCrlf) { ($eol.Lf -gt 0) } else { ($eol.Crlf -gt 0) }
    if ($Fix -and ($badBom -or $badEol)) {
        $res = Set-GdFileEncoding -F $f -NeedBom $kind.NeedBom -NeedCrlf $kind.NeedCrlf
        if ($res -eq 'fixed') {
            Write-Host ("FIX  {0,-24} → BOM={1} CRLF={2}" -f $f.Name, $kind.NeedBom, $kind.NeedCrlf) -ForegroundColor Green
        }
        elseif ($res -eq 'empty') {
            Write-Host ("SKIP {0,-24} 空文件，拒绝重写" -f $f.Name) -ForegroundColor Red
        }
        else {
            Write-Host ("ROLL {0,-24} 重写后回读正文不一致，已还原原字节" -f $f.Name) -ForegroundColor Red
        }
        $bytes = Read-GdBytes -Path $f.FullName
        $hasBom = Test-GdBom -Bytes $bytes
        $eol = Get-GdEolStats -Bytes $bytes
        $badBom = ($hasBom -ne $kind.NeedBom)
        $badEol = if ($kind.NeedCrlf) { ($eol.Lf -gt 0) } else { ($eol.Crlf -gt 0) }
    }
    if ($badBom -or $badEol) {
        $encodingErrors++
        $why = @()
        if ($badBom) { $why += $(if ($hasBom) { '不该有 BOM' } else { '缺 BOM(5.1 下中文会乱) ' }) }
        if ($badEol) { $why += $(if ($kind.NeedCrlf) { '应是 CRLF(cmd.exe 要)' } else { '混进了 CRLF' }) }
        Write-Host ("ENC  {0,-24} {1}" -f $f.Name, ($why -join '，')) -ForegroundColor Red
    }
    elseif ($Verbose2) {
        Write-Host ("enc  {0,-24} OK  BOM={1} LF={2} CRLF={3}" -f $f.Name, $hasBom, $eol.Lf, $eol.Crlf) -ForegroundColor DarkGray
    }

    if ($f.Extension.ToLowerInvariant() -ne '.ps1') { continue }

    # ---------- 1) 语法 ----------
    $tokens = $null; $errs = $null
    [System.Management.Automation.Language.Parser]::ParseFile($f.FullName, [ref]$tokens, [ref]$errs) | Out-Null
    if (@($errs).Count -gt 0) {
        $parseErrors += @($errs).Count
        Write-Host ("SYN  {0,-24} 解析错误 {1} 个" -f $f.Name, @($errs).Count) -ForegroundColor Red
        foreach ($e in @($errs | Select-Object -First 5)) {
            Write-Host ("     第 {0} 行：{1}" -f $e.Extent.StartLineNumber, (Get-GdSnip ($e.Message -replace "`r?`n", ' ') 140)) -ForegroundColor Yellow
        }
        continue
    }
    elseif ($Verbose2) { Write-Host ("syn  {0,-24} OK" -f $f.Name) -ForegroundColor DarkGray }

    # 本文件的规则表里全是这些关键词，扫自己只会刷屏
    if ($f.Name -eq 'check-ps51.ps1') { continue }

    # ---------- 3) 运行期可疑写法 ----------
    $lines = @([System.IO.File]::ReadAllLines($f.FullName))
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $t = [string]$lines[$i]
        $trim = $t.TrimStart()
        foreach ($bp in $badPatterns) {
            # 注释行里出现这些词多是文档，别当 bug 报；除非那条明确标了 Raw(#Requires 长得像注释但真是门)
            if ($trim.StartsWith('#') -and -not $bp.Raw) { continue }
            if ($t -match $bp.P) {
                $suspects++
                Write-Host ("SUS  {0}:{1}  {2}" -f $f.Name, ($i + 1), $bp.N) -ForegroundColor Magenta
                Write-Host ("     {0}" -f (Get-GdSnip $t)) -ForegroundColor DarkGray
            }
        }
    }
    $raw = [string]([System.IO.File]::ReadAllText($f.FullName))
    if ($raw -match 'Invoke-RestMethod|Invoke-WebRequest' -and $raw -notmatch 'SecurityProtocol|Enable-GdTls12') {
        $needsTls += $f.Name
        Write-Host ("SUS  {0,-24} 走 HTTP 但没显式开 TLS 1.2 → 5.1 默认只发 TLS 1.0，握手会失败" -f $f.Name) -ForegroundColor Magenta
    }
}

Write-Host ''
if ($Fix) { Write-Host '已按规矩重写编码/行尾(.ps1 = UTF-8 BOM + LF；.cmd = UTF-8 无 BOM + CRLF)。' -ForegroundColor Green }
Write-Host ("解析错误 {0} 个 / 编码不合规 {1} 个 / 可疑写法 {2} 处" -f $parseErrors, $encodingErrors, $suspects)
if ($needsTls.Count) { Write-Host ("需要补 TLS 1.2 的文件：$($needsTls -join ', ')") -ForegroundColor Yellow }
if ($parseErrors -gt 0 -or $encodingErrors -gt 0) {
    Write-Host '结论：在 PowerShell 5.1 里跑不起来(RED 行是必须修的)。' -ForegroundColor Red
    exit 1
}
if ($suspects -gt 0) {
    Write-Host '结论：语法与编码过关，但上面 MAGENTA 行是 5.1 的运行期地雷，逐条确认后再发布。' -ForegroundColor Yellow
    exit 2
}
Write-Host '结论：这个引擎下没有发现 5.1 不兼容写法。' -ForegroundColor Green
exit 0
