@echo off
rem panel.cmd —— 双击打开图形状态面板(tools\panel.ps1)，给不想碰命令行的人。
rem 引擎挑选：优先 PowerShell 7，没有就用系统自带的 Windows PowerShell 5.1(面板只用 WinForms，两边都能跑)。
rem 面板只是外壳：所有动作都转调 daily-clear.ps1 / status.ps1 / run-via-task.ps1，判定逻辑一份都不重复。
rem 所以「面板里看到的结论」和命令行跑同一套脚本得到的结论是同一个 —— 它不自己算。
rem 注意：本文件必须是 CRLF 行尾、且不能带 UTF-8 BOM(cmd.exe 两样都不认)。
chcp 65001 >nul
setlocal
title 每日清体力 状态面板
set "PS="
where pwsh.exe >nul 2>nul && set "PS=pwsh.exe"
if not defined PS if exist "%ProgramFiles%\PowerShell\7\pwsh.exe" set "PS=%ProgramFiles%\PowerShell\7\pwsh.exe"
if not defined PS set "PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
rem 裸开(没带 -SelfTest 之类参数)且 C# 面板已编译过 → 优先它；带参数照旧走 PowerShell 面板，验收链不受影响。
if "%~1"=="" if exist "%~dp0tools\panel-cs\panel-cs.exe" (
    start "" "%~dp0tools\panel-cs\panel-cs.exe"
    exit /b 0
)
"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\panel.ps1" %*
set "RC=%ERRORLEVEL%"
echo.
if not "%RC%"=="0" echo 面板退出码 %RC%。多半是 WinForms 起不来(极精简系统/组策略)。命令行做的是同一件事，例如：
if not "%RC%"=="0" echo   "%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0daily-clear.ps1" -DryRun
if not "%RC%"=="0" echo 全部参数和排查步骤见 README.md。
if not "%RC%"=="0" pause
exit /b %RC%
