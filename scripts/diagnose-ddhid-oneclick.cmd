@echo off
setlocal

set "SCRIPT=%~dp0diagnose-ddhid.ps1"
if not exist "%SCRIPT%" (
  echo 找不到 diagnose-ddhid.ps1，请确认两个脚本在同一个文件夹里。
  pause
  exit /b 1
)

net session >nul 2>&1
if %errorlevel% neq 0 (
  echo 正在请求管理员权限，请在弹出的 UAC 窗口中点“是”。
  powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  exit /b
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT%"
echo.
echo 完成。请把桌面上的 ddhid-diagnose-*.txt 文件发给开发者。
pause
