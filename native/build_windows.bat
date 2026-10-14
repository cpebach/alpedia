@echo off
chcp 65001 >nul
rem Alpedia の物理コア（Rust）を Windows 用にビルドして lib\alpedia_core.pyd に置く。
rem 初めての人は、先に https://rustup.rs から rustup-init.exe を入れてください（README.md を参照）。
setlocal
cd /d "%~dp0alpedia_core"
where rustup >nul 2>nul
if errorlevel 1 (
  echo.
  echo Rust が見つかりません。https://rustup.rs の rustup-init.exe で入れてから、もう一度実行してください。
  echo （入れたあとは、このウィンドウを閉じて開き直す必要があります）
  pause
  exit /b 1
)
echo [1/3] Rust（Windows 用 GNU ツールチェーン）を準備しています...
rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
if errorlevel 1 goto fail
echo [2/3] ビルドしています（初回は数分かかります）...
set PYO3_NO_PYTHON=1
cargo +stable-x86_64-pc-windows-gnu build --release
if errorlevel 1 goto fail
echo [3/3] コピーしています...
if not exist "..\..\lib" mkdir "..\..\lib"
copy /Y "target\release\alpedia_core.dll" "..\..\lib\alpedia_core.pyd" >nul
if errorlevel 1 goto fail
if exist "..\..\package for windows\Alpedia\game" (
  if not exist "..\..\package for windows\Alpedia\game\lib" mkdir "..\..\package for windows\Alpedia\game\lib"
  copy /Y "target\release\alpedia_core.dll" "..\..\package for windows\Alpedia\game\lib\alpedia_core.pyd" >nul
  echo パッケージ版にも入れました: package for windows\Alpedia\game\lib\alpedia_core.pyd
)
echo.
echo 完了: lib\alpedia_core.pyd
echo ゲームを起動すると、画面左上に「物理:Rust」と出ます。
pause
exit /b 0
:fail
echo.
echo ビルドに失敗しました。上のメッセージを確認してください。
pause
exit /b 1
