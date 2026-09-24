param([string]$Dir = '.')
$fail = 0
Get-ChildItem -Path $Dir -Filter *.ps1 -Recurse | ForEach-Object {
    $tok = $null; $errs = $null
    [System.Management.Automation.Language.Parser]::ParseFile($_.FullName, [ref]$tok, [ref]$errs) | Out-Null
    if ($errs -and $errs.Count) {
        $script:fail++
        Write-Host "ERR $($_.Name)"
        $errs | ForEach-Object { Write-Host "   line $($_.Extent.StartLineNumber): $($_.Message)" }
    } else {
        Write-Host "OK  $($_.Name)"
    }
}
Write-Host "failed files: $fail"
