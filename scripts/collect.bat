@echo off
chcp 65001 >nul
echo ==================================================
echo   开始运行软件清单采集脚本...
echo ==================================================

where pwsh >nul 2>nul
if %ERRORLEVEL% equ 0 (
    pwsh -NoProfile -ExecutionPolicy Bypass -File "%~dp0collect.ps1"
) else (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0collect.ps1"
)

echo.
echo 采集完毕！按任意键退出...
pause >nul
