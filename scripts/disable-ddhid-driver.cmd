@echo off
setlocal EnableExtensions EnableDelayedExpansion

rem Disable the DD-HID kernel driver service.
rem
rem Online usage, from installed FlairBloom or an elevated Windows cmd.exe:
rem   disable-ddhid-driver.cmd --online
rem
rem Offline usage, from Windows Recovery Environment cmd.exe:
rem   disable-ddhid-driver.cmd
rem   disable-ddhid-driver.cmd D:
rem   disable-ddhid-driver.cmd D:\Windows
rem
rem This script does not delete or rename ddhid63340.sys. It only sets:
rem   HKLM\SYSTEM\ControlSetXXX\Services\ddhid63340\Start = 4

set "HIVE=HKLM\OFFSYS"
set "FOUND=0"
set "FAILED=0"

if /I "%~1"=="--online" goto :online

set "WINROOT="

if not "%~1"=="" (
  set "INPUT=%~1"
  if exist "!INPUT!\System32\Config\SYSTEM" set "WINROOT=!INPUT!"
  if exist "!INPUT!\Windows\System32\Config\SYSTEM" set "WINROOT=!INPUT!\Windows"
)

if not defined WINROOT (
  for %%D in (C D E F G H I J K L M N O P Q R S T U V W X Y Z) do (
    if exist "%%D:\Windows\System32\Config\SYSTEM" (
      set "WINROOT=%%D:\Windows"
      goto :found_windows
    )
  )
)

:found_windows
if not defined WINROOT (
  echo Could not find an offline Windows installation.
  echo Pass the Windows drive or Windows directory explicitly, for example:
  echo   %~nx0 D:
  echo   %~nx0 D:\Windows
  exit /b 1
)

echo Offline Windows directory: !WINROOT!

reg unload "%HIVE%" >nul 2>&1
reg load "%HIVE%" "!WINROOT!\System32\Config\SYSTEM"
if errorlevel 1 (
  echo Failed to load SYSTEM hive from !WINROOT!\System32\Config\SYSTEM
  exit /b 1
)

for /f "delims=" %%K in ('reg query "%HIVE%" 2^>nul ^| findstr /R "\\ControlSet[0-9][0-9][0-9]$"') do (
  call :disable_key "%%K\Services\ddhid63340"
)

reg unload "%HIVE%"
if errorlevel 1 (
  echo Warning: failed to unload %HIVE%. Reboot before editing this hive again.
  set "FAILED=1"
)

goto :finish

:online
echo Disabling online DD-HID driver service.
sc.exe stop ddhid63340 >nul 2>&1
call :disable_key "HKLM\SYSTEM\CurrentControlSet\Services\ddhid63340"
for /f "delims=" %%K in ('reg query "HKLM\SYSTEM" 2^>nul ^| findstr /R "\\ControlSet[0-9][0-9][0-9]$"') do (
  call :disable_key "%%K\Services\ddhid63340"
)
goto :finish

:disable_key
set "KEY=%~1"
reg query "!KEY!" >nul 2>&1
if errorlevel 1 exit /b 0
echo Disabling !KEY!
reg add "!KEY!" /v Start /t REG_DWORD /d 4 /f
if errorlevel 1 set "FAILED=1"
set "FOUND=1"
exit /b 0

:finish
if "!FOUND!"=="0" (
  echo ddhid63340 service key was not found.
  exit /b 2
)

if "!FAILED!"=="1" (
  echo Failed to disable one or more ddhid63340 service entries.
  exit /b 1
)

echo DD-HID driver service is disabled. Reboot Windows.
exit /b 0
