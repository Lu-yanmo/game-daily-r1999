@echo off
rem 一键手动跑一轮每日清体力（这个仓库负责的那一路）
rem 走按需计划任务（名字是 settings.json 里 schedule.taskPrefix 加 -Manual，注册时就是最高权限）→ 不再弹 UAC；
rem 任务还没注册时会自动退回本机直跑（那时才会弹一次 UAC）。注册命令：pwsh -File install-task.ps1
rem 引擎自己挑：装了 PowerShell 7 就用 pwsh，只有系统自带的 Windows PowerShell 5.1 也能跑（仓库里的脚本两个引擎都兼容）。
rem 注意：本文件必须是 CRLF 行尾(cmd.exe 对 LF 的 if 块解析不可靠)，.gitattributes 里已固定 eol=crlf。
chcp 65001 >nul
setlocal
set "PS="
where pwsh.exe >nul 2>nul && set "PS=pwsh.exe"
if not defined PS if exist "%LOCALAPPDATA%\Microsoft\WindowsApps\pwsh.exe" set "PS=%LOCALAPPDATA%\Microsoft\WindowsApps\pwsh.exe"
if not defined PS if exist "%ProgramFiles%\PowerShell\7\pwsh.exe" set "PS=%ProgramFiles%\PowerShell\7\pwsh.exe"
if not defined PS set "PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-via-task.ps1"
echo.
echo 退出码 %ERRORLEVEL%（0=全部完成 1=部分完成 2=有失败），日志在 logs\ 目录
pause
