@echo off
chcp 65001 > nul
rem 配布用の zip（dist\Alpedia-win64.zip）を作る。ゲーム部分を dev から入れ直してから zip にする（Python 本体などは作り直さない）。
cd /d %~dp0build
where py >nul 2>nul && (py -3 build_package.py --skip-runtime --zip) || (python build_package.py --skip-runtime --zip)
if errorlevel 1 (echo.& echo 失敗しました。上のメッセージを確認してください。) else (echo.& echo できました: dist\Alpedia-win64.zip)
pause
