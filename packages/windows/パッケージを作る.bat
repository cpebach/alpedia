@echo off
chcp 65001 > nul
cd /d %~dp0build
where py >nul 2>nul && (py -3 build_package.py %*) || (python build_package.py %*)
if errorlevel 1 (echo.& echo 失敗しました。上のメッセージを確認してください。)
pause
