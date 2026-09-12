@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0rgx-update.ps1" %*
exit /b %errorlevel%
